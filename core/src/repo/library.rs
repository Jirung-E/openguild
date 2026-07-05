//! DEV-215: 도서관(Library) 문서 — `.guild/library/{BOOK-NNN}.md`.
//!
//! 프로젝트 knowledge base(참고문서/노트/결정 — 태스크가 아닌 지식).
//! quests 와 동일한 파일 진리원 패턴: `+++` TOML frontmatter + markdown 본문.
//! rules(slug 식별, 번호 없음)와 달리 **quest/campaign 처럼 자체 관리번호**
//! (`BOOK-NNN`)가 부여되어 cross-link(`[[BOOK-001]]`) 대상이 된다 (DEV-184 결정).
//!
//! - **카운터**: `.guild/library/.counter.toml` — quest_types 와 완전히 별개인
//!   단조 증가 카운터. 삭제된 번호는 재사용하지 않는다 (prefix "BOOK" 고정 —
//!   admin 결정, GUI 에서 rename 할 일 없음).
//! - **soft delete**: frontmatter `deleted = true` (quests 와 동일).
//! - git **tracked** — 팀 공유 지식.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::fs::write_atomic;
use super::GuildPaths;

/// 도서관 문서 ID prefix. 고정 — quest_types 와 별개 네임스페이스.
pub const BOOK_PREFIX: &str = "BOOK";

/// `BOOK-NNN` slug 생성 (3자리 zero-pad, quest slug 와 동일 규칙).
pub fn book_slug(number: i64) -> String {
    format!("{BOOK_PREFIX}-{number:03}")
}

/// `BOOK-NNN` slug → number. 형식이 아니면 None.
pub fn parse_book_slug(slug: &str) -> Option<i64> {
    let rest = slug.strip_prefix(BOOK_PREFIX)?.strip_prefix('-')?;
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// 도서관 문서 파일 한 개.
#[derive(Debug, Clone, PartialEq)]
pub struct BookFile {
    pub frontmatter: BookFrontmatter,
    /// markdown 본문 (frontmatter 제외). quests 와 달리 auto 블록 없음 —
    /// 도서관 문서는 관계(parent/prereq) 개념이 없다.
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookFrontmatter {
    /// slug 형식 ("BOOK-001"). 파일명과 일치. 변경 불가.
    pub book_id: String,
    pub title: String,
    /// 생성 시각 (로컬 ISO8601 — quests 와 동일 포맷).
    pub created_at: String,
    /// 마지막 mutation 시각.
    pub updated_at: String,
    /// soft delete flag.
    #[serde(default)]
    pub deleted: bool,
}

impl BookFile {
    pub fn parse(text: &str) -> Result<Self> {
        let (fm_text, body) = split_frontmatter(text)?;
        let frontmatter: BookFrontmatter =
            toml::from_str(fm_text).context("failed to parse book frontmatter (TOML)")?;
        Ok(Self {
            frontmatter,
            body: body.trim().to_string(),
        })
    }

    pub fn read<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let s = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("failed to read: {}", path.as_ref().display()))?;
        Self::parse(&s)
    }

    pub fn serialize(&self) -> Result<String> {
        let fm_toml = toml::to_string_pretty(&self.frontmatter)
            .context("failed to serialize book frontmatter")?;
        let mut out = String::new();
        out.push_str("+++\n");
        out.push_str(&fm_toml);
        if !fm_toml.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("+++\n");
        if !self.body.is_empty() {
            out.push('\n');
            out.push_str(&self.body);
            out.push('\n');
        }
        Ok(out)
    }

    pub fn write<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        write_atomic(path.as_ref(), &self.serialize()?)
    }

    /// frontmatter 의 number (book_id 에서 파싱).
    pub fn number(&self) -> Result<i64> {
        parse_book_slug(&self.frontmatter.book_id)
            .ok_or_else(|| anyhow!("invalid book_id: {:?}", self.frontmatter.book_id))
    }
}

/// `+++ ... +++` frontmatter 분리 (quest.rs 와 동일 규칙 — 재사용 어려운 private
/// 함수라 복제; 형식이 갈라질 수 있어 의도적으로 독립 유지).
fn split_frontmatter(text: &str) -> Result<(&str, &str)> {
    let rest = text
        .strip_prefix("+++")
        .ok_or_else(|| anyhow!("book file must start with +++ frontmatter"))?;
    let rest = rest.strip_prefix('\r').unwrap_or(rest);
    let rest = rest
        .strip_prefix('\n')
        .ok_or_else(|| anyhow!("+++ must be followed by newline"))?;
    let end = rest
        .find("\n+++")
        .ok_or_else(|| anyhow!("closing +++ not found"))?;
    let fm = &rest[..end];
    let after = &rest[end + 4..];
    let after = after.strip_prefix('\r').unwrap_or(after);
    let after = after.strip_prefix('\n').unwrap_or(after);
    Ok((fm, after))
}

// ─── 카운터 (.guild/library/.counter.toml) ───

/// 도서관 번호 카운터 파일. type_def::Counter 와 같은 형태지만 파일 위치/
/// 네임스페이스가 완전히 다르므로 독립 구조체.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LibraryCounter {
    #[serde(default)]
    pub last_number: i64,
}

