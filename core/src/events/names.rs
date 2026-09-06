//! DEV-374: 공개 이벤트 이름 — `리소스.동사`.
//!
//! 문자열 리터럴을 여기 모아 두는 이유는 둘이다:
//! - 오타가 컴파일 에러가 된다(emit 지점과 카탈로그가 같은 상수를 쓴다).
//! - "무엇이 공개돼 있나" 를 한 화면에서 볼 수 있다.
//!
//! **내부 함수와 1:1 일 필요가 없다.** `toggle_comment_resolved` 하나가
//! `comment.resolved` / `comment.unresolved` 둘로 갈리는 것이 그 예다 —
//! "댓글이 해결됨" 을 구독하는데 해제까지 잡히면 쓸모가 없다.

// ── 퀘스트 ──────────────────────────────────────────────
pub const QUEST_CREATED: &str = "quest.created";
pub const QUEST_UPDATED: &str = "quest.updated";
pub const QUEST_DELETED: &str = "quest.deleted";
pub const QUEST_RESTORED: &str = "quest.restored";
pub const QUEST_STATUS_CHANGED: &str = "quest.status_changed";
pub const QUEST_TYPE_CHANGED: &str = "quest.type_changed";
pub const QUEST_PARENT_CHANGED: &str = "quest.parent_changed";
pub const QUEST_TAGS_CHANGED: &str = "quest.tags_changed";
pub const QUEST_DUE_CHANGED: &str = "quest.due_changed";
pub const QUEST_PREREQ_ADDED: &str = "quest.prereq_added";
pub const QUEST_PREREQ_REMOVED: &str = "quest.prereq_removed";

// ── 댓글 (퀘스트) ────────────────────────────────────────
pub const COMMENT_ADDED: &str = "comment.added";
pub const COMMENT_UPDATED: &str = "comment.updated";
pub const COMMENT_DELETED: &str = "comment.deleted";
pub const COMMENT_RESOLVED: &str = "comment.resolved";
pub const COMMENT_UNRESOLVED: &str = "comment.unresolved";
pub const COMMENT_PINNED: &str = "comment.pinned";
pub const COMMENT_UNPINNED: &str = "comment.unpinned";
pub const COMMENT_DISCUSSION_ON: &str = "comment.discussion_on";
pub const COMMENT_DISCUSSION_OFF: &str = "comment.discussion_off";
pub const COMMENT_REACTION_CHANGED: &str = "comment.reaction_changed";

/// 1단계에 실제로 나가는 이벤트 전부. 와일드카드 매칭 검증과 문서에 쓴다.
pub const ALL: &[&str] = &[
    QUEST_CREATED,
    QUEST_UPDATED,
    QUEST_DELETED,
    QUEST_RESTORED,
    QUEST_STATUS_CHANGED,
    QUEST_TYPE_CHANGED,
    QUEST_PARENT_CHANGED,
    QUEST_TAGS_CHANGED,
    QUEST_DUE_CHANGED,
    QUEST_PREREQ_ADDED,
    QUEST_PREREQ_REMOVED,
    COMMENT_ADDED,
    COMMENT_UPDATED,
    COMMENT_DELETED,
    COMMENT_RESOLVED,
    COMMENT_UNRESOLVED,
    COMMENT_PINNED,
    COMMENT_UNPINNED,
    COMMENT_DISCUSSION_ON,
    COMMENT_DISCUSSION_OFF,
    COMMENT_REACTION_CHANGED,
];

/// 구독 패턴이 이벤트 이름과 맞는지.
///
/// 지원하는 형태 — `quest.created`(정확히), `quest.*`(리소스), `*.created`(동사),
/// `*`(전부). 앞에 `pre:` 가 붙으면 단계까지 가른다.
///
/// 와일드카드가 필요한 이유: 이벤트가 21종(앞으로 더 늘)이라 일일이 나열하게
/// 하면 플러그인 작성이 고통스럽고, **새 이벤트가 생겨도 기존 구독자가 못
/// 받는다**.
pub fn matches(pattern: &str, name: &str, phase: super::Phase) -> bool {
    let (want_phase, pat) = match pattern.strip_prefix("pre:") {
        Some(rest) => (super::Phase::Pre, rest),
        None => (super::Phase::Post, pattern),
    };
    if want_phase != phase {
        return false;
    }
    if pat == "*" {
        return true;
    }
    match (pat.split_once('.'), name.split_once('.')) {
        (Some((pr, pv)), Some((nr, nv))) => (pr == "*" || pr == nr) && (pv == "*" || pv == nv),
        _ => pat == name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Phase;

    #[test]
    fn exact_match_needs_same_phase() {
        assert!(matches("quest.created", QUEST_CREATED, Phase::Post));
        // 기본은 post — `pre:` 없이 pre 이벤트를 잡으면 안 된다.
        assert!(!matches("quest.created", QUEST_CREATED, Phase::Pre));
        assert!(matches("pre:quest.created", QUEST_CREATED, Phase::Pre));
        assert!(!matches("pre:quest.created", QUEST_CREATED, Phase::Post));
    }

    #[test]
    fn resource_wildcard() {
        assert!(matches("quest.*", QUEST_STATUS_CHANGED, Phase::Post));
        assert!(!matches("quest.*", COMMENT_ADDED, Phase::Post));
    }

    #[test]
    fn verb_wildcard_crosses_resources() {
        assert!(matches("*.created", QUEST_CREATED, Phase::Post));
        assert!(!matches("*.created", QUEST_UPDATED, Phase::Post));
    }

    #[test]
    fn star_catches_everything_in_its_phase() {
        for n in ALL {
            assert!(matches("*", n, Phase::Post), "{n}");
            assert!(!matches("*", n, Phase::Pre), "{n}");
            assert!(matches("pre:*", n, Phase::Pre), "{n}");
        }
    }

    /// 이름이 전부 `리소스.동사` 여야 와일드카드가 성립한다.
    #[test]
    fn every_name_is_resource_dot_verb() {
        for n in ALL {
            let (r, v) = n.split_once('.').unwrap_or_else(|| panic!("`.` 없음: {n}"));
            assert!(!r.is_empty() && !v.is_empty(), "{n}");
            assert!(!v.contains('.'), "동사에 `.` 가 또 있다: {n}");
        }
    }

    #[test]
    fn names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for n in ALL {
            assert!(seen.insert(*n), "중복: {n}");
        }
    }
}
