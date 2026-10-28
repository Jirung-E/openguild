//! Campaign mutation orchestration (DEV-011).
//!
//! `services::campaigns` (SQL) + 파일 IO + journal append 를 묶음. 호출 순서:
//! 1. journal::append (의도 기록).
//! 2. SQL mutation (services::campaigns::*).
//! 3. `.guild/campaigns/<slug>.md` atomic write — frontmatter 가 DB 의 새 값
//!    반영, 본문 markdown 은 DEV-066 패턴으로 외부 편집 보존 (없으면 빈 body).
//! 4. 체크리스트 변경 ops 는 본문도 같이 수정 후 파일 → DB sync.

use serde_json::json;

use crate::lock::DocKey;
use crate::error::{AppError, AppResult};
use crate::events::{names as ev, payload};
use crate::models::{
    CampaignChecklistItem, CampaignDetail, CampaignRow, CreateCampaignRequest,
    UpdateCampaignRequest,
};
use crate::repo::{CampaignFile, CampaignFrontmatter, extract_checklist_items};
use crate::services::campaigns as sql;
use crate::store::{Store, journal};

// ─────────────────────── 조회 헬퍼 ───────────────────────

pub async fn fetch_detail(store: &Store, slug: &str) -> AppResult<CampaignDetail> {
    let row = sql::fetch_by_slug(&store.index_pool, slug).await?;
    let checklists = sql::list_checklists(&store.index_pool, row.id).await?;
    let linked_quests = sql::list_linked_quests(&store.index_pool, row.id).await?;
    // DEV-093: linked_quests 의 status_slug 기반으로 done 카운트 계산.
    // service 의 list_linked_quests 는 alive only — total = linked_quests.len().
    let quest_total = linked_quests.len() as i64;
    // counts_as_done = 1 인 status_id 들 fetch.
    let done_slugs: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM quest_statuses WHERE counts_as_done = 1",
    )
    .fetch_all(&store.index_pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("done slug fetch: {e}")))?;
    let done_set: std::collections::HashSet<&str> =
        done_slugs.iter().map(|s| s.as_str()).collect();
    let quest_done = linked_quests
        .iter()
        .filter(|q| done_set.contains(q.status_slug.as_str()))
        .count() as i64;
    let quest_progress = if quest_total > 0 {
        quest_done as f64 / quest_total as f64
    } else {
        0.0
    };
    // DEV-233: 상태별 카운트 — CampaignCard 의 summarize() 와 동일 헬퍼 공유.
    let quest_status_counts = sql::quest_status_counts(&store.index_pool, row.id).await?;
    Ok(CampaignDetail {
        campaign: row,
        checklists,
        linked_quests,
        quest_total,
        quest_done,
        quest_progress,
        // DEV-156: 첨부는 Store 가진 호출 계층(GUI 커맨드)에서 채움.
        attachments: Vec::new(),
        quest_status_counts,
    })
}

// ─────────────────────── 생성 / 수정 / 삭제 ───────────────────────

