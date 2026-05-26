//! Counter 정합성 보정 — file (type.toml) + SQL (index.db.quest_counters) 동시 갱신.
//!
//! 두 가지 drift 를 모두 잡는다 (BUG-003 시나리오):
//! 1. **file drift** — `type.toml [counter] last_number < 실제 max quest number`.
//!    `core::counter::check_counters` 가 검사 + auto_fix 시 파일 갱신.
//! 2. **SQL drift** — `quest_counters.last_number ≠ type.toml [counter] last_number`.
//!    본 모듈이 직접 검사 + auto_fix 시 SQL 갱신 (file 을 source of truth 로).
//!
//! file 만 OK 인데 SQL 만 깨진 경우 (외부 수동 SQL 편집 / migration 실수 / 옛
//! check-counters 실행 흔적) — 기존 check_counters 는 검사 안 함 → 다음 quest new
//! 가 UNIQUE constraint 실패. 본 모듈이 그 격차를 메움.
//!
//! 호출자: `openguild-server check-counters --fix` (server CLI).

use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::counter::{check_counters, CheckReport};
use crate::repo::{fs as repo_fs, TypeFile};
use crate::Store;

/// SQL 단독 drift 보고.
#[derive(Debug, Clone)]
pub struct SqlDriftIssue {
    pub prefix: String,
    pub file_last_number: i64,
    pub sql_last_number: i64,
    pub synced_to: i64,
}

#[derive(Debug, Default, Clone)]
pub struct CombinedReport {
    /// file-level (last_number < actual max).
    pub file_report: CheckReport,
    /// SQL drift (SQL last_number ≠ file last_number) — file-level 보정과 무관.
    pub sql_drift: Vec<SqlDriftIssue>,
}

impl CombinedReport {
    pub fn has_any_issue(&self) -> bool {
        !self.file_report.issues.is_empty() || !self.sql_drift.is_empty()
    }
}

/// file + SQL 보정 결합.
///
/// 1. file-only `check_counters` — 파일 카운터가 실제 max 보다 낮은 케이스 처리.
/// 2. 각 type 파일의 last_number 와 SQL `quest_counters.last_number` 비교. 다르면
///    SQL drift 로 보고. `auto_fix=true` 시 file 값을 truth 로 SQL 갱신.
///
/// 두 단계 모두 dry-run (auto_fix=false) 시 보고만.
pub async fn check_and_fix_counters(
    store: &Store,
    auto_fix: bool,
) -> Result<CombinedReport> {
    // ── 1. file drift (기존 동작) ──
    let file_report = check_counters(&store.paths, auto_fix)
        .context("counter file 검사 실패")?;
    let mut combined = CombinedReport {
        file_report,
        ..Default::default()
    };

    // 1.1 file fix 가 발생했으면 SQL 도 같이 갱신 (BUG-003 한방향 사례).
    if auto_fix {
        for issue in &combined.file_report.issues {
            sync_sql_counter(store, &issue.prefix, issue.corrected_to).await?;
        }
    }

    // ── 2. SQL drift ──
    // 모든 type 파일을 다시 읽어 (방금 fix 한 결과 반영된 값) SQL 과 비교.
    let type_paths = repo_fs::list_with_extension(store.paths.types_dir(), "toml")
        .context("types 디렉토리 읽기 실패")?;

    // 한 번에 SQL counter 들을 받아 prefix → SQL last_number 맵 구성.
    let sql_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT t.prefix, c.last_number FROM quest_counters c
         JOIN quest_types t ON t.id = c.quest_type_id",
    )
    .fetch_all(&store.index_pool)
    .await
    .context("quest_counters 조회 실패")?;
    let sql_by_prefix: HashMap<String, i64> = sql_rows.into_iter().collect();

    for tp in &type_paths {
        let tf = match TypeFile::read(tp) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let file_n = tf.counter.last_number;
        // index.db 에 type 이 없으면 (reindex 안 한 새 type) skip — 다음 reindex 가 동기화.
        let Some(&sql_n) = sql_by_prefix.get(&tf.prefix) else {
            tracing::warn!(
                "SQL drift 검사: type {} 가 index.db 에 없음 — reindex 후 재시도 권장",
                tf.prefix
            );
            continue;
        };
        if file_n == sql_n {
            continue;
        }
        // file 을 truth 로 SQL 동기화.
        let issue = SqlDriftIssue {
            prefix: tf.prefix.clone(),
            file_last_number: file_n,
            sql_last_number: sql_n,
            synced_to: file_n,
        };
        tracing::warn!(
            "SQL counter drift: type {} file={}, sql={}",
            issue.prefix, file_n, sql_n
        );
        if auto_fix {
            sync_sql_counter(store, &tf.prefix, file_n).await?;
        }
        combined.sql_drift.push(issue);
    }

    Ok(combined)
}

