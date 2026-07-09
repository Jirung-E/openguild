//! DEV-016 (multi-file): 길드 규칙 — `.guild/rules/{slug}.md` 다중 파일.
//!
//! 팀 컨벤션 / 그라운드 룰 / 코드 스타일 / 배포 체크리스트 등 길드별 자유
//! 문서를 주제별로 분리. 기본은 frontmatter 없는 plain Markdown 이지만,
//! DEV-243(태그)부터 **선택적** `+++ TOML +++` frontmatter 를 지원 — 태그가
//! 하나도 없으면 frontmatter 자체를 생략해(파일 포맷 그대로) 기존 규칙
//! 파일과의 diff/하위호환을 최소화한다.
//!
//! 파일이 진리원 — DB 캐시 없음. server / GUI 가 직접 파일 IO.
//!
//! **Backward compat**: DEV-016 초기 구현은 단일 `.guild/rules.md`. 본 모듈은
//! list 시 자동 감지 → `.guild/rules/general.md` 로 이동. 한 번만 발생.
//!
//! Slug 규약:
//! - 파일명 stem (`release-process.md` → slug `release-process`).
//! - kebab-case 권장. ASCII / 한글 자유.
//! - 빈 string / `/` / `\` / `..` / NUL / `.` 시작은 거부 (디렉토리 traversal /
//!   숨김 파일 방지).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::fs::write_atomic;
use super::GuildPaths;

/// DEV-243: 규칙 frontmatter — 태그만. quest/library 와 달리 다른 메타(상태,
/// 생성일 등)는 없음 — 규칙은 순수 문서라 필요 최소한만.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleFrontmatter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// frontmatter(선택) + 본문으로 분리된 규칙 파일 표현.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFile {
    pub frontmatter: RuleFrontmatter,
    pub body: String,
}

impl RuleFile {
    /// frontmatter 가 없으면(기존 규칙 파일 전부 이 경우) 전체를 body 로,
    /// tags 는 빈 vec. `+++` 로 시작하는데 닫는 `+++` 를 못 찾으면 — 파싱
    /// 실패로 보지 않고 그냥 body 취급(사용자가 우연히 `+++` 로 시작하는
    /// 본문을 쓴 legacy 케이스 보호).
    pub fn parse(text: &str) -> Self {
        let after_open = text.strip_prefix("+++\n").or_else(|| text.strip_prefix("+++\r\n"));
        if let Some(after_open) = after_open {
            let mut pos = 0;
            while pos < after_open.len() {
                let line_end = after_open[pos..]
                    .find('\n')
                    .map(|i| pos + i)
                    .unwrap_or(after_open.len());
                let line = after_open[pos..line_end].trim_end_matches('\r');
                if line == "+++" {
                    let fm_text = &after_open[..pos];
                    let body_start = (line_end + 1).min(after_open.len());
                    let body = after_open[body_start..].to_string();
                    let frontmatter = toml::from_str(fm_text).unwrap_or_default();
                    return RuleFile { frontmatter, body };
                }
                pos = line_end + 1;
            }
        }
        RuleFile {
            frontmatter: RuleFrontmatter::default(),
            body: text.to_string(),
        }
    }

    /// 태그가 없으면 frontmatter 생략(순수 body 그대로) — 기존 파일 포맷 보존.
    pub fn serialize(&self) -> String {
        if self.frontmatter.tags.is_empty() {
            return self.body.clone();
        }
        let fm = toml::to_string_pretty(&self.frontmatter).unwrap_or_default();
        format!("+++\n{fm}+++\n{}", self.body)
    }
}

/// 한 규칙 파일의 표현. list 결과는 content(본문, frontmatter 제외) 포함 —
/// 모든 규칙이 보통 짧으므로 별도 lazy load 불필요.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleEntry {
    pub slug: String,
    pub content: String,
    /// DEV-243: 자유 태그 — quest/library 와 동일 패턴(색/설명은
    /// `.guild/tags/{slug}.toml` 공유 registry). 진리원은 frontmatter,
    /// DB 캐시 없음(규칙 전체가 파일 직독 — file-truth-db-cache 규칙 §4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Slug 형식 검증 — 디렉토리 traversal / 숨김 파일 / 빈 값 방지.