pub async fn create_campaign(store: &Store, body: CreateCampaignRequest) -> AppResult<CampaignRow> {
    let _g = store.lock_docs(&[DocKey::new("counter", "campaign")]).await?;
    let _ = journal::append(
        &store.journal_pool,
        "create_campaign",
        &body,
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let camp = sql::create(&store.index_pool, body).await?;
    write_campaign_file(store, &camp, true).await?;
    // DEV-388: DB 와 파일이 다 쓰인 **뒤에만** 낸다. journal 은 위에서 의도를
    // 먼저 적지만 이벤트는 일어난 일만 싣는다.
    store.emit_post(
        ev::CAMPAIGN_CREATED,
        || json!({ "campaign": payload::campaign(&camp) }),
    );
    Ok(camp)
}

pub async fn update_campaign(
    store: &Store,
    id: i64,
    body: UpdateCampaignRequest,
) -> AppResult<CampaignRow> {
    let _g = super::lock_resolved(store, || campaign_key(store, id)).await?;
    let _ = journal::append(
        &store.journal_pool,
        "update_campaign",
        &json!({ "id": id, "body": body }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    // DEV-226: status 변경 이력 기록 — quest 의 change_status(quest_history)
    // 와 동일 패턴. status 가 요청에 없거나 현재값과 같으면(no-op) 기록 안 함.
    let old_status = if body.status.is_some() {
        Some(sql::fetch_by_id(&store.index_pool, id).await?.status)
    } else {
        None
    };
    let new_status = body.status.clone();

    let description_explicit = body.description.is_some();
    // DEV-463: 본문을 직접 고쳐도 체크리스트가 바뀐다 — 바뀌기 전에 '모두 체크' 였나.
    let checked_before = if description_explicit {
        let prev = sql::fetch_by_id(&store.index_pool, id)
            .await?
            .description
            .unwrap_or_default();
        Some(all_checked(&extract_checklist_items(&prev)))
    } else {
        None
    };
    let camp = sql::update(&store.index_pool, id, body).await?;
    write_campaign_file(store, &camp, description_explicit).await?;
    // REQ-008: 이 문서가 내보내는 cross-link 재계산 — BUG-189 가 doc_history 를
    // 즉시 투영한 것과 같은 이유다(reindex 전까지 반영 안 되면 없는 기능과 같다).
    let _ = crate::ops::backlinks::refresh_for(store, crate::repo::crosslink::DocKind::Campaign, &camp.campaign_slug).await;

    if let (Some(old), Some(new)) = (old_status, new_status)
        && old != new
    {
        record_history(
            store,
            id,
            &camp.campaign_slug,
            "change_status",
            Some(old),
            Some(new),
        )
        .await?;
    }
    // DEV-463: 본문을 직접 고쳐 마지막 칸을 체크한 것도 '모두 체크' 다.
    if let Some(before) = checked_before {
        let after = extract_checklist_items(camp.description.as_deref().unwrap_or(""));
        note_checklist_all_done(store, id, &camp.campaign_slug, before, &after).await?;
    }

    // DEV-388: 상태 이력까지 남긴 뒤가 이 mutation 의 끝이다.
    store.emit_post(
        ev::CAMPAIGN_UPDATED,
        || json!({ "campaign": payload::campaign(&camp) }),
    );
    Ok(camp)
}

/// BUG-255: 배너 확장자 화이트리스트 — 경로 경로(데스크톱)와 bytes 경로(서버)가
/// **같은 목록**을 봐야 한다. 두 벌로 두면 한쪽에서만 되는 확장자가 생긴다.
pub const BANNER_EXTS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

fn check_banner_ext(ext: &str) -> AppResult<String> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    if !BANNER_EXTS.contains(&ext.as_str()) {
        return Err(AppError::BadRequest(crate::tf!(
            "지원하지 않는 이미지 확장자: .{ext} ({})",
            "unsupported image extension: .{ext} ({})",
            BANNER_EXTS.join("/")
        )));
    }
    Ok(ext)
}

/// BUG-255: 배너 쓰기의 **앞부분** — 확장자 검증, journal, 옛 배너 제거,
/// 대상 경로 확보. 파일을 실제로 놓는 것은 호출자 몫이다.
///
/// 데스크톱은 로컬 경로에서 복사하고(`set_banner_image`), 서버는 요청 body 를
/// 그 자리에 스트리밍한다. 둘로 갈라지는 건 "파일을 어떻게 놓느냐" 뿐이라
/// 나머지는 여기와 `commit_banner_image` 가 공유한다.
///
/// 반환: `(rel, abs)` — `.guild` 기준 상대 경로와 절대 경로.
pub async fn begin_banner_image(
    store: &Store,
    slug: &str,
    ext: &str,
    source_note: &str,
) -> AppResult<(String, std::path::PathBuf)> {
    let _g = store.lock_docs(&[DocKey::campaign(slug)]).await?;
    let ext = check_banner_ext(ext)?;

    let _ = journal::append(
        &store.journal_pool,
        "set_campaign_banner",
        &json!({ "slug": slug, "source": source_note }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let camp = sql::fetch_by_slug(&store.index_pool, slug).await?;

    std::fs::create_dir_all(store.paths.assets_dir())
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let rel = format!("assets/{slug}-banner.{ext}");
    let dest = store.paths.dot_guild().join(&rel);
    // 확장자가 바뀌면 옛 파일은 이름이 달라 덮이지 않는다 — 직접 지운다.
    if let Some(old_rel) = &camp.image_path
        && old_rel != &rel
    {
        let _ = std::fs::remove_file(store.paths.dot_guild().join(old_rel));
    }
    Ok((rel, dest))
}

/// BUG-255: 배너 쓰기의 **뒷부분** — 파일이 제자리에 놓인 뒤 DB + frontmatter 갱신.
pub async fn commit_banner_image(store: &Store, slug: &str, rel: &str) -> AppResult<CampaignRow> {
    let _g = store.lock_docs(&[DocKey::campaign(slug)]).await?;
    let camp = sql::fetch_by_slug(&store.index_pool, slug).await?;
    sqlx::query("UPDATE campaigns SET image_path = ? WHERE id = ?")
        .bind(rel)
        .bind(camp.id)
        .execute(&store.index_pool)
        .await?;
    let camp = sql::fetch_by_slug(&store.index_pool, slug).await?;
    write_campaign_file(store, &camp, false).await?;
    // DEV-388: 배너가 실제로 바뀌는 지점은 여기 하나다 — 데스크톱의
    // `set_banner_image` 도 서버의 bytes 라우트도 마지막엔 이걸 부른다.
    // 여기서만 내야 경로마다 한 번씩만 나간다.
    store.emit_post(
        ev::CAMPAIGN_BANNER_CHANGED,
        || json!({ "campaign": payload::campaign(&camp), "banner": camp.image_path }),
    );
    Ok(camp)
}

/// DEV-087: 배너 이미지 설정 — source 파일을 `.guild/assets/{slug}-banner.{ext}`
/// 로 복사 + frontmatter / DB 갱신. 기존 배너는 덮어씀 (캠페인당 1장).
///
/// **로컬 경로 전용** — 데스크톱(Tauri)에서만 부를 수 있다. 브라우저/원격은
/// 경로가 없으므로 서버의 bytes 라우트가 `begin_banner_image` +
/// `commit_banner_image` 를 직접 쓴다(BUG-255).
pub async fn set_banner_image(
    store: &Store,
    slug: &str,
    source_path: &std::path::Path,
) -> AppResult<CampaignRow> {
    if !source_path.exists() {
        return Err(AppError::BadRequest(crate::tf!(
            "이미지 파일 없음: {}",
            "image file not found: {}",
            source_path.display()
        )));
    }
    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    let (rel, dest) =
        begin_banner_image(store, slug, &ext, &source_path.display().to_string()).await?;

    std::fs::copy(source_path, &dest).map_err(|e| {
        AppError::Internal(anyhow::anyhow!(crate::tf!(
            "이미지 복사 실패: {e}",
            "image copy failed: {e}"
        )))
    })?;

    commit_banner_image(store, slug, &rel).await
}

/// DEV-087: 배너 제거 — assets 파일 삭제 + frontmatter / DB NULL.
pub async fn clear_banner_image(store: &Store, slug: &str) -> AppResult<CampaignRow> {
    let _g = store.lock_docs(&[DocKey::campaign(slug)]).await?;
    let _ = journal::append(
        &store.journal_pool,
        "clear_campaign_banner",
        &json!({ "slug": slug }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let camp = sql::fetch_by_slug(&store.index_pool, slug).await?;
    if let Some(rel) = &camp.image_path {
        let _ = std::fs::remove_file(store.paths.dot_guild().join(rel));
    }
    sqlx::query("UPDATE campaigns SET image_path = NULL WHERE id = ?")
        .bind(camp.id)
        .execute(&store.index_pool)
        .await?;
    let camp = sql::fetch_by_slug(&store.index_pool, slug).await?;
    write_campaign_file(store, &camp, false).await?;
    // DEV-388: 걸 때와 같은 이름으로 내고 `banner` 를 null 로 둔다 — 이름을
    // 하나 더 만들지 않는 쪽(names.rs 주석 참고).
    store.emit_post(
        ev::CAMPAIGN_BANNER_CHANGED,
        || json!({ "campaign": payload::campaign(&camp), "banner": null }),
    );
    Ok(camp)
}

pub async fn delete_campaign(store: &Store, id: i64) -> AppResult<()> {
    let _g = super::lock_resolved(store, || campaign_key(store, id)).await?;
    let _ = journal::append(
        &store.journal_pool,
        "delete_campaign",
        &json!({ "id": id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let row = sql::fetch_by_id(&store.index_pool, id).await?;
    sql::delete(&store.index_pool, id).await?;
    // 파일은 frontmatter 의 deleted=true 로 마킹.
    let path = store.paths.campaign_path(&row.campaign_slug);
    if path.exists()
        && let Ok(mut cf) = CampaignFile::read(&path)
    {
        cf.frontmatter.deleted = true;
        cf.frontmatter.updated_at = crate::time::now_local_iso8601();
        let _ = cf.write(&path);
        let _ = crate::file_mtime::touch(store, &path).await; // DEV-178
    }
    // DEV-388: 지운 뒤에는 무엇이었는지 어디서도 못 읽는다. 행은 파일 마킹에
    // 쓰려고 위에서 이미 읽어 뒀으므로 `delete_quest` 의 `doomed` 게이트 없이도
    // 구독자 없는 길드가 더 무는 비용이 없다.
    store.emit_post(
        ev::CAMPAIGN_DELETED,
        || json!({ "campaign": payload::campaign(&row) }),
    );
    Ok(())
}

// ─────────────────────── Link / Unlink ───────────────────────

pub async fn link_quest_by_slug(
    store: &Store,
    campaign_id: i64,
    quest_slug: &str,
) -> AppResult<()> {
    let _g = super::lock_resolved(store, || campaign_key(store, campaign_id)).await?;
    let qid = sql::resolve_quest_id(&store.index_pool, quest_slug).await?;
    let _ = journal::append(
        &store.journal_pool,
        "campaign_link_quest",
        &json!({ "campaign_id": campaign_id, "quest_slug": quest_slug }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    sql::link_quest(&store.index_pool, campaign_id, qid).await?;
    let camp = sql::fetch_by_id(&store.index_pool, campaign_id).await?;
    write_campaign_file(store, &camp, false).await?; // 본문 외부편집 보존
    // DEV-388: 캠페인만 실으면 "무엇이 붙었나" 가 빠진다. 퀘스트는 slug 만
    // 들고 있으므로 ref 로 싣는다(행을 새로 읽어 오지 않는다).
    store.emit_post(ev::CAMPAIGN_QUEST_LINKED, || {
        json!({
            "campaign": payload::campaign(&camp),
            "quest": payload::quest_ref(quest_slug),
        })
    });
    Ok(())
}

pub async fn unlink_quest_by_slug(
    store: &Store,
    campaign_id: i64,
    quest_slug: &str,
) -> AppResult<()> {
    let _g = super::lock_resolved(store, || campaign_key(store, campaign_id)).await?;
    let qid = sql::resolve_quest_id(&store.index_pool, quest_slug).await?;
    let _ = journal::append(
        &store.journal_pool,
        "campaign_unlink_quest",
        &json!({ "campaign_id": campaign_id, "quest_slug": quest_slug }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    // DEV-463: 남은 하나뿐인 미완료 퀘스트를 떼어 내면 남은 것이 모두 완료가 된다.
    let before = {
        let (total, done) = quest_done_counts(store, campaign_id).await?;
        total > 0 && done == total
    };
    sql::unlink_quest(&store.index_pool, campaign_id, qid).await?;
    let camp = sql::fetch_by_id(&store.index_pool, campaign_id).await?;
    write_campaign_file(store, &camp, false).await?;
    note_quests_all_done(store, &[(campaign_id, camp.campaign_slug.clone(), before)]).await?;
    store.emit_post(ev::CAMPAIGN_QUEST_UNLINKED, || {
        json!({
            "campaign": payload::campaign(&camp),
            "quest": payload::quest_ref(quest_slug),
        })
    });
    Ok(())
}

// ─────────────────────── 체크리스트 ops ───────────────────────
//
// 체크리스트는 본문 markdown 의 `- [ ]` 줄로 저장 (B3). 모든 mutation 은
// 파일 본문을 먼저 수정 후 DB 와 sync. 인덱스 (1-based) 는 파일에서의
// 출현 순서.

pub async fn add_checklist_line(
    store: &Store,
    campaign_id: i64,
    text: &str,
) -> AppResult<CampaignChecklistItem> {
    let _g = super::lock_resolved(store, || campaign_key(store, campaign_id)).await?;
    let _ = journal::append(
        &store.journal_pool,
        "campaign_checklist_add",
        &json!({ "campaign_id": campaign_id, "text": text }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let camp = sql::fetch_by_id(&store.index_pool, campaign_id).await?;
    let path = store.paths.campaign_path(&camp.campaign_slug);
    let mut cf = if path.exists() {
        CampaignFile::read(&path).map_err(AppError::Internal)?
    } else {
        new_file_for(&camp)
    };
    // DEV-463: 새 항목은 체크가 안 된 채 붙는다 — '모두 체크' 를 만들 수는 없다.
    let new_body = append_checklist_to_body(&cf.body, text);
    cf.body = new_body;
    cf.frontmatter.updated_at = crate::time::now_local_iso8601();
    cf.write(&path).map_err(AppError::Internal)?;
    let _ = crate::file_mtime::touch(store, &path).await; // DEV-178

    // DB sync — 파일 본문 → DB.
    let items = extract_checklist_items(&cf.body);
    sql::replace_checklists_from_file(&store.index_pool, campaign_id, &items).await?;

    // 새로 추가된 항목 = list 의 마지막 (order_idx 가 가장 큰 것).
    let item = sql::list_checklists(&store.index_pool, campaign_id)
        .await?
        .into_iter()
        .last()
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "no checklist after add (sync failed?)"
            ))
        })?;
    record_history(
        store,
        campaign_id,
        &camp.campaign_slug,
        "checklist_add",
        None,
        Some(item.text.clone()),
    )
    .await?;
    // DEV-388: 어느 줄이 붙었는지를 싣는다 — 인덱스만 주면 구독자가 파일을
    // 다시 읽어야 한다. 저장된 뒤의 값(파서가 정규화한 text)을 쓴다.
    store.emit_post(ev::CAMPAIGN_CHECKLIST_ADDED, || {
        json!({
            "campaign": payload::campaign(&camp),
            "item": { "text": item.text },
        })
    });
    Ok(item)
}

/// 1-based 인덱스로 체크 / 언체크 토글.
pub async fn set_checklist_checked_by_index(
    store: &Store,
    campaign_id: i64,
    one_based_idx: usize,
    checked: bool,
) -> AppResult<()> {
    let _g = super::lock_resolved(store, || campaign_key(store, campaign_id)).await?;
    if one_based_idx == 0 {
        return Err(AppError::BadRequest(
            "checklist index is 1-based, got 0".into(),
        ));
    }
    let _ = journal::append(
        &store.journal_pool,
        "campaign_checklist_set",
        &json!({ "campaign_id": campaign_id, "index": one_based_idx, "checked": checked }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let camp = sql::fetch_by_id(&store.index_pool, campaign_id).await?;
    let path = store.paths.campaign_path(&camp.campaign_slug);
    if !path.exists() {
        return Err(AppError::NotFound(format!(
            "campaign file not found: {}",
            path.display()
        )));
    }
    let mut cf = CampaignFile::read(&path).map_err(AppError::Internal)?;
    // DEV-388: 구독자에게 인덱스만 주면 그 숫자가 무엇이었는지 알 수 없다.
    // DEV-463: 이력에도 무슨 항목인지 남기므로 늘 훑는다.
    let before_items = extract_checklist_items(&cf.body);
    let prev = before_items.get(one_based_idx - 1).map(|i| i.checked);
    let text = before_items.get(one_based_idx - 1).map(|i| i.text.clone());
    let all_before = all_checked(&before_items);
    let new_body = set_checklist_checked_in_body(&cf.body, one_based_idx, checked)?;
    cf.body = new_body;
    cf.frontmatter.updated_at = crate::time::now_local_iso8601();
    cf.write(&path).map_err(AppError::Internal)?;
    let _ = crate::file_mtime::touch(store, &path).await; // DEV-178

    let items = extract_checklist_items(&cf.body);
    sql::replace_checklists_from_file(&store.index_pool, campaign_id, &items).await?;
    // DEV-463: 이미 그 상태였으면(다시 체크) 이력은 안 남긴다.
    if prev != Some(checked) {
        let op = if checked {
            "checklist_check"
        } else {
            "checklist_uncheck"
        };
        record_history(
            store,
            campaign_id,
            &camp.campaign_slug,
            op,
            None,
            text.clone(),
        )
        .await?;
        note_checklist_all_done(store, campaign_id, &camp.campaign_slug, all_before, &items)
            .await?;
    }
    store.emit_post(ev::CAMPAIGN_CHECKLIST_CHECKED, || {
        json!({
            "campaign": payload::campaign(&camp),
            "item": { "text": text, "checked": checked },
        })
    });
    Ok(())
}

/// 1-based 인덱스의 체크리스트 항목을 삭제 (파일 본문 줄 제거 + DB sync).
pub async fn remove_checklist_by_index(
    store: &Store,
    campaign_id: i64,
    one_based_idx: usize,
) -> AppResult<()> {
    let _g = super::lock_resolved(store, || campaign_key(store, campaign_id)).await?;
    if one_based_idx == 0 {
        return Err(AppError::BadRequest("checklist index is 1-based, got 0".into()));
    }
    let _ = journal::append(
        &store.journal_pool,
        "campaign_checklist_rm",
        &json!({ "campaign_id": campaign_id, "index": one_based_idx }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let camp = sql::fetch_by_id(&store.index_pool, campaign_id).await?;
    let path = store.paths.campaign_path(&camp.campaign_slug);
    if !path.exists() {
        return Err(AppError::NotFound(format!("file not found: {}", path.display())));
    }
    let mut cf = CampaignFile::read(&path).map_err(AppError::Internal)?;
    // DEV-388: 지워진 줄은 **지우기 전 본문에만** 있다. DEV-463: 이력에도 남기므로 늘 훑는다.
    let before_items = extract_checklist_items(&cf.body);
    let text = before_items.get(one_based_idx - 1).map(|i| i.text.clone());
    let all_before = all_checked(&before_items);
    let new_body = remove_checklist_in_body(&cf.body, one_based_idx)?;
    cf.body = new_body;
    cf.frontmatter.updated_at = crate::time::now_local_iso8601();
    cf.write(&path).map_err(AppError::Internal)?;
    let _ = crate::file_mtime::touch(store, &path).await; // DEV-178

    let items = extract_checklist_items(&cf.body);
    sql::replace_checklists_from_file(&store.index_pool, campaign_id, &items).await?;
    record_history(
        store,
        campaign_id,
        &camp.campaign_slug,
        "checklist_remove",
        text.clone(),
        None,
    )
    .await?;
    // 체크 안 된 마지막 항목을 지우면 남은 것이 모두 체크된 상태가 된다.
    note_checklist_all_done(store, campaign_id, &camp.campaign_slug, all_before, &items).await?;
    store.emit_post(ev::CAMPAIGN_CHECKLIST_REMOVED, || {
        json!({
            "campaign": payload::campaign(&camp),
            "item": { "text": text },
        })
    });
    Ok(())
}

// ─────────────────────── 파일 IO ───────────────────────

/// frontmatter 는 DB 최신값으로 구성. 본문 markdown 은 DEV-066 패턴:
/// `description_explicit = true` → DB `description` 사용,
/// `false` → 기존 파일 본문 보존 + DB 와 다르면 sync.
pub(crate) async fn write_campaign_file(
    store: &Store,
    camp: &CampaignRow,
    description_explicit: bool,
) -> AppResult<()> {
    let path = store.paths.campaign_path(&camp.campaign_slug);
    std::fs::create_dir_all(store.paths.campaigns_dir())?;

    // linked_quests slug 목록 — 연결된 quest 의 slug.
    let linked = sql::list_linked_quests(&store.index_pool, camp.id).await?;
    let linked_slugs: Vec<String> = linked.into_iter().map(|q| q.quest_id).collect();

    let body = if description_explicit {
        camp.description.clone().unwrap_or_default()
    } else {
        match CampaignFile::read(&path) {
            Ok(existing) if !existing.body.trim().is_empty() => {
                let db_desc = camp.description.as_deref().unwrap_or("");
                if existing.body != db_desc {
                    sqlx::query("UPDATE campaigns SET description = ? WHERE id = ?")
                        .bind(&existing.body)
                        .bind(camp.id)
                        .execute(&store.index_pool)
                        .await?;
                }
                // 체크리스트도 sync.
                let items = extract_checklist_items(&existing.body);
                sql::replace_checklists_from_file(&store.index_pool, camp.id, &items).await?;
                existing.body
            }
            _ => camp.description.clone().unwrap_or_default(),
        }
    };

    let cf = CampaignFile {
        frontmatter: CampaignFrontmatter {
            campaign_id: camp.campaign_slug.clone(),
            title: camp.title.clone(),
            status: camp.status.clone(),
            started_at: camp.started_at.clone().unwrap_or_default(),
            ended_at: camp.ended_at.clone().unwrap_or_default(),
            linked_quests: linked_slugs,
            display_order: camp.display_order,
            image: camp.image_path.clone(),
            created_at: camp.created_at.clone(),
            updated_at: camp.updated_at.clone(),
            deleted: false,
        },
        body,
    };
    cf.write(&path).map_err(AppError::Internal)?;
    // DEV-178: 외부편집 감지용 mtime 캐시 갱신 (drift / lazy refresh 오탐 방지).
    let _ = crate::file_mtime::touch(store, &path).await;
    Ok(())
}

fn new_file_for(camp: &CampaignRow) -> CampaignFile {
    CampaignFile {
        frontmatter: CampaignFrontmatter {
            campaign_id: camp.campaign_slug.clone(),
            title: camp.title.clone(),
            status: camp.status.clone(),
            started_at: camp.started_at.clone().unwrap_or_default(),
            ended_at: camp.ended_at.clone().unwrap_or_default(),
            linked_quests: vec![],
            display_order: camp.display_order,
            image: camp.image_path.clone(),
            created_at: camp.created_at.clone(),
            updated_at: camp.updated_at.clone(),
            deleted: false,
        },
        body: camp.description.clone().unwrap_or_default(),
    }
}

// ─────────────────────── 본문 markdown 조작 ───────────────────────

/// 본문 끝에 `- [ ] {text}` 한 줄 추가. 빈 줄 분리.
fn append_checklist_to_body(body: &str, text: &str) -> String {
    let trimmed = body.trim_end();
    if trimmed.is_empty() {
        return format!("- [ ] {text}\n");
    }
    format!("{trimmed}\n- [ ] {text}\n")
}

/// 1-based 인덱스의 task list 줄을 찾아 `- [x]` / `- [ ]` 로 set.
fn set_checklist_checked_in_body(body: &str, one_based_idx: usize, checked: bool) -> AppResult<String> {
    let mut found = 0usize;
    let mut out = String::with_capacity(body.len());
    let mut done = false;
    for line in body.lines() {
        if !done && is_task_list_line(line) {
            found += 1;
            if found == one_based_idx {
                out.push_str(&set_checked_marker(line, checked));
                out.push('\n');
                done = true;
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if !done {
        return Err(AppError::NotFound(format!(
            "checklist index {one_based_idx} out of range (found {found})"
        )));
    }
    // 끝 trailing newline 정규화 (lines() 가 마지막 \n 손실 케이스 대비).
    Ok(out)
}

/// 1-based 인덱스의 task list 줄을 본문에서 제거.
fn remove_checklist_in_body(body: &str, one_based_idx: usize) -> AppResult<String> {
    let mut found = 0usize;
    let mut out = String::with_capacity(body.len());
    let mut done = false;
    for line in body.lines() {
        if !done && is_task_list_line(line) {
            found += 1;
            if found == one_based_idx {
                done = true;
                continue; // skip this line
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if !done {
        return Err(AppError::NotFound(format!(
            "checklist index {one_based_idx} out of range (found {found})"
        )));
    }
    Ok(out)
}

fn is_task_list_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let after_bullet = if let Some(r) = trimmed.strip_prefix("- ") {
        r
    } else if let Some(r) = trimmed.strip_prefix("* ") {
        r
    } else if let Some(r) = trimmed.strip_prefix("+ ") {
        r
    } else {
        return false;
    };
    after_bullet.starts_with("[ ] ")
        || after_bullet.starts_with("[x] ")
        || after_bullet.starts_with("[X] ")
}

/// 한 task list 줄의 체크 표시를 set.
fn set_checked_marker(line: &str, checked: bool) -> String {
    // leading whitespace + bullet 유지
    let leading_ws_end = line.len() - line.trim_start().len();
    let (lead, rest) = line.split_at(leading_ws_end);
    // rest 는 `- [ ] foo` 형태.
    let bullet_end = rest
        .find(']')
        .expect("is_task_list_line guarantees bracket")
        + 1;
    let after = &rest[bullet_end..];
    let bullet_ch = &rest[..2]; // "- " / "* " / "+ "
    let marker = if checked { "[x]" } else { "[ ]" };
    format!("{lead}{bullet_ch}{marker}{after}")
}

// ─────────────────────── 이력(DEV-226 · DEV-463) ───────────────────────

/// 캠페인 이력 한 줄 — index 의 `campaign_history` 와 파일 사이드카(`.guild/history/{slug}.jsonl`, 진리원) 둘 다.
async fn record_history(
    store: &Store,
    id: i64,
    slug: &str,
    op: &str,
    old: Option<String>,
    new: Option<String>,
) -> AppResult<()> {
    let ts = crate::time::now_local_iso8601();
    sqlx::query(
        "INSERT INTO campaign_history (campaign_id, campaign_slug, ts, op, old_value, new_value)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(slug)
    .bind(&ts)
    .bind(op)
    .bind(&old)
    .bind(&new)
    .execute(&store.index_pool)
    .await?;
    // DEV-180 패턴: 파일 사이드카에도 append — 파일이 진리원.
    crate::repo::history::append(
        &store.paths,
        slug,
        &crate::repo::history::HistoryEntry {
            ts,
            op: op.into(),
            old,
            new,
        },
    )
    .map_err(AppError::Internal)?;
    Ok(())
}

/// 체크리스트가 하나라도 있고 모두 체크됐나.
fn all_checked(items: &[crate::repo::ChecklistLine]) -> bool {
    !items.is_empty() && items.iter().all(|i| i.checked)
}

/// DEV-463(admin): 체크리스트가 **모두 체크된 순간**을 남긴다 — 바뀌기 전에는 아니었고 지금(`after`)은 그렇다.
/// 새 값은 `체크/전체`.
async fn note_checklist_all_done(
    store: &Store,
    id: i64,
    slug: &str,
    before: bool,
    after: &[crate::repo::ChecklistLine],
) -> AppResult<()> {
    if before || !all_checked(after) {
        return Ok(());
    }
    let n = after.len();
    record_history(
        store,
        id,
        slug,
        "checklist_all_done",
        None,
        Some(format!("{n}/{n}")),
    )
    .await
}

/// 연결된 (안 지워진) 퀘스트 수와 그중 완료(`counts_as_done`) 수 — 캠페인 진행 막대와 같은 셈(DEV-093).
async fn quest_done_counts(store: &Store, id: i64) -> AppResult<(i64, i64)> {
    Ok(sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN qs.counts_as_done = 1 THEN 1 ELSE 0 END), 0)
           FROM campaign_quests cq
           JOIN quests q ON q.id = cq.quest_id
           JOIN quest_statuses qs ON qs.id = q.status_id
          WHERE cq.campaign_id = ? AND q.deleted_at IS NULL",
    )
    .bind(id)
    .fetch_one(&store.index_pool)
    .await?)
}

/// 이 퀘스트가 든 캠페인들 — 각각의 (id, slug, 지금 연결 퀘스트가 모두 완료인가).
///
/// 퀘스트 상태를 바꾸는 쪽이 **바꾸기 전에** 불러 두고, 바꾼 뒤 [`note_quests_all_done`] 에 넘긴다.
pub(crate) async fn quests_done_before(
    store: &Store,
    quest_id: i64,
) -> AppResult<Vec<(i64, String, bool)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT c.id, c.campaign_slug FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id
          WHERE cq.quest_id = ? AND c.deleted_at IS NULL",
    )
    .bind(quest_id)
    .fetch_all(&store.index_pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, slug) in rows {
        let (total, done) = quest_done_counts(store, id).await?;
        out.push((id, slug, total > 0 && done == total));
    }
    Ok(out)
}

/// DEV-463(admin): 연결된 퀘스트가 **모두 완료된 순간**을 남긴다 — 퀘스트 상태가 바뀌어서든, 남은 것을 떼어
/// 내서든. 새 값은 `완료/전체`.
pub(crate) async fn note_quests_all_done(
    store: &Store,
    before: &[(i64, String, bool)],
) -> AppResult<()> {
    for (id, slug, was) in before {
        if *was {
            continue;
        }
        let (total, done) = quest_done_counts(store, *id).await?;
        if total > 0 && done == total {
            record_history(
                store,
                *id,
                slug,
                "quests_all_done",
                None,
                Some(format!("{done}/{total}")),
            )
            .await?;
        }
    }
    Ok(())
}

/// 퀘스트가 든 캠페인들의 열쇠 — 퀘스트 상태 변경이 그 캠페인들의 이력에도 쓴다(DEV-463).
pub(crate) async fn campaign_keys_of_quest(store: &Store, quest_id: i64) -> AppResult<Vec<DocKey>> {
    let slugs: Vec<String> = sqlx::query_scalar(
        "SELECT c.campaign_slug FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id
          WHERE cq.quest_id = ? AND c.deleted_at IS NULL",
    )
    .bind(quest_id)
    .fetch_all(&store.index_pool)
    .await?;
    Ok(slugs.iter().map(|s| DocKey::campaign(s)).collect())
}

/// DEV-432: 캠페인 행 id → 문서 열쇠. 없으면 빈 목록 — 변경이 잠근 뒤 "없음" 으로 끝난다.
async fn campaign_key(store: &Store, id: i64) -> AppResult<Vec<DocKey>> {
    let slug: Option<String> = sqlx::query_scalar("SELECT campaign_slug FROM campaigns WHERE id = ?")
        .bind(id)
        .fetch_optional(&store.index_pool)
        .await?;
    Ok(slug.map(|s| DocKey::campaign(&s)).into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_to_empty_body() {
        assert_eq!(append_checklist_to_body("", "foo"), "- [ ] foo\n");
    }

    #[test]
    fn append_to_existing_body() {
        let b = "기획 본문\n";
        let new = append_checklist_to_body(b, "할 일");
        assert!(new.ends_with("- [ ] 할 일\n"));
        assert!(new.contains("기획 본문"));
    }

    #[test]
    fn set_checked_toggles_marker() {
        let body = "본문\n- [ ] A\n- [ ] B\n- [ ] C\n";
        let r = set_checklist_checked_in_body(body, 2, true).unwrap();
        assert!(r.contains("- [x] B"));
        assert!(r.contains("- [ ] A"));
        assert!(r.contains("- [ ] C"));
    }

    #[test]
    fn set_checked_out_of_range() {
        let body = "- [ ] A\n";
        let e = set_checklist_checked_in_body(body, 2, true).unwrap_err();
        assert!(matches!(e, AppError::NotFound(_)));
    }

    #[test]
    fn remove_drops_line() {
        let body = "- [ ] A\n- [x] B\n- [ ] C\n";
        let r = remove_checklist_in_body(body, 2).unwrap();
        assert!(r.contains("- [ ] A"));
        assert!(r.contains("- [ ] C"));
        assert!(!r.contains("- [x] B"));
    }

    // ─── BUG-255: 배너 — 경로 경로(데스크톱)가 리팩토링 후에도 그대로인지 ───

    async fn banner_setup(label: &str) -> (std::path::PathBuf, Store) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-banner-{label}-{ns}-{}", crate::test_seq()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        create_campaign(
            &store,
            crate::models::CreateCampaignRequest {
                title: "camp".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        (dir, store)
    }

    /// 데스크톱 경로(`set_banner_image`)는 BUG-255 에서 `begin_/commit_` 으로
    /// 쪼개졌다 — 동작이 그대로인지 고정한다. 이게 깨지면 앱에서 배너가 죽는다.
    #[tokio::test]
    async fn set_banner_from_path_copies_and_records() {
        let (dir, store) = banner_setup("path").await;
        let src = dir.join("src.png");
        std::fs::write(&src, b"PNGDATA").unwrap();

        let camp = set_banner_image(&store, "C-001", &src).await.unwrap();
        assert_eq!(camp.image_path.as_deref(), Some("assets/C-001-banner.png"));
        let dest = store.paths.dot_guild().join("assets/C-001-banner.png");
        assert_eq!(std::fs::read(&dest).unwrap(), b"PNGDATA");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 확장자가 바뀌면 옛 파일은 이름이 달라 **덮이지 않는다** — 직접 지워야
    /// assets 에 고아가 안 쌓인다. 경로/바이트 두 경로가 공유하는 부분이다.
    #[tokio::test]
    async fn set_banner_removes_old_file_when_extension_changes() {
        let (dir, store) = banner_setup("ext").await;
        let png = dir.join("a.png");
        std::fs::write(&png, b"PNG").unwrap();
        set_banner_image(&store, "C-001", &png).await.unwrap();
        let old = store.paths.dot_guild().join("assets/C-001-banner.png");
        assert!(old.exists());

        let gif = dir.join("b.gif");
        std::fs::write(&gif, b"GIF").unwrap();
        let camp = set_banner_image(&store, "C-001", &gif).await.unwrap();
        assert_eq!(camp.image_path.as_deref(), Some("assets/C-001-banner.gif"));
        assert!(!old.exists(), "옛 .png 가 남았다");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 확장자 화이트리스트는 한 곳(`check_banner_ext`)만 본다 — 경로 경로도
    /// 서버 bytes 경로와 **같은 목록**을 쓴다.
    #[tokio::test]
    async fn set_banner_rejects_unsupported_extension() {
        let (dir, store) = banner_setup("bad").await;
        let txt = dir.join("x.txt");
        std::fs::write(&txt, b"nope").unwrap();

        let e = set_banner_image(&store, "C-001", &txt).await.unwrap_err();
        assert!(matches!(e, AppError::BadRequest(_)), "got {e:?}");
        // 거부됐으면 파일도 안 생겨야 한다.
        assert!(!store.paths.dot_guild().join("assets/C-001-banner.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn banner_ext_is_case_and_dot_insensitive() {
        assert_eq!(check_banner_ext("PNG").unwrap(), "png");
        assert_eq!(check_banner_ext(".JPEG").unwrap(), "jpeg");
        assert!(check_banner_ext("txt").is_err());
        assert!(check_banner_ext("").is_err());
    }

    /// DEV-451: 캠페인 목록의 쿼리 — 키마다, 빼기, `--status` 와 함께, 틀리면 BadRequest.
    #[tokio::test]
    async fn campaign_query_filters_the_list() {
        let (dir, store) = banner_setup("query").await; // C-001 "camp" — 날짜 없음
        let mk = |title: &'static str, start: Option<&'static str>, end: Option<&'static str>| {
            let store = store.clone();
            async move {
                create_campaign(
                    &store,
                    crate::models::CreateCampaignRequest {
                        title: title.into(),
                        description: Some("출시 준비".into()),
                        started_at: start.map(Into::into),
                        ended_at: end.map(Into::into),
                    },
                )
                .await
                .unwrap()
            }
        };
        let late = mk("지난 것", Some("2020-01-01"), Some("2020-02-01")).await; // C-002, 진행 중
        let next = mk("다음 것", Some("2999-01-01"), Some("2999-02-01")).await; // C-003
        let done = mk("끝난 것", Some("2020-01-01"), Some("2020-02-01")).await; // C-004
        sqlx::query("UPDATE campaigns SET status = 'done' WHERE id = ?")
            .bind(done.id)
            .execute(&store.index_pool)
            .await
            .unwrap();
        let quest = crate::ops::quests::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "연결된 퀘스트".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(2),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let qslug = sqlx::query_scalar::<_, String>(
            "SELECT qt.prefix || '-' || printf('%03d', q.number) FROM quests q
               JOIN quest_types qt ON q.quest_type_id = qt.id WHERE q.id = ?",
        )
        .bind(quest.id)
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        link_quest_by_slug(&store, next.id, &qslug).await.unwrap();

        let slugs = |status: Option<&'static str>, q: &str| {
            let pool = store.index_pool.clone();
            let q = q.to_string();
            async move {
                crate::services::campaigns::list_matching(&pool, status, Some(&q))
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|c| c.campaign_slug)
                    .collect::<Vec<_>>()
            }
        };
        let sorted = |v: Vec<String>| {
            let mut v = v;
            v.sort();
            v
        };
        assert_eq!(sorted(slugs(None, "end:overdue").await), vec![late.campaign_slug.clone()]);
        assert_eq!(sorted(slugs(None, "status:done").await), vec![done.campaign_slug.clone()]);
        assert_eq!(sorted(slugs(None, "start:none").await), vec!["C-001".to_string()]);
        assert_eq!(
            sorted(slugs(None, "start:>=2026-01-01").await),
            vec![next.campaign_slug.clone()]
        );
        assert_eq!(
            sorted(slugs(None, &format!("quest:{}", qslug.to_lowercase())).await),
            vec![next.campaign_slug.clone()]
        );
        assert_eq!(
            sorted(slugs(None, "출시 -status:done").await),
            vec![late.campaign_slug.clone(), next.campaign_slug.clone()]
        );
        assert_eq!(sorted(slugs(Some("done"), "출시").await), vec![done.campaign_slug.clone()]);
        assert_eq!(sorted(slugs(None, "c-001").await), vec!["C-001".to_string()]);
        // 없는 키 · 틀린 값은 오류 — 조용히 "없음" 으로 보이지 않게.
        for bad in ["urgency:2", "status:open", "end:soon", "quest:abc"] {
            let e = crate::services::campaigns::list_matching(&store.index_pool, None, Some(bad))
                .await
                .unwrap_err();
            assert!(matches!(e, AppError::BadRequest(_)), "{bad}: {e:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // DEV-463(admin): 체크리스트 변경 · 모두 체크 · 연결 퀘스트 모두 완료를 이력에 남긴다.
    async fn history_store(label: &str) -> (std::path::PathBuf, Store, i64) {
        let dir = std::env::temp_dir().join(format!(
            "og-chist-{label}-{}-{}",
            std::process::id(),
            crate::test_seq()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        let camp = create_campaign(
            &store,
            CreateCampaignRequest {
                title: "캠프".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        (dir, store, camp.id)
    }

    async fn ops_of(store: &Store, id: i64) -> Vec<(String, Option<String>, Option<String>)> {
        let mut v: Vec<_> = sql::list_history(&store.index_pool, id)
            .await
            .unwrap()
            .into_iter()
            .map(|h| (h.op, h.old_value, h.new_value))
            .collect();
        v.reverse(); // 오래된 것부터
        v
    }

    fn op(
        o: &str,
        old: Option<&str>,
        new: Option<&str>,
    ) -> (String, Option<String>, Option<String>) {
        (o.into(), old.map(Into::into), new.map(Into::into))
    }

    #[tokio::test]
    async fn checklist_changes_and_the_all_checked_moment_are_recorded() {
        let (dir, store, id) = history_store("check").await;
        add_checklist_line(&store, id, "하나").await.unwrap();
        add_checklist_line(&store, id, "둘").await.unwrap();
        set_checklist_checked_by_index(&store, id, 1, true)
            .await
            .unwrap();
        set_checklist_checked_by_index(&store, id, 1, true)
            .await
            .unwrap(); // 이미 체크 — 안 남는다
        set_checklist_checked_by_index(&store, id, 2, true)
            .await
            .unwrap(); // 여기서 모두 체크
        set_checklist_checked_by_index(&store, id, 2, false)
            .await
            .unwrap();
        add_checklist_line(&store, id, "셋").await.unwrap();
        remove_checklist_by_index(&store, id, 3).await.unwrap();
        // 체크 안 된 둘을 지우면 남은 하나가 모두 체크 — 지워서 된 것도 그 순간이다.
        remove_checklist_by_index(&store, id, 2).await.unwrap();
        assert_eq!(
            ops_of(&store, id).await,
            vec![
                op("checklist_add", None, Some("하나")),
                op("checklist_add", None, Some("둘")),
                op("checklist_check", None, Some("하나")),
                op("checklist_check", None, Some("둘")),
                op("checklist_all_done", None, Some("2/2")),
                op("checklist_uncheck", None, Some("둘")),
                op("checklist_add", None, Some("셋")),
                op("checklist_remove", Some("셋"), None),
                op("checklist_remove", Some("둘"), None),
                op("checklist_all_done", None, Some("1/1")),
            ]
        );
        // 파일(진리원)에도 같은 줄 수.
        let side = crate::repo::history::read_all(&crate::repo::history::history_path(
            &store.paths,
            "C-001",
        ))
        .unwrap();
        assert_eq!(side.len(), 10);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn checking_the_last_box_by_editing_the_body_is_the_all_checked_moment() {
        let (dir, store, id) = history_store("body").await;
        let set = |d: &str| UpdateCampaignRequest {
            description: Some(d.into()),
            ..Default::default()
        };
        update_campaign(&store, id, set("- [x] 하나\n- [ ] 둘\n"))
            .await
            .unwrap();
        update_campaign(&store, id, set("- [x] 하나\n- [x] 둘\n"))
            .await
            .unwrap();
        update_campaign(&store, id, set("- [x] 하나\n- [x] 둘\n\n끝.\n"))
            .await
            .unwrap(); // 이미 모두 — 안 남는다
        assert_eq!(
            ops_of(&store, id).await,
            vec![op("checklist_all_done", None, Some("2/2"))]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_moment_all_linked_quests_are_done_is_recorded() {
        let (dir, store, id) = history_store("quests").await;
        let mut qids = Vec::new();
        for t in ["가", "나", "다"] {
            let q = crate::ops::create_quest(
                &store,
                crate::models::CreateQuestRequest {
                    quest_type_id: 1,
                    title: t.into(),
                    description: None,
                    status_slug: "open".into(),
                    urgency: Some(3),
                    parent_quest_id: None,
                },
            )
            .await
            .unwrap();
            link_quest_by_slug(&store, id, &q.quest_id).await.unwrap();
            qids.push((q.id, q.quest_id));
        }
        let done = |s: &str| crate::models::ChangeStatusRequest {
            status_slug: s.into(),
        };
        crate::ops::change_status(&store, qids[0].0, done("done"))
            .await
            .unwrap();
        crate::ops::change_status(&store, qids[1].0, done("done"))
            .await
            .unwrap();
        assert!(ops_of(&store, id).await.is_empty(), "아직 하나 남았다");
        // 남은 하나를 떼어 내면 모두 완료.
        unlink_quest_by_slug(&store, id, &qids[2].1).await.unwrap();
        // 되돌렸다가 다시 완료하면 그 순간도 남는다.
        crate::ops::change_status(&store, qids[1].0, done("open"))
            .await
            .unwrap();
        crate::ops::change_status(&store, qids[1].0, done("done"))
            .await
            .unwrap();
        assert_eq!(
            ops_of(&store, id).await,
            vec![
                op("quests_all_done", None, Some("2/2")),
                op("quests_all_done", None, Some("2/2"))
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
