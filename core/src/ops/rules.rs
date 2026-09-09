//! DEV-016 (multi-file): 길드 규칙 mutation orchestration.
//!
//! 다중 파일 `.guild/rules/{slug}.md` — 각 mutation 마다 journal append + atomic
//! file IO. DB 캐시 없음.

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::events::{names as ev, payload};
use crate::repo::history as hist;
use crate::ops::doc_history::{self, DocKind};
use crate::repo::rules as repo;
use crate::store::{journal, Store};

pub use crate::repo::rules::RuleEntry;

/// DEV-388: 이벤트에 실을 규칙 한 건.
///
/// **`emit_*` 클로저 안에서만** 부른다 — 규칙은 DB 캐시가 없어 페이로드를
/// 만들려면 파일을 한 번 더 읽어야 하는데, 구독자가 없으면 그 비용도 치르지
/// 않는 것이 이 저장소의 규칙이다. 못 읽으면 최소한 slug 는 싣는다.
fn rule_payload(store: &Store, slug: &str) -> serde_json::Value {
    match repo::read_rule_entry(&store.paths, slug) {
        Ok(Some(r)) => payload::rule(&r),
        _ => payload::rule_ref(slug),
    }
}

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
    // BUG-227: 존재 확인이 먼저다. 사이드카가 없으면 빈 목록이 되므로, 예전에는
    // **오타를 쳐도 `(no history)` + 성공(exit 0)** 으로 끝났다 — "이력이 없는
    // 문서" 와 "없는 문서" 가 구분되지 않아 스크립트가 조용히 잘못된 결론을
    // 낸다. quest show 처럼 없는 대상은 NotFound 로 돌려준다.
    if repo::read_rule_entry(&store.paths, slug)
        .map_err(AppError::Internal)?
        .is_none()
    {
        return Err(AppError::NotFound(crate::tf!(
            "규칙 없음: {slug}",
            "rule not found: {slug}"
        )));
    }
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
    // BUG-189: 사이드카뿐 아니라 doc_history 캐시에도 즉시 투영 — 예전엔 reindex 를
    // 돌기 전까지 작업기록에 안 떴다.
    doc_history::record(store, DocKind::Rule, slug, "update", None, None).await;
    // REQ-008: 이 문서가 내보내는 cross-link 재계산 — BUG-189 가 doc_history 를
    // 즉시 투영한 것과 같은 이유다(reindex 전까지 반영이 안 되면 기능이 없는 것과
    // 같다). 색인은 파생물이라 실패해도 본 작업은 성공으로 둔다.
    let _ = crate::ops::backlinks::refresh_for(store, crate::repo::crosslink::DocKind::Rule, slug)
        .await;
    // DEV-388: 파일·사이드카·색인이 다 끝난 뒤에만 낸다.
    store.emit_post(
        ev::RULE_UPDATED,
        || json!({ "rule": rule_payload(store, slug) }),
    );
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
    doc_history::record(store, DocKind::Rule, slug, "create", None, None).await; // BUG-189
    store.emit_post(
        ev::RULE_CREATED,
        || json!({ "rule": rule_payload(store, slug) }),
    ); // DEV-388
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
    // DEV-388: 지운 뒤에는 본문도 태그도 어디서도 못 읽는다 — `delete_quest` 와
    // 같은 이유로 지우기 **전에** 잡아 둔다. 구독자가 없으면 읽지도 않는다.
    let doomed = if store.events_wanted(ev::RULE_DELETED, crate::events::Phase::Post) {
        repo::read_rule_entry(&store.paths, slug).ok().flatten()
    } else {
        None
    };
    repo::delete_rule(&store.paths, slug).map_err(AppError::Internal)?;
    doc_history::record(store, DocKind::Rule, slug, "delete", None, None).await;
    // 문서가 사라졌으니 캐시 행도 정리 — reindex 가 dangling 사이드카를 skip 하는 것과 같은 결과.
    doc_history::purge(store, slug).await;
    store.emit_post(ev::RULE_DELETED, || {
        let rule = match &doomed {
            Some(r) => payload::rule(r),
            // 못 잡았으면 최소한 slug 는 싣는다.
            None => payload::rule_ref(slug),
        };
        json!({ "rule": rule })
    });
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
    // BUG-189: 캐시의 옛 slug 행도 함께 옮긴다 — 안 그러면 작업기록의 지난 항목이
    // 지금은 없는 slug 를 가리켜 클릭해도 안 열린다.
    doc_history::rename(store, old_slug, new_slug).await;
    doc_history::record(
        store,
        DocKind::Rule,
        new_slug,
        "rename",
        Some(old_slug.to_string()),
        Some(new_slug.to_string()),
    )
    .await;
    // DEV-388: 규칙은 slug 가 곧 공개 식별자다. 무엇이 무엇이 됐는지 같이
    // 싣지 않으면 구독자는 규칙 하나가 사라지고 하나가 생긴 줄 안다.
    store.emit_post(ev::RULE_RENAMED, || {
        json!({
            "rule": rule_payload(store, new_slug),
            "change": payload::change(old_slug, new_slug),
        })
    });
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
    // DEV-388: 이전 태그는 바꾸기 **전에만** 읽을 수 있다. 태그 교체는 전체
    // 덮어쓰기라 결과만 봐서는 무엇이 붙고 무엇이 떨어졌는지 알 수 없다.
    // 구독자가 없으면 이 조회조차 안 한다.
    let old_tags: Vec<String> =
        if store.events_wanted(ev::RULE_TAGS_CHANGED, crate::events::Phase::Post) {
            repo::read_rule_entry(&store.paths, slug)
                .ok()
                .flatten()
                .map(|r| r.tags)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
    let entry = repo::set_rule_tags(&store.paths, slug, tags).map_err(AppError::Internal)?;
    store.emit_post(ev::RULE_TAGS_CHANGED, || {
        json!({
            "rule": payload::rule(&entry),
            "change": payload::change(old_tags, entry.tags.clone()),
        })
    });
    Ok(entry)
}

