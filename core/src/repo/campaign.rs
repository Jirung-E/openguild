//! Campaign 파일 — `.guild/campaigns/{slug}.md` (DEV-011).
//!
//! 구조:
//! ```text
//! +++
//! campaign_id = "C-001"
//! title = "..."
//! status = "active"
//! started_at = ""
//! ended_at = ""
//! linked_quests = ["DEV-001", ...]
//! created_at = "..."
//! updated_at = "..."
//! +++
//!
//! 마크다운 본문 (기획 내용 + 체크리스트).
//!
//! ## 체크리스트
//! - [x] 완료한 항목
//! - [ ] 진행 중인 항목
//! ```
//!
//! Quest 와 달리 **auto 블록 없음** (campaign 은 parent/sub/prereq 관계
//! 없음). 본문 markdown 전체가 사용자 영역.
//!
//! 체크리스트: 옵션 B3 (DEV-011 본문 명세) — 본문 어디든 `- [ ]` /
//! `- [x]` GFM task list 패턴이 모두 체크리스트로 동기화됨. 사용자가
//! 일반 메모로 `- [ ]` 적은 것도 들어가는 trade-off 의식적 선택.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::fs::write_atomic;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CampaignFrontmatter {
    /// slug 형식 ("C-001"). 파일명과 일치.
    pub campaign_id: String,
    pub title: String,
    /// "active" | "done".
    pub status: String,
    /// ISO date (YYYY-MM-DD). 빈 문자열 = 미정.
    #[serde(default)]
    pub started_at: String,
    #[serde(default)]
    pub ended_at: String,
    /// 연결된 quest 의 slug 목록. 빈 배열 = 연결 없음.
    #[serde(default)]
    pub linked_quests: Vec<String>,
    /// 어드민 수동 정렬 인덱스 (Home 카드 슬라이드 / Campaign 목록 정렬).
    /// 기본 0. 같으면 created_at DESC tie-break.
    #[serde(default)]
    pub display_order: i64,
    /// DEV-087: 배너 이미지 — `.guild/` 상대 경로 (예 "assets/C-001-banner.png").
    /// None = 배너 없음.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted: bool,
}

/// Campaign 파일 한 개.
#[derive(Debug, Clone, PartialEq)]
pub struct CampaignFile {
    pub frontmatter: CampaignFrontmatter,
    /// 본문 markdown 전체 (체크리스트 포함, 트림됨).
    pub body: String,
}

/// 본문에서 추출한 체크리스트 한 줄 (DB sync 용).
/// `order_idx` 는 본문에서의 출현 순서 (0-based).
#[derive(Debug, Clone, PartialEq)]
pub struct ChecklistLine {
    pub text: String,
    pub checked: bool,
    pub order_idx: i64,
}

impl CampaignFile {
    pub fn parse(text: &str) -> Result<Self> {
        let (fm_text, body) = split_frontmatter(text)?;
        let frontmatter: CampaignFrontmatter = toml::from_str(fm_text)
            .context("failed to parse campaign frontmatter (TOML)")?;
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
            .context("failed to serialize campaign frontmatter")?;
        let mut out = String::new();
        out.push_str("+++\n");
        out.push_str(&fm_toml);
        if !fm_toml.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("+++\n\n");
        if !self.body.is_empty() {
            out.push_str(&self.body);
            if !self.body.ends_with('\n') {
                out.push('\n');
            }
        }
        Ok(out)
    }

    pub fn write<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        write_atomic(path.as_ref(), &self.serialize()?)
    }

    /// 본문에서 GFM task list 항목들을 출현 순서대로 추출.
    pub fn checklist_items(&self) -> Vec<ChecklistLine> {
        extract_checklist_items(&self.body)
    }
}

/// 임의의 markdown 문자열에서 GFM task list (`- [ ]` / `- [x]`) 항목들을
/// 출현 순서대로 추출. 들여쓰기 (`  - [ ] ...`) 도 인식, 항목 텍스트는 trim.
///
/// 인식 패턴: `^\s*[-*+]\s+\[[ xX]\]\s+(.+?)\s*$`.
/// 모듈 외부 (services / reindex / GUI 미리보기) 가 직접 호출할 수 있게 노출.
pub fn extract_checklist_items(body: &str) -> Vec<ChecklistLine> {
    let mut out = Vec::new();
    let mut order = 0i64;
    for raw_line in body.lines() {
        let line = raw_line.trim_start();
        // bullet marker
        let after_bullet = if let Some(rest) = line.strip_prefix("- ") {
            rest
        } else if let Some(rest) = line.strip_prefix("* ") {
            rest
        } else if let Some(rest) = line.strip_prefix("+ ") {
            rest
        } else {
            continue;
        };
        // [ ] / [x] / [X]
        let (checked, text_part) = if let Some(rest) = after_bullet.strip_prefix("[ ] ") {
            (false, rest)
        } else if let Some(rest) = after_bullet.strip_prefix("[x] ") {
            (true, rest)
        } else if let Some(rest) = after_bullet.strip_prefix("[X] ") {
            (true, rest)
        } else {
            continue;
        };
        let text = text_part.trim().to_string();
        if text.is_empty() {
            continue;
        }
        out.push(ChecklistLine {
            text,
            checked,
            order_idx: order,
        });
        order += 1;
    }
    out
}

