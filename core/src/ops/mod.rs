//! Orchestration 레이어 — SQL mutation + 파일 IO + journal AOF 를 묶음.
//!
//! 호출자 (server routes, cli Backend::Local) 는 본 모듈의 함수를 호출.
//! 이 모듈은 검증 / SQL / 파일 / journal 을 일관된 순서로 실행:
//!
//! ```text
//! 1. journal INSERT (의도 기록 — durable)
//! 2. SQL mutation (services::quests::*) — index.db 반영
//! 3. .guild/quests/{slug}.md atomic write
//! 4. 영향받는 다른 quest 파일들의 auto 블록 재생성
//! ```
//!
//! crash 시: journal 에 기록된 op 는 다음 시작에서 replay 또는 reindex 시 정합 복구.

pub mod attachments;
pub mod backlinks;
pub mod campaign_comments;
pub mod campaigns;
pub mod comments;
pub mod counter;
pub mod doc_history;
pub mod library;
// REQ-028: 도서관 문서를 파일 시스템으로 펼친다(폴더 구조 그대로).
pub mod library_export;
#[cfg(test)]
mod lock_coverage;
pub mod meta;
pub mod positions;
pub mod quests;
pub mod rules;
pub mod search;
pub mod worklog;

pub use counter::check_and_fix_counters;

/// DEV-432: 고칠 문서를 **알아내서** 잠그고, 잠근 뒤 **다시 알아내** 같은지 본다.
///
/// 퀘스트 번호(행 id)만 받는 변경은 잠그기 전에 어느 문서인지 읽어야 한다. 그 사이 남이 바꿀 수 있다
/// (부모가 바뀌어 고칠 부모 파일이 달라지는 식). 그래서 잠근 뒤 한 번 더 읽어 같으면 그대로 가고,
/// 다르면 다 풀고 다시 잡는다. 몇 번이고 계속 바뀌면 길드 **독점**으로 — 그 안에서는 아무것도 못 바뀐다.
///
/// 목록은 한 번에 넘긴다 — 잡는 순서는 `lock_docs` 가 정한다(부르는 쪽이 못 고른다).
pub(crate) async fn lock_resolved<F, Fut>(
    store: &crate::store::Store,
    resolve: F,
) -> crate::error::AppResult<crate::lock::MutationGuard>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = crate::error::AppResult<Vec<crate::lock::DocKey>>>,
{
    let norm = |mut v: Vec<crate::lock::DocKey>| {
        v.sort();
        v.dedup();
        v
    };
    for _ in 0..RESOLVE_TRIES {
        let want = norm(resolve().await?);
        let g = store.lock_docs(&want).await?;
        if norm(resolve().await?) == want {
            return Ok(g);
        }
        drop(g);
    }
    Ok(store.lock_guild().await?)
}

/// 잠근 뒤 목록이 바뀌어 다시 잡는 횟수 — 넘으면 길드 독점.
const RESOLVE_TRIES: usize = 4;

/// BUG-287: 태그를 **붙이고 떼는** 요청 — 결과 목록 전체가 아니라 의도를 싣는다.
///
/// 전체 목록을 보내면 보내는 쪽이 읽은 때와 쓰는 때 사이에 남이 붙인 태그가 지워진다.
/// 에이전트가 CLI 로 태그를 붙이는 동안 사람이 앱에서 다른 태그를 붙이면 둘 중 하나가
/// 사라졌다. 잠금을 쥔 채 **지금** 목록에 적용해야 둘 다 남는다.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TagEdit {
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

impl TagEdit {
    /// 떼고 나서 붙인다 — 같은 태그가 양쪽에 있으면 붙은 채로 끝난다. 순서는 기존 것
    /// 그대로, 새 것은 뒤에. 공백·빈 값은 버린다(정규화는 `set_*_tags` 가 마저 한다).
    pub fn apply(&self, current: Vec<String>) -> Vec<String> {
        let clean = |v: &[String]| -> Vec<String> {
            v.iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        };
        let remove = clean(&self.remove);
        let mut out: Vec<String> = current.into_iter().filter(|t| !remove.contains(t)).collect();
        for t in clean(&self.add) {
            if !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }
}
pub use meta::{
    count_quests_by_status, count_quests_by_type, create_status, create_type, delete_status,
    delete_tag_def, delete_type, rename_status_slug, rename_type, update_status, update_type,
    upsert_tag_def,
};
pub use quests::{
    add_prerequisite, change_parent, change_quest_type, change_status, create_quest,
    delete_quest, remove_prerequisite, restore_quest, set_due_dates, set_quest_tags,
    update_quest,
};

#[cfg(test)]
mod tag_edit_tests {
    use super::TagEdit;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn keeps_order_and_appends_new() {
        let e = TagEdit { add: v(&["c", "a", " d "]), remove: v(&["b"]) };
        assert_eq!(e.apply(v(&["a", "b", "x"])), v(&["a", "x", "c", "d"]));
    }

    #[test]
    fn add_wins_when_both_sides_name_a_tag() {
        let e = TagEdit { add: v(&["a"]), remove: v(&["a"]) };
        assert_eq!(e.apply(v(&["a"])), v(&["a"]));
    }