const COUNTER_HEADER: &str = "\
# ⚠️ 자동 관리 파일 — 절대 수동으로 수정하지 마십시오.
# last_number 는 부여된 BOOK ID 가 재사용되지 않도록 보호하는 단조 증가 카운터.
";

pub fn read_counter(paths: &GuildPaths) -> Result<LibraryCounter> {
    let p = paths.library_counter_path();
    if !p.exists() {
        return Ok(LibraryCounter::default());
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("failed to read: {}", p.display()))?;
    toml::from_str(&s).context("failed to parse library counter TOML")
}

pub fn write_counter(paths: &GuildPaths, c: &LibraryCounter) -> Result<()> {
    std::fs::create_dir_all(paths.library_dir()).with_context(|| {
        format!("failed to create library dir: {}", paths.library_dir().display())
    })?;
    let body = toml::to_string_pretty(c).context("failed to serialize library counter")?;
    write_atomic(paths.library_counter_path(), &format!("{COUNTER_HEADER}\n{body}"))
}

/// 다음 번호 할당 — counter 와 실존 파일 max 중 큰 쪽 기준(+1)으로 단조 증가
/// 보장. counter 파일이 유실/후퇴해도(git 충돌 등) 실제 파일과 검증해 ID 중복을
/// 막는다 (quest counter 의 검증 철학과 동일).
pub fn allocate_number(paths: &GuildPaths) -> Result<i64> {
    let counter = read_counter(paths)?;
    let max_existing = list_book_files(paths)?
        .iter()
        .filter_map(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .and_then(parse_book_slug)
        })
        .max()
        .unwrap_or(0);
    let next = counter.last_number.max(max_existing) + 1;
    write_counter(paths, &LibraryCounter { last_number: next })?;
    Ok(next)
}

/// `.guild/library/` 안의 book 본체 파일 목록 (`BOOK-NNN.md` 만, 정렬).
pub fn list_book_files(paths: &GuildPaths) -> Result<Vec<std::path::PathBuf>> {
    let dir = paths.library_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out: Vec<_> = std::fs::read_dir(&dir)
        .with_context(|| format!("failed to read dir: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|x| x.to_str()) == Some("md")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| parse_book_slug(s).is_some())
        })
        .collect();
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-lib-{label}-{ns}"));
        std::fs::create_dir_all(p.join(".guild/library")).unwrap();
        p
    }

    fn book(n: i64, title: &str) -> BookFile {
        BookFile {
            frontmatter: BookFrontmatter {
                book_id: book_slug(n),
                title: title.into(),
                created_at: "2026-07-05T22:00:00+09:00".into(),
                updated_at: "2026-07-05T22:00:00+09:00".into(),
                deleted: false,
            },
            body: format!("{title} 본문"),
        }
    }

    #[test]
    fn slug_roundtrip_and_reject() {
        assert_eq!(book_slug(1), "BOOK-001");
        assert_eq!(book_slug(1234), "BOOK-1234");
        assert_eq!(parse_book_slug("BOOK-001"), Some(1));
        assert_eq!(parse_book_slug("BOOK-1234"), Some(1234));
        assert_eq!(parse_book_slug("DEV-001"), None);
        assert_eq!(parse_book_slug("BOOK-"), None);
        assert_eq!(parse_book_slug("BOOK-1a"), None);
        assert_eq!(parse_book_slug("book-001"), None);
    }

    #[test]
    fn file_roundtrip_including_korean_body() {
        let dir = fresh_tmp("rt");
        let paths = GuildPaths::new(&dir);
        let b = book(1, "설계 결정 기록");
        let path = paths.book_path(&b.frontmatter.book_id);
        b.write(&path).unwrap();
        let read = BookFile::read(&path).unwrap();
        assert_eq!(read, b);
        assert_eq!(read.number().unwrap(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn counter_allocates_monotonic_and_survives_regression() {
        let dir = fresh_tmp("cnt");
        let paths = GuildPaths::new(&dir);
        assert_eq!(allocate_number(&paths).unwrap(), 1);
        assert_eq!(allocate_number(&paths).unwrap(), 2);
        // counter 후퇴 시나리오(git 충돌 등) — 실존 파일 max 로 방어.
        book(2, "b").write(paths.book_path("BOOK-002")).unwrap();
        write_counter(&paths, &LibraryCounter { last_number: 0 }).unwrap();
        assert_eq!(allocate_number(&paths).unwrap(), 3, "파일 max(2)+1 로 복원");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_book_files_filters_non_book() {
        let dir = fresh_tmp("list");
        let paths = GuildPaths::new(&dir);
        book(1, "a").write(paths.book_path("BOOK-001")).unwrap();
        book(3, "c").write(paths.book_path("BOOK-003")).unwrap();
        std::fs::write(paths.library_dir().join(".counter.toml"), "last_number = 3").unwrap();
        std::fs::write(paths.library_dir().join("README.md"), "not a book").unwrap();
        let files = list_book_files(&paths).unwrap();
        let stems: Vec<String> = files
            .iter()
            .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(stems, vec!["BOOK-001", "BOOK-003"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_rejects_missing_frontmatter() {
        assert!(BookFile::parse("no frontmatter").is_err());
        assert!(BookFile::parse("+++\nbook_id = \"BOOK-001\"\n").is_err());
    }
}
