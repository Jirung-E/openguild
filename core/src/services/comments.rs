//! DEV-094: Quest 댓글 entry 단위 CRUD.
//!
//! 파일이 진리원 (`.guild/quests/{slug}.comments.md`) — DB 캐시 없음.
//! 각 함수는 파일 read → mutate → write 의 순서. 동시 mutation 은 사용자 단일
//! 데스크탑 가정으로 race 무시 (서버 모드는 axum 가 핸들러 단위 직렬화).

use crate::error::{AppError, AppResult};
use crate::repo::comments as repo;
use crate::store::Store;

pub use crate::repo::comments::CommentEntry;

/// 한 quest 의 모든 entry. 파일 부재 / legacy 단일 텍스트는 빈 vec or 1-entry.
pub fn list_entries(store: &Store, slug: &str) -> AppResult<Vec<CommentEntry>> {
    repo::read_entries(&store.paths, slug).map_err(AppError::Internal)
}

/// 새 entry 추가. `id` 는 기존 max + 1 (없으면 1). `ts` 는 현재 로컬 시각.
/// body 는 trim 후 빈 문자열이면 `BadRequest`.
///
/// `parent_id`: Some 이면 답글 (threaded reply). 그 id 가 현존 entry 셋에 없으면
/// `BadRequest`. None 이면 top-level.
pub fn add_entry(
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
    let mut entries = list_entries(store, slug)?;
    if let Some(pid) = parent_id
        && !entries.iter().any(|e| e.id == pid)
    {
        return Err(AppError::BadRequest(format!(
            "parent comment {pid} not found for quest {slug}"
        )));
    }
    let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
    let entry = CommentEntry {
        id: next_id,
        ts: crate::time::now_local_iso8601(),
        author: author.trim().to_string(),
        body: body_trimmed,
        parent_id,
    };
    entries.push(entry.clone());
    repo::write_entries(&store.paths, slug, &entries).map_err(AppError::Internal)?;
    Ok(entry)
}

/// entry 의 body 만 교체. ts / author 보존. 미존재 시 `NotFound`.
pub fn update_entry(
    store: &Store,
    slug: &str,
    id: u64,
    body: String,
) -> AppResult<CommentEntry> {
    let body_trimmed = body.trim().to_string();
    if body_trimmed.is_empty() {
        return Err(AppError::BadRequest("body is empty".into()));
    }
    let mut entries = list_entries(store, slug)?;
    let Some(idx) = entries.iter().position(|e| e.id == id) else {
        return Err(AppError::NotFound(format!(
            "comment {id} not found for quest {slug}"
        )));
    };
    entries[idx].body = body_trimmed;
    let updated = entries[idx].clone();
    repo::write_entries(&store.paths, slug, &entries).map_err(AppError::Internal)?;
    Ok(updated)
}

/// entry 삭제. 미존재 시 `NotFound`.
pub fn delete_entry(store: &Store, slug: &str, id: u64) -> AppResult<()> {
    let entries = list_entries(store, slug)?;
    let before = entries.len();
    let filtered: Vec<CommentEntry> = entries.into_iter().filter(|e| e.id != id).collect();
    if filtered.len() == before {
        return Err(AppError::NotFound(format!(
            "comment {id} not found for quest {slug}"
        )));
    }
    repo::write_entries(&store.paths, slug, &filtered).map_err(AppError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    async fn fresh(label: &str) -> (std::path::PathBuf, Store) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-svc-comments-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        (dir, store)
    }

    #[tokio::test]
    async fn add_then_list_returns_entries() {
        let (dir, store) = fresh("add-list").await;
        let a = add_entry(&store, "DEV-001", "alice".into(), "first".into(), None).unwrap();
        let b = add_entry(&store, "DEV-001", "".into(), "second".into(), None).unwrap();
        assert_eq!(a.id, 1);
        assert_eq!(b.id, 2);
        let listed = list_entries(&store, "DEV-001").unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].body, "first");
        assert_eq!(listed[1].body, "second");
        assert_eq!(listed[0].author, "alice");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn update_replaces_body_preserves_ts_author() {
        let (dir, store) = fresh("update").await;
        let a = add_entry(&store, "DEV-001", "alice".into(), "first".into(), None).unwrap();
        let updated =
            update_entry(&store, "DEV-001", a.id, "edited".into()).unwrap();
        assert_eq!(updated.body, "edited");
        assert_eq!(updated.author, "alice");
        assert_eq!(updated.ts, a.ts);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_removes_only_target() {
        let (dir, store) = fresh("delete").await;
        let _a = add_entry(&store, "DEV-001", "".into(), "first".into(), None).unwrap();
        let b = add_entry(&store, "DEV-001", "".into(), "second".into(), None).unwrap();
        delete_entry(&store, "DEV-001", b.id).unwrap();
        let listed = list_entries(&store, "DEV-001").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].body, "first");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_missing_returns_not_found() {
        let (dir, store) = fresh("del-missing").await;
        let err = delete_entry(&store, "DEV-001", 99).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn add_empty_body_rejected() {
        let (dir, store) = fresh("empty").await;
        let err = add_entry(&store, "DEV-001", "".into(), "   ".into(), None).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reply_links_parent_id_and_round_trips() {
        let (dir, store) = fresh("reply").await;
        let a = add_entry(&store, "DEV-001", "alice".into(), "top".into(), None).unwrap();
        let r = add_entry(
            &store,
            "DEV-001",
            "bob".into(),
            "answer".into(),
            Some(a.id),
        )
        .unwrap();
        assert_eq!(r.parent_id, Some(a.id));
        let listed = list_entries(&store, "DEV-001").unwrap();
        assert_eq!(listed[0].parent_id, None);
        assert_eq!(listed[1].parent_id, Some(a.id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reply_to_nonexistent_parent_rejected() {
        let (dir, store) = fresh("reply-bad").await;
        let err = add_entry(
            &store,
            "DEV-001",
            "".into(),
            "orphan".into(),
            Some(999),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn next_id_is_max_plus_one_among_alive() {
        // 현재 스펙: next_id = max(alive id) + 1. 삭제된 id 가 마지막이었으면
        // 그 자리가 재사용될 수 있음 (alive 만 보므로). monotonic-after-deletion
        // 보장 X — file-only 저장이라 grave 추적이 부담. 충돌은 ID PK 가 아닌
        // 단순 entry 키이므로 의미상 문제 없음.
        let (dir, store) = fresh("next-id").await;
        let _a = add_entry(&store, "DEV-001", "".into(), "1".into(), None).unwrap();
        let b = add_entry(&store, "DEV-001", "".into(), "2".into(), None).unwrap();
        delete_entry(&store, "DEV-001", b.id).unwrap();
        let c = add_entry(&store, "DEV-001", "".into(), "3".into(), None).unwrap();
        // alive 중 max(id) = 1 → next = 2 (재사용).
        assert_eq!(c.id, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
