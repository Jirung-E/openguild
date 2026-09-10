//! DEV-012 / DEV-094: Quest 별 댓글 / 메모.
//!
//! - **댓글** (`.guild/quests/{slug}.comments.md`) — 팀 공유, git tracked.
//!   여러 사용자의 토론. DEV-094 부터 **entry 단위** — HTML 주석 마커
//!   (`<!-- og-comment id="..." ts="..." author="..." -->`) 로 각 entry 를
//!   구분, 본문 markdown 은 마커 다음 줄부터 다음 마커 직전 (또는 EOF) 까지.
//! - **메모** (`.guild/quests/{slug}.memo.md`) — 비공개, gitignored.
//!   본인만 보는 작업 메모. **단일 텍스트** (entry 분리 X).
//!
//! 파일이 진리원 — DB 캐시 없음.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::fs::write_atomic;
use super::GuildPaths;

/// DEV-094: 한 댓글 entry. 작성 시각 / 작성자 / 본문.
///
/// `id` 는 한 quest 안에서 monotonic 증가 (재사용 안 함) — 삭제된 id 도 비워둠.
/// 파일에 직접 적힌 값이 진리원. JSON serialize 시 snake_case 그대로.
///
/// `parent_id` (Some) → 다른 entry 에 대한 답글 (threaded reply, DEV-094 후속).
/// None → top-level. 답글의 답글도 일단 모두 root 부모의 직접 자식으로 flatten
/// (1-level threading) — GitHub PR 댓글과 같은 모델. 자식은 자기 parent 가
/// 삭제돼도 살아남으며, 그 경우 frontend 가 "(삭제된 댓글에 대한 답글)" 로 표시.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommentEntry {
    pub id: u64,
    /// ISO 8601 + TZ offset (`2026-06-02T01:23:45+09:00`).
    pub ts: String,
    /// 작성자 이름 (자유 문자열, 빈 값 허용).
    #[serde(default)]
    pub author: String,
    /// markdown 본문. 앞뒤 공백 trim 된 상태.
    pub body: String,
    /// 답글의 경우 부모 entry id. 마커의 `reply_to="N"` attr 에서 추출.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<u64>,
    /// DEV-108: 이모지 반응 — 활성 이모지 목록. 마커의 `reactions="👍,✅"`.
    /// single-user 단계 = 이모지당 on/off (목록 포함 여부). multi-user
    /// (DEV-021) 진입 시 user 별 분리로 확장.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<String>,
    /// DEV-142: 토론(discussion) 댓글 표시. 마커의 `discussion="true"`.
    /// discussion 이면서 `resolved=false` 인 댓글이 하나라도 있으면 quest 를
    /// 완료(counts_as_done) 상태로 전환 불가 (ops::quests::change_status 게이트).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub discussion: bool,
    /// DEV-142: 토론 해결 여부. 마커의 `resolved="true"`. discussion 이 아닐 땐 무의미.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resolved: bool,
    /// DEV-234: 상단 고정(pin) 여부. 마커의 `pinned="true"`. quest/campaign
    /// 댓글 둘 다 지원 — discussion 과 달리 quest 전용 게이트 없음.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    /// DEV-182: 본문 편집 시각. 마커의 `edited_at="..."`. 생성 후 한 번도
    /// 수정 안 했으면 None(= "(편집됨)" 표시 안 함). 외부(파일 직접) 편집은
    /// per-entry 귀속이 불가해 반영 안 됨 — API 를 통한 수정만 기록.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
}

/// DEV-094: 마커 패턴 — `<!-- og-comment id="..." ts="..." author="..." -->`.
///
/// 키 순서는 id → ts → author 권장이지만 파서는 순서 무관 — 정규식이 각 키를
/// 개별 캡처. author 키가 없으면 빈 문자열로 처리 (구 entry 안전).
fn entry_marker_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r#"(?m)^<!--\s*og-comment\s+(?P<attrs>[^>]*?)\s*-->\s*$"#,
        )
        .expect("og-comment marker regex")
    })
}

