//! DEV-100: Campaign 댓글 / 메모 — quest 의 DEV-012 / DEV-094 패턴 확장.
//!
//! 저장 (file 진리원):
//! - 댓글: `.guild/campaigns/{slug}.comments.md` — og-comment 마커, git tracked.
//! - 메모: `.guild/campaigns/{slug}.memo.md` — plain text, gitignored.
//!
//! quest 와 달리 DB 캐시 (DEV-102 류) 는 아직 없음 — read 가 file 직접이고
//! snapshot 백업 합류는 후속 quest. entry 포맷 / reactions (DEV-108) 은
//! quest 와 동일 (`repo::comments` 공용).

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::repo::comments::{self as repo, CommentEntry};
use crate::store::{journal, Store};

// ─── DEV-134: DB 캐시 sync (DEV-102 미러) — snapshot 백업 합류 ───

async fn lookup_campaign_id(store: &Store, slug: &str) -> AppResult<Option<i64>> {
    let id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM campaigns WHERE campaign_slug = ?")
            .bind(slug)
            .fetch_optional(&store.index_pool)
            .await?;
    Ok(id)
}

async fn upsert_entry_db(store: &Store, slug: &str, entry: &CommentEntry) -> AppResult<()> {
    let Some(cid) = lookup_campaign_id(store, slug).await? else {
        return Ok(()); // 캐시에 캠페인 없으면 skip — reindex 가 복구.
    };
    sqlx::query(
        "INSERT INTO campaign_comments (campaign_id, entry_id, ts, author, body, parent_id)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(campaign_id, entry_id) DO UPDATE SET
             ts        = excluded.ts,
             author    = excluded.author,
             body      = excluded.body,
             parent_id = excluded.parent_id",
    )
    .bind(cid)
    .bind(entry.id as i64)
    .bind(&entry.ts)
    .bind(&entry.author)
    .bind(&entry.body)
    .bind(entry.parent_id.map(|n| n as i64))
    .execute(&store.index_pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("upsert campaign_comments: {e}")))?;
    Ok(())
}

async fn delete_entry_db(store: &Store, slug: &str, id: u64) -> AppResult<()> {
    let Some(cid) = lookup_campaign_id(store, slug).await? else {
        return Ok(());
    };
    sqlx::query("DELETE FROM campaign_comments WHERE campaign_id = ? AND entry_id = ?")
        .bind(cid)
        .bind(id as i64)
        .execute(&store.index_pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("delete campaign_comments: {e}")))?;
    Ok(())
}

async fn upsert_memo_db(store: &Store, slug: &str, content: &str) -> AppResult<()> {
    let Some(cid) = lookup_campaign_id(store, slug).await? else {
        return Ok(());
    };
    sqlx::query(
        "INSERT INTO campaign_memos (campaign_id, user_id, content, updated_at)
         VALUES (?, 0, ?, ?)
         ON CONFLICT(campaign_id, user_id) DO UPDATE SET
             content = excluded.content, updated_at = excluded.updated_at",
    )
    .bind(cid)
    .bind(content)
    .bind(crate::time::now_local_iso8601())
    .execute(&store.index_pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("upsert campaign_memos: {e}")))?;
    Ok(())
}

/// 한 campaign 의 모든 댓글 entry.
pub fn list_entries(store: &Store, slug: &str) -> AppResult<Vec<CommentEntry>> {
    repo::read_entries_at(&store.paths.campaign_comments_path(slug)).map_err(AppError::Internal)
}

