//! Campaign mutation orchestration (DEV-011).
//!
//! `services::campaigns` (SQL) + 파일 IO + journal append 를 묶음. 호출 순서:
//! 1. journal::append (의도 기록).
//! 2. SQL mutation (services::campaigns::*).
//! 3. `.guild/campaigns/<slug>.md` atomic write — frontmatter 가 DB 의 새 값
//!    반영, 본문 markdown 은 DEV-066 패턴으로 외부 편집 보존 (없으면 빈 body).
//! 4. 체크리스트 변경 ops 는 본문도 같이 수정 후 파일 → DB sync.

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::models::{
    CampaignChecklistItem, CampaignDetail, CampaignRow, CreateCampaignRequest,
    UpdateCampaignRequest,
};
use crate::repo::{extract_checklist_items, CampaignFile, CampaignFrontmatter};
use crate::services::campaigns as sql;
use crate::store::{journal, Store};

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
    Ok(CampaignDetail {
        campaign: row,
        checklists,
        linked_quests,
        quest_total,
        quest_done,
        quest_progress,
    })
}

// ─────────────────────── 생성 / 수정 / 삭제 ───────────────────────

pub async fn create_campaign(
    store: &Store,
    body: CreateCampaignRequest,
) -> AppResult<CampaignRow> {
    let _ = journal::append(&store.journal_pool, "create_campaign", &body, None::<&serde_json::Value>)
        .await
        .map_err(AppError::Internal)?;

    let camp = sql::create(&store.index_pool, body).await?;
    write_campaign_file(store, &camp, true).await?;
    Ok(camp)
}

pub async fn update_campaign(
    store: &Store,
    id: i64,
    body: UpdateCampaignRequest,
) -> AppResult<CampaignRow> {
    let _ = journal::append(
        &store.journal_pool,
        "update_campaign",
        &json!({ "id": id, "body": body }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let description_explicit = body.description.is_some();
    let camp = sql::update(&store.index_pool, id, body).await?;
    write_campaign_file(store, &camp, description_explicit).await?;
    Ok(camp)
}

pub async fn delete_campaign(store: &Store, id: i64) -> AppResult<()> {
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
    }
    Ok(())
}

// ─────────────────────── Link / Unlink ───────────────────────

pub async fn link_quest_by_slug(
    store: &Store,
    campaign_id: i64,
    quest_slug: &str,
) -> AppResult<()> {
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
    Ok(())
}

pub async fn unlink_quest_by_slug(
    store: &Store,
    campaign_id: i64,
    quest_slug: &str,
) -> AppResult<()> {
    let qid = sql::resolve_quest_id(&store.index_pool, quest_slug).await?;
    let _ = journal::append(
        &store.journal_pool,
        "campaign_unlink_quest",
        &json!({ "campaign_id": campaign_id, "quest_slug": quest_slug }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    sql::unlink_quest(&store.index_pool, campaign_id, qid).await?;
    let camp = sql::fetch_by_id(&store.index_pool, campaign_id).await?;
    write_campaign_file(store, &camp, false).await?;
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
    let new_body = append_checklist_to_body(&cf.body, text);
    cf.body = new_body;
    cf.frontmatter.updated_at = crate::time::now_local_iso8601();
    cf.write(&path).map_err(AppError::Internal)?;

    // DB sync — 파일 본문 → DB.
    let items = extract_checklist_items(&cf.body);
    sql::replace_checklists_from_file(&store.index_pool, campaign_id, &items).await?;

    // 새로 추가된 항목 = list 의 마지막 (order_idx 가 가장 큰 것).
    sql::list_checklists(&store.index_pool, campaign_id)
        .await?
        .into_iter()
        .last()
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "no checklist after add (sync failed?)"
            ))
        })
}

/// 1-based 인덱스로 체크 / 언체크 토글.
pub async fn set_checklist_checked_by_index(
    store: &Store,
    campaign_id: i64,
    one_based_idx: usize,
    checked: bool,
) -> AppResult<()> {
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
    let new_body = set_checklist_checked_in_body(&cf.body, one_based_idx, checked)?;
    cf.body = new_body;
    cf.frontmatter.updated_at = crate::time::now_local_iso8601();
    cf.write(&path).map_err(AppError::Internal)?;

    let items = extract_checklist_items(&cf.body);
    sql::replace_checklists_from_file(&store.index_pool, campaign_id, &items).await?;
    Ok(())
}

/// 1-based 인덱스의 체크리스트 항목을 삭제 (파일 본문 줄 제거 + DB sync).
pub async fn remove_checklist_by_index(
    store: &Store,
    campaign_id: i64,
    one_based_idx: usize,
) -> AppResult<()> {
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
    let new_body = remove_checklist_in_body(&cf.body, one_based_idx)?;
    cf.body = new_body;
    cf.frontmatter.updated_at = crate::time::now_local_iso8601();
    cf.write(&path).map_err(AppError::Internal)?;

    let items = extract_checklist_items(&cf.body);
    sql::replace_checklists_from_file(&store.index_pool, campaign_id, &items).await?;
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
            created_at: camp.created_at.clone(),
            updated_at: camp.updated_at.clone(),
            deleted: false,
        },
        body,
    };
    cf.write(&path).map_err(AppError::Internal)?;
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
}
