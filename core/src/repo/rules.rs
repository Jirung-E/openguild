//! DEV-016 (multi-file): 길드 규칙 — `.guild/rules/{slug}.md` 다중 파일.
//!
//! 팀 컨벤션 / 그라운드 룰 / 코드 스타일 / 배포 체크리스트 등 길드별 자유
//! 문서를 주제별로 분리. frontmatter 없는 plain Markdown.
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

/// 한 규칙 파일의 표현. list 결과는 content 포함 — 모든 규칙이 보통 짧으므로
/// 별도 lazy load 불필요. 큰 길드에서는 후속 quest 로 metadata-only list 도 가능.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleEntry {
    pub slug: String,
    pub content: String,
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
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read rule: {}", path.display()))?;
        entries.push(RuleEntry { slug, content });
    }
    Ok(entries)
}

/// 단일 규칙 읽기. 파일 부재 시 `Ok(None)`. slug 검증 실패는 Err.
pub fn read_rule(paths: &GuildPaths, slug: &str) -> Result<Option<String>> {
    validate_slug(slug)?;
    let _ = migrate_legacy_single_file(paths);
    let p = paths.rule_path(slug);
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("failed to read rule: {}", p.display()))?;
    Ok(Some(s))
}

/// 규칙 작성 (atomic). 신규 / 기존 모두 허용 — 멱등. 빈 문자열도 OK.
pub fn write_rule(paths: &GuildPaths, slug: &str, content: &str) -> Result<()> {
    validate_slug(slug)?;
    let dir = paths.rules_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create rules dir: {}", dir.display()))?;
    write_atomic(paths.rule_path(slug), content)
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
