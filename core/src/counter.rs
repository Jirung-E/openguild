//! Counter (type 의 last_number) 무결성 검증 + 자동 보정.
//!
//! 모든 type 의 `[counter].last_number` 가 그 type 의 실제 max quest 번호
//! 보다 작으면 ID 중복 위험 → 자동 보정 + 경고.
//!
//! 호출 시점: Store::open 직후 (다음 단계에서 통합), 또는 `openguild-server check-counters`.

use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::repo::{fs as repo_fs, GuildPaths, QuestFile, TypeFile};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CounterIssue {
    pub prefix: String,
    pub stored_last_number: i64,
    pub actual_max_number: i64,
    pub corrected_to: i64,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckReport {
    pub types_checked: usize,
    pub issues: Vec<CounterIssue>,
}

/// 검증 + (auto_fix=true) 보정.
///
/// 각 type 의 last_number 와 실제 quest 파일들의 max number 비교:
/// - last_number >= max → 정상 (잠재적으로 누락된 번호는 OK, 단조 증가만 보장)
/// - last_number < max → **데이터 손상 위험** (last_number 가 줄어들었거나 직접 편집됨)
///   - auto_fix=true: type 파일의 last_number 를 max 로 갱신.
pub fn check_counters(paths: &GuildPaths, auto_fix: bool) -> Result<CheckReport> {
    let mut report = CheckReport::default();

    // type prefix → max number 집계
    let mut max_by_prefix: HashMap<String, i64> = HashMap::new();
    let quest_paths = repo_fs::list_with_extension(paths.quests_dir(), "md")?;
    for path in &quest_paths {
        if let Ok(qf) = QuestFile::read(path) {
            let Some(prefix) = qf.type_prefix() else {
                continue;
            };
            let Ok(num) = qf.number() else {
                continue;
            };
            let entry = max_by_prefix.entry(prefix.to_string()).or_insert(0);
            if num > *entry {
                *entry = num;
            }
        }
    }

    // type 파일들 순회
    let type_paths = repo_fs::list_with_extension(paths.types_dir(), "toml")?;
    for tp in &type_paths {
        let mut tf = match TypeFile::read(tp) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("type 파일 파싱 실패 {}: {e:#}", tp.display());
                continue;
            }
        };
        report.types_checked += 1;
        let actual_max = max_by_prefix.get(&tf.prefix).copied().unwrap_or(0);
        if tf.counter.last_number < actual_max {
            let issue = CounterIssue {
                prefix: tf.prefix.clone(),
                stored_last_number: tf.counter.last_number,
                actual_max_number: actual_max,
                corrected_to: actual_max,
            };
            tracing::warn!(
                "counter inconsistency: type {} last_number = {}, but max quest number = {}",
                issue.prefix,
                issue.stored_last_number,
                issue.actual_max_number
            );
            if auto_fix {
                tf.counter.last_number = actual_max;
                tf.write(tp).with_context(|| {
                    format!("counter 보정 후 type 파일 쓰기 실패: {}", tp.display())
                })?;
            }
            report.issues.push(issue);
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{seed_guild_dir, Counter, QuestFile, QuestFrontmatter, TypeFile};

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-counter-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_quest(paths: &GuildPaths, slug: &str, status: &str) {
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: slug.into(),
                title: "t".into(),
                status: status.into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path(slug)).unwrap();
    }

    #[test]
    fn no_issues_when_counter_ge_max() {
        let dir = fresh_tmp("ok");
        seed_guild_dir(&dir).unwrap();
        let paths = GuildPaths::new(&dir);

        // DEV.toml last_number 를 5 로 (실제 quests 는 1개)
        let mut t = TypeFile::read(paths.type_path("DEV")).unwrap();
        t.counter.last_number = 5;
        t.write(paths.type_path("DEV")).unwrap();

        write_quest(&paths, "DEV-001", "open");

        let report = check_counters(&paths, false).unwrap();
        assert_eq!(report.types_checked, 3); // DEV, BUG, REQ
        assert!(report.issues.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_undercount() {
        let dir = fresh_tmp("under");
        seed_guild_dir(&dir).unwrap();
        let paths = GuildPaths::new(&dir);

        // DEV last_number = 0, but quest DEV-003 exists
        write_quest(&paths, "DEV-001", "open");
        write_quest(&paths, "DEV-003", "open");

        let report = check_counters(&paths, false).unwrap();
        assert_eq!(report.issues.len(), 1);
        let issue = &report.issues[0];
        assert_eq!(issue.prefix, "DEV");
        assert_eq!(issue.stored_last_number, 0);
        assert_eq!(issue.actual_max_number, 3);

        // auto_fix false 면 파일은 안 바뀜
        let t = TypeFile::read(paths.type_path("DEV")).unwrap();
        assert_eq!(t.counter.last_number, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_fix_corrects_counter() {
        let dir = fresh_tmp("fix");
        seed_guild_dir(&dir).unwrap();
        let paths = GuildPaths::new(&dir);

        write_quest(&paths, "DEV-001", "open");
        write_quest(&paths, "DEV-007", "open");
        write_quest(&paths, "BUG-002", "open");

        let report = check_counters(&paths, true).unwrap();
        // DEV: 0 < 7, BUG: 0 < 2
        assert_eq!(report.issues.len(), 2);

        let dev = TypeFile::read(paths.type_path("DEV")).unwrap();
        assert_eq!(dev.counter.last_number, 7);
        let bug = TypeFile::read(paths.type_path("BUG")).unwrap();
        assert_eq!(bug.counter.last_number, 2);

        // 재실행 — 이미 보정됐으니 이슈 없음
        let report2 = check_counters(&paths, false).unwrap();
        assert!(report2.issues.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_unknown_type_prefix() {
        let dir = fresh_tmp("unknown");
        seed_guild_dir(&dir).unwrap();
        let paths = GuildPaths::new(&dir);

        // type 파일이 없는 prefix 의 quest
        write_quest(&paths, "ZZZ-001", "open");

        let report = check_counters(&paths, false).unwrap();
        // 기본 3 type 만 체크 — ZZZ 는 무시
        assert_eq!(report.types_checked, 3);
        assert!(report.issues.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn counter_with_no_quests_is_fine() {
        let dir = fresh_tmp("empty");
        seed_guild_dir(&dir).unwrap();
        let paths = GuildPaths::new(&dir);

        // seed 가 counter = 0 으로 시작. 어떤 quest 도 없음.
        let report = check_counters(&paths, true).unwrap();
        assert_eq!(report.types_checked, 3);
        assert!(report.issues.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// counter 가 max 보다 크게 직접 편집된 경우 — 검증은 OK 통과 (단조 증가 보장).
    /// 단, "번호 건너뜀" 손실은 사용자가 알아야 — 본 검증 범위 X.
    #[test]
    fn higher_counter_than_max_is_allowed() {
        let dir = fresh_tmp("higher");
        seed_guild_dir(&dir).unwrap();
        let paths = GuildPaths::new(&dir);

        let mut t = TypeFile::read(paths.type_path("DEV")).unwrap();
        t.counter.last_number = 100;
        t.write(paths.type_path("DEV")).unwrap();

        write_quest(&paths, "DEV-005", "open");

        let report = check_counters(&paths, false).unwrap();
        assert!(report.issues.is_empty(), "100 >= 5 OK");

        let _ = std::fs::remove_dir_all(&dir);

        // Counter type import 사용 — 컴파일러 미사용 경고 방지.
        let _: Counter = Counter::default();
    }
}
