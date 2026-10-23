//! DEV-449: CLI 로 형광펜을 칠할 때 **자리 찾기** — 마크다운 원문에서 읽기 화면의 글자를 만들고, 그 안에서
//! 칠할 글을 찾아 앞뒤 글자를 붙인다.
//!
//! 형광펜은 화면에 보이는 글자로 자리를 다시 찾는다(DEV-443, `text-quote.ts` 의 `findQuote`). 화면은
//! 칠한 글이 **그대로** 들어 있는 곳만 찾으므로, 원문의 `**굵게**` · `[링크](주소)` 를 그대로 적으면 화면에서
//! 영영 못 찾는다. 그래서 여기서 화면(`MarkdownView` — marked + `[[..]]` 링크 바꾸기)과 같은 글자를 만들고,
//! 그 안에 있는 글만 받는다.
//!
//! 화면 글자를 완전히 똑같이 만들 수는 없다(문단 사이 빈 줄 수 같은 것). 그래서 **한 문단 안의 글만** 받는다
//! — 문단 안의 글자는 화면과 같고, 앞뒤 글자는 어긋나도 화면이 가장 잘 맞는 곳을 고를 뿐이다.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::error::{AppError, AppResult};

/// 앞뒤로 적어 둘 글자 수 — 화면(`text-quote.ts` 의 `CONTEXT`)과 같다.
pub const CONTEXT: usize = 32;

/// 저장할 인용 — 칠한 글과 그 앞뒤.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub exact: String,
    pub prefix: String,
    pub suffix: String,
}

/// 읽기 화면의 글자. `sep` 은 글자마다 "문단 경계로 넣은 줄바꿈인가".
struct Screen {
    text: Vec<char>,
    sep: Vec<bool>,
    /// 아직 안 내보낸 글 — 화면에서 한 텍스트 노드가 될 글. `[[..]]` 바꾸기는 이 단위로 한다.
    pending: String,
}

impl Screen {
    fn put(&mut self, s: &str, sep: bool) {
        for c in s.chars() {
            self.text.push(c);
            self.sep.push(sep);
        }
    }
    /// 화면의 텍스트 노드가 끊기는 곳 — 모아 둔 글을 `[[..]]` 를 바꿔 내보낸다.
    fn flush(&mut self) {
        let p = std::mem::take(&mut self.pending);
        let shown = cross_links(&p);
        self.put(&shown, false);
    }
    fn raw(&mut self, s: &str) {
        self.flush();
        self.put(s, false);
    }
    fn boundary(&mut self) {
        self.flush();
        self.put("\n", true);
    }
}

/// `[[DEV-033]]` · `[[quest:dev-33]]` · `[[코딩 규칙]]` → 화면에 보이는 글. `MarkdownView.applyRefToAnchor`:
/// 규칙은 이름 그대로, 나머지(퀘스트 · 캠페인 · 도서관)는 대문자. 접두가 없으면 모양으로 가른다.
fn cross_links(s: &str) -> String {
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\[\[([^\[\]\n\r]{1,64})\]\]").unwrap());
    static ID: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^[A-Za-z]+-\d+$").unwrap());
    RE.replace_all(s, |c: &regex::Captures| {
        let token = c[1].trim();
        if token.is_empty() {
            return c[0].to_string();
        }
        let (kind, id) = match token.split_once(':') {
            Some((k, rest)) if !k.is_empty() => match k.to_lowercase().as_str() {
                "quest" | "q" | "campaign" | "c" | "book" | "library" | "lib" => {
                    (Some(false), rest)
                }
                "rule" | "rules" | "r" => (Some(true), rest),
                _ => (None, token),
            },
            _ => (None, token),
        };
        let rule = kind.unwrap_or_else(|| !ID.is_match(&id.to_uppercase()));
        if rule {
            id.to_string()
        } else {
            id.to_uppercase()
        }
    })
    .into_owned()
}