/// 새 댓글 entry 추가.
pub async fn add_entry(
    store: &Store,
    slug: &str,
    author: String,
    body: String,
    parent_id: Option<u64>,
) -> AppResult<CommentEntry> {
    let body_trimmed = body.trim().to_string();
    if body_trimmed.is_empty() {
        return Err(AppError::BadRequest("body is empty".into()));
    }
    let _ = journal::append(
        &store.journal_pool,
        "add_campaign_comment",
        &json!({ "slug": slug, "author": author, "len": body_trimmed.len(), "parent_id": parent_id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let path = store.paths.campaign_comments_path(slug);
    let mut entries = repo::read_entries_at(&path).map_err(AppError::Internal)?;
    if let Some(pid) = parent_id
        && !entries.iter().any(|e| e.id == pid)
    {
        return Err(AppError::BadRequest(format!(
            "parent comment {pid} not found for campaign {slug}"
        )));
    }
    let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
    let entry = CommentEntry {
        id: next_id,
        ts: crate::time::now_local_iso8601(),
        author: author.trim().to_string(),
        body: body_trimmed,
        parent_id,
        reactions: Vec::new(),
        discussion: false,
        resolved: false,
    };
    entries.push(entry.clone());
    repo::write_entries_at(&path, &entries).map_err(AppError::Internal)?;
    // BUG-068: sibling 파일 mtime 캐시 동기화 (drift 오탐 방지).
    let _ = crate::file_mtime::touch(store, &path).await;
    // DEV-134: file 진리원 갱신 후 DB 캐시 UPSERT.
    upsert_entry_db(store, slug, &entry).await?;
    Ok(entry)
}

/// entry 본문 수정 (ts / author 보존).
pub async fn update_entry(
    store: &Store,
    slug: &str,
    id: u64,
    body: String,
) -> AppResult<CommentEntry> {
    let body_trimmed = body.trim().to_string();
    if body_trimmed.is_empty() {
        return Err(AppError::BadRequest("body is empty".into()));
    }
    let _ = journal::append(
        &store.journal_pool,
        "update_campaign_comment",
        &json!({ "slug": slug, "id": id, "len": body_trimmed.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let path = store.paths.campaign_comments_path(slug);
    let mut entries = repo::read_entries_at(&path).map_err(AppError::Internal)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::NotFound(format!("comment {id} not found for {slug}")))?;
    entry.body = body_trimmed;
    let updated = entry.clone();
    repo::write_entries_at(&path, &entries).map_err(AppError::Internal)?;
    // BUG-068: sibling 파일 mtime 캐시 동기화 (drift 오탐 방지).
    let _ = crate::file_mtime::touch(store, &path).await;
    // DEV-134: 캐시 UPSERT.
    upsert_entry_db(store, slug, &updated).await?;
    Ok(updated)
}

/// entry 삭제.
pub async fn delete_entry(store: &Store, slug: &str, id: u64) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "delete_campaign_comment",
        &json!({ "slug": slug, "id": id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let path = store.paths.campaign_comments_path(slug);
    let mut entries = repo::read_entries_at(&path).map_err(AppError::Internal)?;
    let before = entries.len();
    entries.retain(|e| e.id != id);
    if entries.len() == before {
        return Err(AppError::NotFound(format!("comment {id} not found for {slug}")));
    }
    repo::write_entries_at(&path, &entries).map_err(AppError::Internal)?;
    // BUG-068: sibling 파일 mtime 캐시 동기화 (drift 오탐 방지).
    let _ = crate::file_mtime::touch(store, &path).await;
    // DEV-134: 캐시 row 삭제.
    delete_entry_db(store, slug, id).await?;
    Ok(())
}

/// DEV-108 동일: 이모지 반응 토글 (author 별 — 누가 반응했는지 호버 표시).
pub async fn toggle_reaction(
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
    let author = author.trim();
    if author.contains(bad) {
        return Err(AppError::BadRequest(
            "author 는 , \" : | 를 포함할 수 없음".into(),
        ));
    }
    let author = if author.is_empty() { "(익명)" } else { author };
    let _ = journal::append(
        &store.journal_pool,
        "toggle_campaign_comment_reaction",
        &json!({ "slug": slug, "id": id, "emoji": emoji, "author": author }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    use crate::repo::comments::{join_reaction, split_reaction};
    let path = store.paths.campaign_comments_path(slug);
    let mut entries = repo::read_entries_at(&path).map_err(AppError::Internal)?;
    let entry = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::NotFound(format!("comment {id} not found for {slug}")))?;
    if let Some(pos) = entry
        .reactions
        .iter()
        .position(|r| split_reaction(r).0 == emoji)
    {
        let (_, mut authors) = split_reaction(&entry.reactions[pos]);
        if let Some(ap) = authors.iter().position(|a| a == author) {
            authors.remove(ap);
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
    repo::write_entries_at(&path, &entries).map_err(AppError::Internal)?;
    // BUG-068: sibling 파일 mtime 캐시 동기화 (drift 오탐 방지).
    let _ = crate::file_mtime::touch(store, &path).await;
    Ok(updated)
}

/// 메모 읽기. 부재 시 None.
pub fn get_memo(store: &Store, slug: &str) -> AppResult<Option<String>> {
    repo::read_text_at(&store.paths.campaign_memo_path(slug)).map_err(AppError::Internal)
}

/// 메모 쓰기 (전체 교체).
pub async fn set_memo(store: &Store, slug: &str, content: String) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "set_campaign_memo",
        &json!({ "slug": slug, "len": content.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    repo::write_text_at(&store.paths.campaign_memo_path(slug), &content)
        .map_err(AppError::Internal)?;
    let _ = crate::file_mtime::touch(store, &store.paths.campaign_memo_path(slug)).await;
    // DEV-134: 캐시 UPSERT — snapshot 백업 대상.
    upsert_memo_db(store, slug, &content).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed_guild_dir;

    async fn setup(label: &str) -> (std::path::PathBuf, Store) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-camp-cmt-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        std::fs::create_dir_all(store.paths.campaigns_dir()).unwrap();
        (dir, store)
    }

    #[tokio::test]
    async fn add_list_update_delete_roundtrip() {
        let (dir, store) = setup("crud").await;
        let e1 = add_entry(&store, "C-001", "alice".into(), "first".into(), None)
            .await
            .unwrap();
        assert_eq!(e1.id, 1);
        let e2 = add_entry(&store, "C-001", "bob".into(), "reply".into(), Some(1))
            .await
            .unwrap();
        assert_eq!(e2.parent_id, Some(1));

        let list = list_entries(&store, "C-001").unwrap();
        assert_eq!(list.len(), 2);

        let u = update_entry(&store, "C-001", 1, "edited".into()).await.unwrap();
        assert_eq!(u.body, "edited");
        assert_eq!(u.author, "alice", "author 보존");

        delete_entry(&store, "C-001", 2).await.unwrap();
        assert_eq!(list_entries(&store, "C-001").unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-134: 캠페인 row 가 캐시에 있으면 add/update/delete/set_memo 가
    /// campaign_comments / campaign_memos 캐시도 sync.
    #[tokio::test]
    async fn db_cache_sync_when_campaign_exists() {
        let (dir, store) = setup("cache").await;
        // 캐시에 campaign row 직접 INSERT (ops::campaigns::create 대신 최소 fixture).
        sqlx::query(
            "INSERT INTO campaigns (campaign_slug, title, status, display_order, created_at, updated_at)
             VALUES ('C-001', 't', 'active', 0, 'x', 'x')",
        )
        .execute(&store.index_pool)
        .await
        .unwrap();

        let e = add_entry(&store, "C-001", "a".into(), "hello".into(), None)
            .await
            .unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_comments")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 1, "add 후 캐시 1 row");

        update_entry(&store, "C-001", e.id, "edited".into()).await.unwrap();
        let body: String =
            sqlx::query_scalar("SELECT body FROM campaign_comments WHERE entry_id = 1")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(body, "edited");

        set_memo(&store, "C-001", "note".into()).await.unwrap();
        let memo: String = sqlx::query_scalar("SELECT content FROM campaign_memos")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(memo, "note");

        delete_entry(&store, "C-001", e.id).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_comments")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "delete 후 캐시 0 row");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reaction_toggle_and_memo() {
        let (dir, store) = setup("react").await;
        add_entry(&store, "C-001", "a".into(), "x".into(), None).await.unwrap();
        let r1 = toggle_reaction(&store, "C-001", 1, "👍", "alice").await.unwrap();
        assert_eq!(r1.reactions, vec!["👍:alice"]);
        let r2 = toggle_reaction(&store, "C-001", 1, "👍", "alice").await.unwrap();
        assert!(r2.reactions.is_empty());

        assert!(get_memo(&store, "C-001").unwrap().is_none());
        set_memo(&store, "C-001", "private note".into()).await.unwrap();
        assert_eq!(get_memo(&store, "C-001").unwrap().as_deref(), Some("private note"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn add_rejects_missing_parent_and_empty_body() {
        let (dir, store) = setup("valid").await;
        assert!(add_entry(&store, "C-001", "a".into(), "  ".into(), None).await.is_err());
        assert!(add_entry(&store, "C-001", "a".into(), "x".into(), Some(99)).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
