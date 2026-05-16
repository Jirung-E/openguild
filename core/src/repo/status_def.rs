//! Quest 상태 파일 — `.guild/statuses/{sort_order}-{slug}.toml`.
//!
//! 파일명의 sort_order prefix 가 디렉토리 정렬 보장 (`1-open.toml`, `2-in_progress.toml`).
//! slug 부분이 quest frontmatter 의 `status` 값과 일치.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::fs::write_atomic;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusFile {
    pub sort_order: i64,
    pub name_en: String,
    pub name_ko: String,
    pub color: String,
}

impl StatusFile {
    pub fn parse(s: &str) -> Result<Self> {
        toml::from_str(s).context("failed to parse status TOML")
    }

    pub fn read<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let s = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("failed to read: {}", path.as_ref().display()))?;
        Self::parse(&s)
    }

    pub fn serialize(&self) -> String {
        toml::to_string_pretty(self).expect("status 직렬화 실패")
    }

    pub fn write<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        write_atomic(path.as_ref(), &self.serialize())
    }

    /// 파일명 (예: `1-open.toml`) 에서 slug 부분 추출 (`open`).
    pub fn slug_from_filename(name: &str) -> Option<&str> {
        let stem = name.strip_suffix(".toml")?;
        // `{order}-{slug}` 형식 — 첫 `-` 이후가 slug.
        let dash = stem.find('-')?;
        Some(&stem[dash + 1..])
    }

    /// `{sort_order}-{slug}.toml` 표준 파일명 구성.
    pub fn filename(sort_order: i64, slug: &str) -> String {
        format!("{sort_order}-{slug}.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-status-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn parse_basic() {
        let s = r##"
            sort_order = 1
            name_en = "Open"
            name_ko = "게시됨"
            color = "#8B95A1"
        "##;
        let st = StatusFile::parse(s).unwrap();
        assert_eq!(st.sort_order, 1);
        assert_eq!(st.name_en, "Open");
        assert_eq!(st.name_ko, "게시됨");
    }

    #[test]
    fn round_trip() {
        let st = StatusFile {
            sort_order: 2,
            name_en: "In Progress".into(),
            name_ko: "진행 중".into(),
            color: "#4A90D9".into(),
        };
        let s = st.serialize();
        let parsed = StatusFile::parse(&s).unwrap();
        assert_eq!(parsed, st);
    }

    #[test]
    fn write_and_read() {
        let dir = fresh_tmp("rw");
        let st = StatusFile {
            sort_order: 3,
            name_en: "Done".into(),
            name_ko: "완료".into(),
            color: "#30A46C".into(),
        };
        let path = dir.join("3-done.toml");
        st.write(&path).unwrap();
        let read = StatusFile::read(&path).unwrap();
        assert_eq!(read, st);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slug_from_filename_extracts_after_first_dash() {
        assert_eq!(StatusFile::slug_from_filename("1-open.toml"), Some("open"));
        assert_eq!(
            StatusFile::slug_from_filename("2-in_progress.toml"),
            Some("in_progress")
        );
        assert_eq!(
            StatusFile::slug_from_filename("10-on_hold.toml"),
            Some("on_hold")
        );
        // 첫 `-` 만 분리 — slug 안에 `-` 포함 가능
        assert_eq!(
            StatusFile::slug_from_filename("5-needs-review.toml"),
            Some("needs-review")
        );
    }

    #[test]
    fn slug_from_filename_returns_none_on_malformed() {
        assert_eq!(StatusFile::slug_from_filename("noextension"), None);
        assert_eq!(StatusFile::slug_from_filename("nodash.toml"), None);
    }

    #[test]
    fn filename_format() {
        assert_eq!(StatusFile::filename(1, "open"), "1-open.toml");
        assert_eq!(
            StatusFile::filename(2, "in_progress"),
            "2-in_progress.toml"
        );
    }
}
