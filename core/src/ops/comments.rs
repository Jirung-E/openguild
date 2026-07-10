//! DEV-012 / DEV-094: Quest 별 댓글 / 메모 mutation orchestration.
//!
//! DEV-094 부터 **댓글은 entry 단위** — `services::comments::{add/update/delete}_entry`.
//! 메모는 그대로 단일 텍스트.
//!
//! 흐름 (DEV-102 부터):
//! 1. journal::append (의도 기록).
//! 2. service 호출 (파일 read → mutate → atomic write).
//! 3. DB cache sync (`quest_comments` / `quest_memos`) — snapshot 백업 대상.
//!
//! 파일이 진리원, DB 는 snapshot 백업 + 빠른 쿼리용 캐시. quest 가 index.db 에
//! 없으면 (drift 상태) DB UPSERT 는 silently skip — 다음 reindex 가 일관시킴.

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::repo::comments as repo;
use crate::services::comments as svc;
use crate::store::{journal, Store};

pub use crate::repo::comments::CommentEntry;

/// DEV-102: slug → quests.id 매핑. drift 상태에선 None.
async fn lookup_quest_id(store: &Store, slug: &str) -> AppResult<Option<i64>> {
    sqlx::query_scalar::<_, i64>(
        "SELECT q.id FROM quests q
           JOIN quest_types t ON t.id = q.quest_type_id
          WHERE t.prefix || '-' || printf('%03d', q.number) = ?",
    )
    .bind(slug)
    .fetch_optional(&store.index_pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("lookup quest id by slug: {e}")))
}

/// DEV-102: 한 댓글 entry 를 `quest_comments` 에 UPSERT. drift 상태 (quest 없음)
/// 면 skip — 다음 reindex 시 일관.
async fn upsert_comment_entry_db(
    store: &Store,
    slug: &str,
    entry: &CommentEntry,
) -> AppResult<()> {
    let Some(qid) = lookup_quest_id(store, slug).await? else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO quest_comments
            (quest_id, entry_id, ts, author, body, parent_id, discussion, resolved, pinned, edited_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(quest_id, entry_id) DO UPDATE SET
             ts         = excluded.ts,
             author     = excluded.author,
             body       = excluded.body,
             parent_id  = excluded.parent_id,
             discussion = excluded.discussion,
             resolved   = excluded.resolved,
             pinned     = excluded.pinned,
             edited_at  = excluded.edited_at",
    )
    .bind(qid)
    .bind(entry.id as i64)
    .bind(&entry.ts)
    .bind(&entry.author)
    .bind(&entry.body)
    .bind(entry.parent_id.map(|n| n as i64))
    .bind(entry.discussion as i64)
    .bind(entry.resolved as i64)
    .bind(entry.pinned as i64)
    .bind(&entry.edited_at)
    .execute(&store.index_pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("upsert quest_comments: {e}")))?;
    Ok(())
}

/// DEV-102: 한 댓글 entry 를 `quest_comments` 에서 삭제.
async fn delete_comment_entry_db(store: &Store, slug: &str, id: u64) -> AppResult<()> {
    let Some(qid) = lookup_quest_id(store, slug).await? else {
        return Ok(());
    };
    sqlx::query("DELETE FROM quest_comments WHERE quest_id = ? AND entry_id = ?")
        .bind(qid)
        .bind(id as i64)
        .execute(&store.index_pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("delete quest_comments: {e}")))?;
    Ok(())
}

/// DEV-102: 메모를 `quest_memos` 에 UPSERT (single-user user_id=0).
async fn upsert_memo_db(store: &Store, slug: &str, content: &str) -> AppResult<()> {
    let Some(qid) = lookup_quest_id(store, slug).await? else {
        return Ok(());
    };
    let ts = crate::time::now_local_iso8601();
    sqlx::query(
        "INSERT INTO quest_memos (quest_id, user_id, content, updated_at)
         VALUES (?, 0, ?, ?)
         ON CONFLICT(quest_id, user_id) DO UPDATE SET
             content    = excluded.content,
             updated_at = excluded.updated_at",
    )
    .bind(qid)
    .bind(content)
    .bind(&ts)
    .execute(&store.index_pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("upsert quest_memos: {e}")))?;
    Ok(())
}