// ─── (deprecated) 단일 파일 API ───
// 호환을 위해 유지 — 내부적으로 general slug 로 위임. 새 호출처는 multi API 사용.

pub fn get_rules(store: &Store) -> AppResult<Option<String>> {
    repo::read(&store.paths).map_err(AppError::Internal)
}

/// DEV-388: **이벤트를 내지 않는다**(카탈로그에서도 Excluded). 단일 파일을
/// 통째로 덮어쓰는 레거시 경로라 "어느 규칙이 어떻게 바뀌었나" 를 실을 수
/// 없다 — `set_comments` 와 같은 이유다.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed_guild_dir;

    async fn setup(label: &str) -> (std::path::PathBuf, Store) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-ruleops-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        (dir, store)
    }

    /// BUG-227: 없는 규칙의 이력 조회는 **성공이 아니라 NotFound**.
    ///
    /// 이력은 사이드카 파일을 읽는 구조라 파일이 없으면 빈 목록이 된다. 그래서
    /// 예전엔 오타를 쳐도 `(no history)` + exit 0 으로 끝나, "이력이 없는 규칙"
    /// 과 "없는 규칙" 이 구분되지 않았다(스크립트가 조용히 잘못된 결론을 낸다).
    #[tokio::test]
    async fn history_of_missing_rule_is_not_found() {
        let (_dir, store) = setup("hist-missing").await;

        let err = history(&store, "존재하지-않는-규칙").unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "NotFound 여야: {err:?}");

        // 대조군: 실제로 있는 규칙은 이력이 없어도 Ok.
        set_rule(&store, "코딩-규칙", "# 코딩 규칙\n".into())
            .await
            .unwrap();
        history(&store, "코딩-규칙").expect("존재하는 규칙은 Ok");
    }
}