pub fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        anyhow::bail!("slug is empty");
    }
    if slug.contains('/') || slug.contains('\\') {
        anyhow::bail!("slug contains path separator: {slug:?}");
    }
    if slug.contains('\0') {
        anyhow::bail!("slug contains NUL: {slug:?}");
    }
    if slug == "." || slug == ".." {
        anyhow::bail!("slug cannot be . or ..");
    }
    if slug.starts_with('.') {
        anyhow::bail!("slug cannot start with . (hidden): {slug:?}");
    }
    // 윈도우 예약 이름 방어 (CON / PRN / AUX 등) — 일반적이지 않은 길드명에서는
    // 거의 안 부딪힘. 일단 strict 안 함.
    Ok(())
}

/// `.guild/rules.md` (legacy 단일) → `.guild/rules/general.md` 로 이동.
///
/// 조건:
/// - 단일 파일이 존재.
/// - `.guild/rules/general.md` 가 아직 없음 (덮어쓰기 방지).
///
/// 둘 다 만족하지 않으면 noop. 마이그레이션 자체가 실패해도 (e.g. 권한) Error
/// 반환 — 호출자가 fallback 판단.
fn migrate_legacy_single_file(paths: &GuildPaths) -> Result<()> {
    let legacy = paths.rules_path();
    if !legacy.exists() {
        return Ok(());
    }
    let target_dir = paths.rules_dir();
    let target = paths.rule_path("general");
    if target.exists() {
        return Ok(()); // 이미 마이그레이션됨 또는 사용자가 직접 general 만듦.
    }
    std::fs::create_dir_all(&target_dir)
        .with_context(|| format!("failed to create rules dir: {}", target_dir.display()))?;
    std::fs::rename(&legacy, &target).with_context(|| {
        format!(
            "failed to migrate legacy {} → {}",
            legacy.display(),
            target.display()
        )
    })?;
    Ok(())
}

/// 모든 규칙 파일 나열. 파일명 sort 순. legacy 단일 파일은 자동 마이그레이션
/// 후 포함됨.
pub fn list_rules(paths: &GuildPaths) -> Result<Vec<RuleEntry>> {
    // backward compat — 단일 파일이 있으면 multi-file 구조로 이동.
    let _ = migrate_legacy_single_file(paths); // 실패해도 list 는 계속 (legacy 안 보임).

    let dir = paths.rules_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<RuleEntry> = Vec::new();
    let read_dir = std::fs::read_dir(&dir)
        .with_context(|| format!("failed to read rules dir: {}", dir.display()))?;
    let mut paths_to_read: Vec<std::path::PathBuf> = Vec::new();
    for ent in read_dir {
        let ent = ent?;
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        paths_to_read.push(path);
    }
    paths_to_read.sort();
    for path in paths_to_read {
        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if slug.is_empty() {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read rule: {}", path.display()))?;
        let rf = RuleFile::parse(&raw);
        entries.push(RuleEntry {
            slug,
            content: rf.body,
            tags: rf.frontmatter.tags,
        });
    }
    Ok(entries)
}

/// 단일 규칙 읽기 — 본문만(frontmatter 는 투명하게 걷어냄). 파일 부재 시
/// `Ok(None)`. slug 검증 실패는 Err. 태그까지 필요하면 [`read_rule_entry`].
pub fn read_rule(paths: &GuildPaths, slug: &str) -> Result<Option<String>> {
    Ok(read_rule_entry(paths, slug)?.map(|e| e.content))
}

/// 단일 규칙 읽기 — 태그 포함 전체(slug/본문/태그).
pub fn read_rule_entry(paths: &GuildPaths, slug: &str) -> Result<Option<RuleEntry>> {
    validate_slug(slug)?;
    let _ = migrate_legacy_single_file(paths);
    let p = paths.rule_path(slug);
    if !p.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&p)
        .with_context(|| format!("failed to read rule: {}", p.display()))?;
    let rf = RuleFile::parse(&raw);
    Ok(Some(RuleEntry {
        slug: slug.to_string(),
        content: rf.body,
        tags: rf.frontmatter.tags,
    }))
}

