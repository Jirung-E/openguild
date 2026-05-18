//! Counter 정합성 보정 — file (type.toml) + SQL (index.db.quest_counters) 동시 갱신.
//!
//! `core::counter::check_counters` 는 file 만 검사/수정한다. 본 모듈은 그 위에
//! SQL `quest_counters` 동기화를 얹어, drift 보정 후 즉시 새 quest 생성이 막히지
//! 않도록 보장.
//!
//! 호출자: `openguild-server check-counters --fix` (server CLI).

use anyhow::{Context, Result};

use crate::counter::{check_counters, CheckReport};
use crate::Store;

/// file + SQL 보정 결합.
///
/// `auto_fix=false` 면 file-only `check_counters` 와 동일 (보고만).
/// `auto_fix=true` 면 추가로 각 issue 의 prefix → quest_type_id 조회 후
/// `UPDATE quest_counters SET last_number = ? WHERE quest_type_id = ?` 실행.
///
/// 다음 `create_quest` 가 `UPDATE ... last_number = last_number + 1 RETURNING ...`
/// 로 next number 를 받아 INSERT 시도할 때 file 의 max 와 충돌하지 않게 됨
/// (BUG-003 핵심 시나리오).
pub async fn check_and_fix_counters(store: &Store, auto_fix: bool) -> Result<CheckReport> {
    let report = check_counters(&store.paths, auto_fix)
        .context("counter file 검사 실패")?;

    if !auto_fix {
        return Ok(report);
    }

    // SQL 동기화 — auto_fix 일 때만 (검사만 한 경우엔 변경 없음).
    for issue in &report.issues {
        let type_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM quest_types WHERE prefix = ?",
        )
        .bind(&issue.prefix)
        .fetch_optional(&store.index_pool)
        .await
        .with_context(|| format!("quest_types prefix={} 조회 실패", issue.prefix))?;

        let Some(type_id) = type_id else {
            // index.db 에 type 이 없으면 (reindex 미실행 등) skip — 다음 reindex 시 정합.
            tracing::warn!(
                "counter SQL 동기화: type {} 가 index.db 에 없음 — skip",
                issue.prefix
            );
            continue;
        };

        sqlx::query(
            "UPDATE quest_counters SET last_number = ? WHERE quest_type_id = ?",
        )
        .bind(issue.corrected_to)
        .bind(type_id)
        .execute(&store.index_pool)
        .await
        .with_context(|| {
            format!(
                "quest_counters UPDATE 실패 (type {}, last_number → {})",
                issue.prefix, issue.corrected_to
            )
        })?;
    }

    Ok(report)
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
            },
            description: String::new(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path(slug)).unwrap();
    }

    /// 핵심 시나리오: file 에 DEV-005 가 있고 SQL counter 는 0 인 drift 상태에서
    /// check_and_fix_counters(--fix) → file + SQL 모두 5 로 정합.
    #[tokio::test]
    async fn fix_propagates_to_sql_counter() {
        let dir = fresh_tmp("propagate");
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();

        // SQL 의 quest_counters 에 row 가 있는지 (Store::open 직후엔 비어있음 — reindex
        // 가 채움). 본 테스트는 reindex 후 시작.
        // dogfood 와 같은 시나리오: 파일을 외부에서 만들고 reindex 안 한 경우는
        // 다른 테스트로. 여기는 reindex 한 뒤 새 파일이 추가된 시나리오를 모사.

        // 1. reindex 로 SQL 초기 동기화 (현재 file 의 max=0, counter=0 정합).
        crate::reindex::reindex(&store).await.unwrap();

        // 2. 외부에서 파일 추가 — DEV-005 (number=5).
        write_quest_file(&store.paths, "DEV-005");
        // file counter 도 0 → 5 로 직접 수정해야 file-side 가 drift.
        // 단순화를 위해 ops 가 보는 입력은 "file 에 DEV-005 있고 counter 는 0 인 상태".
        // 그래서 counter file 은 그대로 (0 유지).

        // 3. SQL 도 0 (reindex 후 그대로).
        let pre_sql: i64 = sqlx::query_scalar(
            "SELECT last_number FROM quest_counters WHERE quest_type_id = (SELECT id FROM quest_types WHERE prefix = 'DEV')",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(pre_sql, 0);

        // 4. check_and_fix_counters(--fix) 실행.
        let report = check_and_fix_counters(&store, true).await.unwrap();
        assert_eq!(report.issues.len(), 1, "DEV drift 1건 보고");
        assert_eq!(report.issues[0].prefix, "DEV");
        assert_eq!(report.issues[0].actual_max_number, 5);

        // 5. SQL 도 5 로 갱신됐는지.
        let post_sql: i64 = sqlx::query_scalar(
            "SELECT last_number FROM quest_counters WHERE quest_type_id = (SELECT id FROM quest_types WHERE prefix = 'DEV')",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(post_sql, 5, "SQL counter 도 보정되어야 함 (BUG-003 핵심)");

        // 6. file 도 보정됐는지 (기존 check_counters 동작 확인).
        let tf = crate::repo::TypeFile::read(store.paths.type_path("DEV")).unwrap();
        assert_eq!(tf.counter.last_number, 5);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn no_fix_means_no_sql_update() {
        let dir = fresh_tmp("noop");
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        write_quest_file(&store.paths, "DEV-003");

        let pre: i64 = sqlx::query_scalar(
            "SELECT last_number FROM quest_counters WHERE quest_type_id = (SELECT id FROM quest_types WHERE prefix = 'DEV')",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();

        let report = check_and_fix_counters(&store, false).await.unwrap();
        assert_eq!(report.issues.len(), 1);

        let post: i64 = sqlx::query_scalar(
            "SELECT last_number FROM quest_counters WHERE quest_type_id = (SELECT id FROM quest_types WHERE prefix = 'DEV')",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(post, pre, "auto_fix=false 면 SQL 그대로");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn fix_then_create_quest_succeeds() {
        // BUG-003 의 핵심 사용자 시나리오 재현:
        // drift 상태 → check-counters --fix → 즉시 quest 새로 만들기 → 성공해야 함.
        let dir = fresh_tmp("create-after-fix");
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        // 외부 파일로 BUG-001 가 생긴 상황 (사용자가 직접 만들었거나 import).
        write_quest_file(&store.paths, "BUG-001");
        // 이 상태에서 ops::create_quest 호출하면 UNIQUE constraint fail (BUG-003 증상).
        // fix 후엔 성공해야 함.

        // 먼저 reindex 로 BUG-001 을 index 에 넣음 (file → SQL).
        crate::reindex::reindex(&store).await.unwrap();
        // reindex 가 counter 도 max=1 로 동기화해주는지? — 확인 필요.
        // (만약 그렇다면 BUG-003 은 reindex 직후엔 발생 안 함 — 외부 파일을 만들고
        // reindex 안 한 경우만 문제.)
        // 본 테스트는 단순화 — fix 후 create 가 동작하는지만 검증.

        // 일부러 SQL counter 를 0 으로 되돌려 drift 만들기.
        sqlx::query(
            "UPDATE quest_counters SET last_number = 0 WHERE quest_type_id = (SELECT id FROM quest_types WHERE prefix = 'BUG')",
        )
        .execute(&store.index_pool)
        .await
        .unwrap();
        // file counter 도 0 으로 (직접 편집 시뮬).
        let mut tf = crate::repo::TypeFile::read(store.paths.type_path("BUG")).unwrap();
        tf.counter.last_number = 0;
        tf.write(store.paths.type_path("BUG")).unwrap();

        // check_and_fix 실행.
        let report = check_and_fix_counters(&store, true).await.unwrap();
        assert_eq!(report.issues.len(), 1);

        // 이제 새 BUG quest 생성 시도 — BUG-002 가 되어야 함.
        let bug_type_id: i64 = sqlx::query_scalar(
            "SELECT id FROM quest_types WHERE prefix = 'BUG'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();

        let new = crate::ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: bug_type_id,
                title: "후속".into(),
                description: None,
                status_id: 1,
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .expect("BUG-003 fix 후엔 create 가 성공해야 함");

        assert_eq!(new.quest_id, "BUG-002");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
