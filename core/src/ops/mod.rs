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
#[cfg(test)]
mod lock_coverage;
pub mod meta;
pub mod positions;
pub mod quests;
pub mod rules;
pub mod search;
pub mod worklog;

pub use counter::check_and_fix_counters;

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
