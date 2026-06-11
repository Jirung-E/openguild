//! DEV-060: quest 템플릿 — `.guild/templates/{name}.md`.
//!
//! 형식: quest 파일과 같은 `+++` TOML frontmatter (필드 전부 선택) + 본문.
//! frontmatter 가 없으면 파일 전체가 본문 (plain markdown 템플릿).
//!
//! ```text
//! +++
//! title = "버그 리포트"     # 새 quest 의 기본 제목 (선택)
//! type = "BUG"              # 기본 type prefix (선택)
//! urgency = 2               # 기본 urgency (선택)
//! tags = ["triage"]         # 기본 tags (선택)
//! +++
//! ## 증상
//!
//! ## 재현
//! ```
//!
//! 적용 우선순위: CLI / GUI 의 명시 입력 > 템플릿 값 > 시스템 기본.
//! 파일이 진리원 — DB 캐시 없음 (양이 적고 read 빈도 낮음).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemplateFrontmatter {
    /// 새 quest 의 기본 제목.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 기본 type prefix (예: "DEV").
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_prefix: Option<String>,
    /// 기본 urgency (1..=4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub urgency: Option<i64>,
    /// 기본 tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TemplateFile {
    /// 파일명 stem (`bug-report.md` → `bug-report`).
    pub name: String,
    pub frontmatter: TemplateFrontmatter,
    /// 새 quest 의 기본 본문 (description).
    pub body: String,
}

impl TemplateFile {
    /// 문자열 파싱. frontmatter 없으면 전체가 body.
    pub fn parse(name: &str, s: &str) -> Result<Self> {
        let trimmed = s.trim_start();
        if let Some(rest) = trimmed.strip_prefix("+++") {
            let Some(end) = rest.find("\n+++") else {
                anyhow::bail!("template '{name}': closing +++ delimiter 없음");
            };
            let fm_str = &rest[..end];
            let frontmatter: TemplateFrontmatter = toml::from_str(fm_str)
                .with_context(|| format!("template '{name}': frontmatter TOML 파싱 실패"))?;
            // "\n+++" 뒤 — 줄 끝까지 스킵.
            let after = &rest[end + 4..];
            let body = after.strip_prefix('\n').unwrap_or(after).trim().to_string();
            Ok(Self {
                name: name.to_string(),
                frontmatter,
                body,
            })
        } else {
            Ok(Self {
                name: name.to_string(),
                frontmatter: TemplateFrontmatter::default(),
                body: s.trim().to_string(),
            })
        }
    }

    pub fn read<P: AsRef<Path>>(path: P) -> Result<Self> {
        let p = path.as_ref();
        let name = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string();
        let s = std::fs::read_to_string(p)
            .with_context(|| format!("failed to read template: {}", p.display()))?;
        Self::parse(&name, &s)
    }
}

/// `.guild/templates/*.md` 전체 로드 — 이름 알파벳 순. 디렉토리 없으면 빈 vec.
/// 파싱 실패 파일은 skip (Err 로 전체를 막지 않음 — 사용자가 작성 중일 수 있음).
pub fn list_templates(paths: &super::GuildPaths) -> Result<Vec<TemplateFile>> {
    let dir = paths.templates_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .with_context(|| format!("failed to read templates dir: {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    entries.sort();
    for p in entries {
        match TemplateFile::read(&p) {
            Ok(t) => out.push(t),
            Err(e) => {
                tracing::warn!("template skip: {} — {e:#}", p.display());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_with_frontmatter() {
        let t = TemplateFile::parse(
            "bug-report",
            "+++\ntitle = \"버그 리포트\"\ntype = \"BUG\"\nurgency = 2\ntags = [\"triage\"]\n+++\n\n## 증상\n\n## 재현\n",
        )
        .unwrap();
        assert_eq!(t.frontmatter.title.as_deref(), Some("버그 리포트"));
        assert_eq!(t.frontmatter.type_prefix.as_deref(), Some("BUG"));
        assert_eq!(t.frontmatter.urgency, Some(2));
        assert_eq!(t.frontmatter.tags, vec!["triage"]);
        assert!(t.body.starts_with("## 증상"));
    }

    #[test]
    fn parse_plain_markdown() {
        let t = TemplateFile::parse("plain", "## 그냥 본문\n내용").unwrap();
        assert!(t.frontmatter.title.is_none());
        assert_eq!(t.body, "## 그냥 본문\n내용");
    }

    #[test]
    fn parse_partial_frontmatter() {
        let t = TemplateFile::parse("partial", "+++\nurgency = 1\n+++\nbody").unwrap();
        assert!(t.frontmatter.title.is_none());
        assert_eq!(t.frontmatter.urgency, Some(1));
        assert_eq!(t.body, "body");
    }

    #[test]
    fn list_missing_dir_is_empty() {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-tpl-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = super::super::GuildPaths::new(&dir);
        assert!(list_templates(&paths).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_reads_md_files_sorted() {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-tpl-list-{ns}"));
        let paths = super::super::GuildPaths::new(&dir);
        std::fs::create_dir_all(paths.templates_dir()).unwrap();
        std::fs::write(paths.template_path("b-two"), "body b").unwrap();
        std::fs::write(paths.template_path("a-one"), "+++\nurgency = 1\n+++\nbody a").unwrap();
        // .md 아닌 파일은 무시.
        std::fs::write(paths.templates_dir().join("readme.txt"), "x").unwrap();
        let list = list_templates(&paths).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "a-one");
        assert_eq!(list[1].name, "b-two");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
