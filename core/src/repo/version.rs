//! DEV-433: 문서 번호(version) — 이 문서를 openguild 가 몇 번 고쳤나.
//!
//! **순서를 알리는 데만 쓴다.** 플러그인이 "v8 을 이미 봤으니 늦게 온 v7 은 버린다" 를 판단한다 — 프로세스가
//! 다르면 알림이 도착하는 순서를 보장할 수 없어서다. "바뀌었나" 판단에는 쓰지 않는다(REQ-036): 밖에서 파일을
//! 직접 고치거나 git 으로 합치면 번호가 안 맞을 수 있다.
//!
//! **파일마다 따로 센다**(DEV-433 결정). 본문 파일과 댓글 파일은 각자 번호를 가진다 — 댓글을 달 때 본문
//! 파일을 다시 쓰지 않으려고(git 에 본문 변경이 쌓이고 병합이 부딪힌다).
//!
//! | 파일 | 번호 자리 |
//! |---|---|
//! | 퀘스트 · 캠페인 · 도서관 문서 · 규칙 | frontmatter 의 `version = N` 한 줄 |
//! | 퀘스트 · 캠페인 댓글 | 맨 첫 줄 `<!-- og-comments version="N" -->` |
//!
//! 번호는 **글자 수준에서만** 다룬다 — 파일 모델 구조체는 번호를 모른다(TOML 은 모르는 키를 건너뛴다). 그래서
//! 옛 openguild 도 새 파일을 읽는다. 쓸 때 내용이 그대로면 쓰지 않고 번호도 그대로다 — 같은 내용을 다시 쓰는
//! 경로(자동 블록 다시 만들기 등)가 번호와 git 을 어지럽히지 않게.
//!
//! 번호가 없는 옛 파일은 0 으로 본다. 첨부 목록 · 비공개 메모 · 일지 노트는 번호가 없다 — 첨부 목록은 JSON
//! 배열이라 자리를 만들려면 형식을 바꿔야 하고(옛 openguild 가 못 읽는다), 메모와 일지는 알림 순서가 문제 될
//! 구독자가 없다.

use anyhow::Result;
use std::path::Path;

use super::fs::write_atomic;

/// 번호를 어디에 적나.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// `+++` 로 감싼 TOML frontmatter 안.
    Frontmatter,
    /// 댓글 파일 맨 첫 줄.
    CommentsHeader,
}

const FRONT_KEY: &str = "version = ";
const HEADER_OPEN: &str = "<!-- og-comments version=\"";
const HEADER_CLOSE: &str = "\" -->";

/// 파일 내용에 적힌 번호. 없으면 0.
pub fn of(text: &str, place: Place) -> u64 {
    match place {
        Place::Frontmatter => frontmatter_lines(text)
            .and_then(|(lines, _)| {
                lines
                    .iter()
                    .find_map(|l| l.trim_end_matches('\r').strip_prefix(FRONT_KEY))
                    .and_then(|v| v.trim().parse().ok())
            })
            .unwrap_or(0),
        Place::CommentsHeader => header(text).map(|(v, _)| v).unwrap_or(0),
    }
}

/// 파일에서 번호를 읽는다. 없는 파일 · 번호 없는 파일은 0.
pub fn read(path: &Path, place: Place) -> u64 {
    std::fs::read_to_string(path).map_or(0, |t| of(&t, place))
}

/// 번호 자리를 걷어 낸 내용 — 댓글 파서가 머리 줄을 본문으로 읽지 않게.
pub fn strip(text: &str, place: Place) -> String {
    match place {
        Place::Frontmatter => stamp(text, 0, place),
        Place::CommentsHeader => match header(text) {
            Some((_, rest)) => rest.to_string(),
            None => text.to_string(),
        },
    }
}

/// 번호 `v` 를 적은 내용. `v == 0` 이면 번호 자리를 뺀다. 이미 적힌 번호는 바꾼다.
pub fn stamp(text: &str, v: u64, place: Place) -> String {
    match place {
        Place::Frontmatter => {
            let Some((lines, body)) = frontmatter_lines(text) else {
                // frontmatter 가 없는 파일(규칙의 옛 모양)에는 새로 만든다.
                return if v == 0 { text.to_string() } else { format!("+++\n{FRONT_KEY}{v}\n+++\n{text}") };
            };
            let mut out = String::from("+++\n");
            for l in lines.iter().filter(|l| !l.trim_end_matches('\r').starts_with(FRONT_KEY)) {
                out.push_str(l);
                out.push('\n');
            }
            if v > 0 {
                out.push_str(&format!("{FRONT_KEY}{v}\n"));
            }
            out.push_str("+++\n");
            out.push_str(body);
            out
        }
        Place::CommentsHeader => {
            let rest = strip(text, place);
            if v == 0 { rest } else { format!("{HEADER_OPEN}{v}{HEADER_CLOSE}\n{rest}") }
        }
    }
}