/// DEV-102: legacy 전체-텍스트 댓글 sync — 파일의 entry 들을 한 번에 적재.
/// quest_comments 의 기존 row 는 DELETE 후 INSERT.
async fn replace_comments_db(store: &Store, slug: &str) -> AppResult<()> {
    let Some(qid) = lookup_quest_id(store, slug).await? else {
        return Ok(());
    };
    let entries = svc::list_entries(store, slug)?;
    let mut tx = store
        .index_pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("begin tx: {e}")))?;
    sqlx::query("DELETE FROM quest_comments WHERE quest_id = ?")
        .bind(qid)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("clear quest_comments: {e}")))?;
    for entry in &entries {
        sqlx::query(
            "INSERT INTO quest_comments
                (quest_id, entry_id, ts, author, body, parent_id, discussion, resolved, pinned)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(qid)
        .bind(entry.id as i64)
        .bind(&entry.ts)
        .bind(&entry.author)
        .bind(&entry.body)
        .bind(entry.parent_id.map(|n| n as i64))
        .bind(entry.discussion as i64)
        .bind(entry.resolved as i64)
        .bind(entry.pinned as i64)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("insert quest_comments: {e}")))?;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("commit tx: {e}")))?;
    Ok(())
}

// ─── DEV-012 legacy: 단일 텍스트 댓글 API (호환용, deprecated) ───
// DEV-094 의 entry API 가 권장. 본 함수는 frontend / CLI 가 아직 호출하지 않으면
// 제거 가능 — 단 reindex / migration 코드가 read_comments 를 직접 부르는 경우
// 안전 위해 유지.

pub fn get_comments(store: &Store, slug: &str) -> AppResult<Option<String>> {
    repo::read_comments(&store.paths, slug).map_err(AppError::Internal)
}

