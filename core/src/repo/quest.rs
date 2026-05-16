//! Quest 파일 — `.guild/quests/{slug}.md`.
//!
//! 구조:
//! ```text
//! +++
//! quest_id = "DEV-001"
//! title = "..."
//! ...
//! +++
//!
//! (사용자가 작성하는 description)
//!
//! <!-- openguild:auto-begin -->
//! ## Sub-quests / Parent / Prerequisites 등 — 자동 생성
//! <!-- openguild:auto-end -->
//! ```
//!
//! Frontmatter 는 TOML (`+++` delimiter). Body 는 description + auto 블록.
//! Auto 블록 생성 자체는 F4 (auto.rs) 에서 처리 — 본 모듈은 read/write 와
//! 사용자 description 분리만 담당.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::fs::write_atomic;

/// Auto 블록 시작 마커. 도구가 매번 재생성하는 영역의 시작.
pub const AUTO_BEGIN: &str = "<!-- openguild:auto-begin — 아래는 자동 생성. 직접 수정하지 마세요. -->";
/// Auto 블록 종료 마커.
pub const AUTO_END: &str = "<!-- openguild:auto-end -->";

/// Quest 파일 한 개의 모든 내용.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestFile {
    pub frontmatter: QuestFrontmatter,
    /// 사용자가 작성한 description (auto 블록 제외, frontmatter 제외).
    /// 앞뒤 빈 줄은 직렬화 시 정규화됨.
    pub description: String,
    /// Auto 블록 내용 (마커 제외, 마커 안의 텍스트).
    /// 도구가 매 mutation 시 덮어씀. F4 에서 렌더링.
    pub auto_block: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestFrontmatter {
    /// slug 형식 ("DEV-001"). 파일명과 일치. 변경 불가.
    pub quest_id: String,
    pub title: String,
    /// status 파일의 slug (예: "open", "in_progress").
    pub status: String,
    /// 1=Critical / 2=High / 3=Medium / 4=Low.
    pub urgency: i64,
    /// 부모 quest_id. None 이면 root. TOML 에서는 키 생략으로 표현.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// 선행 quest_id 배열.
    #[serde(default)]
    pub prerequisites: Vec<String>,
    /// 생성 시각 (RFC 3339 UTC).
    pub created_at: String,
    /// 마지막 mutation 시각.
    pub updated_at: String,
    /// soft delete flag.
    #[serde(default)]
    pub deleted: bool,
}

impl QuestFile {
    /// 문자열로부터 파싱. frontmatter / description / auto 블록을 분리.
    pub fn parse(text: &str) -> Result<Self> {
        let (fm_text, after_fm) = split_frontmatter(text)?;
        let frontmatter: QuestFrontmatter =
            toml::from_str(fm_text).context("failed to parse quest frontmatter (TOML)")?;
        let (description, auto_block) = split_auto_block(after_fm);
        Ok(Self {
            frontmatter,
            description: description.trim().to_string(),
            auto_block: auto_block.trim().to_string(),
        })
    }

    /// 파일 경로에서 읽고 파싱.
    pub fn read<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let s = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("failed to read: {}", path.as_ref().display()))?;
        Self::parse(&s)
    }

    /// 표준 형식으로 직렬화.
    pub fn serialize(&self) -> Result<String> {
        let fm_toml = toml::to_string_pretty(&self.frontmatter)
            .context("failed to serialize quest frontmatter")?;
        let mut out = String::new();
        out.push_str("+++\n");
        out.push_str(&fm_toml);
        if !fm_toml.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("+++\n\n");
        if !self.description.is_empty() {
            out.push_str(&self.description);
            out.push_str("\n\n");
        }
        out.push_str(AUTO_BEGIN);
        out.push('\n');
        if !self.auto_block.is_empty() {
            out.push_str(&self.auto_block);
            if !self.auto_block.ends_with('\n') {
                out.push('\n');
            }
        }
        out.push_str(AUTO_END);
        out.push('\n');
        Ok(out)
    }

    /// 파일에 atomic 쓰기.
    pub fn write<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        write_atomic(path.as_ref(), &self.serialize()?)
    }

    /// `quest_id` 의 type prefix 추출 ("DEV-001" → "DEV").
    pub fn type_prefix(&self) -> Option<&str> {
        self.frontmatter.quest_id.split('-').next()
    }

    /// `quest_id` 의 number 추출 ("DEV-001" → 1).
    pub fn number(&self) -> Result<i64> {
        let (_, num) = self
            .frontmatter
            .quest_id
            .split_once('-')
            .ok_or_else(|| anyhow!("invalid quest_id: {}", self.frontmatter.quest_id))?;
        num.parse()
            .with_context(|| format!("invalid quest number: {num}"))
    }
}

