//! DEV-068: Tag 정의 파일 — `.guild/tags/{slug}.toml`.
//!
//! 파일이 진리원. `quest_tag_defs` (migration 0013) 가 캐시.
//! `quest_tags` 의 tag 가 def 없어도 정상 — 기본 색 (회색).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::fs::write_atomic;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagFile {
    /// 사용자 친화 색 (예: `#58a6ff`). 빈 문자열 = 기본 (UI 가 회색 fallback).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
    /// 자유 설명. 빈 문자열 OK.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl TagFile {
    pub fn parse(s: &str) -> Result<Self> {
        toml::from_str(s).context("failed to parse tag TOML")
    }

    pub fn read<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let s = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("failed to read: {}", path.as_ref().display()))?;
        Self::parse(&s)
    }

    pub fn serialize(&self) -> String {
        toml::to_string_pretty(self).expect("tag 직렬화 실패")
    }

    pub fn write<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        write_atomic(path.as_ref(), &self.serialize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_ok() {
        let f = TagFile::parse("").unwrap();
        assert!(f.color.is_empty());
        assert!(f.description.is_empty());
    }

    #[test]
    fn round_trip_with_color() {
        let f = TagFile {
            color: "#58a6ff".into(),
            description: "frontend".into(),
        };
        let s = f.serialize();
        let parsed = TagFile::parse(&s).unwrap();
        assert_eq!(parsed, f);
    }

    #[test]
    fn parse_only_color() {
        let f = TagFile::parse("color = \"#7BB87F\"").unwrap();
        assert_eq!(f.color, "#7BB87F");
        assert!(f.description.is_empty());
    }
}
