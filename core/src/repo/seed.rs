//! 기본 시드 데이터 — `openguild init` 또는 `migrate-to-files` 가 사용.
//!
//! 현 SQL `seed.sql` / migration 의 기본 types / statuses 와 동일 내용.

use anyhow::{Context, Result};

use super::{Counter, GuildPaths, StatusFile, TypeFile};

/// 기본 quest 타입 3개 (DEV / BUG / REQ).
pub fn default_types() -> Vec<TypeFile> {
    vec![
        TypeFile {
            prefix: "DEV".into(),
            color: "#4A90D9".into(),
            description: Some("일반 개발 작업".into()),
            counter: Counter { last_number: 0 },
        },
        TypeFile {
            prefix: "BUG".into(),
            color: "#E94F4F".into(),
            description: Some("버그 보고".into()),
            counter: Counter { last_number: 0 },
        },
        TypeFile {
            prefix: "REQ".into(),
            color: "#7BB87F".into(),
            description: Some("기능 요청".into()),
            counter: Counter { last_number: 0 },
        },
    ]
}

/// 기본 quest 상태 7개. (sort_order, slug, file)
pub fn default_statuses() -> Vec<(&'static str, StatusFile)> {
    vec![
        (
            "open",
            StatusFile {
                sort_order: 1,
                name_en: "Open".into(),
                name_ko: "게시됨".into(),
                color: "#8B95A1".into(),
                counts_as_done: false,
            },
        ),
        (
            "in_progress",
            StatusFile {
                sort_order: 2,
                name_en: "In Progress".into(),
                name_ko: "진행 중".into(),
                color: "#4A90D9".into(),
                counts_as_done: false,
            },
        ),
        (
            "testing",
            StatusFile {
                sort_order: 3,
                name_en: "Testing".into(),
                name_ko: "테스트 중".into(),
                color: "#A47AE2".into(),
                counts_as_done: false,
            },
        ),
        (
            "done",
            StatusFile {
                sort_order: 4,
                name_en: "Done".into(),
                name_ko: "완료".into(),
                color: "#7BB87F".into(),
                // DEV-093: done 은 자동 "완료" 카운트.
                counts_as_done: true,
            },
        ),
        (
            "returned",
            StatusFile {
                sort_order: 5,
                name_en: "Returned".into(),
                name_ko: "반려".into(),
                color: "#D97757".into(),
                counts_as_done: false,
            },
        ),
        (
            "cancelled",
            StatusFile {
                sort_order: 6,
                name_en: "Cancelled".into(),
                name_ko: "취소됨".into(),
                color: "#E94F4F".into(),
                // DEV-093: cancelled 도 "완료" 카운트 (= 더 이상 처리 안 함).
                counts_as_done: true,
            },
        ),
        (
            "on_hold",
            StatusFile {
                sort_order: 7,
                name_en: "On Hold".into(),
                name_ko: "보류".into(),
                color: "#F5A623".into(),
                counts_as_done: false,
            },
        ),
    ]
}

/// `.guild/` 디렉토리 구조 + 기본 시드 파일 + `.gitignore` 작성.
///
/// 이미 존재하는 파일은 건드리지 않음 (idempotent — 재실행 안전).
/// 새 길드 초기화 또는 기존 길드에 빠진 시드 추가 시 호출 가능.
pub fn seed_guild_dir<P: AsRef<std::path::Path>>(guild_root: P) -> Result<SeedReport> {
    let paths = GuildPaths::new(guild_root.as_ref());
    let mut report = SeedReport::default();

    // 디렉토리 생성
    for dir in [
        paths.dot_guild(),
        paths.quests_dir(),
        paths.types_dir(),
        paths.statuses_dir(),
        paths.backups_dir(),
        paths.snapshots_dir(),
        // DEV-069: 본문 첨부 (이미지 등) — git tracked. `![](attachments/x.png)`.
        paths.attachments_dir(),
        // DEV-180: 퀘스트 이력 사이드카 — git tracked.
        paths.history_dir(),
        // DEV-215: 도서관 문서 — git tracked.
        paths.library_dir(),
    ] {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create dir: {}", dir.display()))?;
    }

    // .gitignore
    let gitignore = paths.dot_guild().join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, GuildPaths::gitignore_content())
            .with_context(|| format!("failed to write {}", gitignore.display()))?;
        report.gitignore_created = true;
    }

    // types/
    for t in default_types() {
        let path = paths.type_path(&t.prefix);
        if !path.exists() {
            t.write(&path)?;
            report.types_created.push(t.prefix.clone());
        }
    }

    // statuses/
    for (slug, st) in default_statuses() {
        let filename = StatusFile::filename(st.sort_order, slug);
        let path = paths.statuses_dir().join(&filename);
        if !path.exists() {
            st.write(&path)?;
            report.statuses_created.push(slug.to_string());
        }
    }

    Ok(report)
}

