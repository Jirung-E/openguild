//! DEV-094: Quest 댓글 entry 단위 CRUD.
//!
//! 파일이 진리원 (`.guild/quests/{slug}.comments.md`)이며 DB는 횡단 검색용 캐시다.
//! 파일 단위 CRUD는 read → mutate → write 순서로 처리하고, `ops::comments`가
//! 파일 mutation 뒤 DB 캐시를 동기화한다.

use crate::error::{AppError, AppResult};
use crate::repo::comments as repo;
use crate::store::Store;
use sqlx::SqlitePool;

pub use crate::repo::comments::CommentEntry;

/// 길드 전체 댓글 횡단 검색 결과 (quest/campaign 통합).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct GlobalComment {
    pub scope: String,
    pub slug: String,
    pub entry_id: i64,
    pub ts: String,
    pub author: String,
    pub body: String,
    pub discussion: bool,
    pub resolved: bool,
    pub parent_id: Option<i64>,
    pub pinned: bool,
    pub reactions: String,
}

#[derive(Debug, Default)]
pub struct GlobalCommentsFilter<'a> {
    pub author: Option<&'a str>,
    pub since: Option<&'a str>,
    pub until: Option<&'a str>,
    pub grep: Option<&'a str>,
    pub discussion: bool,
    pub unresolved: bool,
}

/// Quest + campaign 댓글 캐시를 한 번에 검색한다. 출력 정렬은 오래된 순이며,
/// CLI의 top-only/reply-to/reverse/limit/tree는 이 값 위에서 동일하게 후처리한다.
pub async fn search_global(
    pool: &SqlitePool,
    filter: GlobalCommentsFilter<'_>,
) -> AppResult<Vec<GlobalComment>> {
    let mut conds_q = String::new();
    let mut conds_c = String::new();
    let mut push_both = |quest: &str, campaign: &str| {
        conds_q.push_str(" AND ");
        conds_q.push_str(quest);
        conds_c.push_str(" AND ");
        conds_c.push_str(campaign);
    };
    let mut binds = Vec::new();
    if let Some(author) = filter.author {
        push_both("LOWER(c.author) = LOWER(?)", "LOWER(c.author) = LOWER(?)");
        binds.push(author.to_string());
    }
    if let Some(since) = filter.since {
        push_both("c.ts >= ?", "c.ts >= ?");
        binds.push(crate::time::normalize_filter_ts(since));
    }
    if let Some(until) = filter.until {
        push_both("c.ts <= ?", "c.ts <= ?");
        binds.push(crate::time::normalize_filter_ts(until));
    }
    if let Some(grep) = filter.grep {
        push_both(
            "LOWER(c.body) LIKE '%' || LOWER(?) || '%'",
            "LOWER(c.body) LIKE '%' || LOWER(?) || '%'",
        );
        binds.push(grep.to_string());
    }
    if filter.discussion {
        push_both("c.discussion = 1", "0 = 1");
    }
    if filter.unresolved {
        push_both("c.discussion = 1 AND c.resolved = 0", "0 = 1");
    }

    let sql = format!(
        "SELECT * FROM (
           SELECT 'quest' AS scope,
                  qt.prefix || '-' || printf('%03d', q.number) AS slug,
                  c.entry_id, c.ts, c.author, c.body,
                  c.discussion, c.resolved, c.parent_id, c.pinned, c.reactions
             FROM quest_comments c
             JOIN quests q ON q.id = c.quest_id
             JOIN quest_types qt ON qt.id = q.quest_type_id
            WHERE 1 = 1{conds_q}
           UNION ALL
           SELECT 'campaign' AS scope, ca.campaign_slug AS slug,
                  c.entry_id, c.ts, c.author, c.body,
                  0 AS discussion, 0 AS resolved, c.parent_id, c.pinned, c.reactions
             FROM campaign_comments c
             JOIN campaigns ca ON ca.id = c.campaign_id
            WHERE 1 = 1{conds_c}
         )
         ORDER BY ts ASC"
    );
    let mut query = sqlx::query_as::<_, GlobalComment>(&sql);
    for value in &binds {
        query = query.bind(value);
    }
    for value in &binds {
        query = query.bind(value);
    }
    Ok(query.fetch_all(pool).await?)
}

