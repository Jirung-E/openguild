//! REQ-008: 본문 cross-link(`[[...]]`) 토큰 추출 — backlink 역인덱스용.
//!
//! **문법은 프론트엔드와 반드시 같아야 한다.** 렌더(파랑/빨강 링크)는
//! `gui/frontend/src/lib/stores/questIndex.ts` + `MarkdownView.svelte` 가 하고,
//! backlink 색인은 여기가 한다. 두 곳이 갈라지면 "본문엔 링크로 보이는데
//! backlink 엔 안 잡히는"(혹은 그 반대) 상태가 된다. 아래 규칙은 그쪽에서
//! 그대로 옮긴 것이고, 바꿀 일이 생기면 **양쪽을 같이** 고쳐야 한다.
//!
//! - 토큰: `[[` + 대괄호/개행이 아닌 1~64자 + `]]`
//!   (BUG-156: 규칙 slug 는 공백을 포함할 수 있어 공백은 허용, 개행만 배제)
//! - 접두 `kind:` 별칭 — quest/q, campaign/c, rule/rules/r, book/library/lib
//!   (DEV-219). 접두는 대소문자 무시.
//! - 접두가 없으면 kind 미상 — 호출측이 실재 문서와 대조해 결정한다.

use std::sync::LazyLock;

use regex::Regex;

/// 문서 종류. 프론트의 `Kind` 와 같은 집합.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocKind {
    Quest,
    Campaign,
    Rule,
    Book,
}

impl DocKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DocKind::Quest => "quest",
            DocKind::Campaign => "campaign",
            DocKind::Rule => "rule",
            DocKind::Book => "book",
        }
    }
}

/// DEV-219 접두 별칭 → kind. 프론트 `KIND_ALIASES` 미러.
fn kind_from_alias(prefix: &str) -> Option<DocKind> {
    match prefix.to_lowercase().as_str() {
        "quest" | "q" => Some(DocKind::Quest),
        "campaign" | "c" => Some(DocKind::Campaign),
        "rule" | "rules" | "r" => Some(DocKind::Rule),
        "book" | "library" | "lib" => Some(DocKind::Book),
        _ => None,
    }
}

/// 추출된 토큰 하나. `kind` 가 `None` 이면 접두 없는 bare 토큰.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossLink {
    pub kind: Option<DocKind>,
    pub id: String,
}

/// 프론트 `CROSS_LINK_RE` 미러. 대괄호/개행 제외 1~64자.
static TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\[\]\n\r]{1,64})\]\]").expect("cross-link regex"));

/// 본문에서 cross-link 토큰을 모두 뽑는다. 등장 순서 유지, 중복은 그대로
/// (중복 제거는 색인 측 PRIMARY KEY 가 처리한다).
pub fn extract(body: &str) -> Vec<CrossLink> {
    TOKEN_RE
        .captures_iter(body)
        .filter_map(|c| {
            let raw = c.get(1)?.as_str().trim();
            if raw.is_empty() {
                return None;
            }
            // 프론트 `parseCrossLinkToken` 미러 — 첫 `:` 앞이 알려진 별칭일 때만
            // 접두로 인정한다. 그 외(예: `http://…`)는 통째로 id 로 본다.
            if let Some(i) = raw.find(':')
                && i > 0
                && let Some(kind) = kind_from_alias(&raw[..i])
            {
                let id = raw[i + 1..].trim();
                if id.is_empty() {
                    return None;
                }
                return Some(CrossLink { kind: Some(kind), id: id.to_string() });
            }
            Some(CrossLink { kind: None, id: raw.to_string() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(body: &str) -> Vec<(Option<DocKind>, String)> {
        extract(body).into_iter().map(|c| (c.kind, c.id)).collect()
    }

    #[test]
    fn extracts_bare_tokens() {
        assert_eq!(
            ids("앞 [[DEV-033]] 중간 [[C-001]] 뒤"),
            vec![(None, "DEV-033".into()), (None, "C-001".into())]
        );
    }

    #[test]
    fn extracts_namespaced_tokens() {
        assert_eq!(
            ids("[[quest:DEV-1]] [[c:C-2]] [[rules:my-rule]] [[library:BOOK-3]]"),
            vec![
                (Some(DocKind::Quest), "DEV-1".into()),
                (Some(DocKind::Campaign), "C-2".into()),
                (Some(DocKind::Rule), "my-rule".into()),
                (Some(DocKind::Book), "BOOK-3".into()),
            ]
        );
    }

    /// BUG-156: 규칙 slug 는 공백을 포함할 수 있다.
    #[test]
    fn allows_spaces_in_slug() {
        assert_eq!(ids("[[코딩 규칙]]"), vec![(None, "코딩 규칙".into())]);
    }

    /// 알려지지 않은 접두는 접두가 아니라 id 의 일부다.
    #[test]
    fn unknown_prefix_is_part_of_id() {
        assert_eq!(ids("[[http://x]]"), vec![(None, "http://x".into())]);
    }

    /// 개행을 가로지르는 `[[` … `]]` 는 매칭되지 않는다(문단 간 오탐 방지).
    #[test]
    fn does_not_match_across_newlines() {
        assert!(extract("[[DEV-1\nDEV-2]]").is_empty());
    }

    #[test]
    fn ignores_empty_and_overlong_tokens() {
        assert!(extract("[[]]").is_empty());
        let long = "x".repeat(65);
        assert!(extract(&format!("[[{long}]]")).is_empty());
    }

    #[test]
    fn case_insensitive_prefix() {
        assert_eq!(ids("[[Quest:DEV-1]] [[LIB:BOOK-2]]"),
            vec![(Some(DocKind::Quest), "DEV-1".into()), (Some(DocKind::Book), "BOOK-2".into())]);
    }
}