fn screen(markdown: &str) -> Screen {
    let mut s = Screen {
        text: Vec::new(),
        sep: Vec::new(),
        pending: String::new(),
    };
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    // 화면에 글자로 안 나오는 것 — mermaid 블록(그림으로 바뀐다), 이미지(대체 글은 속성이다).
    let mut hidden = 0usize;
    let mut code = false;
    let mut link = 0usize;
    for ev in Parser::new_ext(markdown, opts) {
        match ev {
            Event::Start(Tag::CodeBlock(kind)) => {
                s.flush();
                let mermaid = matches!(&kind, CodeBlockKind::Fenced(l)
                    if l.split_whitespace().next() == Some("mermaid"));
                if mermaid { hidden += 1 } else { code = true }
            }
            Event::End(TagEnd::CodeBlock) => {
                if code {
                    code = false
                } else {
                    hidden = hidden.saturating_sub(1)
                }
                s.boundary();
            }
            Event::Start(Tag::Image { .. }) => hidden += 1,
            Event::End(TagEnd::Image) => hidden = hidden.saturating_sub(1),
            _ if hidden > 0 => {}
            // 코드와 링크 안은 화면이 `[[..]]` 를 안 바꾼다.
            Event::Text(t) if code || link > 0 => s.raw(&t),
            Event::Text(t) => s.pending.push_str(&t),
            Event::Code(t) => s.raw(&t),
            // marked 는 문단 안 줄바꿈을 글자 그대로 둔다. `<br>` 은 글자가 없다.
            Event::SoftBreak => s.pending.push('\n'),
            Event::TaskListMarker(_) => s.raw(" "),
            Event::Start(Tag::Link { .. }) => {
                s.flush();
                link += 1;
            }
            Event::End(TagEnd::Link) => {
                s.flush();
                link = link.saturating_sub(1);
            }
            Event::Start(
                Tag::List(_) | Tag::BlockQuote(_) | Tag::Table(_) | Tag::TableHead | Tag::TableRow,
            )
            | Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::List(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::HtmlBlock
                | TagEnd::Table
                | TagEnd::TableHead
                | TagEnd::TableRow
                | TagEnd::TableCell,
            )
            | Event::Rule => s.boundary(),
            // 강조 · 줄긋기 · 줄 끊기 · HTML — 글자는 없고 텍스트 노드만 끊는다.
            _ => s.flush(),
        }
    }
    s.flush();
    s
}

/// 읽기 화면에 보이는 글자 — 시험 · 진단용.
pub fn screen_text(markdown: &str) -> String {
    screen(markdown).text.into_iter().collect()
}

/// 원문 `markdown` 의 읽기 화면에서 `wanted` 를 찾아 인용을 만든다. 공백 · 줄바꿈의 차이는 무시하고,
/// 저장하는 글은 화면의 글자 그대로다. 같은 글이 여러 번 나오면 `nth`(1부터)로 고른다.
pub fn quote(markdown: &str, wanted: &str, nth: Option<usize>) -> AppResult<Quote> {
    let bad = |m: String| Err(AppError::BadRequest(m));
    let want: Vec<char> = collapse(wanted.trim().chars())
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    if want.is_empty() {
        return bad(crate::tf!(
            "칠할 글이 비어 있습니다",
            "nothing to highlight"
        ));
    }
    let s = screen(markdown);
    let flat = collapse(s.text.iter().copied());

    let mut hits = Vec::new();
    let mut i = 0;
    while i + want.len() <= flat.len() {
        if flat[i..i + want.len()]
            .iter()
            .map(|(c, _)| *c)
            .eq(want.iter().copied())
        {
            hits.push(i);
            i += want.len();
        } else {
            i += 1;
        }
    }
    let at = match (hits.len(), nth) {
        (0, _) => {
            return bad(crate::tf!(
                "이 글을 찾지 못했습니다 — 읽기 화면에 보이는 글자 그대로 적으세요(`**` · `[..](..)` 같은 마크다운 기호는 빼고)",
                "text not found — write it as the reading view shows it (without markdown such as `**` or `[..](..)`)"
            ));
        }
        (_, Some(0)) => return bad(crate::tf!("--nth 는 1부터입니다", "--nth starts at 1")),
        (n, Some(k)) if k > n => {
            return bad(crate::tf!(
                "{k}번째가 없습니다 — 이 글은 {n}번 나옵니다",
                "there is no occurrence {k} — the text appears {n} time(s)"
            ));
        }
        (_, Some(k)) => hits[k - 1],
        (1, None) => hits[0],
        (n, None) => {
            return bad(crate::tf!(
                "이 글이 {n}번 나옵니다 — --nth 로 몇 번째인지 고르세요(1~{n})",
                "the text appears {n} times — pick one with --nth (1-{n})"
            ));
        }
    };
    let start = flat[at].1;
    let end = flat[at + want.len() - 1].1 + 1;
    if s.sep[start..end].iter().any(|&b| b) {
        return bad(crate::tf!(
            "여러 문단에 걸쳐 칠할 수 없습니다 — 문단마다 따로 칠하세요",
            "a highlight cannot span paragraphs — highlight each paragraph separately"
        ));
    }
    let take = |a: usize, b: usize| s.text[a..b].iter().collect::<String>();
    Ok(Quote {
        exact: take(start, end),
        prefix: take(start.saturating_sub(CONTEXT), start),
        suffix: take(end, (end + CONTEXT).min(s.text.len())),
    })
}

