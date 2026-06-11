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
    };
    entries.push(entry.clone());
    repo::write_entries_at(&path, &entries).map_err(AppError::Internal)?;
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
    Ok(())
}

/// DEV-108 동일: 이모지 반응 토글.
pub async fn toggle_reaction(
    store: &Store,
    slug: &str,
    id: u64,
    emoji: &str,
) -> AppResult<CommentEntry> {
    let emoji = emoji.trim();
    if emoji.is_empty() || emoji.contains(',') || emoji.contains('"') {
        return Err(AppError::BadRequest(
            "emoji 는 비어있지 않아야 하고 ',' / '\"' 를 포함할 수 없음".into(),
        ));
    }
    let _ = journal::append(
        &store.journal_pool,
        "toggle_campaign_comment_reaction",
        &json!({ "slug": slug, "id": id, "emoji": emoji }),
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
    if let Some(pos) = entry.reactions.iter().position(|r| r == emoji) {
        entry.reactions.remove(pos);
    } else {
        entry.reactions.push(emoji.to_string());
    }
    let updated = entry.clone();
    repo::write_entries_at(&path, &entries).map_err(AppError::Internal)?;
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
        .map_err(AppError::Internal)
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

    #[tokio::test]
    async fn reaction_toggle_and_memo() {
        let (dir, store) = setup("react").await;
        add_entry(&store, "C-001", "a".into(), "x".into(), None).await.unwrap();
        let r1 = toggle_reaction(&store, "C-001", 1, "👍").await.unwrap();
        assert_eq!(r1.reactions, vec!["👍"]);
        let r2 = toggle_reaction(&store, "C-001", 1, "👍").await.unwrap();
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
