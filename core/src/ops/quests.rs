//! Quest mutation orchestration — SQL + file + journal.
//!
//! 각 함수는 `&Store` 받고 `AppResult<T>` 반환.
//! 호출자 (server routes / cli Backend::Local) 가 사용.

use crate::error::AppResult;
use crate::models::{
    AddPrerequisiteRequest, ChangeParentRequest, ChangeStatusRequest, ChangeTypeRequest,
    CreateQuestRequest, QuestRow, UpdateQuestRequest,
};
use crate::repo::{QuestFile, QuestFrontmatter, QuestRef, QuestRelations, auto};
use crate::services::quests as sql;
use crate::snapshot;
use crate::store::{Store, journal};
use serde_json::json;
use sqlx::SqlitePool;

/// 매 mutation 끝에 호출 — 자동 백업 정책 검토 + 필요시 snapshot.
/// snapshot 실패해도 mutation 결과엔 영향 X (stderr 경고만).
async fn after_mutation(store: &Store) {
    // DEV-022: journal replay 중에는 auto-snapshot 억제. replay 도중 snapshot 이
    // journal 을 truncate 하면 아직 적용 안 한 ops 가 사라져 replay 가 깨진다.
    if store.is_replaying() {
        return;
    }
    let policy = snapshot::AutoSnapshotPolicy::from_env();

    // DEV-299: 스냅샷 생성은 ~2초 걸린다(BUG-167 이 줄인 건 매 mutation 의
    // *검사* 비용이고, 임계치에 걸렸을 때의 실제 생성은 여전히 동기다).
    // 서버/GUI 처럼 프로세스가 살아 있는 환경에서는 백그라운드로 돌려 응답을
    // 막지 않는다. CLI 는 명령이 끝나면 런타임째 종료돼 spawn 한 task 가
    // 유실되므로 기본(동기)을 유지한다 — 그래서 opt-in 이다.
    if store.background_snapshots_enabled() {
        use std::sync::atomic::Ordering;
        // 이미 돌고 있으면 건너뛴다 — 연속 mutation 이 스냅샷을 겹쳐 실행하면
        // 디스크 I/O 가 중복되고 journal truncate 와 맞물릴 수 있다.
        // 다음 mutation 이 다시 판단하므로 스냅샷을 영구히 놓치지는 않는다.
        if store
            .snapshot_in_flight
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let bg = store.clone();
        tokio::spawn(async move {
            let result = snapshot::maybe_auto_snapshot(&bg, policy).await;
            bg.snapshot_in_flight.store(false, Ordering::SeqCst);
            report_snapshot(result);
        });
        return;
    }

    report_snapshot(snapshot::maybe_auto_snapshot(store, policy).await);
}

/// DEV-299: 동기/백그라운드 양쪽이 같은 형식으로 보고하도록 분리.
fn report_snapshot(result: anyhow::Result<Option<crate::snapshot::SnapshotInfo>>) {
    match result {
        Ok(Some(info)) => {
            eprintln!(
                "[auto-backup] snapshot 생성됨: {} ({} bytes)",
                info.timestamp, info.size_bytes
            );
        }
        Ok(None) => {}
        Err(e) => {
            eprintln!("[auto-backup] snapshot 실패 (mutation 자체는 성공): {e:#}");
        }
    }
}