/// BUG-283: 본문 줄이 **마커로 오인될 수 있는가**.
///
/// 앞의 `\` 는 이스케이프 표시이므로 판정에서 걷어낸 뒤 본다 —
/// `\<!-- og-comment ... -->` 도 (한 번 풀면) 마커가 되는 줄이다.
fn looks_like_marker(line: &str) -> bool {
    let bare = line.trim_start_matches('\\');
    marker_shape_re().is_match(bare)
}

/// 줄 하나만 보는 마커 모양 검사 — `entry_marker_re` 와 같은 모양이되
/// 여러 줄 앵커가 없다.
fn marker_shape_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r#"^<!--\s*og-comment\s+[^>]*?\s*-->\s*$"#)
            .expect("og-comment shape regex")
    })
}

/// BUG-283: 본문을 파일에 넣기 전에 **구분자를 무력화**한다.
///
/// 사이드카는 HTML 주석 마커로 entry 를 가르는데([[DEV-094]]) 본문은 그대로
/// 흘려 쓰고 있었다. 그래서 본문에 마커 모양의 줄을 넣으면 다음 읽기 때 **진짜
/// 마커가 됐다** — 작성자·시각·id 를 전부 지정한 entry 를 위조할 수 있었다.
/// 서버 모드에서는 댓글을 달 수 있는 누구나 남의 이름으로 글을 만들 수 있다.
///
/// 거부(reject)는 접었다. 이 저장소의 문서 자체가 이 마커를 인용하고 있고,
/// 포맷을 설명하는 댓글을 못 쓰게 하는 건 과하다. 대신 `\` 를 하나 덧붙여
/// 왕복시킨다 — 읽을 때 [`unescape_body`] 가 정확히 되돌린다.
fn escape_body(body: &str) -> String {
    if !body.lines().any(looks_like_marker) {
        return body.to_string(); // 흔한 경우 — 복사를 안 만든다.
    }
    body.lines()
        .map(|l| {
            if looks_like_marker(l) {
                format!("\\{l}")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// [`escape_body`] 의 역. 마커가 될 수 있는 줄에서만 `\` 를 하나 뗀다 —
/// 본문에 원래 있던 `\` 를 함부로 건드리면 안 된다.
fn unescape_body(body: &str) -> String {
    if !body.lines().any(|l| l.starts_with('\\') && looks_like_marker(l)) {
        return body.to_string();
    }
    body.lines()
        .map(|l| {
            if l.starts_with('\\') && looks_like_marker(l) {
                l[1..].to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 한 attrs 문자열 (`id="1" ts="..." author="x"`) 에서 키 값 1개 추출.
/// 미존재 시 None.
fn extract_attr(attrs: &str, key: &str) -> Option<String> {
    // key="value" — value 안 `"` 는 escape 되지 않음 (간단 형식). 사용자가
    // author 에 `"` 를 넣으면 깨지므로 직렬화 시 escape (sanitize_attr).
    let pat = format!(r#"{key}\s*=\s*"([^"]*)""#);
    let re = regex::Regex::new(&pat).ok()?;
    re.captures(attrs)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// 직렬화 시 attr value 의 `"` 를 안전한 형태로 (간단히 ' 로 교체).
fn sanitize_attr(s: &str) -> String {
    s.replace('"', "'")
}

/// DEV-108 (누가 반응했는지 호버 표시): reaction 항목 문자열을 (emoji, authors)
/// 로 분해. 형식 = `emoji` (legacy) 또는 `emoji:author1|author2`.
/// 구분자 `:` / `|` 는 emoji / author 에 들어갈 수 없음 (toggle 에서 검증).
pub fn split_reaction(s: &str) -> (String, Vec<String>) {
    match s.split_once(':') {
        Some((emoji, rest)) => (
            emoji.to_string(),
            rest.split('|')
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect(),
        ),
        None => (s.to_string(), Vec::new()),
    }
}

/// (emoji, authors) → reaction 항목 문자열. authors 가 비면 emoji 만 (legacy 호환).
pub fn join_reaction(emoji: &str, authors: &[String]) -> String {
    if authors.is_empty() {
        emoji.to_string()
    } else {
        format!("{emoji}:{}", authors.join("|"))
    }
}

/// 텍스트 → entry 목록.
///
/// 마커가 0개면:
/// - 본문이 비어있으면 빈 vec.
/// - 본문이 있으면 **legacy 단일 entry** 로 인식 (id=1, ts="" / author=""
///   body=전체 trim). 첫 mutation 시 정식 마커로 재serialize.
pub fn parse_entries(text: &str) -> Vec<CommentEntry> {
    let re = entry_marker_re();
    let mut markers: Vec<(usize, usize, String)> = Vec::new();
    for m in re.captures_iter(text) {
        let whole = m.get(0).expect("whole match");
        let attrs = m
            .name("attrs")
            .map(|x| x.as_str().to_string())
            .unwrap_or_default();
        markers.push((whole.start(), whole.end(), attrs));
    }

    if markers.is_empty() {
        let body = text.trim();
        if body.is_empty() {
            return Vec::new();
        }
        return vec![CommentEntry {
            id: 1,
            ts: String::new(),
            author: String::new(),
            body: unescape_body(body),
            parent_id: None,
            reactions: Vec::new(),
            discussion: false,
            resolved: false,
            pinned: false,
            edited_at: None,
        }];
    }

    let mut out: Vec<CommentEntry> = Vec::with_capacity(markers.len());
    for i in 0..markers.len() {
        let (_, hdr_end, attrs) = &markers[i];
        let body_end = if i + 1 < markers.len() {
            markers[i + 1].0
        } else {
            text.len()
        };
        let body = unescape_body(text[*hdr_end..body_end].trim());

        // id 가 없거나 파싱 실패면 skip (corrupted entry).
        let Some(id_str) = extract_attr(attrs, "id") else { continue };
        let Ok(id) = id_str.parse::<u64>() else { continue };
        let ts = extract_attr(attrs, "ts").unwrap_or_default();
        let author = extract_attr(attrs, "author").unwrap_or_default();
        // 답글이면 `reply_to="N"`. 없거나 파싱 실패면 None.
        let parent_id = extract_attr(attrs, "reply_to")
            .and_then(|s| s.parse::<u64>().ok());
        // DEV-108: `reactions="👍,✅"` — 콤마 구분, 빈 항목 제거.
        let reactions = extract_attr(attrs, "reactions")
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        // DEV-142: `discussion="true"` / `resolved="true"` — 그 외 값/부재는 false.
        let discussion = extract_attr(attrs, "discussion").as_deref() == Some("true");
        let resolved = extract_attr(attrs, "resolved").as_deref() == Some("true");
        // DEV-234: `pinned="true"` — 그 외 값/부재는 false.
        let pinned = extract_attr(attrs, "pinned").as_deref() == Some("true");
        // DEV-182: `edited_at="..."` — 없으면 None(수정된 적 없음).
        let edited_at = extract_attr(attrs, "edited_at");
        out.push(CommentEntry {
            id,
            ts,
            author,
            body,
            parent_id,
            reactions,
            discussion,
            resolved,
            pinned,
            edited_at,
        });
    }
    out
}

/// entry 목록 → 직렬화된 markdown 텍스트.
///
/// 각 entry 사이 빈 줄 1개. 마지막 entry 끝에 trailing newline 1개.
/// 빈 vec → 빈 문자열.
///
/// `parent_id` 가 Some 이면 마커에 `reply_to="N"` attribute 추가.
pub fn serialize_entries(entries: &[CommentEntry]) -> String {
    let mut out = String::new();
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let reply_attr = match e.parent_id {
            Some(pid) => format!(" reply_to=\"{pid}\""),
            None => String::new(),
        };
        // DEV-108: reactions attr — 비어있으면 생략 (구 파일과 byte 동일 유지).
        let reactions_attr = if e.reactions.is_empty() {
            String::new()
        } else {
            format!(" reactions=\"{}\"", sanitize_attr(&e.reactions.join(",")))
        };
        // DEV-142: discussion / resolved — true 일 때만 출력 (구 파일 byte 호환).
        let discussion_attr = if e.discussion { " discussion=\"true\"" } else { "" };
        let resolved_attr = if e.resolved { " resolved=\"true\"" } else { "" };
        // DEV-234: pinned — 마찬가지로 true 일 때만 출력.
        let pinned_attr = if e.pinned { " pinned=\"true\"" } else { "" };
        // DEV-182: edited_at — Some 일 때만 출력(구 파일 byte 호환).
        let edited_attr = match &e.edited_at {
            Some(ts) => format!(" edited_at=\"{}\"", sanitize_attr(ts)),
            None => String::new(),
        };
        out.push_str(&format!(
            "<!-- og-comment id=\"{}\" ts=\"{}\" author=\"{}\"{}{}{}{}{}{} -->\n",
            e.id,
            sanitize_attr(&e.ts),
            sanitize_attr(&e.author),
            reply_attr,
            reactions_attr,
            discussion_attr,
            resolved_attr,
            pinned_attr,
            edited_attr,
        ));
        // BUG-283: 여기서 감싸지 않으면 본문이 자기 울타리를 넘는다.
        out.push_str(&escape_body(e.body.trim()));
        out.push('\n');
    }
    out
}

/// 파일에서 entry 목록 읽기. 파일 부재 시 빈 vec.
pub fn read_entries(paths: &GuildPaths, slug: &str) -> Result<Vec<CommentEntry>> {
    let raw = read_comments(paths, slug)?;
    Ok(raw.as_deref().map(parse_entries).unwrap_or_default())
}

/// entry 목록을 파일에 쓰기 (atomic). 빈 vec 이면 빈 파일로 (또는 삭제 — 일단 빈 파일).
pub fn write_entries(
    paths: &GuildPaths,
    slug: &str,
    entries: &[CommentEntry],
) -> Result<()> {
    let text = serialize_entries(entries);
    write_atomic(paths.comments_path(slug), &text)
}

// ─── DEV-100: path 기반 generic IO — quest / campaign 공용 ───

/// 임의 경로의 댓글 파일 → entry 목록. 부재 시 빈 vec.
pub fn read_entries_at(path: &std::path::Path) -> Result<Vec<CommentEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read comments: {}", path.display()))?;
    Ok(parse_entries(&s))
}

/// 임의 경로에 entry 목록 쓰기 (atomic).
pub fn write_entries_at(path: &std::path::Path, entries: &[CommentEntry]) -> Result<()> {
    write_atomic(path, &serialize_entries(entries))
}

/// 임의 경로의 단일 텍스트 (메모) 읽기. 부재 시 None.
pub fn read_text_at(path: &std::path::Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read: {}", path.display()))?;
    Ok(Some(s))
}

/// 임의 경로에 단일 텍스트 쓰기 (atomic).
pub fn write_text_at(path: &std::path::Path, content: &str) -> Result<()> {
    write_atomic(path, content)
}

/// 공개 댓글 파일 읽기. 부재 시 `Ok(None)`.
pub fn read_comments(paths: &GuildPaths, slug: &str) -> Result<Option<String>> {
    let p = paths.comments_path(slug);
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("failed to read comments: {}", p.display()))?;
    Ok(Some(s))
}

/// 공개 댓글 파일 쓰기 (atomic).
pub fn write_comments(paths: &GuildPaths, slug: &str, content: &str) -> Result<()> {
    write_atomic(paths.comments_path(slug), content)
}

/// 비공개 메모 파일 읽기. 부재 시 `Ok(None)`.
pub fn read_memo(paths: &GuildPaths, slug: &str) -> Result<Option<String>> {
    let p = paths.memo_path(slug);
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("failed to read memo: {}", p.display()))?;
    Ok(Some(s))
}

/// 비공개 메모 파일 쓰기 (atomic).
pub fn write_memo(paths: &GuildPaths, slug: &str, content: &str) -> Result<()> {
    write_atomic(paths.memo_path(slug), content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_paths(label: &str) -> (std::path::PathBuf, GuildPaths) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("og-comments-{label}-{ns}"));
        std::fs::create_dir_all(root.join(".guild/quests")).unwrap();
        (root.clone(), GuildPaths::new(root))
    }

    #[test]
    fn comments_roundtrip() {
        let (root, p) = fresh_paths("c-rt");
        assert!(read_comments(&p, "DEV-001").unwrap().is_none());
        write_comments(&p, "DEV-001", "# Discussion\n- LGTM").unwrap();
        assert_eq!(
            read_comments(&p, "DEV-001").unwrap().as_deref(),
            Some("# Discussion\n- LGTM")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn memo_roundtrip() {
        let (root, p) = fresh_paths("m-rt");
        assert!(read_memo(&p, "DEV-001").unwrap().is_none());
        write_memo(&p, "DEV-001", "TODO: 본문 정리").unwrap();
        assert_eq!(
            read_memo(&p, "DEV-001").unwrap().as_deref(),
            Some("TODO: 본문 정리")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ─── DEV-094: entry 파서 ───

    #[test]
    fn parse_empty_string_yields_empty_vec() {
        assert!(parse_entries("").is_empty());
        assert!(parse_entries("   \n  ").is_empty());
    }

    #[test]
    fn parse_legacy_unmarked_becomes_single_entry() {
        let s = "Just free markdown.\n\nMultiple lines.";
        let v = parse_entries(s);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, 1);
        assert_eq!(v[0].ts, "");
        assert_eq!(v[0].author, "");
        assert_eq!(v[0].body, "Just free markdown.\n\nMultiple lines.");
    }

    #[test]
    fn parse_marked_entries_extracts_all_fields() {
        let s = "<!-- og-comment id=\"1\" ts=\"2026-06-02T00:00:00+09:00\" author=\"alice\" -->\nFirst.\n\n<!-- og-comment id=\"2\" ts=\"2026-06-02T01:00:00+09:00\" author=\"\" -->\nSecond body\nwith newlines.\n";
        let v = parse_entries(s);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, 1);
        assert_eq!(v[0].author, "alice");
        assert_eq!(v[0].body, "First.");
        assert_eq!(v[1].id, 2);
        assert_eq!(v[1].author, "");
        assert_eq!(v[1].body, "Second body\nwith newlines.");
    }

    #[test]
    fn serialize_then_parse_roundtrip() {
        let entries = vec![
            CommentEntry {
                id: 1,
                ts: "2026-06-02T00:00:00+09:00".into(),
                author: "alice".into(),
                body: "Hello\nworld".into(),
                parent_id: None,
                reactions: Vec::new(),
                discussion: false,
                resolved: false,
                pinned: false,
            edited_at: None,
            },
            CommentEntry {
                id: 2,
                ts: "2026-06-02T01:00:00+09:00".into(),
                author: "".into(),
                body: "Second.".into(),
                parent_id: None,
                reactions: Vec::new(),
                discussion: false,
                resolved: false,
                pinned: false,
            edited_at: None,
            },
            // 답글: id=3 이 id=1 에 대한 reply.
            CommentEntry {
                id: 3,
                ts: "2026-06-02T02:00:00+09:00".into(),
                author: "bob".into(),
                body: "Reply to 1".into(),
                parent_id: Some(1),
                reactions: Vec::new(),
                discussion: false,
                resolved: false,
                pinned: false,
            edited_at: None,
            },
        ];
        let s = serialize_entries(&entries);
        let parsed = parse_entries(&s);
        assert_eq!(parsed, entries);
        // 직렬화 결과에 reply_to 가 들어 있어야.
        assert!(s.contains("reply_to=\"1\""));
    }

    /// DEV-108: reactions attr roundtrip + 빈 reactions 는 attr 생략.
    #[test]
    fn reactions_roundtrip() {
        let entries = vec![CommentEntry {
            id: 1,
            ts: "x".into(),
            author: "a".into(),
            body: "b".into(),
            parent_id: None,
            reactions: vec!["👍".into(), "✅".into()],
            discussion: false,
            resolved: false,
            pinned: false,
        edited_at: None,
        }];
        let s = serialize_entries(&entries);
        assert!(s.contains("reactions=\"👍,✅\""));
        assert_eq!(parse_entries(&s), entries);

        // 빈 reactions — attr 자체가 없어야 (구 파일과 호환).
        let none = vec![CommentEntry {
            id: 1,
            ts: "x".into(),
            author: "a".into(),
            body: "b".into(),
            parent_id: None,
            reactions: Vec::new(),
            discussion: false,
            resolved: false,
            pinned: false,
        edited_at: None,
        }];
        assert!(!serialize_entries(&none).contains("reactions"));
    }

    /// DEV-142: discussion / resolved attr roundtrip + 기본 false 는 attr 생략.
    #[test]
    fn discussion_resolved_roundtrip() {
        let entries = vec![CommentEntry {
            id: 1,
            ts: "x".into(),
            author: "a".into(),
            body: "토론 필요".into(),
            parent_id: None,
            reactions: Vec::new(),
            discussion: true,
            resolved: true,
            pinned: false,
        edited_at: None,
        }];
        let s = serialize_entries(&entries);
        assert!(s.contains("discussion=\"true\""));
        assert!(s.contains("resolved=\"true\""));
        assert_eq!(parse_entries(&s), entries);

        // 기본값 (false) — attr 자체가 없어야 (구 파일과 byte 호환).
        let plain = vec![CommentEntry {
            id: 1,
            ts: "x".into(),
            author: "a".into(),
            body: "b".into(),
            parent_id: None,
            reactions: Vec::new(),
            discussion: false,
            resolved: false,
            pinned: false,
        edited_at: None,
        }];
        let ps = serialize_entries(&plain);
        assert!(!ps.contains("discussion"));
        assert!(!ps.contains("resolved"));
    }

    /// DEV-234: pinned attr roundtrip + 기본 false 는 attr 생략 (구 파일 byte 호환).
    #[test]
    fn pinned_roundtrip() {
        let entries = vec![CommentEntry {
            id: 1,
            ts: "x".into(),
            author: "a".into(),
            body: "중요 결정".into(),
            parent_id: None,
            reactions: Vec::new(),
            discussion: false,
            resolved: false,
            pinned: true,
        edited_at: None,
        }];
        let s = serialize_entries(&entries);
        assert!(s.contains("pinned=\"true\""));
        assert_eq!(parse_entries(&s), entries);

        let plain = vec![CommentEntry {
            id: 1,
            ts: "x".into(),
            author: "a".into(),
            body: "b".into(),
            parent_id: None,
            reactions: Vec::new(),
            discussion: false,
            resolved: false,
            pinned: false,
        edited_at: None,
        }];
        assert!(!serialize_entries(&plain).contains("pinned"));
    }

    #[test]
    fn parse_corrupted_entry_without_id_skipped() {
        let s = "<!-- og-comment ts=\"x\" -->\nbody\n";
        // id 필수 — 없으면 skip.
        assert!(parse_entries(s).is_empty());
    }

    #[test]
    fn serialize_sanitizes_double_quotes_in_author() {
        let entries = vec![CommentEntry {
            id: 1,
            ts: "x".into(),
            author: "ali\"ce".into(),
            body: "body".into(),
            parent_id: None,
            reactions: Vec::new(),
            discussion: false,
            resolved: false,
            pinned: false,
        edited_at: None,
        }];
        let s = serialize_entries(&entries);
        // " → ' 로 치환되어 attribute 안전.
        assert!(s.contains("author=\"ali'ce\""));
    }

    #[test]
    fn parse_recognizes_reply_to_marker() {
        let s = "<!-- og-comment id=\"5\" ts=\"x\" author=\"\" reply_to=\"3\" -->\nReply body.\n";
        let v = parse_entries(s);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, 5);
        assert_eq!(v[0].parent_id, Some(3));
    }

    #[test]
    fn parse_top_level_has_no_parent_id() {
        let s = "<!-- og-comment id=\"1\" ts=\"x\" author=\"\" -->\nHi.\n";
        let v = parse_entries(s);
        assert_eq!(v[0].parent_id, None);
    }

    #[test]
    fn comments_and_memo_independent_paths() {
        let (root, p) = fresh_paths("indep");
        write_comments(&p, "BUG-007", "public").unwrap();
        write_memo(&p, "BUG-007", "private").unwrap();
        assert_eq!(read_comments(&p, "BUG-007").unwrap().as_deref(), Some("public"));
        assert_eq!(read_memo(&p, "BUG-007").unwrap().as_deref(), Some("private"));
        // 두 파일이 서로 다른 경로.
        assert_ne!(p.comments_path("BUG-007"), p.memo_path("BUG-007"));
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── BUG-283: 본문이 자기 울타리를 못 넘는다 ─────────────

    fn one(body: &str) -> Vec<CommentEntry> {
        vec![CommentEntry {
            id: 1,
            ts: "2026-01-01T00:00:00+09:00".into(),
            author: "kim".into(),
            body: body.into(),
            parent_id: None,
            reactions: Vec::new(),
            discussion: false,
            resolved: false,
            pinned: false,
            edited_at: None,
        }]
    }

    /// 시험이 실제로 **마커를 줄 맨 앞에** 두는지부터 본다.
    ///
    /// 처음 쓴 시험은 Rust 줄 이음이 뭉개져 마커가 들여쓰기된 채 들어갔고,
    /// 정규식은 줄 맨 앞만 보므로 **아무것도 안 보면서 통과했다.** 되돌림
    /// 시험에서야 드러났다. 전제를 먼저 못 박아 둔다.
    #[test]
    fn the_forgery_fixture_really_starts_a_line() {
        assert!(
            forged_body().lines().any(looks_like_marker),
            "시험 본문에 마커가 없다 — 이 시험은 아무것도 안 보고 있다"
        );
    }

    fn forged_body() -> String {
        [
            "평범한 댓글입니다.",
            r#"<!-- og-comment id="99" ts="2020-01-01T00:00:00+09:00" author="admin" -->"#,
            "위조 시도",
        ]
        .join("\n")
    }

    /// **이 결함의 본체.** 본문에 마커를 쓰면 entry 가 하나 더 생기고, 그
    /// entry 의 작성자·시각·id 가 전부 본문에 적힌 값이 됐다 — 서버 모드에서는
    /// 댓글을 달 수 있는 누구나 남의 이름으로 글을 만들 수 있었다.
    #[test]
    fn a_marker_in_the_body_cannot_forge_an_entry() {
        let body = forged_body();
        let round = parse_entries(&serialize_entries(&one(&body)));

        assert_eq!(round.len(), 1, "본문이 entry 로 갈라졌다: {round:#?}");
        assert_eq!(round[0].author, "kim", "작성자가 위조됐다");
        assert_eq!(round[0].id, 1, "id 가 위조됐다");
        assert_eq!(round[0].ts, "2026-01-01T00:00:00+09:00", "시각이 위조됐다");
        assert_eq!(round[0].body, body, "본문이 원문과 다르다(왕복 실패)");
    }

    /// 이스케이프의 이스케이프. 사용자가 `\<!-- og-comment ... -->` 라고 쓴
    /// 것도 그대로 돌아와야 한다 — 안 그러면 저장할 때마다 `\` 가 하나씩 는다.
    #[test]
    fn an_escaped_marker_in_the_body_also_round_trips() {
        let body = [
            "설명:",
            r#"\<!-- og-comment id="1" ts="x" author="y" -->"#,
        ]
        .join("\n");
        let round = parse_entries(&serialize_entries(&one(&body)));
        assert_eq!(round.len(), 1);
        assert_eq!(round[0].body, body);

        let twice = parse_entries(&serialize_entries(&round));
        assert_eq!(twice[0].body, body, "저장할 때마다 백슬래시가 는다");
    }

    /// 마커가 **아닌** 주석은 손대지 않는다. 과하게 잡으면 평범한 본문이 망가진다.
    #[test]
    fn ordinary_html_comments_are_left_alone() {
        for body in [
            "<!-- 그냥 주석 -->",
            "<!-- og-comments 는 복수형이라 마커가 아니다 -->",
            r#"앞에 글자가 있으면 <!-- og-comment id="1" ts="x" author="y" -->"#,
            r"\평범한 백슬래시로 시작하는 줄",
        ] {
            let text = serialize_entries(&one(body));
            // **파일에 남는 것**까지 본다. 왕복만 보면 과하게 이스케이프해도
            // 통과한다 — 감싸기와 풀기가 같은 판정을 쓰기 때문이다. 그래도
            // 사이드카는 사람이 읽고 git diff 에 뜨는 파일이라, 안 건드려도 될
            // 줄에 백슬래시를 흘리면 안 된다.
            let stored = text.lines().nth(1).unwrap_or_default();
            assert_eq!(stored, body.lines().next().unwrap(), "파일에 군더더기가 남았다: {body}");

            let round = parse_entries(&text);
            assert_eq!(round.len(), 1, "{body}");
            assert_eq!(round[0].body, body, "{body}");
        }
    }

}