/// 규칙 본문 작성 (atomic). 신규 / 기존 모두 허용 — 멱등. 빈 문자열도 OK.
/// 기존 태그가 있으면 보존(본문만 교체하는 mutation 이라 tags 는 대상 아님 —
/// quest 의 write_quest_file 이 existing tags 보존하는 것과 동일 의도).
pub fn write_rule(paths: &GuildPaths, slug: &str, content: &str) -> Result<()> {
    validate_slug(slug)?;
    let existing_tags = read_rule_entry(paths, slug)
        .ok()
        .flatten()
        .map(|e| e.tags)
        .unwrap_or_default();
    let rf = RuleFile {
        frontmatter: RuleFrontmatter { tags: existing_tags },
        body: content.to_string(),
    };
    write_rule_file(paths, slug, &rf)
}

/// DEV-243: 규칙의 tags 전체 교체. 본문은 그대로 두고 frontmatter 만 갱신.
pub fn set_rule_tags(paths: &GuildPaths, slug: &str, tags: Vec<String>) -> Result<RuleEntry> {
    use std::collections::HashSet;
    validate_slug(slug)?;
    let existing = read_rule_entry(paths, slug)?
        .ok_or_else(|| anyhow::anyhow!("rule not found: {slug}"))?;

    let mut seen: HashSet<String> = HashSet::new();
    let normalized: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .filter(|t| seen.insert(t.clone()))
        .collect();

    let rf = RuleFile {
        frontmatter: RuleFrontmatter { tags: normalized.clone() },
        body: existing.content,
    };
    write_rule_file(paths, slug, &rf)?;
    Ok(RuleEntry {
        slug: slug.to_string(),
        content: rf.body,
        tags: normalized,
    })
}

fn write_rule_file(paths: &GuildPaths, slug: &str, rf: &RuleFile) -> Result<()> {
    let dir = paths.rules_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create rules dir: {}", dir.display()))?;
    write_atomic(paths.rule_path(slug), &rf.serialize())
}

/// 신규 규칙 — 같은 slug 가 이미 있으면 Err. (`write_rule` 은 멱등 덮어쓰기.)
pub fn create_rule(paths: &GuildPaths, slug: &str, content: &str) -> Result<()> {
    validate_slug(slug)?;
    let p = paths.rule_path(slug);
    if p.exists() {
        anyhow::bail!("rule already exists: {slug}");
    }
    write_rule(paths, slug, content)
}

/// 규칙 삭제. 파일 없으면 Err.
pub fn delete_rule(paths: &GuildPaths, slug: &str) -> Result<()> {
    validate_slug(slug)?;
    let p = paths.rule_path(slug);
    if !p.exists() {
        anyhow::bail!("rule not found: {slug}");
    }
    std::fs::remove_file(&p)
        .with_context(|| format!("failed to delete rule: {}", p.display()))
}

/// 규칙 이름 변경. old 가 없거나 new 가 이미 있으면 Err. new slug 도 검증.
pub fn rename_rule(paths: &GuildPaths, old_slug: &str, new_slug: &str) -> Result<()> {
    validate_slug(old_slug)?;
    validate_slug(new_slug)?;
    if old_slug == new_slug {
        return Ok(());
    }
    let old = paths.rule_path(old_slug);
    let new = paths.rule_path(new_slug);
    if !old.exists() {
        anyhow::bail!("rule not found: {old_slug}");
    }
    if new.exists() {
        anyhow::bail!("target slug already exists: {new_slug}");
    }
    std::fs::rename(&old, &new).with_context(|| {
        format!(
            "failed to rename rule {} → {}",
            old.display(),
            new.display()
        )
    })
}

// ─────────────────────── Backward compat (deprecated) ───────────────────────
// 기존 단일 파일 API. 새 다중 파일 API 도입 후엔 호출하지 말 것.
// list_rules / read_rule 가 자동 마이그레이션을 수행하므로 본 함수들은 사실상
// noop 에 가까움. 아직 호출처 (구 server/ops/Tauri commands) 가 남아있으면
// migration 단계 동안만 유지.