/// `+++\n...+++\n` 의 frontmatter 와 그 이후 본문을 분리. quest.rs 와 동일 로직.
fn split_frontmatter(text: &str) -> Result<(&str, &str)> {
    let after_open = text
        .strip_prefix("+++\n")
        .or_else(|| text.strip_prefix("+++\r\n"))
        .ok_or_else(|| anyhow!("missing opening `+++` delimiter"))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fm() -> CampaignFrontmatter {
        CampaignFrontmatter {
            campaign_id: "C-001".into(),
            title: "v1.0 출시".into(),
            status: "active".into(),
            started_at: "2026-06-01".into(),
            ended_at: "".into(),
            linked_quests: vec!["DEV-001".into(), "DEV-005".into()],
            display_order: 0,
            image: None,
            created_at: "2026-05-25T15:00:00Z".into(),
            updated_at: "2026-05-25T15:00:00Z".into(),
            deleted: false,
        }
    }

    #[test]
    fn parse_minimal_campaign() {
        let s = r#"+++
campaign_id = "C-001"
title = "v1.0 출시"
status = "active"
started_at = "2026-06-01"
ended_at = ""
linked_quests = ["DEV-001"]
display_order = 0
created_at = "2026-05-25T15:00:00Z"
updated_at = "2026-05-25T15:00:00Z"
+++

# v1.0 출시

기획 본문.

## 체크리스트
- [x] API 문서
- [ ] QA 통과
"#;
        let c = CampaignFile::parse(s).unwrap();
        assert_eq!(c.frontmatter.campaign_id, "C-001");
        assert_eq!(c.frontmatter.status, "active");
        assert_eq!(c.frontmatter.linked_quests, vec!["DEV-001"]);
        assert!(c.body.contains("기획 본문"));
        assert!(c.body.contains("- [x] API 문서"));
    }

    #[test]
    fn serialize_roundtrip() {
        let c = CampaignFile {
            frontmatter: sample_fm(),
            body: "본문\n\n## 체크리스트\n- [ ] 할 일".into(),
        };
        let s = c.serialize().unwrap();
        let parsed = CampaignFile::parse(&s).unwrap();
        assert_eq!(parsed.frontmatter, c.frontmatter);
        assert_eq!(parsed.body, c.body);
    }

    #[test]
    fn extract_checklist_basic() {
        let body = "기획\n\n## 체크리스트\n- [x] A\n- [ ] B\n- [X] C 대문자\n";
        let items = extract_checklist_items(body);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "A");
        assert!(items[0].checked);
        assert_eq!(items[0].order_idx, 0);
        assert_eq!(items[1].text, "B");
        assert!(!items[1].checked);
        assert_eq!(items[2].text, "C 대문자");
        assert!(items[2].checked);
    }

    #[test]
    fn extract_checklist_b3_includes_anywhere() {
        // B3: 본문 어디든 task list 패턴이 체크리스트로 인식됨.
        let body = "## 메모\n- [ ] 메모로 적은 것\n\n## 체크리스트\n- [x] 진짜 체크\n";
        let items = extract_checklist_items(body);
        assert_eq!(items.len(), 2, "본문 어디든 task list 모두 추출 (B3 trade-off)");
        assert_eq!(items[0].text, "메모로 적은 것");
        assert_eq!(items[1].text, "진짜 체크");
    }

    #[test]
    fn extract_checklist_supports_indent_and_alt_bullets() {
        let body = "- [ ] a\n  - [x] b indented\n* [ ] c\n+ [x] d\n";
        let items = extract_checklist_items(body);
        assert_eq!(items.len(), 4);
        assert_eq!(items[1].text, "b indented");
    }

    #[test]
    fn extract_checklist_ignores_plain_bullets_and_empty() {
        let body = "- 일반 bullet\n- [x] valid\n- [ ]\n- [ ] \n";
        let items = extract_checklist_items(body);
        // valid 하나만 (empty text 제외).
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "valid");
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "og-camp-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("C-001.md");

        let c = CampaignFile {
            frontmatter: sample_fm(),
            body: "본문\n\n- [ ] 할일".into(),
        };
        c.write(&path).unwrap();
        let read = CampaignFile::read(&path).unwrap();
        assert_eq!(read.frontmatter, c.frontmatter);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
