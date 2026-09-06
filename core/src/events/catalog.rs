//! DEV-374: 내부 mutation ↔ 공개 이벤트 대조표, 그리고 **누락 검사**.
//!
//! 새 mutation 을 추가하고 emit 을 빠뜨리면 그 이벤트만 **조용히 구독 불가**가
//! 된다. 아무도 모른다. 그래서 검사로 막는다 — `journal::append` 호출은 이미
//! 54곳에 강제돼 있으므로(빠뜨리면 복구가 깨진다) 좋은 기준점이 된다.
//!
//! # 세 갈래로 나누는 이유
//!
//! 1단계는 퀘스트+댓글만 낸다(admin 확정). 나머지 40여 함수를 전부 "누락" 으로
//! 잡으면 **검사가 처음부터 빨간불**이라 아무도 안 본다. 그래서:
//!
//! - [`Status::Emitted`] — 지금 낸다.
//! - [`Status::Planned`] — 낼 예정. 아직 안 냈다.
//! - [`Status::Excluded`] — **일부러 안 낸다.** 이유를 반드시 적는다.
//!
//! 검사는 **어느 갈래에도 없는 op** 만 실패시킨다. 그래야 "빠뜨린 것" 과
//! "일부러 뺀 것" 이 구분되고, 검사가 제 일을 한다.

use super::names as ev;

