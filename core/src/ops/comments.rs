//! DEV-012 / DEV-094: Quest 별 댓글 / 메모 mutation orchestration.
//!
//! DEV-094 부터 **댓글은 entry 단위** — `services::comments::{add/update/delete}_entry`.
//! 메모는 그대로 단일 텍스트.
//!
//! 1. journal::append (의도 기록).
//! 2. service 호출 (파일 read → mutate → atomic write).

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::repo::comments as repo;
use crate::services::comments as svc;
use crate::store::{journal, Store};

pub use crate::repo::comments::CommentEntry;

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
    repo::write_comments(&store.paths, slug, &content).map_err(AppError::Internal)
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
    svc::add_entry(store, slug, author, body, parent_id)
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
    svc::update_entry(store, slug, id, body)
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
    svc::delete_entry(store, slug, id)
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
