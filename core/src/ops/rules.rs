//! DEV-016: 길드 규칙 (`.guild/rules.md`) mutation orchestration.
//!
//! 1. journal::append (의도 기록).
//! 2. atomic file write.
//!
//! DB 캐시 없음 — `services::rules::get` 도 파일 직접 read.

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::repo::rules as repo;
use crate::store::{journal, Store};

/// 규칙 조회 — 파일 부재 시 `None`.
pub fn get_rules(store: &Store) -> AppResult<Option<String>> {
    repo::read(&store.paths).map_err(AppError::Internal)
}

/// 규칙 저장 (전체 교체). 빈 문자열도 그대로 (= 의도적으로 비움).
pub async fn set_rules(store: &Store, content: String) -> AppResult<()> {
    let _ = journal::append(
        &store.journal_pool,
        "set_rules",
        &json!({ "len": content.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    repo::write(&store.paths, &content).map_err(AppError::Internal)?;
    Ok(())
}