    #[test]
    fn missing_fields_mean_nothing() {
        let e: TagEdit = serde_json::from_str(r#"{"add":["x"]}"#).unwrap();
        assert_eq!(e.apply(v(&["y"])), v(&["y", "x"]));
    }
}

/// DEV-432: 문서 잠금 — 서로 다른 문서는 기다리지 않고, 같은 문서는 줄 서서 둘 다 남는다.
#[cfg(test)]
mod doc_lock_tests {
    use crate::lock::{DocKey, GuildMode};
    use crate::store::Store;
    use std::time::Duration;

    async fn guild(label: &str) -> (std::path::PathBuf, Store) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-doclock-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        (dir, store)
    }

    async fn quest(store: &Store, title: &str) -> crate::models::QuestRow {
        super::create_quest(
            store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: title.into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap()
    }

    async fn comment(store: &Store, slug: &str, body: &str) {
        super::comments::add_comment_entry(store, slug, "admin".into(), body.into(), None, false)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_comment_on_another_quest_does_not_wait() {
        let (dir, store) = guild("other-quest").await;
        let a = quest(&store, "A").await;
        let b = quest(&store, "B").await;

        // 누가 A 를 고치는 중이다.
        let held = store.lock_docs(&[DocKey::quest(&a.quest_id)]).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), comment(&store, &b.quest_id, "B 에 댓글"))
            .await
            .expect("다른 퀘스트를 고치는 동안 B 댓글이 기다렸다");
        drop(held);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn comments_on_the_same_quest_queue_up_and_both_stay() {
        let (dir, store) = guild("same-quest").await;
        let a = quest(&store, "A").await;

        let held = store.lock_docs(&[DocKey::quest(&a.quest_id)]).await.unwrap();
        let (s1, s2) = (store.clone(), store.clone());
        let (k1, k2) = (a.quest_id.clone(), a.quest_id.clone());
        let one = tokio::spawn(async move { comment(&s1, &k1, "하나").await });
        let two = tokio::spawn(async move { comment(&s2, &k2, "둘").await });
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!one.is_finished() && !two.is_finished(), "같은 퀘스트를 고치는 동안 댓글이 끼어들었다");
        drop(held);
        one.await.unwrap();
        two.await.unwrap();

        let all = super::comments::list_comment_entries(&store, &a.quest_id).unwrap();
        let mut bodies: Vec<&str> = all.iter().map(|c| c.body.as_str()).collect();
        bodies.sort();
        assert_eq!(bodies, vec!["둘", "하나"], "BUG-287 의 유실이 돌아왔다");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 잠근 뒤 다시 알아낸 목록이 다르면 풀고 다시 잡는다. 계속 다르면 길드 독점으로.
    #[tokio::test]
    async fn a_list_that_changed_while_waiting_is_locked_again() {
        let (dir, store) = guild("resolve-again").await;
        let calls = std::sync::atomic::AtomicUsize::new(0);
        // 첫 번 알아낸 것과 둘째 번이 다르다 — 셋째 번부터는 같다.
        let g = super::lock_resolved(&store, || {
            let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move {
                let id = if n == 0 { "DEV-001" } else { "DEV-002" };
                Ok(vec![DocKey::quest(id)])
            }
        })
        .await
        .unwrap();
        assert_eq!(g.mode(), GuildMode::Shared);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 4, "다시 잡지 않았다");
        drop(g);

        let n = std::sync::atomic::AtomicUsize::new(0);
        let g = super::lock_resolved(&store, || {
            let i = n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move { Ok(vec![DocKey::new("quest", format!("DEV-{i:03}"))]) }
        })
        .await
        .unwrap();
        assert_eq!(g.mode(), GuildMode::Exclusive, "계속 바뀌는데 길드 독점으로 가지 않았다");
        drop(g);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 부모를 바꾸는 동안에는 옛 부모 · 새 부모 파일도 잠겨 있다 — 그 파일을 고치는 변경이 끼어들지 않는다.
    #[tokio::test]
    async fn changing_a_parent_waits_for_the_old_and_new_parent() {
        let (dir, store) = guild("parent-keys").await;
        let p1 = quest(&store, "옛 부모").await;
        let p2 = quest(&store, "새 부모").await;
        let c = quest(&store, "자식").await;
        super::change_parent(&store, c.id, crate::models::ChangeParentRequest { parent_quest_id: Some(p1.id) })
            .await
            .unwrap();

        for held_key in [&p1.quest_id, &p2.quest_id] {
            let held = store.lock_docs(&[DocKey::quest(held_key)]).await.unwrap();
            let s = store.clone();
            let target = if held_key == &p1.quest_id { p2.id } else { p1.id };
            let h = tokio::spawn(async move {
                super::change_parent(&s, c.id, crate::models::ChangeParentRequest { parent_quest_id: Some(target) }).await
            });
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(!h.is_finished(), "{held_key} 을 쥔 동안 부모 바꾸기가 끝났다");
            drop(held);
            h.await.unwrap().unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
