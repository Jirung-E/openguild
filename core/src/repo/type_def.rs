//! Quest 타입 파일 — `.guild/types/{prefix}.toml`.
//!
//! `[counter]` 섹션의 `last_number` 는 자동 관리 — 사용자 수동 수정 금지.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::fs::write_atomic;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeFile {
    pub prefix: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub counter: Counter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Counter {
    #[serde(default)]
    pub last_number: i64,
}

impl TypeFile {
    /// TOML 파싱.
    pub fn parse(s: &str) -> Result<Self> {
        toml::from_str(s).context("failed to parse type TOML")
    }

    /// 파일로부터 파싱.
    pub fn read<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let s = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("failed to read: {}", path.as_ref().display()))?;
        Self::parse(&s)
    }

    /// TOML 문자열 직렬화 (위쪽에 counter 관리 경고 헤더 포함).
    pub fn serialize(&self) -> String {
        let body = toml::to_string_pretty(self).expect("type 직렬화 실패");
        format!("{TYPE_FILE_HEADER}\n{body}")
    }

    /// 파일에 atomic 쓰기.
    pub fn write<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        write_atomic(path.as_ref(), &self.serialize())
    }
}

/// 모든 type 파일 위쪽에 들어가는 경고 헤더.
pub const TYPE_FILE_HEADER: &str = "\
# ⚠️ [counter] 섹션은 자동 관리 필드 — 절대 수동으로 수정하지 마십시오.
# last_number 는 부여된 quest ID 가 재사용되지 않도록 보호하는 단조 증가 카운터.
#   - 줄이면 ID 중복으로 데이터 손상 발생.
#   - 늘리면 번호 건너뛰어 추적이 어려워짐.
# 시작 시 실제 quest 파일들의 max 번호와 검증하여 보호합니다.
";

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-type-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn parse_minimal() {
        let s = r##"
            prefix = "DEV"
            color = "#4A90D9"
        "##;
        let t = TypeFile::parse(s).unwrap();
        assert_eq!(t.prefix, "DEV");
        assert_eq!(t.color, "#4A90D9");
        assert_eq!(t.description, None);
        assert_eq!(t.counter.last_number, 0);
    }

    #[test]
    fn parse_full() {
        let s = r##"
            prefix = "DEV"
            color = "#4A90D9"
            description = "일반 개발 작업"

            [counter]
            last_number = 19
        "##;
        let t = TypeFile::parse(s).unwrap();
        assert_eq!(t.prefix, "DEV");
        assert_eq!(t.description.as_deref(), Some("일반 개발 작업"));
        assert_eq!(t.counter.last_number, 19);
    }

    #[test]
    fn round_trip() {
        let t = TypeFile {
            prefix: "BUG".into(),
            color: "#E5484D".into(),
            description: Some("버그 보고".into()),
            counter: Counter { last_number: 3 },
        };
        let s = t.serialize();
        // 경고 헤더 포함
        assert!(s.starts_with("# ⚠️"));
        let parsed = TypeFile::parse(&s).unwrap();
        assert_eq!(parsed, t);
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = fresh_tmp("rw");
        let t = TypeFile {
            prefix: "REQ".into(),
            color: "#8E4EC6".into(),
            description: None,
            counter: Counter { last_number: 7 },
        };
        let path = dir.join("REQ.toml");
        t.write(&path).unwrap();
        let read = TypeFile::read(&path).unwrap();
        assert_eq!(read, t);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn serialize_includes_counter_warning() {
        let t = TypeFile {
            prefix: "DEV".into(),
            color: "#000".into(),
            description: None,
            counter: Counter { last_number: 1 },
        };
        let s = t.serialize();
        assert!(s.contains("자동 관리"));
        assert!(s.contains("수동으로 수정하지 마"));
        assert!(s.contains("last_number"));
    }

    #[test]
    fn parse_fails_on_invalid_toml() {
        let err = TypeFile::parse("this is = = not toml").unwrap_err();
        assert!(err.to_string().contains("type TOML"));
    }
}