/// (deprecated) 단일 파일 read. 마이그레이션 트리거 후 general slug 의 content.
pub fn read(paths: &GuildPaths) -> Result<Option<String>> {
    read_rule(paths, "general")
}

/// (deprecated) 단일 파일 write. general slug 로 위임.
pub fn write(paths: &GuildPaths, content: &str) -> Result<()> {
    write_rule(paths, "general", content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_paths(label: &str) -> (std::path::PathBuf, GuildPaths) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("og-rules-{label}-{ns}"));
        let dot = root.join(".guild");
        std::fs::create_dir_all(&dot).unwrap();
        (root.clone(), GuildPaths::new(root))
    }

    #[test]
    fn list_empty_when_no_dir_and_no_legacy() {
        let (root, p) = fresh_paths("empty");
        assert!(list_rules(&p).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_create_list_read_delete_cycle() {
        let (root, p) = fresh_paths("cycle");
        write_rule(&p, "branch-policy", "# branch=quest_id").unwrap();
        write_rule(&p, "release", "## CHANGELOG / tag / Release").unwrap();

        let v = list_rules(&p).unwrap();
        assert_eq!(v.len(), 2);
        // sort 순 — alphabetical.
        assert_eq!(v[0].slug, "branch-policy");
        assert_eq!(v[1].slug, "release");

        assert_eq!(
            read_rule(&p, "branch-policy").unwrap().as_deref(),
            Some("# branch=quest_id")
        );

        delete_rule(&p, "branch-policy").unwrap();
        let v = list_rules(&p).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].slug, "release");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_rule_rejects_duplicate() {
        let (root, p) = fresh_paths("dup");
        create_rule(&p, "x", "a").unwrap();
        assert!(create_rule(&p, "x", "b").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    // BUG-243(신규 기능 — 태그): 태그 없는 기존 파일은 순수 body 그대로.
    #[test]
    fn new_rule_has_no_frontmatter_until_tagged() {
        let (root, p) = fresh_paths("tags-none");
        create_rule(&p, "plain", "# 그냥 규칙").unwrap();
        let raw = std::fs::read_to_string(p.rule_path("plain")).unwrap();
        assert_eq!(raw, "# 그냥 규칙", "태그 없으면 frontmatter 생략");
        let entry = read_rule_entry(&p, "plain").unwrap().unwrap();
        assert!(entry.tags.is_empty());
        assert_eq!(entry.content, "# 그냥 규칙");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn set_rule_tags_adds_frontmatter_and_roundtrips() {
        let (root, p) = fresh_paths("tags-set");
        create_rule(&p, "commit-rules", "커밋은 이렇게").unwrap();

        let entry =
            set_rule_tags(&p, "commit-rules", vec!["git".into(), "convention".into()]).unwrap();
        assert_eq!(entry.tags, vec!["git", "convention"]);
        assert_eq!(entry.content, "커밋은 이렇게", "본문은 그대로");

        // 파일에 frontmatter 가 실제로 생김.
        let raw = std::fs::read_to_string(p.rule_path("commit-rules")).unwrap();
        assert!(raw.starts_with("+++\n"));
        assert!(raw.contains("커밋은 이렇게"));

        // 재조회해도 동일.
        let reread = read_rule_entry(&p, "commit-rules").unwrap().unwrap();
        assert_eq!(reread.tags, vec!["git", "convention"]);
        assert_eq!(reread.content, "커밋은 이렇게");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_rule_preserves_existing_tags() {
        let (root, p) = fresh_paths("tags-preserve");
        create_rule(&p, "x", "v1").unwrap();
        set_rule_tags(&p, "x", vec!["a".into()]).unwrap();

        // 본문만 교체(quest 편집기의 "저장"과 동일 경로) — 태그는 보존돼야 함.
        write_rule(&p, "x", "v2").unwrap();
        let entry = read_rule_entry(&p, "x").unwrap().unwrap();
        assert_eq!(entry.content, "v2");
        assert_eq!(entry.tags, vec!["a"], "본문 저장이 태그를 지우면 안 됨");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn set_rule_tags_normalizes_trim_dedupe_empty() {
        let (root, p) = fresh_paths("tags-normalize");
        create_rule(&p, "x", "v").unwrap();
        let entry = set_rule_tags(
            &p,
            "x",
            vec![" git ".into(), "git".into(), "".into(), "  ".into(), "b".into()],
        )
        .unwrap();
        assert_eq!(entry.tags, vec!["git", "b"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn legacy_body_starting_with_plus_plus_plus_not_misparsed() {
        // 우연히 `+++` 로 시작하지만 닫는 `+++` 가 없는 본문 — frontmatter 로
        // 오인하지 않고 그대로 body 로 취급.
        let rf = RuleFile::parse("+++\n이건 그냥 본문이지 frontmatter 아님");
        assert!(rf.frontmatter.tags.is_empty());
        assert_eq!(rf.body, "+++\n이건 그냥 본문이지 frontmatter 아님");
    }

    #[test]
    fn rename_moves_file() {
        let (root, p) = fresh_paths("rename");
        write_rule(&p, "old-name", "body").unwrap();
        rename_rule(&p, "old-name", "new-name").unwrap();
        assert!(read_rule(&p, "old-name").unwrap().is_none());
        assert_eq!(
            read_rule(&p, "new-name").unwrap().as_deref(),
            Some("body")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rename_to_existing_fails() {
        let (root, p) = fresh_paths("rename-conflict");
        write_rule(&p, "a", "1").unwrap();
        write_rule(&p, "b", "2").unwrap();
        assert!(rename_rule(&p, "a", "b").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn slug_validation_rejects_unsafe() {
        assert!(validate_slug("").is_err());
        assert!(validate_slug("foo/bar").is_err());
        assert!(validate_slug("foo\\bar").is_err());
        assert!(validate_slug("..").is_err());
        assert!(validate_slug(".hidden").is_err());
        assert!(validate_slug("ok-name").is_ok());
        assert!(validate_slug("한글규칙").is_ok());
    }

    #[test]
    fn legacy_single_file_migrated_on_list() {
        let (root, p) = fresh_paths("migrate");
        // legacy 단일 파일만 둠.
        std::fs::write(p.rules_path(), "# old single rule").unwrap();
        assert!(p.rules_path().exists());

        let v = list_rules(&p).unwrap();
        // 마이그레이션 → general slug.
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].slug, "general");
        assert_eq!(v[0].content, "# old single rule");
        // 원본 파일은 사라짐.
        assert!(!p.rules_path().exists());
        // 새 위치 존재.
        assert!(p.rule_path("general").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn legacy_migration_skipped_when_general_exists() {
        let (root, p) = fresh_paths("migrate-skip");
        // 두 파일 다 존재 — general 이 이미 있어서 마이그레이션 skip.
        std::fs::write(p.rules_path(), "OLD legacy").unwrap();
        std::fs::create_dir_all(p.rules_dir()).unwrap();
        std::fs::write(p.rule_path("general"), "NEW general").unwrap();

        let v = list_rules(&p).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].content, "NEW general");
        // legacy 파일은 그대로 (사용자 검토용).
        assert!(p.rules_path().exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    // 기존 backward compat 테스트 (read / write 단일 API).
    #[test]
    fn read_missing_returns_none() {
        let (root, p) = fresh_paths("none");
        assert!(read(&p).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_then_read_roundtrip_via_legacy_api() {
        let (root, p) = fresh_paths("rt");
        write(&p, "# Rules\n- branch = quest_id\n").unwrap();
        let got = read(&p).unwrap();
        assert_eq!(got.as_deref(), Some("# Rules\n- branch = quest_id\n"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_empty_is_allowed() {
        let (root, p) = fresh_paths("empty-content");
        write(&p, "").unwrap();
        let got = read(&p).unwrap();
        assert_eq!(got.as_deref(), Some(""));
        let _ = std::fs::remove_dir_all(&root);
    }
}