/// 한 quest 의 모든 entry. 파일 부재 / legacy 단일 텍스트는 빈 vec or 1-entry.
pub fn list_entries(store: &Store, slug: &str) -> AppResult<Vec<CommentEntry>> {
    repo::read_entries(&store.paths, slug).map_err(AppError::Internal)
}

/// 새 entry 추가. `id` 는 기존 max + 1 (없으면 1). `ts` 는 현재 로컬 시각.
/// body 는 trim 후 빈 문자열이면 `BadRequest`.
///
/// `parent_id`: Some 이면 답글 (threaded reply). 그 id 가 현존 entry 셋에 없으면
/// `BadRequest`. None 이면 top-level.
///
/// `discussion`: DEV-366 — 토론 댓글로 **생성 시점에** 지정한다. 예전엔 항상
/// false 로 만든 뒤 `toggle_comment_discussion` 으로 뒤집어야 했는데, 그러면
/// 쓰기가 2회로 갈라져 두 번째가 실패하면 평댓글이 남았다(원격은 HTTP 왕복
/// 2회라 더 잘 드러난다). 한 번의 write_entries 로 끝낸다.
pub fn add_entry(
    store: &Store,
    slug: &str,
    author: String,
    body: String,
    parent_id: Option<u64>,
    discussion: bool,
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
        reactions: Vec::new(),
        discussion,
        resolved: false,
        pinned: false,
        edited_at: None,
    };
    entries.push(entry.clone());
    repo::write_entries(&store.paths, slug, &entries).map_err(AppError::Internal)?;
    Ok(entry)
}

/// entry 의 body 만 교체. ts / author 보존. `edited_at` 은 현재 시각으로 갱신
/// (DEV-182 — 편집됨 표시용). 미존재 시 `NotFound`.
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
    entries[idx].edited_at = Some(crate::time::now_local_iso8601());
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
    use crate::models::CreateQuestRequest;
    use crate::ops::{comments as comment_ops, quests as quest_ops};
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
        let a = add_entry(&store, "DEV-001", "alice".into(), "first".into(), None, false).unwrap();
        let b = add_entry(&store, "DEV-001", "".into(), "second".into(), None, false).unwrap();
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
        let a = add_entry(&store, "DEV-001", "alice".into(), "first".into(), None, false).unwrap();
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
        let _a = add_entry(&store, "DEV-001", "".into(), "first".into(), None, false).unwrap();
        let b = add_entry(&store, "DEV-001", "".into(), "second".into(), None, false).unwrap();
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
        let err = add_entry(&store, "DEV-001", "".into(), "   ".into(), None, false).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reply_links_parent_id_and_round_trips() {
        let (dir, store) = fresh("reply").await;
        let a = add_entry(&store, "DEV-001", "alice".into(), "top".into(), None, false).unwrap();
        let r = add_entry(
            &store,
            "DEV-001",
            "bob".into(),
            "answer".into(),
            Some(a.id),
            false,
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
            false,
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
        let _a = add_entry(&store, "DEV-001", "".into(), "1".into(), None, false).unwrap();
        let b = add_entry(&store, "DEV-001", "".into(), "2".into(), None, false).unwrap();
        delete_entry(&store, "DEV-001", b.id).unwrap();
        let c = add_entry(&store, "DEV-001", "".into(), "3".into(), None, false).unwrap();
        // alive 중 max(id) = 1 → next = 2 (재사용).
        assert_eq!(c.id, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn global_search_filters_synced_quest_comments() {
        let (dir, store) = fresh("global-search").await;
        let quest = quest_ops::create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "search target".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        comment_ops::add_comment_entry(
            &store,
            &quest.quest_id,
            "alice".into(),
            "remote parity needle".into(),
            None,
            false,
        )
        .await
        .unwrap();

        let rows = search_global(
            &store.index_pool,
            GlobalCommentsFilter {
                author: Some("ALICE"),
                grep: Some("needle"),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].scope, "quest");
        assert_eq!(rows[0].slug, quest.quest_id);
        assert_eq!(rows[0].body, "remote parity needle");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