/// 번호를 이어 가며 쓴다 — 지금 파일의 번호를 읽어, 내용이 그대로면 쓰지 않고(번호 그대로), 다르면 하나 올려
/// 쓴다. 쓴(또는 그대로인) 번호를 돌려준다. `contents` 에 번호가 적혀 있어도 무시한다 — 번호는 파일이 정한다.
pub fn write(path: &Path, contents: &str, place: Place) -> Result<u64> {
    let old = std::fs::read_to_string(path).ok();
    let base = old.as_deref().map_or(0, |t| of(t, place));
    if old.as_deref() == Some(stamp(contents, base, place).as_str()) {
        return Ok(base);
    }
    let v = base + 1;
    write_atomic(path, &stamp(contents, v, place))?;
    Ok(v)
}

/// `+++\n` 부터 닫는 `+++` 앞까지의 줄들과, 닫는 줄 뒤의 나머지.
fn frontmatter_lines(text: &str) -> Option<(Vec<&str>, &str)> {
    let after = text.strip_prefix("+++\n").or_else(|| text.strip_prefix("+++\r\n"))?;
    let mut pos = 0;
    let mut lines = Vec::new();
    while pos < after.len() {
        let end = after[pos..].find('\n').map_or(after.len(), |i| pos + i);
        let line = &after[pos..end];
        if line.trim_end_matches('\r') == "+++" {
            let rest = &after[(end + 1).min(after.len())..];
            return Some((lines, rest));
        }
        lines.push(line);
        pos = end + 1;
    }
    None
}

/// 댓글 파일 첫 줄의 번호와 그 뒤 나머지.
fn header(text: &str) -> Option<(u64, &str)> {
    let first_end = text.find('\n').unwrap_or(text.len());
    let first = text[..first_end].trim_end_matches('\r');
    let v = first.strip_prefix(HEADER_OPEN)?.strip_suffix(HEADER_CLOSE)?.parse().ok()?;
    Some((v, &text[(first_end + 1).min(text.len())..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "+++\nquest_id = \"DEV-001\"\ntitle = \"t\"\n+++\n\n본문\n";

    fn tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("og-version-{label}-{ns}"));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn each_write_that_changes_something_counts_up() {
        let d = tmp("up");
        let p = d.join("DEV-001.md");
        assert_eq!(write(&p, DOC, Place::Frontmatter).unwrap(), 1);
        assert_eq!(write(&p, &DOC.replace("본문", "고친 본문"), Place::Frontmatter).unwrap(), 2);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("version = 2\n+++\n"), "{text}");
        assert!(text.contains("고친 본문"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn writing_the_same_thing_again_keeps_the_number_and_the_file() {
        let d = tmp("same");
        let p = d.join("DEV-001.md");
        write(&p, DOC, Place::Frontmatter).unwrap();
        let before = std::fs::metadata(&p).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(write(&p, DOC, Place::Frontmatter).unwrap(), 1);
        assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), before, "같은 내용인데 다시 썼다");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 번호 없는 옛 파일은 0 — 내용이 그대로면 손대지 않는다(옛 길드를 열자마자 파일이 다 바뀌면 안 된다).
    #[test]
    fn an_old_file_without_a_number_is_zero_and_left_alone() {
        let d = tmp("old");
        let p = d.join("DEV-001.md");
        std::fs::write(&p, DOC).unwrap();
        assert_eq!(read(&p, Place::Frontmatter), 0);
        assert_eq!(write(&p, DOC, Place::Frontmatter).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), DOC);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 넘겨받은 내용에 적힌 번호는 무시한다 — 옛 파일을 읽어 고친 것을 넘겨도 번호를 되돌리지 못한다.
    #[test]
    fn the_file_decides_the_number_not_the_caller() {
        let d = tmp("file-decides");
        let p = d.join("DEV-001.md");
        for _ in 0..3 {
            let n = read(&p, Place::Frontmatter);
            write(&p, &stamp(&DOC.replace("본문", &format!("본문 {n}")), n, Place::Frontmatter), Place::Frontmatter).unwrap();
        }
        let stale = stamp(DOC, 1, Place::Frontmatter);
        assert_eq!(write(&p, &stale, Place::Frontmatter).unwrap(), 4);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_without_frontmatter_gets_one() {
        assert_eq!(stamp("규칙 본문\n", 3, Place::Frontmatter), "+++\nversion = 3\n+++\n규칙 본문\n");
        assert_eq!(of("+++\nversion = 3\n+++\n규칙 본문\n", Place::Frontmatter), 3);
    }

    #[test]
    fn comments_carry_the_number_on_the_first_line() {
        let body = "<!-- og-comment id=\"1\" ts=\"t\" author=\"a\" -->\n안녕\n";
        let s = stamp(body, 5, Place::CommentsHeader);
        assert_eq!(s, format!("<!-- og-comments version=\"5\" -->\n{body}"));
        assert_eq!(of(&s, Place::CommentsHeader), 5);
        assert_eq!(strip(&s, Place::CommentsHeader), body);
        assert_eq!(of(body, Place::CommentsHeader), 0);
        // 댓글이 다 지워져도 머리 줄만 남는다 — 본문으로 읽히면 안 된다.
        assert_eq!(strip(&stamp("", 2, Place::CommentsHeader), Place::CommentsHeader), "");
    }
}
