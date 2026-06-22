//! DEV-016 (multi-file): 길드 규칙 mutation orchestration.
//!
//! 다중 파일 `.guild/rules/{slug}.md` — 각 mutation 마다 journal append + atomic
//! file IO. DB 캐시 없음.

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::repo::rules as repo;
use crate::store::{journal, Store};

pub use crate::repo::rules::RuleEntry;

// ─── multi-file API (DEV-016 후속) ───

pub fn list_rules(store: &Store) -> AppResult<Vec<RuleEntry>> {
    repo::list_rules(&store.paths).map_err(AppError::Internal)
}

pub fn get_rule(store: &Store, slug: &str) -> AppResult<Option<String>> {
    repo::read_rule(&store.paths, slug).map_err(AppError::Internal)
}

pub async fn set_rule(store: &Store, slug: &str, content: String) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "set_rule",
        &json!({ "slug": slug, "len": content.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    repo::write_rule(&store.paths, slug, &content).map_err(AppError::Internal)
}

pub async fn create_rule(store: &Store, slug: &str, content: String) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "create_rule",
        &json!({ "slug": slug, "len": content.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    repo::create_rule(&store.paths, slug, &content).map_err(AppError::Internal)
}

pub async fn delete_rule(store: &Store, slug: &str) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "delete_rule",
        &json!({ "slug": slug }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    repo::delete_rule(&store.paths, slug).map_err(AppError::Internal)
}

pub async fn rename_rule(
    store: &Store,
    old_slug: &str,
    new_slug: &str,
) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "rename_rule",
        &json!({ "old_slug": old_slug, "new_slug": new_slug }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    repo::rename_rule(&store.paths, old_slug, new_slug).map_err(AppError::Internal)
}

// ─── (deprecated) 단일 파일 API ───
// 호환을 위해 유지 — 내부적으로 general slug 로 위임. 새 호출처는 multi API 사용.

pub fn get_rules(store: &Store) -> AppResult<Option<String>> {
    repo::read(&store.paths).map_err(AppError::Internal)
}

pub async fn set_rules(store: &Store, content: String) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "set_rules",
        &json!({ "len": content.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    repo::write(&store.paths, &content).map_err(AppError::Internal)
}