/// `+++\n...+++\n` 의 frontmatter 와 그 이후 본문을 분리.
fn split_frontmatter(text: &str) -> Result<(&str, &str)> {
    let after_open = text
        .strip_prefix("+++\n")
        .or_else(|| text.strip_prefix("+++\r\n"))
        .ok_or_else(|| anyhow!("missing opening `+++` delimiter"))?;

    // 닫는 `+++` 만 있는 라인 찾기.
    let mut pos = 0;
    while pos < after_open.len() {
        let line_end = after_open[pos..]
            .find('\n')
            .map(|i| pos + i)
            .unwrap_or(after_open.len());
        let line = after_open[pos..line_end].trim_end_matches('\r');
        if line == "+++" {
            let fm = &after_open[..pos];
            let body_start = (line_end + 1).min(after_open.len());
            return Ok((fm, &after_open[body_start..]));
        }
        pos = line_end + 1;
    }
    Err(anyhow!("missing closing `+++` delimiter"))
}

/// frontmatter 이후 본문을 description 과 auto 블록으로 분리.
/// auto 블록이 없으면 빈 문자열.
fn split_auto_block(body: &str) -> (&str, &str) {
    let Some(begin_pos) = body.find(AUTO_BEGIN) else {
        return (body, "");
    };
    let after_begin = begin_pos + AUTO_BEGIN.len();
    let Some(end_rel) = body[after_begin..].find(AUTO_END) else {
        // 닫는 마커가 없으면 — auto-begin 이후를 전부 auto 영역으로 간주.
        return (&body[..begin_pos], &body[after_begin..]);
    };
    let end_pos = after_begin + end_rel;
    (&body[..begin_pos], &body[after_begin..end_pos])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-quest-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn sample_fm() -> QuestFrontmatter {
        QuestFrontmatter {
            quest_id: "DEV-001".into(),
            title: "Tauri desktop 앱".into(),
            status: "open".into(),
            urgency: 2,
            parent: None,
            prerequisites: vec![],
            created_at: "2026-05-16T15:00:00Z".into(),
            updated_at: "2026-05-16T15:00:00Z".into(),
            deleted: false,
        }
    }

    #[test]
    fn parse_minimal_quest() {
        let s = r#"+++
quest_id = "DEV-001"
title = "Tauri desktop 앱"
status = "open"
urgency = 2
prerequisites = []
created_at = "2026-05-16T15:00:00Z"
updated_at = "2026-05-16T15:00:00Z"
deleted = false
+++

설명 본문.
"#;
        let q = QuestFile::parse(s).unwrap();
        assert_eq!(q.frontmatter.quest_id, "DEV-001");
        assert_eq!(q.frontmatter.title, "Tauri desktop 앱");
        assert_eq!(q.frontmatter.urgency, 2);
        assert_eq!(q.frontmatter.parent, None);
        assert!(q.frontmatter.prerequisites.is_empty());
        assert_eq!(q.description, "설명 본문.");
        assert_eq!(q.auto_block, "");
    }

    #[test]
    fn parse_with_parent_and_prereqs() {
        let s = r#"+++
quest_id = "DEV-004"
title = "Invoke 핸들러"
status = "open"
urgency = 3
parent = "DEV-001"
prerequisites = ["DEV-002", "DEV-003"]
created_at = "2026-05-16T15:00:00Z"
updated_at = "2026-05-16T15:00:00Z"
deleted = false
+++

본문.
"#;
        let q = QuestFile::parse(s).unwrap();
        assert_eq!(q.frontmatter.parent.as_deref(), Some("DEV-001"));
        assert_eq!(
            q.frontmatter.prerequisites,
            vec!["DEV-002".to_string(), "DEV-003".to_string()]
        );
    }

    #[test]
    fn parse_with_auto_block() {
        let s = format!(
            "+++\n\
             quest_id = \"DEV-001\"\n\
             title = \"X\"\n\
             status = \"open\"\n\
             urgency = 3\n\
             prerequisites = []\n\
             created_at = \"2026-05-16T15:00:00Z\"\n\
             updated_at = \"2026-05-16T15:00:00Z\"\n\
             deleted = false\n\
             +++\n\
             \n\
             내 description.\n\
             \n\
             {AUTO_BEGIN}\n\
             ## Sub-quests\n\
             - [DEV-002](DEV-002.md) — child\n\
             {AUTO_END}\n"
        );
        let q = QuestFile::parse(&s).unwrap();
        assert_eq!(q.description, "내 description.");
        assert!(q.auto_block.contains("Sub-quests"));
        assert!(q.auto_block.contains("DEV-002"));
    }

    #[test]
    fn parse_fails_without_opening_delimiter() {
        let err = QuestFile::parse("not a quest file").unwrap_err();
        assert!(err.to_string().contains("opening"));
    }

    #[test]
    fn parse_fails_without_closing_delimiter() {
        let s = "+++\nquest_id = \"X\"\n\n no closing";
        let err = QuestFile::parse(s).unwrap_err();
        assert!(err.to_string().contains("closing"));
    }

    #[test]
    fn round_trip_no_auto_block() {
        let q = QuestFile {
            frontmatter: sample_fm(),
            description: "한 줄 설명.".to_string(),
            auto_block: String::new(),
        };
        let serialized = q.serialize().unwrap();
        let parsed = QuestFile::parse(&serialized).unwrap();
        assert_eq!(parsed.frontmatter, q.frontmatter);
        assert_eq!(parsed.description, q.description);
    }

    #[test]
    fn round_trip_with_auto_block() {
        let q = QuestFile {
            frontmatter: QuestFrontmatter {
                parent: Some("DEV-001".into()),
                prerequisites: vec!["DEV-002".into()],
                ..sample_fm()
            },
            description: "본문\n여러 줄.".into(),
            auto_block: "## Parent\n[DEV-001](DEV-001.md)".into(),
        };
        let serialized = q.serialize().unwrap();
        let parsed = QuestFile::parse(&serialized).unwrap();
        assert_eq!(parsed.frontmatter, q.frontmatter);
        assert_eq!(parsed.description, q.description);
        assert_eq!(parsed.auto_block.trim(), q.auto_block.trim());
    }

    #[test]
    fn serialize_omits_parent_when_none() {
        let q = QuestFile {
            frontmatter: sample_fm(),
            description: String::new(),
            auto_block: String::new(),
        };
        let s = q.serialize().unwrap();
        assert!(!s.contains("parent ="), "parent key should be omitted: {s}");
    }

    #[test]
    fn serialize_includes_parent_when_some() {
        let q = QuestFile {
            frontmatter: QuestFrontmatter {
                parent: Some("DEV-001".into()),
                ..sample_fm()
            },
            description: String::new(),
            auto_block: String::new(),
        };
        let s = q.serialize().unwrap();
        assert!(s.contains("parent = \"DEV-001\""));
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = fresh_tmp("rw");
        let q = QuestFile {
            frontmatter: sample_fm(),
            description: "본문".into(),
            auto_block: "## Sub-quests\n- (없음)".into(),
        };
        let path = dir.join("DEV-001.md");
        q.write(&path).unwrap();
        let read = QuestFile::read(&path).unwrap();
        assert_eq!(read.frontmatter, q.frontmatter);
        assert_eq!(read.description, q.description);
        assert_eq!(read.auto_block.trim(), q.auto_block.trim());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn type_prefix_and_number_extraction() {
        let q = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "BUG-042".into(),
                ..sample_fm()
            },
            description: String::new(),
            auto_block: String::new(),
        };
        assert_eq!(q.type_prefix(), Some("BUG"));
        assert_eq!(q.number().unwrap(), 42);
    }

    #[test]
    fn number_fails_on_malformed_id() {
        let q = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "no-number-here".into(),
                ..sample_fm()
            },
            description: String::new(),
            auto_block: String::new(),
        };
        // "no-number-here" → split "no", "number-here". parse "number-here" as i64 → fails.
        assert!(q.number().is_err());
    }
}