/// `seed_guild_dir` 의 결과 — 무엇이 새로 만들어졌는지 보고.
#[derive(Debug, Default, Clone)]
pub struct SeedReport {
    pub gitignore_created: bool,
    pub types_created: Vec<String>,
    pub statuses_created: Vec<String>,
}

impl SeedReport {
    pub fn is_empty(&self) -> bool {
        !self.gitignore_created
            && self.types_created.is_empty()
            && self.statuses_created.is_empty()
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
        let p = std::env::temp_dir().join(format!("og-seed-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn default_types_has_three() {
        let types = default_types();
        assert_eq!(types.len(), 3);
        let prefixes: Vec<_> = types.iter().map(|t| t.prefix.as_str()).collect();
        assert_eq!(prefixes, vec!["DEV", "BUG", "REQ"]);
        for t in &types {
            assert_eq!(t.counter.last_number, 0);
        }
    }

    #[test]
    fn default_statuses_has_seven() {
        let statuses = default_statuses();
        assert_eq!(statuses.len(), 7);
        let slugs: Vec<_> = statuses.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            slugs,
            vec!["open", "in_progress", "testing", "done", "returned", "cancelled", "on_hold"]
        );
        // sort_order 가 1..=7
        for (i, (_, st)) in statuses.iter().enumerate() {
            assert_eq!(st.sort_order, (i + 1) as i64);
        }
    }

    #[test]
    fn seed_creates_full_structure() {
        let dir = fresh_tmp("full");
        let report = seed_guild_dir(&dir).unwrap();

        assert!(report.gitignore_created);
        assert_eq!(report.types_created.len(), 3);
        assert_eq!(report.statuses_created.len(), 7);

        let paths = GuildPaths::new(&dir);
        assert!(paths.dot_guild().is_dir());
        assert!(paths.quests_dir().is_dir());
        assert!(paths.types_dir().is_dir());
        assert!(paths.statuses_dir().is_dir());
        assert!(paths.backups_dir().is_dir());
        assert!(paths.snapshots_dir().is_dir());

        assert!(paths.dot_guild().join(".gitignore").is_file());
        assert!(paths.type_path("DEV").is_file());
        assert!(paths.type_path("BUG").is_file());
        assert!(paths.type_path("REQ").is_file());
        assert!(paths.statuses_dir().join("1-open.toml").is_file());
        assert!(paths.statuses_dir().join("3-testing.toml").is_file());
        assert!(paths.statuses_dir().join("5-returned.toml").is_file());
        assert!(paths.statuses_dir().join("7-on_hold.toml").is_file());

        // 파일 내용 검증
        let dev = TypeFile::read(paths.type_path("DEV")).unwrap();
        assert_eq!(dev.color, "#4A90D9");

        let open_st = StatusFile::read(paths.statuses_dir().join("1-open.toml")).unwrap();
        assert_eq!(open_st.name_en, "Open");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seed_is_idempotent() {
        let dir = fresh_tmp("idem");
        let first = seed_guild_dir(&dir).unwrap();
        assert!(!first.is_empty());

        let second = seed_guild_dir(&dir).unwrap();
        assert!(second.is_empty(), "second run should be no-op: {second:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seed_partial_recovery() {
        let dir = fresh_tmp("partial");
        // 일부만 미리 만들어두기
        seed_guild_dir(&dir).unwrap();
        let paths = GuildPaths::new(&dir);

        // 하나 삭제 후 재실행 → 그것만 채워짐
        std::fs::remove_file(paths.type_path("BUG")).unwrap();
        let report = seed_guild_dir(&dir).unwrap();
        assert_eq!(report.types_created, vec!["BUG".to_string()]);
        assert!(report.statuses_created.is_empty());
        assert!(!report.gitignore_created);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
