//! Auto 블록 렌더러 — quest 파일 body 안의 `<!-- openguild:auto-* -->` 마커 사이를
//! 매 mutation 시 재생성.
//!
//! 입력: 대상 quest 의 관계 (parent / sub-quests / prerequisites).
//! 출력: 마커 안에 들어갈 마크다운 (마커 자체는 포함 안 함 — 그건 [`QuestFile::serialize`] 가 넣음).
//!
//! 책임 분리:
//! - 본 모듈: 순수 렌더링 (입력 → 문자열). DB / 파일 IO 없음.
//! - 호출자 (services): index.db 에서 관계 fetch 해서 본 함수에 넘김.

/// 한 quest 의 관계 표시 입력. 표시 순서: title 이 작은 quest_id 우선이 아니라 호출자가 정렬.
#[derive(Debug, Clone)]
pub struct QuestRelations {
    /// 부모. None 이면 root quest.
    pub parent: Option<QuestRef>,
    /// 직계 sub-quests (현 quest 가 parent 인 quest 들).
    pub sub_quests: Vec<QuestRef>,
    /// 선행 prerequisites.
    pub prerequisites: Vec<QuestRef>,
}

/// auto 블록에서 quest 한 개를 표시할 때 필요한 정보.
#[derive(Debug, Clone)]
pub struct QuestRef {
    pub quest_id: String,
    pub title: String,
}

impl QuestRef {
    pub fn new(quest_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            quest_id: quest_id.into(),
            title: title.into(),
        }
    }
}

/// Quest 의 관계들로 auto 블록 본문 생성 (마커 제외).
///
/// 결과는 줄바꿈으로 끝남. 빈 섹션은 "(없음)" 으로 표시.
/// 모든 섹션이 비어있고 parent 가 없으면 빈 문자열 반환 (auto 블록 자체를 비울 수도 있음 — 호출자 결정).
pub fn render(rel: &QuestRelations) -> String {
    let mut out = String::new();

    if let Some(p) = &rel.parent {
        out.push_str("## Parent\n");
        out.push_str(&format_link(p));
        out.push_str("\n\n");
    }

    out.push_str("## Sub-quests\n");
    if rel.sub_quests.is_empty() {
        out.push_str("- (없음)\n");
    } else {
        for q in &rel.sub_quests {
            out.push_str("- ");
            out.push_str(&format_link_item(q));
            out.push('\n');
        }
    }
    out.push('\n');

    out.push_str("## Prerequisites\n");
    if rel.prerequisites.is_empty() {
        out.push_str("- (없음)\n");
    } else {
        for q in &rel.prerequisites {
            out.push_str("- ");
            out.push_str(&format_link_item(q));
            out.push('\n');
        }
    }

    out
}

/// `[DEV-001](DEV-001.md) — 제목` 형식의 한 줄.
fn format_link_item(q: &QuestRef) -> String {
    format!("[{0}]({0}.md) — {1}", q.quest_id, q.title)
}

/// `[DEV-001](DEV-001.md) — 제목` 형식 (단일 link 용; 줄바꿈 없음).
fn format_link(q: &QuestRef) -> String {
    format_link_item(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(id: &str, title: &str) -> QuestRef {
        QuestRef::new(id, title)
    }

    #[test]
    fn root_with_no_subs_or_prereqs() {
        let rel = QuestRelations {
            parent: None,
            sub_quests: vec![],
            prerequisites: vec![],
        };
        let s = render(&rel);
        assert!(!s.contains("Parent"));
        assert!(s.contains("## Sub-quests\n- (없음)"));
        assert!(s.contains("## Prerequisites\n- (없음)"));
    }

    #[test]
    fn root_with_subs() {
        let rel = QuestRelations {
            parent: None,
            sub_quests: vec![r("DEV-002", "api 어댑터"), r("DEV-003", "Tauri init")],
            prerequisites: vec![],
        };
        let s = render(&rel);
        assert!(!s.contains("## Parent"));
        assert!(s.contains("[DEV-002](DEV-002.md) — api 어댑터"));
        assert!(s.contains("[DEV-003](DEV-003.md) — Tauri init"));
        assert!(s.contains("Prerequisites\n- (없음)"));
    }

    #[test]
    fn child_with_parent_and_prereqs() {
        let rel = QuestRelations {
            parent: Some(r("DEV-001", "Tauri desktop")),
            sub_quests: vec![],
            prerequisites: vec![r("DEV-002", "api 어댑터"), r("DEV-003", "Tauri init")],
        };
        let s = render(&rel);
        assert!(s.contains("## Parent\n[DEV-001](DEV-001.md) — Tauri desktop"));
        assert!(s.contains("Sub-quests\n- (없음)"));
        assert!(s.contains("[DEV-002](DEV-002.md) — api 어댑터"));
        assert!(s.contains("[DEV-003](DEV-003.md) — Tauri init"));
    }

    #[test]
    fn parent_section_comes_first() {
        let rel = QuestRelations {
            parent: Some(r("DEV-001", "p")),
            sub_quests: vec![r("DEV-010", "s")],
            prerequisites: vec![r("DEV-020", "pr")],
        };
        let s = render(&rel);
        let parent_pos = s.find("## Parent").unwrap();
        let subs_pos = s.find("## Sub-quests").unwrap();
        let prereq_pos = s.find("## Prerequisites").unwrap();
        assert!(parent_pos < subs_pos);
        assert!(subs_pos < prereq_pos);
    }

    #[test]
    fn output_ends_with_newline() {
        let rel = QuestRelations {
            parent: None,
            sub_quests: vec![r("DEV-002", "x")],
            prerequisites: vec![],
        };
        let s = render(&rel);
        assert!(s.ends_with('\n'));
    }
}
