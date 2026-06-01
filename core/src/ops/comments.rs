//! DEV-012: Quest 별 댓글 / 메모 mutation orchestration.
//!
//! 1. journal::append (의도 기록).
//! 2. atomic file write.
//!
//! DB 캐시 없음 — `repo::comments::read_*` 도 파일 직접 read.

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::repo::comments as repo;
use crate::store::{journal, Store};

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
    repo::write_comments(&store.paths, slug, &content).map_err(AppError::Internal)
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
    repo::write_memo(&store.paths, slug, &content).map_err(AppError::Internal)
}