#[derive(Debug, Clone, Copy)]
pub enum Status {
    /// 이 op 이 내는 공개 이벤트들. 하나가 둘로 갈릴 수 있다
    /// (`toggle_comment_resolved` → resolved / unresolved).
    Emitted(&'static [&'static str]),
    /// 낼 예정 — 1단계 범위 밖.
    Planned,
    /// 일부러 안 낸다. 문자열은 **이유**다.
    Excluded(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Entry {
    /// `journal::append` 에 넘기는 내부 op 이름.
    pub op: &'static str,
    pub status: Status,
}

const fn e(op: &'static str, status: Status) -> Entry {
    Entry { op, status }
}

pub const CATALOG: &[Entry] = &[
    // ── 퀘스트 — 1단계 ──────────────────────────────────
    e("create_quest", Status::Emitted(&[ev::QUEST_CREATED])),
    e("update_quest", Status::Emitted(&[ev::QUEST_UPDATED])),
    e("delete_quest", Status::Emitted(&[ev::QUEST_DELETED])),
    e("restore_quest", Status::Emitted(&[ev::QUEST_RESTORED])),
    e(
        "change_status",
        Status::Emitted(&[ev::QUEST_STATUS_CHANGED]),
    ),
    e(
        "change_quest_type",
        Status::Emitted(&[ev::QUEST_TYPE_CHANGED]),
    ),
    e(
        "change_parent",
        Status::Emitted(&[ev::QUEST_PARENT_CHANGED]),
    ),
    e("set_quest_tags", Status::Emitted(&[ev::QUEST_TAGS_CHANGED])),
    e("set_due_dates", Status::Emitted(&[ev::QUEST_DUE_CHANGED])),
    e(
        "add_prerequisite",
        Status::Emitted(&[ev::QUEST_PREREQ_ADDED]),
    ),
    e(
        "remove_prerequisite",
        Status::Emitted(&[ev::QUEST_PREREQ_REMOVED]),
    ),
    // ── 댓글(퀘스트) — 1단계 ────────────────────────────
    e("add_comment_entry", Status::Emitted(&[ev::COMMENT_ADDED])),
    e(
        "update_comment_entry",
        Status::Emitted(&[ev::COMMENT_UPDATED]),
    ),
    e(
        "delete_comment_entry",
        Status::Emitted(&[ev::COMMENT_DELETED]),
    ),
    e(
        "toggle_comment_resolved",
        Status::Emitted(&[ev::COMMENT_RESOLVED, ev::COMMENT_UNRESOLVED]),
    ),
    e(
        "toggle_comment_pinned",
        Status::Emitted(&[ev::COMMENT_PINNED, ev::COMMENT_UNPINNED]),
    ),
    e(
        "toggle_comment_discussion",
        Status::Emitted(&[ev::COMMENT_DISCUSSION_ON, ev::COMMENT_DISCUSSION_OFF]),
    ),
    e(
        "toggle_comment_reaction",
        Status::Emitted(&[ev::COMMENT_REACTION_CHANGED]),
    ),
    // ── 일부러 안 내는 것 ────────────────────────────────
    e(
        "set_comments",
        Status::Excluded(
            "댓글 파일 전체를 통째로 덮어쓰는 내부 경로다. \
             무엇이 달라졌는지 알 수 없어 이벤트로 만들 수 없다 — \
             개별 변경은 add/update/delete/toggle 이 낸다.",
        ),
    ),
    e(
        "set_rules",
        Status::Excluded("규칙 파일 일괄 쓰기 — 위 set_comments 와 같은 이유."),
    ),
    e(
        "set_memo",
        Status::Excluded(
            "메모는 **개인 기록**이다(남에게 안 보인다). \
             플러그인으로 내보내면 그 전제가 깨진다.",
        ),
    ),
    e(
        "set_campaign_memo",
        Status::Excluded("위 set_memo 와 같은 이유 — 개인 기록."),
    ),
    e("set_worklog_note", Status::Planned),
    // ── 낼 예정 (2단계 이후) ────────────────────────────
    e("create_campaign", Status::Planned),
    e("update_campaign", Status::Planned),
    e("delete_campaign", Status::Planned),
    e("campaign_link_quest", Status::Planned),
    e("campaign_unlink_quest", Status::Planned),
    e("campaign_checklist_add", Status::Planned),
    e("campaign_checklist_rm", Status::Planned),
    e("campaign_checklist_set", Status::Planned),
    e("set_campaign_banner", Status::Planned),
    e("clear_campaign_banner", Status::Planned),
    e("add_campaign_comment", Status::Planned),
    e("update_campaign_comment", Status::Planned),
    e("delete_campaign_comment", Status::Planned),
    e("toggle_campaign_comment_pinned", Status::Planned),
    e("toggle_campaign_comment_reaction", Status::Planned),
    e("create_book", Status::Planned),
    e("update_book", Status::Planned),
    e("delete_book", Status::Planned),
    e("set_book_tags", Status::Planned),
    e("create_folder", Status::Planned),
    e("delete_folder", Status::Planned),
    e("create_rule", Status::Planned),
    e("set_rule", Status::Planned),
    e("delete_rule", Status::Planned),
    e("rename_rule", Status::Planned),
    e("set_rule_tags", Status::Planned),
    e("save_attachment", Status::Planned),
    e("add_attachment", Status::Planned),
    e("remove_attachment", Status::Planned),
];

/// 카탈로그에 있는 op 인지.
pub fn known(op: &str) -> bool {
    CATALOG.iter().any(|e| e.op == op)
}

/// 이 op 이 내는 공개 이벤트들(없으면 빈 슬라이스).
pub fn events_of(op: &str) -> &'static [&'static str] {
    CATALOG
        .iter()
        .find(|e| e.op == op)
        .and_then(|e| match e.status {
            Status::Emitted(names) => Some(names),
            _ => None,
        })
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// `core/src/ops/*.rs` 에서 `journal::append(..., "op", ...)` 의 op 이름을 뽑는다.
    ///
    /// 소스를 훑는 이유: 이 검사가 지키려는 것이 "**코드에 새 mutation 이
    /// 생겼는데** 카탈로그에 안 들어왔다" 이기 때문이다. 런타임 값으로는
    /// 잡을 수 없다.
    fn ops_in_source() -> HashSet<String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ops");
        let mut found = HashSet::new();
        for entry in std::fs::read_dir(&dir).expect("ops 디렉터리").flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&p).expect("ops 파일 읽기");
            // `journal::append(` 뒤 첫 큰따옴표 문자열이 op 이름이다.
            // 인자가 여러 줄에 걸쳐 있어 정규식 한 줄로는 못 잡는다.
            let mut rest = src.as_str();
            while let Some(i) = rest.find("journal::append(") {
                rest = &rest[i + "journal::append(".len()..];
                let Some(q1) = rest.find('"') else { break };
                let Some(q2) = rest[q1 + 1..].find('"') else {
                    break;
                };
                found.insert(rest[q1 + 1..q1 + 1 + q2].to_string());
                rest = &rest[q1 + 1 + q2..];
            }
        }
        found
    }

    /// **이 검사가 이 파일의 존재 이유다.**
    ///
    /// 새 mutation 을 만들고 카탈로그에 안 넣으면 여기서 걸린다. 넣기만 하고
    /// emit 을 안 해도 되지만(`Planned`), **모르는 채로 지나가지는 못한다.**
    #[test]
    fn every_mutation_is_classified() {
        let found = ops_in_source();
        assert!(
            found.len() > 40,
            "소스에서 op 을 {}개밖에 못 찾았다 — 추출이 깨졌다",
            found.len()
        );
        let missing: Vec<_> = found.iter().filter(|op| !known(op)).collect();
        assert!(
            missing.is_empty(),
            "카탈로그에 없는 mutation: {missing:?}\n\
             새 mutation 을 추가했다면 events/catalog.rs 에 Emitted / Planned / \
             Excluded(이유) 중 하나로 넣으세요. 넣지 않으면 그 이벤트는 조용히 \
             구독 불가가 됩니다."
        );
    }

    /// 반대 방향 — 카탈로그에 있는데 소스에 없는 op(오타/삭제 잔재).
    #[test]
    fn catalog_has_no_stale_entries() {
        let found = ops_in_source();
        let stale: Vec<_> = CATALOG
            .iter()
            .map(|e| e.op)
            .filter(|op| !found.contains(*op))
            .collect();
        assert!(stale.is_empty(), "소스에 없는 카탈로그 항목: {stale:?}");
    }

    #[test]
    fn no_duplicate_ops() {
        let mut seen = HashSet::new();
        for e in CATALOG {
            assert!(seen.insert(e.op), "카탈로그 중복: {}", e.op);
        }
    }

    /// `Emitted` 가 가리키는 이름은 전부 `names::ALL` 에 있어야 한다.
    #[test]
    fn emitted_names_are_declared() {
        let all: HashSet<_> = ev::ALL.iter().copied().collect();
        for e in CATALOG {
            if let Status::Emitted(names) = e.status {
                for n in names {
                    assert!(all.contains(n), "{}: 선언되지 않은 이름 {n}", e.op);
                }
            }
        }
    }

    /// 선언한 이름은 전부 어딘가에서 나와야 한다 — 죽은 이름 금지.
    #[test]
    fn declared_names_are_all_emitted_by_something() {
        let emitted: HashSet<_> = CATALOG
            .iter()
            .filter_map(|e| match e.status {
                Status::Emitted(n) => Some(n),
                _ => None,
            })
            .flatten()
            .copied()
            .collect();
        let orphan: Vec<_> = ev::ALL.iter().filter(|n| !emitted.contains(**n)).collect();
        assert!(orphan.is_empty(), "아무도 내지 않는 이름: {orphan:?}");
    }

    /// 제외에는 반드시 이유가 있어야 한다 — "빠뜨린 것" 과 구분되지 않으면
    /// 이 갈래가 의미를 잃는다.
    #[test]
    fn exclusions_state_a_reason() {
        for e in CATALOG {
            if let Status::Excluded(why) = e.status {
                assert!(why.len() > 15, "{}: 제외 이유가 너무 짧다", e.op);
            }
        }
    }
}
