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
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

    /// 파일에 쓸 직렬화 — `parse()` 와 round-trip. frontmatter 가 전부 비면
    /// 본문만(plain markdown). 설정된 필드만 emit (skip_serializing_if).
    pub fn to_file_string(&self) -> Result<String> {
        let fm = toml::to_string(&self.frontmatter)
            .context("template frontmatter TOML 직렬화 실패")?;
        let mut out = String::new();
        if !fm.trim().is_empty() {
            out.push_str("+++\n");
            out.push_str(&fm);
            if !fm.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("+++\n\n");
        }
        out.push_str(self.body.trim_end());
        out.push('\n');
        Ok(out)
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

/// 템플릿 이름은 `.guild/templates` 바로 아래의 파일명 stem 하나여야 한다.
pub fn validate_template_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("템플릿 이름이 비어 있음");
    }
    if name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(|ch| ch.is_control())
    {
        anyhow::bail!("유효하지 않은 템플릿 이름: '{name}'");
    }
    Ok(name)
}

/// DEV-158: 템플릿 저장 — `.guild/templates/{name}.md`. 디렉토리 자동 생성.
/// `overwrite=false` 인데 파일이 이미 있으면 에러. 반환: 쓰여진 경로.
pub fn save_template(
    paths: &super::GuildPaths,
    tpl: &TemplateFile,
    overwrite: bool,
) -> Result<PathBuf> {
    // 파일명이 곧 식별자다. 경로 구분자/상위 디렉터리를 허용하면 HTTP 입력으로
    // `.guild/templates` 밖에 쓸 수 있으므로 단일 안전한 path segment만 받는다.
    let name = validate_template_name(&tpl.name)?;
    let path = paths.template_path(name);
    if path.exists() && !overwrite {
        anyhow::bail!(
            "템플릿 '{}' 이미 존재 — 덮어쓰려면 force 사용 ({})",
            tpl.name,
            path.display()
        );
    }
    std::fs::create_dir_all(paths.templates_dir())
        .with_context(|| format!("templates 디렉토리 생성 실패: {}", paths.templates_dir().display()))?;
    std::fs::write(&path, tpl.to_file_string()?)
        .with_context(|| format!("템플릿 쓰기 실패: {}", path.display()))?;
    Ok(path)
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
    fn template_name_rejects_path_segments() {
        assert!(validate_template_name("../escape").is_err());
        assert!(validate_template_name("nested/name").is_err());
        assert!(validate_template_name(r"nested\name").is_err());
        assert_eq!(validate_template_name("bug-report").unwrap(), "bug-report");
    }

    #[test]
    fn parse_partial_frontmatter() {
        let t = TemplateFile::parse("partial", "+++\nurgency = 1\n+++\nbody").unwrap();
        assert!(t.frontmatter.title.is_none());
        assert_eq!(t.frontmatter.urgency, Some(1));
        assert_eq!(t.body, "body");
    }

    #[test]
    fn to_file_string_roundtrips() {
        let mut t = TemplateFile {
            name: "bug".into(),
            ..Default::default()
        };
        t.frontmatter.title = Some("버그 리포트".into());
        t.frontmatter.type_prefix = Some("BUG".into());
        t.frontmatter.urgency = Some(2);
        t.frontmatter.tags = vec!["triage".into()];
        t.body = "## 증상\n\n## 재현".into();
        let s = t.to_file_string().unwrap();
        let back = TemplateFile::parse("bug", &s).unwrap();
        assert_eq!(back.frontmatter.title.as_deref(), Some("버그 리포트"));
        assert_eq!(back.frontmatter.type_prefix.as_deref(), Some("BUG"));
        assert_eq!(back.frontmatter.urgency, Some(2));
        assert_eq!(back.frontmatter.tags, vec!["triage"]);
        assert_eq!(back.body, "## 증상\n\n## 재현");
    }

    #[test]
    fn plain_body_roundtrips_without_frontmatter() {
        let t = TemplateFile {
            name: "plain".into(),
            frontmatter: TemplateFrontmatter::default(),
            body: "그냥 본문".into(),
        };
        let s = t.to_file_string().unwrap();
        assert!(!s.starts_with("+++"));
        assert_eq!(TemplateFile::parse("plain", &s).unwrap().body, "그냥 본문");
    }

    #[test]
    fn save_template_writes_and_guards_overwrite() {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-tpl-save-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = super::super::GuildPaths::new(&dir);
        let t = TemplateFile {
            name: "feature".into(),
            ..Default::default()
        };
        let p = save_template(&paths, &t, false).unwrap();
        assert!(p.exists());
        // 덮어쓰기 금지면 에러.
        assert!(save_template(&paths, &t, false).is_err());
        // force 면 성공.
        assert!(save_template(&paths, &t, true).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
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