pub async fn set_comments(store: &Store, slug: &str, content: String) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "set_comments",
        &json!({ "slug": slug, "len": content.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    repo::write_comments(&store.paths, slug, &content).map_err(AppError::Internal)?;
    let _ = crate::file_mtime::touch(store, &store.paths.comments_path(slug)).await;
    // DEV-102: 파일을 통째로 갈았으므로 캐시도 전체 재적재.
    replace_comments_db(store, slug).await
}

// ─── DEV-094: entry 단위 ───

/// 한 quest 의 모든 댓글 entry 목록.
pub fn list_comment_entries(store: &Store, slug: &str) -> AppResult<Vec<CommentEntry>> {
    svc::list_entries(store, slug)
}

/// 새 댓글 entry 추가. `parent_id`: Some = 답글 (threaded reply), None = top-level.
pub async fn add_comment_entry(
    store: &Store,
    slug: &str,
    author: String,
    body: String,
    parent_id: Option<u64>,
) -> AppResult<CommentEntry> {
    let _ = journal::append(
        &store.journal_pool,
        "add_comment_entry",
        &json!({
            "slug": slug,
            "author": author,
            "len": body.len(),
            "parent_id": parent_id,
        }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    let entry = svc::add_entry(store, slug, author, body, parent_id)?;
    let _ = crate::file_mtime::touch(store, &store.paths.comments_path(slug)).await;
    // DEV-102: file 진리원 갱신 후 DB 캐시 UPSERT.
    upsert_comment_entry_db(store, slug, &entry).await?;
    Ok(entry)
}

/// 댓글 entry 본문 수정 (ts / author 보존).
pub async fn update_comment_entry(
    store: &Store,
    slug: &str,
    id: u64,
    body: String,
) -> AppResult<CommentEntry> {
    let _ = journal::append(
        &store.journal_pool,
        "update_comment_entry",
        &json!({ "slug": slug, "id": id, "len": body.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    let updated = svc::update_entry(store, slug, id, body)?;
    let _ = crate::file_mtime::touch(store, &store.paths.comments_path(slug)).await;
    // DEV-102: 같은 entry_id 의 row 를 UPSERT (body 만 변경됨).
    upsert_comment_entry_db(store, slug, &updated).await?;
    Ok(updated)
}

/// DEV-108: 이모지 반응 토글 — 있으면 제거, 없으면 추가. single-user 전제
/// (이모지당 on/off). 토글 후 entry 반환.
pub async fn toggle_comment_reaction(
    store: &Store,
    slug: &str,
    id: u64,
    emoji: &str,
    author: &str,
) -> AppResult<CommentEntry> {
    let emoji = emoji.trim();
    let bad = |c: char| matches!(c, ',' | '"' | ':' | '|');
    if emoji.is_empty() || emoji.contains(bad) {
        return Err(AppError::BadRequest(
            "emoji 는 비어있지 않아야 하고 , \" : | 를 포함할 수 없음".into(),
        ));
    }
    // DEV-108: 누가 반응했는지 기록 — 빈 author 는 '(익명)' 으로 대체해 항상 1명
    // 이상 보장 (toggle 로직 단순화 + 호버에 표시할 이름 보장).
    let author = author.trim();
    if author.contains(bad) {
        return Err(AppError::BadRequest(
            "author 는 , \" : | 를 포함할 수 없음".into(),
        ));
    }
    let author = if author.is_empty() { "(익명)" } else { author };
    let _ = journal::append(
        &store.journal_pool,
        "toggle_comment_reaction",
        &json!({ "slug": slug, "id": id, "emoji": emoji, "author": author }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    use crate::repo::comments::{join_reaction, split_reaction};
    let mut entries = svc::list_entries(store, slug)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::NotFound(format!("comment {id} not found for {slug}")))?;
    // 같은 emoji 항목을 찾아 author 를 토글. 없으면 새 항목.
    if let Some(pos) = entry
        .reactions
        .iter()
        .position(|r| split_reaction(r).0 == emoji)
    {
        let (_, mut authors) = split_reaction(&entry.reactions[pos]);
        if let Some(ap) = authors.iter().position(|a| a == author) {
            authors.remove(ap); // 이미 반응함 → 해제.
        } else {
            authors.push(author.to_string());
        }
        if authors.is_empty() {
            entry.reactions.remove(pos);
        } else {
            entry.reactions[pos] = join_reaction(emoji, &authors);
        }
    } else {
        entry
            .reactions
            .push(join_reaction(emoji, &[author.to_string()]));
    }
    let updated = entry.clone();
    crate::repo::comments::write_entries(&store.paths, slug, &entries)
        .map_err(AppError::Internal)?;
    // BUG-068: sibling 파일 mtime 캐시 동기화 (drift 오탐 방지).
    let _ = crate::file_mtime::touch(store, &store.paths.comments_path(slug)).await;
    // reactions 는 file-only (DB 캐시 컬럼 없음 — read 경로가 file 직접이라
    // 무방. 캐시 재구축도 file 에서 다시 파싱). body 등은 그대로라 UPSERT 생략.
    Ok(updated)
}

/// DEV-142: 댓글의 토론(discussion) 플래그 토글. discussion 이 켜지면 resolve
/// 전까지 quest 완료 전환이 차단된다. discussion 을 끄면 resolved 도 같이 해제
/// (의미 없는 잔여 상태 방지).
pub async fn toggle_comment_discussion(
    store: &Store,
    slug: &str,
    id: u64,
) -> AppResult<CommentEntry> {
    let _ = journal::append(
        &store.journal_pool,
        "toggle_comment_discussion",
        &json!({ "slug": slug, "id": id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let mut entries = svc::list_entries(store, slug)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::NotFound(format!("comment {id} not found for {slug}")))?;
    entry.discussion = !entry.discussion;
    if !entry.discussion {
        entry.resolved = false;
    }
    let updated = entry.clone();
    crate::repo::comments::write_entries(&store.paths, slug, &entries)
        .map_err(AppError::Internal)?;
    // BUG-068: sibling 파일 mtime 캐시 동기화 (drift 오탐 방지).
    let _ = crate::file_mtime::touch(store, &store.paths.comments_path(slug)).await;
    // DEV-142 후속: discussion/resolved 를 DB 캐시에도 반영 (목록/홈 집계용).
    upsert_comment_entry_db(store, slug, &updated).await?;
    Ok(updated)
}

/// DEV-142: discussion 댓글의 resolved 토글. discussion 이 아닌 댓글엔 BadRequest.
pub async fn toggle_comment_resolved(
    store: &Store,
    slug: &str,
    id: u64,
) -> AppResult<CommentEntry> {
    let _ = journal::append(
        &store.journal_pool,
        "toggle_comment_resolved",
        &json!({ "slug": slug, "id": id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let mut entries = svc::list_entries(store, slug)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::NotFound(format!("comment {id} not found for {slug}")))?;
    if !entry.discussion {
        return Err(AppError::BadRequest(
            "discussion 댓글이 아니면 resolve 할 수 없음".into(),
        ));
    }
    entry.resolved = !entry.resolved;
    let updated = entry.clone();
    crate::repo::comments::write_entries(&store.paths, slug, &entries)
        .map_err(AppError::Internal)?;
    // BUG-068: sibling 파일 mtime 캐시 동기화 (drift 오탐 방지).
    let _ = crate::file_mtime::touch(store, &store.paths.comments_path(slug)).await;
    // DEV-142 후속: discussion/resolved 를 DB 캐시에도 반영 (목록/홈 집계용).
    upsert_comment_entry_db(store, slug, &updated).await?;
    // DEV-236: resolve/reopen 전환을 quest_history 에 기록 — worklog 가 이미
    // quest_history 를 소스로 쓰므로 이것만으로 타임라인에 표출됨. discussion
    // 최초 표시(marked)는 범위 밖(admin 결정 — "과기록은 노이즈, 우선 resolve
    // 전환만").
    let op = if updated.resolved { "discussion_resolved" } else { "discussion_reopened" };
    record_discussion_history(store, slug, op, id).await?;
    Ok(updated)
}

/// DEV-236: discussion resolve/reopen 전환을 quest_history(+ 사이드카)에 기록.
/// worklog 활동 타임라인이 이미 quest_history 를 소스로 쓰므로 kind 매핑만
/// 추가하면(ops::worklog) 자동 표출된다. quest 가 drift 상태(캐시에 없음)면
/// silently skip — 다음 reindex 가 일관시킴(다른 history 기록과 동일 정책).
async fn record_discussion_history(
    store: &Store,
    slug: &str,
    op: &str,
    comment_id: u64,
) -> AppResult<()> {
    let Some(qid) = lookup_quest_id(store, slug).await? else {
        return Ok(());
    };
    let ts = crate::time::now_local_iso8601();
    let comment_id_str = comment_id.to_string();
    sqlx::query(
        "INSERT INTO quest_history (quest_id, quest_slug, ts, op, old_value, new_value)
         VALUES (?, ?, ?, ?, NULL, ?)",
    )
    .bind(qid)
    .bind(slug)
    .bind(&ts)
    .bind(op)
    .bind(&comment_id_str)
    .execute(&store.index_pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("insert quest_history (discussion): {e}")))?;
    crate::repo::history::append(
        &store.paths,
        slug,
        &crate::repo::history::HistoryEntry {
            ts,
            op: op.to_string(),
            old: None,
            new: Some(comment_id_str),
        },
    )
    .map_err(AppError::Internal)?;
    Ok(())
}

/// DEV-234: 댓글 상단 고정(pin) 토글. discussion 과 달리 quest 전용 게이트
/// 없음 — root/답글 무관하게 켤 수 있고, 실제 "몇 개까지" "root 만" 같은
/// 제약은 GUI 가 담당(pin 버튼을 root 댓글에만 노출).
pub async fn toggle_comment_pinned(store: &Store, slug: &str, id: u64) -> AppResult<CommentEntry> {
    let _ = journal::append(
        &store.journal_pool,
        "toggle_comment_pinned",
        &json!({ "slug": slug, "id": id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let mut entries = svc::list_entries(store, slug)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::NotFound(format!("comment {id} not found for {slug}")))?;
    entry.pinned = !entry.pinned;
    let updated = entry.clone();
    crate::repo::comments::write_entries(&store.paths, slug, &entries)
        .map_err(AppError::Internal)?;
    // BUG-068: sibling 파일 mtime 캐시 동기화 (drift 오탐 방지).
    let _ = crate::file_mtime::touch(store, &store.paths.comments_path(slug)).await;
    upsert_comment_entry_db(store, slug, &updated).await?;
    Ok(updated)
}

/// 댓글 entry 삭제.
pub async fn delete_comment_entry(store: &Store, slug: &str, id: u64) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "delete_comment_entry",
        &json!({ "slug": slug, "id": id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    svc::delete_entry(store, slug, id)?;
    let _ = crate::file_mtime::touch(store, &store.paths.comments_path(slug)).await;
    // DEV-102: 같은 entry_id 의 cache row 도 삭제.
    delete_comment_entry_db(store, slug, id).await
}

pub fn get_memo(store: &Store, slug: &str) -> AppResult<Option<String>> {
    repo::read_memo(&store.paths, slug).map_err(AppError::Internal)
}

pub async fn set_memo(store: &Store, slug: &str, content: String) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "set_memo",
        &json!({ "slug": slug, "len": content.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    repo::write_memo(&store.paths, slug, &content).map_err(AppError::Internal)?;
    let _ = crate::file_mtime::touch(store, &store.paths.memo_path(slug)).await;
    // DEV-102: snapshot 백업 대상이 되도록 DB 캐시도 갱신.
    upsert_memo_db(store, slug, &content).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CreateQuestRequest;
    use crate::ops::quests as quest_ops;
    use crate::repo::seed_guild_dir;

    async fn fresh(label: &str) -> (std::path::PathBuf, Store, String) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-ops-comments-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        // 정상 quest 1 개 생성 — DB에도 들어가야 lookup_quest_id 가 Some 반환.
        let q = quest_ops::create_quest(
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
        (dir, store, q.quest_id)
    }

    /// DEV-102: add_comment_entry 후 quest_comments 캐시에 row 생긴다.
    #[tokio::test]
    async fn add_comment_entry_syncs_db_cache() {
        let (dir, store, slug) = fresh("add").await;
        let e = add_comment_entry(&store, &slug, "alice".into(), "hello".into(), None)
            .await
            .unwrap();
        let row: (String, String, Option<i64>) = sqlx::query_as(
            "SELECT author, body, parent_id FROM quest_comments WHERE entry_id = ?",
        )
        .bind(e.id as i64)
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(row.0, "alice");
        assert_eq!(row.1, "hello");
        assert_eq!(row.2, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-102: update_comment_entry 가 body 만 캐시에서도 갱신, ts/author 보존.
    #[tokio::test]
    async fn update_comment_entry_syncs_db_cache() {
        let (dir, store, slug) = fresh("upd").await;
        let e = add_comment_entry(&store, &slug, "alice".into(), "v1".into(), None)
            .await
            .unwrap();
        let _ = update_comment_entry(&store, &slug, e.id, "v2".into()).await.unwrap();
        let body: String = sqlx::query_scalar(
            "SELECT body FROM quest_comments WHERE entry_id = ?",
        )
        .bind(e.id as i64)
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(body, "v2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-102: delete_comment_entry 후 캐시 row 도 삭제됨.
    #[tokio::test]
    async fn delete_comment_entry_removes_from_db_cache() {
        let (dir, store, slug) = fresh("del").await;
        let e = add_comment_entry(&store, &slug, "".into(), "x".into(), None)
            .await
            .unwrap();
        delete_comment_entry(&store, &slug, e.id).await.unwrap();
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM quest_comments WHERE entry_id = ?",
        )
        .bind(e.id as i64)
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(n, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-102: set_memo 가 user_id=0 row UPSERT — 두 번 호출해도 PK 충돌 없이
    /// content 갱신.
    #[tokio::test]
    async fn set_memo_upserts_db_cache() {
        let (dir, store, slug) = fresh("memo").await;
        set_memo(&store, &slug, "v1".into()).await.unwrap();
        set_memo(&store, &slug, "v2".into()).await.unwrap();
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT user_id, content FROM quest_memos",
        )
        .fetch_all(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1, "single-user 단계 — row 1 개");
        assert_eq!(rows[0].0, 0);
        assert_eq!(rows[0].1, "v2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-108: 이모지 반응 토글 — 추가 / 제거 + 파일 roundtrip + validation.
    #[tokio::test]
    async fn toggle_reaction_roundtrip_and_validation() {
        let (dir, store, slug) = fresh("react").await;
        add_comment_entry(&store, &slug, "a".into(), "x".into(), None)
            .await
            .unwrap();

        // DEV-108: author 별 토글 — emoji:author 인코딩으로 누가 반응했는지 기록.
        let on = toggle_comment_reaction(&store, &slug, 1, "👍", "alice")
            .await
            .unwrap();
        assert_eq!(on.reactions, vec!["👍:alice"]);
        // 파일에 attr 로 남아 재파싱에도 살아있어야.
        let listed = list_comment_entries(&store, &slug).unwrap();
        assert_eq!(listed[0].reactions, vec!["👍:alice"]);

        // 다른 author 가 같은 emoji → authors 누적.
        let two = toggle_comment_reaction(&store, &slug, 1, "👍", "bob")
            .await
            .unwrap();
        assert_eq!(two.reactions, vec!["👍:alice|bob"]);

        // alice 재토글 → 본인만 빠짐.
        let one = toggle_comment_reaction(&store, &slug, 1, "👍", "alice")
            .await
            .unwrap();
        assert_eq!(one.reactions, vec!["👍:bob"]);

        // 마지막 author 빠지면 reaction 자체 제거.
        let off = toggle_comment_reaction(&store, &slug, 1, "👍", "bob")
            .await
            .unwrap();
        assert!(off.reactions.is_empty());

        // 빈 author → '(익명)' 으로 기록.
        let anon = toggle_comment_reaction(&store, &slug, 1, "✅", "")
            .await
            .unwrap();
        assert_eq!(anon.reactions, vec!["✅:(익명)"]);

        // validation — 빈 emoji / 콤마 / 콜론 거부, 없는 entry NotFound.
        assert!(toggle_comment_reaction(&store, &slug, 1, " ", "a").await.is_err());
        assert!(toggle_comment_reaction(&store, &slug, 1, "a,b", "a").await.is_err());
        assert!(toggle_comment_reaction(&store, &slug, 1, "a:b", "a").await.is_err());
        assert!(toggle_comment_reaction(&store, &slug, 99, "👍", "a").await.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-102: 답글 추가 시 parent_id 컬럼이 정확히 세팅.
    #[tokio::test]
    async fn reply_comment_persists_parent_id_in_db() {
        let (dir, store, slug) = fresh("reply").await;
        let top = add_comment_entry(&store, &slug, "a".into(), "1".into(), None)
            .await
            .unwrap();
        let r = add_comment_entry(&store, &slug, "b".into(), "2".into(), Some(top.id))
            .await
            .unwrap();
        let parent_id: Option<i64> = sqlx::query_scalar(
            "SELECT parent_id FROM quest_comments WHERE entry_id = ?",
        )
        .bind(r.id as i64)
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(parent_id, Some(top.id as i64));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-234: pin 토글 — 파일 attr + DB 캐시 모두 반영, discussion 과 달리
    /// 게이트 없이 아무 entry 나 켤 수 있음.
    #[tokio::test]
    async fn toggle_pinned_roundtrip_and_syncs_db_cache() {
        let (dir, store, slug) = fresh("pin").await;
        let e = add_comment_entry(&store, &slug, "a".into(), "결정사항".into(), None)
            .await
            .unwrap();
        assert!(!e.pinned);

        let on = toggle_comment_pinned(&store, &slug, e.id).await.unwrap();
        assert!(on.pinned);
        let cached: i64 =
            sqlx::query_scalar("SELECT pinned FROM quest_comments WHERE entry_id = ?")
                .bind(e.id as i64)
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(cached, 1);
        // 파일에서 다시 읽어도 유지.
        let listed = list_comment_entries(&store, &slug).unwrap();
        assert!(listed[0].pinned);

        let off = toggle_comment_pinned(&store, &slug, e.id).await.unwrap();
        assert!(!off.pinned);

        assert!(toggle_comment_pinned(&store, &slug, 999).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-236: resolve/reopen 전환이 quest_history 에 기록되는지(설계 공백
    /// 수정 — 이전엔 journal 감사로그에만 남아 worklog 에 안 잡혔음).
    #[tokio::test]
    async fn toggle_resolved_records_quest_history() {
        let (dir, store, slug) = fresh("history").await;
        let e = add_comment_entry(&store, &slug, "a".into(), "토론 필요".into(), None)
            .await
            .unwrap();
        toggle_comment_discussion(&store, &slug, e.id).await.unwrap();
        // discussion 최초 표시는 기록 대상 아님(admin 결정 — 과기록 방지).
        let n0: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_history WHERE quest_slug = ?")
            .bind(&slug)
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n0, 0);

        toggle_comment_resolved(&store, &slug, e.id).await.unwrap();
        let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT op, old_value, new_value FROM quest_history WHERE quest_slug = ? ORDER BY id",
        )
        .bind(&slug)
        .fetch_all(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "discussion_resolved");
        assert_eq!(rows[0].2.as_deref(), Some(e.id.to_string().as_str()));

        // 재개(unresolve) 도 기록.
        toggle_comment_resolved(&store, &slug, e.id).await.unwrap();
        let rows2: Vec<String> = sqlx::query_scalar(
            "SELECT op FROM quest_history WHERE quest_slug = ? ORDER BY id",
        )
        .bind(&slug)
        .fetch_all(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(rows2, vec!["discussion_resolved", "discussion_reopened"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