/// 새 quest 생성. 영향:
/// - 새 파일 `.guild/quests/{slug}.md` 생성.
/// - parent 가 지정되었으면 그 quest 파일의 auto 블록 갱신 (sub-quest 목록에 추가).
pub async fn create_quest(store: &Store, body: CreateQuestRequest) -> AppResult<QuestRow> {
    // 1. journal append (의도 기록).
    let _ = journal::append(
        &store.journal_pool,
        "create_quest",
        &body,
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    // 2. SQL mutation (기존 검증 + INSERT 재사용).
    let parent_id = body.parent_quest_id;
    let quest = sql::create(&store.index_pool, body).await?;

    // 3. 새 파일 작성. description 은 create body 가 명시 (Some 또는 None).
    write_quest_file(store, &quest, true).await?;

    // 3b. DEV-242: type 파일 counter 도 함께 갱신 — 이전엔 DB(quest_counters)
    // 만 올라가고 파일은 `check counters --fix` 를 수동 실행해야만 갱신돼
    // file-truth 원칙(§1: mutation 은 파일+DB 동시 기록)과 어긋난 채 낡았음.
    sync_type_counter_file(store, &quest.type_prefix, quest.number);

    // 4. parent 있으면 부모 파일의 auto 블록 갱신. parent description 은 안 건드림.
    if let Some(pid) = parent_id {
        let parent = sql::fetch_by_id(&store.index_pool, pid).await?;
        write_quest_file(store, &parent, false).await?;
    }

    after_mutation(store).await;
    Ok(quest)
}

/// DEV-242: `.guild/types/{prefix}.toml` 의 `[counter].last_number` 를 새로
/// 부여된 번호로 끌어올린다(파일 값이 더 크면 보존 — 단조 증가). 실패는
/// 경고만 — 번호 부여의 실제 정합성은 DB counter + self-heal 이 담당하고,
/// 파일 counter 는 백업/표시 값이라 생성 자체를 막을 이유가 없음.
fn sync_type_counter_file(store: &Store, prefix: &str, number: i64) {
    let path = store.paths.type_path(prefix);
    match crate::repo::TypeFile::read(&path) {
        Ok(mut tf) => {
            if tf.counter.last_number < number {
                tf.counter.last_number = number;
                if let Err(e) = tf.write(&path) {
                    tracing::warn!("type counter 파일 갱신 실패 ({prefix}): {e:#}");
                }
            }
        }
        Err(e) => tracing::warn!("type 파일 읽기 실패 ({prefix}): {e:#}"),
    }
}

/// Quest 의 title / description / urgency 수정.
pub async fn update_quest(store: &Store, id: i64, body: UpdateQuestRequest) -> AppResult<QuestRow> {
    let _ = journal::append(
        &store.journal_pool,
        "update_quest",
        &json!({ "id": id, "body": body }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    // DEV-066: description 이 mutation 의 명시적 대상인지 (file→DB sync 분기용).
    let description_explicit = body.description.is_some();
    let quest = sql::update(&store.index_pool, id, body).await?;
    write_quest_file(store, &quest, description_explicit).await?;
    after_mutation(store).await;
    Ok(quest)
}

/// DEV-076: 희망 / 필수 기한 변경 — DB 갱신 + 파일 sync.
///
/// 각 인자: `Some(Some(date))` = 설정, `Some(None)` = 해제, `None` = 변경 없음.
pub async fn set_due_dates(
    store: &Store,
    id: i64,
    desired_due: Option<Option<String>>,
    required_due: Option<Option<String>>,
) -> AppResult<QuestRow> {
    let _ = journal::append(
        &store.journal_pool,
        "set_due_dates",
        &json!({ "id": id, "desired_due": desired_due, "required_due": required_due }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    let quest = sql::set_due_dates(&store.index_pool, id, desired_due, required_due).await?;
    write_quest_file(store, &quest, false).await?;
    after_mutation(store).await;
    Ok(quest)
}

/// DEV-068: 한 quest 의 tags 전체 교체. file frontmatter + DB 캐시 모두 갱신.
///
/// 입력 tags 는 trim + 빈 문자열 제거 + 중복 제거 후 stable order 보존
/// (들어온 순서대로, 같은 tag 의 첫 등장만). 새 tags 가 비면 frontmatter
/// 에서 키 자체 생략.
pub async fn set_quest_tags(store: &Store, id: i64, tags: Vec<String>) -> AppResult<QuestRow> {
    use std::collections::HashSet;

    // 정규화: trim + 빈 거 제거 + 중복 제거 (순서 보존).
    let mut seen: HashSet<String> = HashSet::new();
    let normalized: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .filter(|t| seen.insert(t.clone()))
        .collect();

    let _ = journal::append(
        &store.journal_pool,
        "set_quest_tags",
        &json!({ "id": id, "tags": &normalized }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    let quest = sql::fetch_by_id(&store.index_pool, id).await?;

    // 1) DB 캐시 갱신 — 트랜잭션 안에서 wipe + INSERT.
    let mut tx = store
        .index_pool
        .begin()
        .await
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!("begin tx: {e}")))?;
    sqlx::query("DELETE FROM quest_tags WHERE quest_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!("clear quest_tags: {e}")))?;
    for tag in &normalized {
        sqlx::query("INSERT INTO quest_tags (quest_id, tag) VALUES (?, ?)")
            .bind(id)
            .bind(tag)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                crate::error::AppError::Internal(anyhow::anyhow!("insert quest_tags: {e}"))
            })?;
    }
    tx.commit()
        .await
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!("commit tx: {e}")))?;

    // 2) 파일 frontmatter 갱신 — write_quest_file 이 existing tags 보존하므로
    //    여기선 직접 frontmatter 갱신 후 다시 read 해서 강제 sync.
    //    가장 단순: write_quest_file 한 번 호출하기 전 파일의 tags 를 새 값으로
    //    덮어쓰기 — 파일 한 번 더 read + write.
    let path = store.paths.quest_path(&quest.quest_id);
    if let Ok(mut qf) = crate::repo::QuestFile::read(&path) {
        qf.frontmatter.tags = normalized.clone();
        qf.write(&path).map_err(crate::error::AppError::Internal)?;
    }

    // set_quest_tags 에선 sql:: 함수가 따로 없어 write_quest_file 만으로는
    // tags 갱신 안 됨 (existing 보존). 그래서 위에서 직접 frontmatter 수정.
    after_mutation(store).await;
    Ok(quest)
}

/// 상태 변경.
pub async fn change_status(
    store: &Store,
    id: i64,
    body: ChangeStatusRequest,
) -> AppResult<QuestRow> {
    // BUG-011: no-op (현재 상태 == 요청 상태) 면 일찍 반환 — journal/history/
    // updated_at 모두 변동 없음.
    // DEV-048: slug 기반 비교로 변경 — body.status_slug 와 현재 status.slug 직접 비교.
    let old_status_slug: Option<String> = sqlx::query_scalar(
        "SELECT s.slug
         FROM quests q JOIN quest_statuses s ON s.id = q.status_id
         WHERE q.id = ?",
    )
    .bind(id)
    .fetch_optional(&store.index_pool)
    .await?;

    if old_status_slug.as_deref() == Some(body.status_slug.as_str()) {
        // 이미 그 상태 — 그대로 반환. quest_history 기록 X.
        return sql::fetch_by_id(&store.index_pool, id).await;
    }

    // DEV-142: 완료(counts_as_done=true) 상태로의 전환은 미해결 토론(discussion)
    // 댓글이 하나라도 있으면 차단 — CLI / GUI 공통 게이트. 댓글은 file 진리원에서
    // 직접 확인 (discussion/resolved 는 마커 attr, DB 캐시 컬럼 없음).
    let target_counts_as_done: Option<bool> =
        sqlx::query_scalar("SELECT counts_as_done FROM quest_statuses WHERE slug = ?")
            .bind(&body.status_slug)
            .fetch_optional(&store.index_pool)
            .await?;
    if target_counts_as_done == Some(true) {
        let slug: Option<String> = sqlx::query_scalar(
            "SELECT qt.prefix || '-' || printf('%03d', q.number)
             FROM quests q JOIN quest_types qt ON q.quest_type_id = qt.id
             WHERE q.id = ?",
        )
        .bind(id)
        .fetch_optional(&store.index_pool)
        .await?;
        if let Some(slug) = slug {
            let entries = crate::repo::comments::read_entries(&store.paths, &slug)
                .map_err(crate::error::AppError::Internal)?;
            let unresolved: Vec<u64> = entries
                .iter()
                .filter(|e| e.discussion && !e.resolved)
                .map(|e| e.id)
                .collect();
            if !unresolved.is_empty() {
                let ids = unresolved
                    .iter()
                    .map(|i| format!("#{i}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(crate::error::AppError::BadRequest(format!(
                    "미해결 토론(discussion) 댓글 {}개({ids})를 먼저 resolve 해야 \
                     완료 상태로 전환할 수 있습니다.",
                    unresolved.len()
                )));
            }
        }
    }

    let _ = journal::append(
        &store.journal_pool,
        "change_status",
        &json!({ "id": id, "status_slug": body.status_slug }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    let new_status_slug = body.status_slug.clone();
    let quest = sql::change_status(&store.index_pool, id, body).await?;

    // DEV-013: history 기록. DEV-041: ts 명시 bind. DEV-042: status slug 저장.
    // DEV-049: quest_slug 도 함께 저장. DEV-048: new_status_slug 는 body 에서 그대로.
    let ts = crate::time::now_local_iso8601();
    sqlx::query(
        "INSERT INTO quest_history (quest_id, quest_slug, ts, op, old_value, new_value) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(&quest.quest_id)
    .bind(&ts)
    .bind("change_status")
    .bind(&old_status_slug)
    .bind(&new_status_slug)
    .execute(&store.index_pool)
    .await?;
    // DEV-180: 파일 사이드카에도 append — 파일이 진리원, quest_history 는 캐시.
    crate::repo::history::append(
        &store.paths,
        &quest.quest_id,
        &crate::repo::history::HistoryEntry {
            ts: ts.clone(),
            op: "change_status".into(),
            old: old_status_slug.clone(),
            new: Some(new_status_slug.clone()),
        },
    )
    .map_err(crate::error::AppError::Internal)?;

    write_quest_file(store, &quest, false).await?;
    after_mutation(store).await;
    Ok(quest)
}

/// DEV-055: quest 의 type 변경.
///
/// slug 가 바뀌므로 (예 `DEV-001` → `BUG-013`) cascade 가 필요:
/// 1. SQL: `quests.quest_type_id` / `number` 갱신, `quest_history.quest_slug`,
///    `quest_positions.quest_slug` 도 cascade (services::change_type 가 담당).
/// 2. 본 함수가 추가로:
///    - quest_history INSERT — op="change_type", old_slug → new_slug 기록.
///    - 파일: `.guild/quests/<old_slug>.md` → `<new_slug>.md` rename.
///    - 관련 quest 들 (parent / sub / prereq / dependent) 의 .md 파일 auto-block
///      재생성 — 그쪽이 본인을 mention 하던 게 새 slug 로 갱신되어야.
///
/// **본문 안 자유 텍스트 mention** (예 다른 quest 의 description 에 "DEV-001
/// 참조" 같은 게 적힌 것) 은 자동 갱신 X — 사용자 결정 (DEV-055 본문 정책).
/// 사용자가 grep + replace 필요 시 직접.
pub async fn change_quest_type(
    store: &Store,
    id: i64,
    body: ChangeTypeRequest,
) -> AppResult<QuestRow> {
    let _ = journal::append(
        &store.journal_pool,
        "change_quest_type",
        &json!({ "id": id, "new_type_prefix": &body.new_type_prefix }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    let (quest, old_slug, new_slug) =
        sql::change_type(&store.index_pool, id, &body.new_type_prefix).await?;

    // NoOp (같은 type) — 추가 작업 없이 반환.
    if old_slug == new_slug {
        return Ok(quest);
    }

    // history INSERT — type 변경 자체를 audit.
    let ts = crate::time::now_local_iso8601();
    sqlx::query(
        "INSERT INTO quest_history (quest_id, quest_slug, ts, op, old_value, new_value)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(&new_slug)
    .bind(&ts)
    .bind("change_type")
    .bind(&old_slug)
    .bind(&new_slug)
    .execute(&store.index_pool)
    .await?;
    // DEV-180: 사이드카도 rename cascade 후 append (파일이 진리원).
    crate::repo::history::rename(&store.paths, &old_slug, &new_slug)
        .map_err(crate::error::AppError::Internal)?;
    crate::repo::history::append(
        &store.paths,
        &new_slug,
        &crate::repo::history::HistoryEntry {
            ts: ts.clone(),
            op: "change_type".into(),
            old: Some(old_slug.clone()),
            new: Some(new_slug.clone()),
        },
    )
    .map_err(crate::error::AppError::Internal)?;

    // 파일: 새 slug 로 새 파일 쓰고 옛 파일 삭제.
    let old_path = store.paths.quest_path(&old_slug);
    let new_path = store.paths.quest_path(&new_slug);
    // DEV-066: rename 이라 path 도 새 경로 — 파일 본문이 옛 경로에 있으므로
    // 옛 파일에서 description 을 미리 읽어 보존. 옛 파일 삭제 직전.
    // 새 경로엔 파일이 없으니 false 모드로는 file→DB sync 가 안 됨.
    // 미리 보존된 description 을 직접 DB sync 후 write.
    if let Ok(old_qf) = QuestFile::read(&old_path)
        && !old_qf.description.trim().is_empty()
    {
        let db_desc = quest.description.as_deref().unwrap_or("");
        if old_qf.description != db_desc {
            sqlx::query("UPDATE quests SET description = ? WHERE id = ?")
                .bind(&old_qf.description)
                .bind(quest.id)
                .execute(&store.index_pool)
                .await?;
        }
    }
    let quest = sql::fetch_by_id(&store.index_pool, quest.id).await?;
    write_quest_file(store, &quest, true).await?; // new_path (description 이미 sync 됨)
    if old_path != new_path {
        let _ = std::fs::remove_file(&old_path);
    }
    // DEV-242: 대상 type 에 새 번호가 부여됐으므로 그 type 파일 counter 도 갱신.
    sync_type_counter_file(store, &quest.type_prefix, quest.number);

    // 관련 quest 파일 갱신 — auto-block 안 sub-quests / prerequisites / parent
    // 표기에 본인 slug 가 들어가므로 새 slug 로 갱신되어야.
    let related_ids: Vec<i64> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT q.id FROM quests q
        WHERE q.deleted_at IS NULL
          AND (
            -- 본인의 parent
            q.id = (SELECT parent_quest_id FROM quests WHERE id = ?1)
            -- 본인의 sub-quest 들
            OR q.parent_quest_id = ?1
            -- 본인의 prereq 들
            OR q.id IN (SELECT prerequisite_id FROM quest_dependencies WHERE quest_id = ?1)
            -- 본인을 prereq 으로 갖는 quest 들 (dependent)
            OR q.id IN (SELECT quest_id FROM quest_dependencies WHERE prerequisite_id = ?1)
          )
        "#,
    )
    .bind(id)
    .fetch_all(&store.index_pool)
    .await?;

    for rid in related_ids {
        if let Ok(q) = sql::fetch_by_id(&store.index_pool, rid).await {
            write_quest_file(store, &q, false).await?;
        }
    }

    after_mutation(store).await;
    Ok(quest)
}

/// 부모 변경 (또는 분리).
/// 영향: 본인 / 옛 부모 (sub 목록 변화) / 새 부모 (sub 추가).
pub async fn change_parent(
    store: &Store,
    id: i64,
    body: ChangeParentRequest,
) -> AppResult<QuestRow> {
    let _ = journal::append(
        &store.journal_pool,
        "change_parent",
        &json!({ "id": id, "parent_quest_id": body.parent_quest_id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    // 옛 부모 id 확보 (SQL 호출 전).
    let old_parent_id = parent_id_of(&store.index_pool, id).await?;
    let new_parent_id = body.parent_quest_id;

    let quest = sql::change_parent(&store.index_pool, id, body).await?;
    write_quest_file(store, &quest, false).await?;

    // 옛 부모 / 새 부모 파일 갱신 (서로 다른 경우만 별개로).
    let mut touched: Vec<i64> = Vec::new();
    if let Some(p) = old_parent_id {
        touched.push(p);
    }
    if let Some(p) = new_parent_id
        && !touched.contains(&p)
    {
        touched.push(p);
    }
    for pid in touched {
        if let Ok(q) = sql::fetch_by_id(&store.index_pool, pid).await {
            write_quest_file(store, &q, false).await?;
        }
    }
    after_mutation(store).await;
    Ok(quest)
}

/// soft delete + cascade. 영향:
/// - 본인: soft-deleted (frontmatter deleted: true)
/// - cascade 자식: 같이 soft-deleted
/// - cascade 안 한 직계 자식: parent 분리 → 그들 파일도 갱신 (Parent 섹션 사라짐)
/// - 본인을 prereq 로 가진 다른 quest 들: 자기 파일 prereq 목록은 SQL 단에서 유지 (관계 끊지 않음 — 본인이 사라지면 표시만 안 됨).
///   다만 다른 quest 의 auto 블록에서 본인이 표시되었었는데 이젠 deleted_at IS NULL 필터로 안 보임 → 갱신 필요.
pub async fn delete_quest(store: &Store, id: i64, cascade_ids: &[i64]) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "delete_quest",
        &json!({ "id": id, "cascade_ids": cascade_ids }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    // 영향받는 quest id 들 (cascade 안 된 자식, 본인을 prereq 으로 갖는 quests) 사전 수집.
    let detached_children: Vec<i64> = if cascade_ids.is_empty() {
        sqlx::query_scalar("SELECT id FROM quests WHERE parent_quest_id = ? AND deleted_at IS NULL")
            .bind(id)
            .fetch_all(&store.index_pool)
            .await?
    } else {
        let placeholders = cascade_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql_str = format!(
            "SELECT id FROM quests WHERE parent_quest_id = ? AND deleted_at IS NULL AND id NOT IN ({placeholders})"
        );
        let mut q = sqlx::query_scalar(&sql_str).bind(id);
        for c in cascade_ids {
            q = q.bind(c);
        }
        q.fetch_all(&store.index_pool).await?
    };

    let dependents: Vec<i64> = sqlx::query_scalar(
        "SELECT q.id FROM quests q JOIN quest_dependencies d ON q.id = d.quest_id
         WHERE d.prerequisite_id = ? AND q.deleted_at IS NULL",
    )
    .bind(id)
    .fetch_all(&store.index_pool)
    .await?;

    // 본인 quest_id slug (deleted 표시할 때 frontmatter 갱신용)
    let self_quest = sql::fetch_by_id(&store.index_pool, id).await?;
    let cascade_quests: Vec<QuestRow> = {
        let mut v = Vec::new();
        for cid in cascade_ids {
            if let Ok(q) = sql::fetch_by_id(&store.index_pool, *cid).await {
                v.push(q);
            }
        }
        v
    };

    // SQL mutation 실행 (soft delete + 자식 처리).
    sql::delete(&store.index_pool, id, cascade_ids).await?;

    // 본인 파일 — frontmatter deleted: true 로.
    write_quest_file_as_deleted(store, &self_quest).await?;
    for cq in &cascade_quests {
        write_quest_file_as_deleted(store, cq).await?;
    }

    // 분리된 자식 파일 갱신 (Parent 섹션 제거)
    for cid in detached_children {
        if let Ok(q) = sql::fetch_by_id(&store.index_pool, cid).await {
            write_quest_file(store, &q, false).await?;
        }
    }
    // 본인 / 자식들을 prereq 으로 가진 quest 들의 auto 블록 갱신.
    for did in dependents {
        if let Ok(q) = sql::fetch_by_id(&store.index_pool, did).await {
            write_quest_file(store, &q, false).await?;
        }
    }
    after_mutation(store).await;
    Ok(())
}

/// soft delete 취소.
pub async fn restore_quest(store: &Store, id: i64) -> AppResult<QuestRow> {
    let _ = journal::append(
        &store.journal_pool,
        "restore_quest",
        &json!({ "id": id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    let quest = sql::restore(&store.index_pool, id).await?;
    write_quest_file(store, &quest, false).await?;
    // 부모 / dependent 영향 — restore 가 alive 상태로 되돌리므로 부모의 sub 목록에 다시 포함됨.
    if let Some(pid) = parent_id_of(&store.index_pool, id).await?
        && let Ok(p) = sql::fetch_by_id(&store.index_pool, pid).await
    {
        write_quest_file(store, &p, false).await?;
    }
    after_mutation(store).await;
    Ok(quest)
}

pub async fn add_prerequisite(
    store: &Store,
    id: i64,
    body: AddPrerequisiteRequest,
) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "add_prerequisite",
        &json!({ "id": id, "prerequisite_id": body.prerequisite_id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    sql::add_prerequisite(&store.index_pool, id, body).await?;
    // 본인 파일만 갱신 (frontmatter prerequisites 배열 추가).
    let quest = sql::fetch_by_id(&store.index_pool, id).await?;
    write_quest_file(store, &quest, false).await?;
    after_mutation(store).await;
    Ok(())
}

pub async fn remove_prerequisite(store: &Store, id: i64, prereq_id: i64) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "remove_prerequisite",
        &json!({ "id": id, "prerequisite_id": prereq_id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(crate::error::AppError::Internal)?;

    sql::remove_prerequisite(&store.index_pool, id, prereq_id).await?;
    if let Ok(q) = sql::fetch_by_id(&store.index_pool, id).await {
        write_quest_file(store, &q, false).await?;
    }
    after_mutation(store).await;
    Ok(())
}

// ─────────────────────────── 내부 헬퍼 ───────────────────────────

/// soft-deleted quest 의 파일을 frontmatter `deleted: true` 로 작성.
/// 본인은 더 이상 alive 가 아니므로 `write_quest_file` 의 fetch_relations 가
/// 본인을 못 찾을 가능성 (deleted_at filter). 직접 frontmatter 만 갱신.
async fn write_quest_file_as_deleted(store: &Store, quest: &QuestRow) -> AppResult<()> {
    // BUG-012 와 동일 — DB 의 description 이 truth. 기존 파일 fallback 제거.
    let path = store.paths.quest_path(&quest.quest_id);
    let frontmatter = QuestFrontmatter {
        quest_id: quest.quest_id.clone(),
        title: quest.title.clone(),
        status: quest.status_slug.clone(),
        urgency: quest.urgency,
        parent: None, // soft-deleted 는 parent 표시 X (sub 관계 끊긴 것으로 간주)
        prerequisites: vec![],
        created_at: quest.created_at.clone(),
        updated_at: quest.updated_at.clone(),
        deleted: true,
        desired_due: None,
        required_due: None,
        tags: vec![],
    };
    let qf = QuestFile {
        frontmatter,
        description: quest.description.clone().unwrap_or_default(),
        auto_block: String::new(),
    };
    qf.write(&path).map_err(crate::error::AppError::Internal)?;
    // drift: soft-delete 로 쓴 파일도 cached_mtime 동기화 (오탐 방지).
    let mtime = crate::repo::fs::mtime_unix_nanos(&path);
    sqlx::query("UPDATE quests SET cached_mtime = ? WHERE id = ?")
        .bind(mtime)
        .bind(quest.id)
        .execute(&store.index_pool)
        .await?;
    Ok(())
}

/// 한 quest 의 파일을 현재 SQL 상태로 재작성.
/// frontmatter / description / auto 블록 모두 fresh 하게 구성.
///
/// `description_explicit` 의 의미 (DEV-066):
/// - `true` — 호출자가 description 을 명시적으로 mutation 한 경우
///   (`update_quest` 가 `body.description = Some(...)` 받았을 때). DB 의
///   description 을 그대로 파일에 반영 → GUI / CLI 편집이 파일에 정확히 적용.
/// - `false` — description 을 안 건드리는 mutation (`change_status` /
///   `change_parent` / `add_prerequisite` 등). 이때:
///     - 기존 파일이 있고 description 본문이 비어있지 않으면 **파일의 본문을
///       truth 로 사용** + DB 도 그것으로 sync (`UPDATE quests SET description`).
///       → BUG-012 가 의도적으로 제거한 fallback 의 안전한 재도입: 사용자가
///       파일을 외부 편집한 뒤 reindex 없이 status/parent 등을 mutation 해도
///       외부 편집이 보존되고 DB 도 점진적으로 정합.
///     - 파일이 없거나 본문이 비어있으면 DB 값 사용 (현재 동작).
///
/// 외부 편집 보존은 description 본문만 — frontmatter (status/urgency/parent/
/// prereq) 는 항상 DB 가 truth (AGENTS.md 정책: 사용자가 frontmatter 직접
/// 수정 금지).
pub(crate) async fn write_quest_file(
    store: &Store,
    quest: &QuestRow,
    description_explicit: bool,
) -> AppResult<()> {
    let pool = &store.index_pool;
    let path = store.paths.quest_path(&quest.quest_id);

    let relations = fetch_relations(pool, quest.id).await?;
    let auto_block = auto::render(&relations).trim().to_string();

    // DEV-066: description 본문 결정.
    //   - description_explicit=true → DB 값 그대로 (BUG-012 의 의도).
    //   - false + 파일 존재 + 파일 본문 non-empty → 파일 본문 사용 + DB sync.
    //   - 그 외 → DB 값.
    // DEV-068: tags 는 frontmatter 가 진리원. write_quest_file 이 매 mutation
    // 마다 호출되므로 기존 파일의 tags 를 보존 (없으면 빈 vec). description
    // 보존 패턴과 동일.
    let existing_file = QuestFile::read(&path).ok();
    let existing_tags: Vec<String> = existing_file
        .as_ref()
        .map(|q| q.frontmatter.tags.clone())
        .unwrap_or_default();

    let description = if description_explicit {
        quest.description.clone().unwrap_or_default()
    } else {
        match &existing_file {
            Some(existing) if !existing.description.trim().is_empty() => {
                // 파일 본문이 DB 와 다르면 DB 도 sync.
                let db_desc = quest.description.as_deref().unwrap_or("");
                if existing.description != db_desc {
                    sqlx::query("UPDATE quests SET description = ? WHERE id = ?")
                        .bind(&existing.description)
                        .bind(quest.id)
                        .execute(pool)
                        .await?;
                }
                existing.description.clone()
            }
            _ => quest.description.clone().unwrap_or_default(),
        }
    };

    let frontmatter = QuestFrontmatter {
        quest_id: quest.quest_id.clone(),
        title: quest.title.clone(),
        status: quest.status_slug.clone(),
        urgency: quest.urgency,
        parent: relations.parent.as_ref().map(|r| r.quest_id.clone()),
        prerequisites: relations
            .prerequisites
            .iter()
            .map(|r| r.quest_id.clone())
            .collect(),
        created_at: quest.created_at.clone(),
        updated_at: quest.updated_at.clone(),
        deleted: false, // 본 함수는 alive quest 만 다룸; soft-delete 는 별도 ops
        // DEV-076: DB → 파일 sync. quest 의 due 필드 그대로 propagate.
        desired_due: quest.desired_due.clone(),
        required_due: quest.required_due.clone(),
        // DEV-068: 기존 frontmatter 의 tags 그대로 보존. set_tags 는 별도 함수.
        tags: existing_tags,
    };

    let qf = QuestFile {
        frontmatter,
        description,
        auto_block,
    };

    qf.write(&path).map_err(crate::error::AppError::Internal)?;

    // drift/DEV-121: 방금 쓴 파일의 mtime 을 cached_mtime 에 기록. detect_drift 와
    // incremental sync 가 per-row cached_mtime 으로 "파일이 DB 보다 새것인가" 를
    // 판단하므로, ops 가 쓴 파일이 곧바로 stale 로 오탐되지 않게 동기화해 둔다.
    let mtime = crate::repo::fs::mtime_unix_nanos(&path);
    sqlx::query("UPDATE quests SET cached_mtime = ? WHERE id = ?")
        .bind(mtime)
        .bind(quest.id)
        .execute(pool)
        .await?;
    Ok(())
}

/// quest id 의 관계 fetch (auto 블록 렌더링용).
async fn fetch_relations(pool: &SqlitePool, id: i64) -> AppResult<QuestRelations> {
    // parent
    let parent = if let Some(pid) = parent_id_of(pool, id).await? {
        let p = sql::fetch_by_id(pool, pid).await?;
        Some(QuestRef::new(p.quest_id, p.title))
    } else {
        None
    };

    // sub-quests (alive children whose parent_quest_id == id)
    let subs: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT qt.prefix || '-' || printf('%03d', q.number), q.title
         FROM quests q JOIN quest_types qt ON q.quest_type_id = qt.id
         WHERE q.parent_quest_id = ? AND q.deleted_at IS NULL
         ORDER BY q.id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    // prerequisites
    let prereqs: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT qt.prefix || '-' || printf('%03d', q.number), q.title
         FROM quests q JOIN quest_types qt ON q.quest_type_id = qt.id
         JOIN quest_dependencies d ON q.id = d.prerequisite_id
         WHERE d.quest_id = ? AND q.deleted_at IS NULL
         ORDER BY q.id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    Ok(QuestRelations {
        parent,
        sub_quests: subs
            .into_iter()
            .map(|(id, t)| QuestRef::new(id, t))
            .collect(),
        prerequisites: prereqs
            .into_iter()
            .map(|(id, t)| QuestRef::new(id, t))
            .collect(),
    })
}

async fn parent_id_of(pool: &SqlitePool, id: i64) -> AppResult<Option<i64>> {
    let row: Option<Option<i64>> = sqlx::query_scalar(
        "SELECT parent_quest_id FROM quests WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed_guild_dir;

    /// DEV-299: auto-snapshot 정책은 프로세스 전역 env 로 읽는다
    /// (`AutoSnapshotPolicy::from_env`). 테스트는 같은 프로세스에서 병렬 실행되므로
    /// env 를 만지는 테스트끼리는 직렬화해야 한다 — locale.rs 의 ENV_LOCK 과 같은 패턴.
    /// await 를 건너 유지되므로 async-aware Mutex 를 쓴다(std Mutex 는 clippy
    /// `await_holding_lock` 위반이자 실제로 런타임을 막을 수 있다).
    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-ops-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup_store(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        Store::open(dir).await.unwrap()
    }

    #[tokio::test]
    async fn create_quest_writes_file_and_journal() {
        let dir = fresh_tmp("create");
        let store = setup_store(&dir).await;

        let body = CreateQuestRequest {
            quest_type_id: 1, // DEV
            title: "first quest".into(),
            description: Some("body text".into()),
            status_slug: "open".into(),
            urgency: Some(2),
            parent_quest_id: None,
        };
        let quest = create_quest(&store, body).await.unwrap();
        assert_eq!(quest.quest_id, "DEV-001");
        assert_eq!(quest.title, "first quest");

        // 파일 생성됨
        let path = dir.join(".guild/quests/DEV-001.md");
        assert!(path.is_file(), "quest file should be created");

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("quest_id = \"DEV-001\""));
        assert!(content.contains("title = \"first quest\""));
        assert!(content.contains("status = \"open\""));
        assert!(content.contains("urgency = 2"));
        assert!(content.contains("body text")); // description 본문에 들어감

        // auto 블록도 있음 (root quest 라 Parent 섹션 없음)
        assert!(content.contains("openguild:auto-begin"));
        assert!(content.contains("Sub-quests"));
        assert!(content.contains("- (없음)")); // 자식 없으니 표시

        // journal 에 1 op
        let count = journal::count(&store.journal_pool).await.unwrap();
        assert_eq!(count, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn create_subquest_updates_parent_file() {
        let dir = fresh_tmp("sub");
        let store = setup_store(&dir).await;

        // parent 만들기
        let parent = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "parent".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();

        // 자식 만들기
        let child = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "child".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: Some(parent.id),
            },
        )
        .await
        .unwrap();

        // 자식 파일은 parent 표시
        let child_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-002.md")).unwrap();
        assert!(child_content.contains("parent = \"DEV-001\""));
        assert!(child_content.contains("## Parent"));
        assert!(child_content.contains("[DEV-001](DEV-001.md)"));

        // 부모 파일은 sub-quest 목록 갱신됨
        let parent_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(parent_content.contains("[DEV-002](DEV-002.md)"));
        assert!(parent_content.contains("child"));

        // journal: 2 ops
        let count = journal::count(&store.journal_pool).await.unwrap();
        assert_eq!(count, 2);

        // child 의 id 도 확실히 다른지
        assert_ne!(parent.id, child.id);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn change_status_updates_file_frontmatter() {
        let dir = fresh_tmp("status");
        let store = setup_store(&dir).await;
        let q = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "t".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();

        change_status(
            &store,
            q.id,
            ChangeStatusRequest {
                status_slug: "in_progress".into(),
            },
        )
        .await
        .unwrap();

        let content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(content.contains("status = \"in_progress\""));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-142: 미해결 discussion 댓글이 있으면 완료(done) 전환 차단,
    /// resolve 후엔 통과.
    #[tokio::test]
    async fn change_status_blocked_by_unresolved_discussion() {
        let dir = fresh_tmp("disc-gate");
        let store = setup_store(&dir).await;
        let q = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "t".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let slug = q.quest_id.clone();

        // discussion 댓글 추가 + discussion 플래그 on.
        let c = crate::ops::comments::add_comment_entry(
            &store,
            &slug,
            "admin".into(),
            "논의 필요".into(),
            None,
        )
        .await
        .unwrap();
        crate::ops::comments::toggle_comment_discussion(&store, &slug, c.id)
            .await
            .unwrap();

        // done(counts_as_done) 전환 → 차단.
        let blocked = change_status(
            &store,
            q.id,
            ChangeStatusRequest {
                status_slug: "done".into(),
            },
        )
        .await;
        assert!(blocked.is_err(), "미해결 discussion 이면 완료 차단되어야");

        // in_progress(counts_as_done=false) 전환 → 허용 (게이트는 완료에만 적용).
        change_status(
            &store,
            q.id,
            ChangeStatusRequest {
                status_slug: "in_progress".into(),
            },
        )
        .await
        .expect("non-done 전환은 허용");

        // resolve 후 done 전환 → 통과.
        crate::ops::comments::toggle_comment_resolved(&store, &slug, c.id)
            .await
            .unwrap();
        change_status(
            &store,
            q.id,
            ChangeStatusRequest {
                status_slug: "done".into(),
            },
        )
        .await
        .expect("resolve 후엔 완료 가능");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn update_quest_changes_title_in_file() {
        let dir = fresh_tmp("upd");
        let store = setup_store(&dir).await;
        let q = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "old".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();

        update_quest(
            &store,
            q.id,
            UpdateQuestRequest {
                title: Some("new title".into()),
                description: None,
                urgency: Some(1),
            },
        )
        .await
        .unwrap();

        let content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(content.contains("title = \"new title\""));
        assert!(content.contains("urgency = 1"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-012: GUI 에서 description 수정 → 파일에도 반영되어야 함.
    /// 이전엔 write_quest_file 의 "기존 파일 description 보존" fallback 이
    /// DB 의 새 description 을 덮어써서 파일이 옛 값 그대로 유지됐음.
    #[tokio::test]
    async fn update_quest_description_writes_to_file() {
        let dir = fresh_tmp("upd-desc");
        let store = setup_store(&dir).await;
        let q = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "x".into(),
                description: Some("original body".into()),
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();

        // 1차 수정: original → new body.
        update_quest(
            &store,
            q.id,
            UpdateQuestRequest {
                title: None,
                description: Some("new body".into()),
                urgency: None,
            },
        )
        .await
        .unwrap();

        let content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(
            content.contains("new body"),
            "새 description 이 파일에 반영"
        );
        assert!(
            !content.contains("original body"),
            "옛 description 은 사라져야"
        );

        // 2차 수정: new → newer.
        update_quest(
            &store,
            q.id,
            UpdateQuestRequest {
                title: None,
                description: Some("newer body".into()),
                urgency: None,
            },
        )
        .await
        .unwrap();

        let content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(content.contains("newer body"));
        assert!(!content.contains("new body\n")); // 'new body' substring 은 'newer body' 안에도 있으니 newline 으로 정확 매칭.

        // 3차: description 안 건드리는 mutation 은 description 보존.
        update_quest(
            &store,
            q.id,
            UpdateQuestRequest {
                title: Some("renamed".into()),
                description: None,
                urgency: None,
            },
        )
        .await
        .unwrap();
        let content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(content.contains("renamed"));
        assert!(
            content.contains("newer body"),
            "description 안 건드린 mutation 에선 보존"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-066: 외부 편집한 description 이 description 안 건드리는 mutation
    /// (change_status) 호출 시 보존되고 DB 에도 sync 되어야 함.
    /// BUG-012 이후 의도된 trade-off (외부 편집 → DB 옛 값으로 덮어쓰기) 의 해결.
    #[tokio::test]
    async fn change_status_preserves_external_description_and_syncs_db() {
        let dir = fresh_tmp("dev066-ext-edit");
        let store = setup_store(&dir).await;
        let q = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "x".into(),
                description: Some("initial body".into()),
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();

        let path = dir.join(".guild/quests/DEV-001.md");

        // 외부 편집 시뮬: 파일을 직접 열어 description 본문을 새로 씀.
        // (reindex 호출 안 함 — 사용자가 의도적으로 skip 한 시나리오.)
        let raw = std::fs::read_to_string(&path).unwrap();
        let edited = raw.replace("initial body", "EXTERNALLY EDITED BODY");
        std::fs::write(&path, edited).unwrap();

        // description 안 건드리는 mutation 호출 (status 변경).
        change_status(
            &store,
            q.id,
            ChangeStatusRequest {
                status_slug: "in_progress".into(),
            },
        )
        .await
        .unwrap();

        // 1) 외부 편집한 본문이 파일에서 보존되어야 함.
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("EXTERNALLY EDITED BODY"),
            "외부 편집 보존. 실제: {content}"
        );
        assert!(
            !content.contains("initial body"),
            "옛 DB 값으로 덮어쓰지 않음. 실제: {content}"
        );

        // 2) status 변경은 정상 반영.
        assert!(content.contains("status = \"in_progress\""));

        // 3) DB 도 외부 편집 값으로 sync 되어야 함.
        let q2 = crate::services::quests::fetch_by_id(&store.index_pool, q.id)
            .await
            .unwrap();
        assert_eq!(
            q2.description.as_deref().unwrap_or(""),
            "EXTERNALLY EDITED BODY",
            "DB description 이 파일 본문으로 sync 됨"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn change_parent_updates_three_files() {
        let dir = fresh_tmp("parent");
        let store = setup_store(&dir).await;
        let p1 = create_quest(&store, mk("p1", None)).await.unwrap();
        let p2 = create_quest(&store, mk("p2", None)).await.unwrap();
        let c = create_quest(&store, mk("c", Some(p1.id))).await.unwrap();

        // c 의 부모를 p1 → p2 로 변경.
        change_parent(
            &store,
            c.id,
            ChangeParentRequest {
                parent_quest_id: Some(p2.id),
            },
        )
        .await
        .unwrap();

        // c 파일 parent 갱신
        let c_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-003.md")).unwrap();
        assert!(c_content.contains("parent = \"DEV-002\""));

        // 옛 부모 p1 파일에 c 는 더 이상 sub 가 아님
        let p1_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(!p1_content.contains("[DEV-003](DEV-003.md)"));

        // 새 부모 p2 파일에 c 가 sub 로 표시
        let p2_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-002.md")).unwrap();
        assert!(p2_content.contains("[DEV-003](DEV-003.md)"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_quest_marks_deleted_in_file() {
        let dir = fresh_tmp("del");
        let store = setup_store(&dir).await;
        let q = create_quest(&store, mk("to-delete", None)).await.unwrap();

        delete_quest(&store, q.id, &[]).await.unwrap();

        let content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(content.contains("deleted = true"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_with_cascade_marks_children_deleted() {
        let dir = fresh_tmp("del-cas");
        let store = setup_store(&dir).await;
        let p = create_quest(&store, mk("p", None)).await.unwrap();
        let c = create_quest(&store, mk("c", Some(p.id))).await.unwrap();

        delete_quest(&store, p.id, &[c.id]).await.unwrap();

        let p_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        let c_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-002.md")).unwrap();
        assert!(p_content.contains("deleted = true"));
        assert!(c_content.contains("deleted = true"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_without_cascade_detaches_children() {
        let dir = fresh_tmp("del-detach");
        let store = setup_store(&dir).await;
        let p = create_quest(&store, mk("p", None)).await.unwrap();
        let _ = create_quest(&store, mk("c", Some(p.id))).await.unwrap();

        delete_quest(&store, p.id, &[]).await.unwrap();

        // 자식 c 는 alive 지만 parent 가 없음
        let c_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-002.md")).unwrap();
        assert!(
            !c_content.contains("parent ="),
            "parent should be omitted: {c_content}"
        );
        assert!(!c_content.contains("deleted = true"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn restore_quest_clears_deleted_flag() {
        let dir = fresh_tmp("restore");
        let store = setup_store(&dir).await;
        let q = create_quest(&store, mk("x", None)).await.unwrap();
        delete_quest(&store, q.id, &[]).await.unwrap();
        restore_quest(&store, q.id).await.unwrap();

        let content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(content.contains("deleted = false"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn add_remove_prerequisite_updates_file() {
        let dir = fresh_tmp("prereq");
        let store = setup_store(&dir).await;
        let q1 = create_quest(&store, mk("q1", None)).await.unwrap();
        let q2 = create_quest(&store, mk("q2", None)).await.unwrap();

        add_prerequisite(
            &store,
            q1.id,
            AddPrerequisiteRequest {
                prerequisite_id: q2.id,
            },
        )
        .await
        .unwrap();

        let q1_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(q1_content.contains("prerequisites = [\"DEV-002\"]"));
        assert!(q1_content.contains("[DEV-002](DEV-002.md)"));

        remove_prerequisite(&store, q1.id, q2.id).await.unwrap();
        let q1_after = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(q1_after.contains("prerequisites = []"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── DEV-055: change_quest_type ───

    #[tokio::test]
    async fn change_quest_type_renames_file_and_cascades_slug() {
        let dir = fresh_tmp("type-rename");
        let store = setup_store(&dir).await;

        let q = create_quest(&store, mk("first", None)).await.unwrap();
        assert_eq!(q.quest_id, "DEV-001");
        assert!(dir.join(".guild/quests/DEV-001.md").exists());

        // status 한 번 바꿔서 history row 1개 만들기.
        let _ = change_status(
            &store,
            q.id,
            ChangeStatusRequest {
                status_slug: "in_progress".into(),
            },
        )
        .await
        .unwrap();

        // type 변경: DEV → BUG.
        let updated = change_quest_type(
            &store,
            q.id,
            ChangeTypeRequest {
                new_type_prefix: "BUG".into(),
            },
        )
        .await
        .unwrap();

        assert_eq!(
            updated.quest_id, "BUG-001",
            "새 slug 부여 (BUG counter=0+1)"
        );
        assert_eq!(updated.type_prefix, "BUG");
        assert_eq!(updated.number, 1);

        // 파일 rename 확인.
        assert!(
            !dir.join(".guild/quests/DEV-001.md").exists(),
            "옛 파일 삭제"
        );
        assert!(
            dir.join(".guild/quests/BUG-001.md").exists(),
            "새 파일 생성"
        );
        let content = std::fs::read_to_string(dir.join(".guild/quests/BUG-001.md")).unwrap();
        assert!(content.contains("quest_id = \"BUG-001\""));

        // quest_history.quest_slug 가 cascade.
        let slugs: Vec<String> =
            sqlx::query_scalar("SELECT quest_slug FROM quest_history WHERE quest_id = ?")
                .bind(q.id)
                .fetch_all(&store.index_pool)
                .await
                .unwrap();
        assert!(
            slugs.iter().all(|s| s == "BUG-001"),
            "history slug cascade: {slugs:?}"
        );

        // change_type 자체도 history 에 기록됨.
        let ops: Vec<String> =
            sqlx::query_scalar("SELECT op FROM quest_history WHERE quest_id = ? ORDER BY id")
                .bind(q.id)
                .fetch_all(&store.index_pool)
                .await
                .unwrap();
        assert!(ops.contains(&"change_type".to_string()), "ops: {ops:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn change_quest_type_same_type_is_noop() {
        let dir = fresh_tmp("type-noop");
        let store = setup_store(&dir).await;

        let q = create_quest(&store, mk("x", None)).await.unwrap();
        let before_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM quest_history WHERE quest_id = ?")
                .bind(q.id)
                .fetch_one(&store.index_pool)
                .await
                .unwrap();

        let result = change_quest_type(
            &store,
            q.id,
            ChangeTypeRequest {
                new_type_prefix: "dev".into(),
            }, // 대소문자 무시.
        )
        .await
        .unwrap();

        assert_eq!(result.quest_id, "DEV-001");
        let after_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM quest_history WHERE quest_id = ?")
                .bind(q.id)
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(before_count, after_count, "NoOp — history 변화 없음");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn change_quest_type_rejects_unknown_type() {
        let dir = fresh_tmp("type-unknown");
        let store = setup_store(&dir).await;
        let q = create_quest(&store, mk("x", None)).await.unwrap();

        let err = change_quest_type(
            &store,
            q.id,
            ChangeTypeRequest {
                new_type_prefix: "NOPE".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::AppError::BadRequest(_)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn change_quest_type_updates_related_quest_files() {
        // 부모 / 자식 / 선행 관계 — 그쪽 파일들의 auto-block 안 slug 도 갱신.
        let dir = fresh_tmp("type-related");
        let store = setup_store(&dir).await;

        let parent = create_quest(&store, mk("p", None)).await.unwrap(); // DEV-001
        let child = create_quest(&store, mk("c", Some(parent.id)))
            .await
            .unwrap(); // DEV-002
        let prereq = create_quest(&store, mk("pre", None)).await.unwrap(); // DEV-003

        // child 에 prereq 추가.
        add_prerequisite(
            &store,
            child.id,
            AddPrerequisiteRequest {
                prerequisite_id: prereq.id,
            },
        )
        .await
        .unwrap();

        // child (DEV-002) 의 type 을 BUG 로 변경 → BUG-001.
        let updated = change_quest_type(
            &store,
            child.id,
            ChangeTypeRequest {
                new_type_prefix: "BUG".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.quest_id, "BUG-001");

        // 부모 파일의 sub-quest 목록에 새 slug (BUG-001) 표시.
        let parent_content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(
            parent_content.contains("BUG-001"),
            "parent.sub 갱신:\n{parent_content}"
        );
        assert!(!parent_content.contains("DEV-002"), "옛 slug 사라져야");

        // prereq 파일의 (dependent 표기는 auto-block 에 없을 수도 — render 확인).
        // 본 테스트는 parent 만 핵심으로 검증.

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 간단한 CreateQuestRequest 빌더.
    fn mk(title: &str, parent: Option<i64>) -> CreateQuestRequest {
        CreateQuestRequest {
            quest_type_id: 1,
            title: title.into(),
            description: None,
            status_slug: "open".into(),
            urgency: Some(3),
            parent_quest_id: parent,
        }
    }

    #[tokio::test]
    async fn create_preserves_description_on_overwrite() {
        // 같은 파일이 이미 있을 때 (외부 편집 후 mutate 같은 시나리오) description 보존.
        // 본 케이스에서는 동일 slug 로 두 번 create 불가하니, 대신 외부 파일을 미리 두고 mutate 동작 검증.
        // 다만 동일 slug 2회 create 시도는 SQL 단에서 unique constraint 위반.
        // 본 테스트는 description preservation 로직만 격리 검증 — skip 가능 시 별도 unit test 로.
        // 여기서는 단순 sanity check: 첫 create 후 description 이 파일에 있는지.
        let dir = fresh_tmp("desc");
        let store = setup_store(&dir).await;
        let _ = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "x".into(),
                description: Some("custom body".into()),
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(content.contains("custom body"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-242: quest 생성이 type 파일의 [counter].last_number 도 갱신 —
    /// 이전엔 DB 만 올라가고 파일은 --fix 수동 실행 전까지 낡아 있었음.
    #[tokio::test]
    async fn create_quest_syncs_type_counter_file() {
        let dir = fresh_tmp("counter-file");
        let store = setup_store(&dir).await;

        let before = crate::repo::TypeFile::read(store.paths.type_path("DEV")).unwrap();
        assert_eq!(before.counter.last_number, 0);

        let q = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "counter sync".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(q.number, 1);

        let after = crate::repo::TypeFile::read(store.paths.type_path("DEV")).unwrap();
        assert_eq!(after.counter.last_number, 1, "파일 counter 도 함께 갱신");

        // 파일 counter 가 더 크면(수동 --fix 등) 보존 — 단조 증가.
        let mut tf = crate::repo::TypeFile::read(store.paths.type_path("DEV")).unwrap();
        tf.counter.last_number = 99;
        tf.write(store.paths.type_path("DEV")).unwrap();
        sync_type_counter_file(&store, "DEV", 2);
        let kept = crate::repo::TypeFile::read(store.paths.type_path("DEV")).unwrap();
        assert_eq!(kept.counter.last_number, 99, "더 큰 파일 값은 안 내림");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG: 외부 편집으로 quests 테이블의 max(number) 가 counter 보다 커진 경우
    /// (ef010d5 같은 사용자 직접 추가), 다음 create_quest 가 counter+1 로 INSERT 시도
    /// → `UNIQUE (quest_type_id, number)` 충돌. self-heal 이 자동 보정해야 함.
    #[tokio::test]
    async fn create_self_heals_when_counter_lags_behind_actual_max() {
        let dir = fresh_tmp("counter-lag");
        let store = setup_store(&dir).await;

        // 1) 정상 경로로 DEV-001 만들고,
        let q1 = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "first".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(q1.number, 1);

        // 2) 외부 편집을 시뮬레이션 — quests 에 DEV-005 직접 INSERT (counter 는 안 건드림).
        sqlx::query(
            "INSERT INTO quests
               (id, quest_type_id, number, title, status_id, urgency, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(100)
        .bind(1)
        .bind(5)
        .bind("external")
        .bind(1)
        .bind(3)
        .execute(&store.index_pool)
        .await
        .unwrap();
        // 이 시점 counter.last_number = 1 (DEV-001 으로 +1 만 됨), max(number) = 5.

        // 3) 새 quest 만들면 self-heal 이 counter 를 5 로 끌어올린 뒤 +1 → DEV-006.
        let q2 = create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "after heal".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(q2.number, 6, "self-heal 이 max(actual)+1 부여해야");
        assert_eq!(q2.quest_id, "DEV-006");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-299: 백그라운드 모드에서 auto-snapshot 은 mutation 을 막지 않는다.
    ///
    /// 기본(동기)에서는 임계치에 걸린 mutation 이 스냅샷 생성(~2초)만큼 멈춘다.
    /// 여기서는 "mutation 반환 시점에 스냅샷이 아직 안 끝났을 수 있고, 잠시 뒤엔
    /// 완료된다"를 확인한다 — journal truncate 로 관측.
    #[tokio::test]
    async fn background_snapshot_does_not_block_mutation() {
        let dir = fresh_tmp("bg-snap");
        let store = setup_store(&dir).await;
        store.set_background_snapshots(true);
        // SAFETY: 이 테스트 프로세스 안에서만 정책을 낮춘다(임계치 1회 도달용).
        let _guard = ENV_LOCK.lock().await;
        // SAFETY: ENV_LOCK 으로 직렬화된 구간 — 정책 임계치를 1로 낮춘다.
        unsafe {
            std::env::set_var("OPENGUILD_AUTO_BACKUP_OPS", "1");
        }

        for i in 0..3 {
            create_quest(
                &store,
                CreateQuestRequest {
                    quest_type_id: 1,
                    title: format!("q{i}"),
                    description: None,
                    status_slug: "open".into(),
                    urgency: Some(3),
                    parent_quest_id: None,
                },
            )
            .await
            .unwrap();
        }

        // 백그라운드 task 가 끝날 시간을 준다(스냅샷은 파일 복사라 짧다).
        for _ in 0..40 {
            if !store
                .snapshot_in_flight
                .load(std::sync::atomic::Ordering::SeqCst)
                && crate::snapshot::latest_snapshot_timestamp(&store.paths)
                    .ok()
                    .flatten()
                    .is_some()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        assert!(
            crate::snapshot::latest_snapshot_timestamp(&store.paths)
                .unwrap()
                .is_some(),
            "백그라운드 스냅샷이 실제로 생성돼야 한다(유실 금지)"
        );

        unsafe {
            std::env::remove_var("OPENGUILD_AUTO_BACKUP_OPS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-299: 기본값은 동기 — CLI 처럼 즉시 종료되는 프로세스에서 스냅샷이
    /// 유실되면 안 되므로, 켜지 않으면 mutation 반환 시점에 이미 끝나 있어야 한다.
    #[tokio::test]
    async fn snapshot_is_synchronous_by_default() {
        let dir = fresh_tmp("sync-snap");
        let store = setup_store(&dir).await;
        assert!(
            !store.background_snapshots_enabled(),
            "기본값이 백그라운드면 CLI 에서 스냅샷이 유실된다"
        );
        let _guard = ENV_LOCK.lock().await;
        // SAFETY: ENV_LOCK 으로 직렬화된 구간 — 정책 임계치를 1로 낮춘다.
        unsafe {
            std::env::set_var("OPENGUILD_AUTO_BACKUP_OPS", "1");
        }

        for i in 0..3 {
            create_quest(
                &store,
                CreateQuestRequest {
                    quest_type_id: 1,
                    title: format!("q{i}"),
                    description: None,
                    status_slug: "open".into(),
                    urgency: Some(3),
                    parent_quest_id: None,
                },
            )
            .await
            .unwrap();
        }

        // 대기 없이 즉시 — 동기라면 이미 존재해야 한다.
        assert!(
            crate::snapshot::latest_snapshot_timestamp(&store.paths)
                .unwrap()
                .is_some(),
            "동기 모드는 mutation 반환 전에 스냅샷이 끝나 있어야 한다"
        );

        unsafe {
            std::env::remove_var("OPENGUILD_AUTO_BACKUP_OPS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