/// `quest_counters.last_number` 를 prefix 의 type id 로 UPDATE.
async fn sync_sql_counter(store: &Store, prefix: &str, value: i64) -> Result<()> {
    let type_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM quest_types WHERE prefix = ?")
            .bind(prefix)
            .fetch_optional(&store.index_pool)
            .await
            .with_context(|| format!("quest_types prefix={prefix} 조회 실패"))?;
    let Some(type_id) = type_id else {
        tracing::warn!("SQL 동기화: type {prefix} 가 index.db 에 없음 — skip");
        return Ok(());
    };
    sqlx::query("UPDATE quest_counters SET last_number = ? WHERE quest_type_id = ?")
        .bind(value)
        .bind(type_id)
        .execute(&store.index_pool)
        .await
        .with_context(|| format!("quest_counters UPDATE 실패 (type {prefix}, → {value})"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed_guild_dir;
    use crate::repo::{QuestFile, QuestFrontmatter};

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-ops-counter-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_quest_file(paths: &crate::repo::GuildPaths, slug: &str) {
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: slug.into(),
                title: "t".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
            },
            description: String::new(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path(slug)).unwrap();
    }

    async fn sql_counter(store: &Store, prefix: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT c.last_number FROM quest_counters c
             JOIN quest_types t ON t.id = c.quest_type_id
             WHERE t.prefix = ?",
        )
        .bind(prefix)
        .fetch_one(&store.index_pool)
        .await
        .unwrap()
    }

    /// 시나리오 1 — file < max (legacy BUG-003). file + SQL 둘 다 보정.
    #[tokio::test]
    async fn file_below_max_fixes_both() {
        let dir = fresh_tmp("file-low");
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        write_quest_file(&store.paths, "DEV-005");
        // file counter 는 그대로 (0).

        let report = check_and_fix_counters(&store, true).await.unwrap();
        assert_eq!(report.file_report.issues.len(), 1);
        assert_eq!(sql_counter(&store, "DEV").await, 5);
        assert_eq!(
            TypeFile::read(store.paths.type_path("DEV"))
                .unwrap()
                .counter
                .last_number,
            5
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 시나리오 2 — file 은 OK, SQL 만 깨짐. file 을 truth 로 SQL 갱신.
    /// (사용자가 dogfood 에서 보고한 정확한 케이스.)
    #[tokio::test]
    async fn sql_drift_only_synced_from_file() {
        let dir = fresh_tmp("sql-only");
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();

        // file counter 를 6 으로 (실제 quest 없어도 단조 증가만 보장이라 OK).
        let mut tf = TypeFile::read(store.paths.type_path("BUG")).unwrap();
        tf.counter.last_number = 6;
        tf.write(store.paths.type_path("BUG")).unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        // reindex 직후엔 SQL counter 가 file 값 6.
        assert_eq!(sql_counter(&store, "BUG").await, 6);

        // 외부에서 SQL 만 망가뜨림 (수동 UPDATE / 옛 fix 실수 등).
        sqlx::query(
            "UPDATE quest_counters SET last_number = 0
             WHERE quest_type_id = (SELECT id FROM quest_types WHERE prefix = 'BUG')",
        )
        .execute(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(sql_counter(&store, "BUG").await, 0);

        // 파일은 OK 라 check_counters 는 issue 없다고 보고.
        let dry = check_and_fix_counters(&store, false).await.unwrap();
        assert!(dry.file_report.issues.is_empty(), "file 은 정상");
        assert_eq!(dry.sql_drift.len(), 1, "SQL drift 1건 보고");
        assert_eq!(dry.sql_drift[0].prefix, "BUG");
        assert_eq!(dry.sql_drift[0].file_last_number, 6);
        assert_eq!(dry.sql_drift[0].sql_last_number, 0);

        // dry-run 은 SQL 안 건드림.
        assert_eq!(sql_counter(&store, "BUG").await, 0);

        // fix 실행 → SQL 도 6 으로.
        let fix = check_and_fix_counters(&store, true).await.unwrap();
        assert!(fix.file_report.issues.is_empty());
        assert_eq!(fix.sql_drift.len(), 1);
        assert_eq!(sql_counter(&store, "BUG").await, 6);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 시나리오 3 — create_quest 가 SQL drift 를 self-heal (BUG-003 + 외부 편집 시나리오).
    ///
    /// 이전에는 drift 상태에서 create 가 UNIQUE 충돌로 실패했지만, 그 후
    /// `services::quests::create` 에 counter self-heal SQL 한 줄 추가 → 명시적
    /// `check-counters --fix` 없어도 create 가 max(actual)+1 부여. file 의
    /// last_number 자체는 별도 (admin reindex / check-counters 가 책임).
    #[tokio::test]
    async fn create_self_heals_sql_drift_without_explicit_fix() {
        let dir = fresh_tmp("create-after-sql-fix");
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();

        for n in 1..=6 {
            write_quest_file(&store.paths, &format!("BUG-{n:03}"));
        }
        let mut tf = TypeFile::read(store.paths.type_path("BUG")).unwrap();
        tf.counter.last_number = 6;
        tf.write(store.paths.type_path("BUG")).unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        assert_eq!(sql_counter(&store, "BUG").await, 6);

        // 외부에서 SQL 만 0 으로 망가뜨림.
        sqlx::query(
            "UPDATE quest_counters SET last_number = 0
             WHERE quest_type_id = (SELECT id FROM quest_types WHERE prefix = 'BUG')",
        )
        .execute(&store.index_pool)
        .await
        .unwrap();

        let bug_type_id: i64 =
            sqlx::query_scalar("SELECT id FROM quest_types WHERE prefix = 'BUG'")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();

        // self-heal 덕분에 명시적 fix 없이도 create 가 성공.
        let new = crate::ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: bug_type_id,
                title: "self-healed".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .expect("self-heal 후 create 성공해야 함");
        assert_eq!(new.quest_id, "BUG-007");
        assert_eq!(sql_counter(&store, "BUG").await, 7);

        // 새 quest 파일 (BUG-007.md) 도 디스크에 생겼으므로 check-counters
        // 가 file last_number=6 < actual max=7 을 잡아 file 도 7 로 보정 →
        // file + SQL 둘 다 7 로 정합 (sql_drift 0건).
        let fix = check_and_fix_counters(&store, true).await.unwrap();
        assert_eq!(fix.file_report.issues.len(), 1);
        assert_eq!(fix.sql_drift.len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn dry_run_no_writes() {
        let dir = fresh_tmp("dry");
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        write_quest_file(&store.paths, "DEV-003");
        // file counter 도 깨끗하게 두기 (실제 max=3, file=0).

        let pre_sql = sql_counter(&store, "DEV").await;
        let pre_file = TypeFile::read(store.paths.type_path("DEV"))
            .unwrap()
            .counter
            .last_number;

        let report = check_and_fix_counters(&store, false).await.unwrap();
        assert!(report.has_any_issue());

        let post_sql = sql_counter(&store, "DEV").await;
        let post_file = TypeFile::read(store.paths.type_path("DEV"))
            .unwrap()
            .counter
            .last_number;
        assert_eq!(post_sql, pre_sql, "dry-run 은 SQL 안 건드림");
        assert_eq!(post_file, pre_file, "dry-run 은 file 안 건드림");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