/// 공백이 이어지면 하나(`' '`)로 — 각 글자가 원래 몇 번째 글자였는지와 함께.
fn collapse(chars: impl Iterator<Item = char>) -> Vec<(char, usize)> {
    let mut out: Vec<(char, usize)> = Vec::new();
    for (i, c) in chars.enumerate() {
        if c.is_whitespace() {
            if out.last().is_some_and(|(p, _)| *p == ' ') {
                continue;
            }
            out.push((' ', i));
        } else {
            out.push((c, i));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_marks_are_not_on_the_screen() {
        let md = "# 제목\n\n**굵은** 글과 `코드`, [링크 글](https://x.test) 그리고 ~~지움~~.\n";
        let t = screen_text(md);
        assert!(t.contains("굵은 글과 코드, 링크 글 그리고 지움."), "{t:?}");
        assert!(!t.contains("**") && !t.contains("https://"), "{t:?}");
        // 원문 기호째 적으면 화면에 없으니 못 찾는다 — 저장하면 화면에서 영영 못 칠한다.
        assert!(quote(md, "**굵은** 글", None).is_err());
        let q = quote(md, "굵은 글과 코드", None).unwrap();
        assert_eq!(q.exact, "굵은 글과 코드");
        assert!(q.prefix.ends_with("제목\n"), "{:?}", q.prefix);
        assert!(q.suffix.starts_with(", 링크 글"), "{:?}", q.suffix);
    }

    /// 화면은 `[[..]]` 를 링크로 바꿔 ID 를 대문자로 보인다(규칙은 이름 그대로). 코드 안은 안 바꾼다.
    #[test]
    fn cross_links_read_as_the_screen_shows_them() {
        let md = "앞 [[dev-33]] · [[c:c-1]] · [[코딩 규칙]] · [[rule:My-Rule]] 뒤 `[[dev-1]]`\n";
        let t = screen_text(md);
        assert!(
            t.contains("앞 DEV-33 · C-1 · 코딩 규칙 · My-Rule 뒤 [[dev-1]]"),
            "{t:?}"
        );
        assert!(quote(md, "DEV-33 · C-1", None).is_ok());
    }

    /// 원문의 줄바꿈은 화면에서도 글자로 남는다(marked) — 공백으로 적어도 찾고, 저장은 화면 글자 그대로.
    #[test]
    fn spaces_and_line_breaks_match_loosely_and_are_stored_as_shown() {
        let md = "한 문단이\n두 줄로   이어진다.\n";
        let q = quote(md, "문단이 두 줄로 이어진다", None).unwrap();
        assert_eq!(q.exact, "문단이\n두 줄로   이어진다");
    }

    #[test]
    fn repeated_text_needs_nth() {
        let md = "사과 하나, 사과 둘, 사과 셋\n";
        let e = quote(md, "사과", None).unwrap_err().to_string();
        assert!(e.contains('3'), "{e}");
        let q = quote(md, "사과", Some(2)).unwrap();
        assert!(
            q.prefix.ends_with("하나, ") && q.suffix.starts_with(" 둘"),
            "{q:?}"
        );
        assert!(quote(md, "사과", Some(4)).is_err());
        assert!(quote(md, "사과", Some(0)).is_err());
        // 하나뿐이면 --nth 1 도 된다.
        assert!(quote(md, "하나", Some(1)).is_ok());
    }

    #[test]
    fn one_paragraph_at_a_time() {
        let md = "첫 문단 끝.\n\n둘째 문단 시작.\n\n- 항목 가\n- 항목 나\n";
        assert!(quote(md, "끝. 둘째", None).is_err());
        assert!(quote(md, "가 항목", None).is_err());
        assert!(quote(md, "항목 나", None).is_ok());
    }

    /// 코드 블록 안은 여러 줄이어도 한 덩어리다. mermaid 는 그림이 되므로 글자가 없다.
    #[test]
    fn code_blocks_keep_lines_and_mermaid_has_no_text() {
        let md = "```rust\nlet a = 1;\nlet b = 2;\n```\n\n```mermaid\ngraph TD; 가나다\n```\n";
        let q = quote(md, "let a = 1;\nlet b", None).unwrap();
        assert_eq!(q.exact, "let a = 1;\nlet b");
        assert!(quote(md, "가나다", None).is_err());
    }

    #[test]
    fn empty_is_refused() {
        assert!(quote("본문", "  \n ", None).is_err());
    }
}
