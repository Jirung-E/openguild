//! DEV-016 (multi-file): 길드 규칙 mutation orchestration.
//!
//! 다중 파일 `.guild/rules/{slug}.md` — 각 mutation 마다 journal append + atomic
//! file IO. DB 캐시 없음.

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::repo::history as hist;
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

/// DEV-290: 규칙의 변경 이력 (최신 → 과거). 규칙은 DB history 테이블이 없어
/// `.guild/history/{slug}.jsonl` 사이드카에서 직접 읽는다(append-only 라 역순).
pub fn history(store: &Store, slug: &str) -> AppResult<Vec<hist::HistoryEntry>> {
    let path = hist::history_path(&store.paths, slug);
    let mut v = hist::read_all(&path).map_err(AppError::Internal)?;
    v.reverse();
    Ok(v)
}

/// DEV-243: 태그 포함 전체 조회.
pub fn get_rule_entry(store: &Store, slug: &str) -> AppResult<Option<RuleEntry>> {
    repo::read_rule_entry(&store.paths, slug).map_err(AppError::Internal)
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
    repo::write_rule(&store.paths, slug, &content).map_err(AppError::Internal)?;
    // DEV-288: 활동 기록. set 은 upsert 라 신규/수정 겸용이지만 update 로 기록.
    hist::record(&store.paths, slug, "update", None, None);
    Ok(())
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
    repo::create_rule(&store.paths, slug, &content).map_err(AppError::Internal)?;
    hist::record(&store.paths, slug, "create", None, None); // DEV-288
    Ok(())
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
    repo::delete_rule(&store.paths, slug).map_err(AppError::Internal)?;
    hist::record(&store.paths, slug, "delete", None, None); // DEV-288
    Ok(())
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
    repo::rename_rule(&store.paths, old_slug, new_slug).map_err(AppError::Internal)?;
    // DEV-288: 사이드카도 새 slug 로 옮기고 rename 이벤트 기록 (quest change_type 패턴).
    let _ = hist::rename(&store.paths, old_slug, new_slug);
    hist::record(
        &store.paths,
        new_slug,
        "rename",
        Some(old_slug.to_string()),
        Some(new_slug.to_string()),
    );
    Ok(())
}

/// DEV-243: 규칙 태그 전체 교체.
pub async fn set_rule_tags(store: &Store, slug: &str, tags: Vec<String>) -> AppResult<RuleEntry> {
    let _ = journal::append(
        &store.journal_pool,
        "set_rule_tags",
        &json!({ "slug": slug, "tags": &tags }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    repo::set_rule_tags(&store.paths, slug, tags).map_err(AppError::Internal)
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
