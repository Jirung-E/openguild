//! `Store` — 새 services 가 사용하는 컨텍스트.
//!
//! 보유:
//! - `paths`: `.guild/` 디렉토리 경로 헬퍼
//! - `index_pool`: `.guild/index.db` — 빠른 쿼리용 캐시
//! - `journal_pool`: `.guild/backups/journal.db` — AOF
//!
//! 모든 mutation 은 이 순서로:
//!   1. 검증 (`index_pool` 쿼리)
//!   2. journal INSERT (`journal_pool`)
//!   3. 파일 작성 (`paths` + `repo::*::write`)
//!   4. `index_pool` UPDATE
//!
//! 읽기는 `index_pool` 만 사용 (파일 직접 읽기는 reindex 시점만).

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use crate::db;
use crate::repo::GuildPaths;

/// Path 를 SQLite URL (`sqlite:<path>?mode=<mode>`) 로 변환.
///
/// Windows 의 `\\?\C:\…` extended-length prefix 를 제거해야 함:
/// `recents::add` 가 `canonicalize` 한 path 를 저장하는데, Windows 에선 그
/// 결과가 `\\?\` 로 시작 → `replace('\\', '/')` 후 `//?/C:/…` 가 되고,
/// SQLite URL 파서가 첫 `?` 를 query string 시작으로 오인해서 깨짐.
/// (`migrate.rs` 에 동일한 처리가 이미 있음 — DEV-052 후속 4회차에 공통화.)
fn sqlite_file_url(path: &std::path::Path, mode: &str) -> String {
    let raw = path.to_string_lossy();
    let cleaned = raw
        .trim_start_matches(r"\\?\")
        .trim_start_matches(r"\\\\?\\")
        .replace('\\', "/");
    format!("sqlite:{cleaned}?mode={mode}")
}

#[derive(Clone)]
pub struct Store {
    pub paths: GuildPaths,
    pub index_pool: SqlitePool,
    pub journal_pool: SqlitePool,
}

impl Store {
    /// 길드 루트 경로로 Store 생성. 필요한 디렉토리 / DB 가 없으면 만들고 마이그레이션.
    /// 시드는 별도 — 호출자가 `seed::seed_guild_dir` 명시 호출.
    pub async fn open<P: AsRef<std::path::Path>>(guild_root: P) -> Result<Self> {
        let paths = GuildPaths::new(guild_root.as_ref());

        // 디렉토리 보장
        std::fs::create_dir_all(paths.dot_guild())?;
        std::fs::create_dir_all(paths.backups_dir())?;

        // index.db
        let index_url = sqlite_file_url(&paths.index_db(), "rwc");
        let index_pool = db::create_pool(&index_url)
            .await
            .with_context(|| format!("failed to open index db: {index_url}"))?;
        db::run_migrations(&index_pool)
            .await
            .context("failed to migrate index db")?;

        // journal.db
        let journal_url = sqlite_file_url(&paths.journal_db(), "rwc");
        let journal_pool = db::create_pool(&journal_url)
            .await
            .with_context(|| format!("failed to open journal db: {journal_url}"))?;
        journal::ensure_schema(&journal_pool)
            .await
            .context("failed to init journal schema")?;

        Ok(Self {
            paths,
            index_pool,
            journal_pool,
        })
    }

    /// Test 용 — 메모리 풀로 생성 (실제 디스크 IO 없음).
    /// 단, paths 는 실제 경로 (파일 IO 테스트 시 사용).
    #[cfg(test)]
    pub async fn open_in_memory<P: AsRef<std::path::Path>>(guild_root: P) -> Result<Self> {
        let paths = GuildPaths::new(guild_root.as_ref());
        std::fs::create_dir_all(paths.dot_guild())?;
        std::fs::create_dir_all(paths.backups_dir())?;

        let index_pool = db::create_pool("sqlite::memory:").await?;
        db::run_migrations(&index_pool).await?;

        let journal_pool = db::create_pool("sqlite::memory:").await?;
        journal::ensure_schema(&journal_pool).await?;

        Ok(Self {
            paths,
            index_pool,
            journal_pool,
        })
    }
}

/// Journal 관련 모듈 (AOF append).
pub mod journal {
    use anyhow::Result;
    use serde::Serialize;
    use sqlx::SqlitePool;

    /// journal.db 의 ops 테이블 스키마 보장.
    pub async fn ensure_schema(pool: &SqlitePool) -> Result<()> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS ops (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                op TEXT NOT NULL,
                args TEXT NOT NULL,
                result TEXT
            )"#,
        )
        .execute(pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_ops_ts ON ops(ts)")
            .execute(pool)
            .await?;
        Ok(())
    }

    /// 한 줄 append. `args` / `result` 는 JSON serialize 가능한 무엇이든.
    pub async fn append<A: Serialize, R: Serialize>(
        pool: &SqlitePool,
        op: &str,
        args: &A,
        result: Option<&R>,
    ) -> Result<i64> {
        let ts = now_iso();
        let args_json = serde_json::to_string(args)?;
        let result_json = match result {
            Some(r) => Some(serde_json::to_string(r)?),
            None => None,
        };
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO ops (ts, op, args, result) VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(&ts)
        .bind(op)
        .bind(&args_json)
        .bind(&result_json)
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    /// 행 개수.
    pub async fn count(pool: &SqlitePool) -> Result<i64> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ops")
            .fetch_one(pool)
            .await?;
        Ok(n)
    }

    /// ts 순서대로 모든 op 가져오기 (replay 용).
    pub async fn list_all(pool: &SqlitePool) -> Result<Vec<OpRow>> {
        let rows = sqlx::query_as::<_, OpRow>(
            "SELECT id, ts, op, args, result FROM ops ORDER BY id ASC",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// truncate (snapshot 만들고 난 뒤).
    pub async fn truncate(pool: &SqlitePool) -> Result<()> {
        sqlx::query("DELETE FROM ops").execute(pool).await?;
        // SQLite 가 rowid 도 리셋되도록
        sqlx::query("DELETE FROM sqlite_sequence WHERE name = 'ops'")
            .execute(pool)
            .await
            .ok(); // sqlite_sequence 가 없을 수 있음 — 에러 무시
        Ok(())
    }

    #[derive(Debug, Clone, sqlx::FromRow)]
    pub struct OpRow {
        pub id: i64,
        pub ts: String,
        pub op: String,
        pub args: String,
        pub result: Option<String>,
    }

    /// chrono 의존 없이 timestamp 문자열. ISO 8601 UTC.
    fn now_iso() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let (y, mo, d, h, mi, s) = epoch_to_ymdhms(secs);
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    }

    fn epoch_to_ymdhms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
        let s = (secs % 60) as u32;
        let mi = ((secs / 60) % 60) as u32;
        let h = ((secs / 3600) % 24) as u32;
        let mut days = (secs / 86400) as i64;
        let mut year: i64 = 1970;
        loop {
            let dy = if is_leap(year) { 366 } else { 365 };
            if days >= dy {
                days -= dy;
                year += 1;
            } else {
                break;
            }
        }
        let dim = days_in_months(year);
        let mut month: usize = 0;
        while month < 12 && days >= dim[month] as i64 {
            days -= dim[month] as i64;
            month += 1;
        }
        (year as u32, (month + 1) as u32, (days + 1) as u32, h, mi, s)
    }

    fn is_leap(y: i64) -> bool {
        (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
    }

    fn days_in_months(y: i64) -> [u32; 12] {
        [
            31,
            if is_leap(y) { 29 } else { 28 },
            31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
        ]
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
        let p = std::env::temp_dir().join(format!("og-store-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn open_creates_directories_and_dbs() {
        let dir = fresh_tmp("open");
        let store = Store::open(&dir).await.unwrap();
        assert!(store.paths.dot_guild().is_dir());
        assert!(store.paths.backups_dir().is_dir());
        assert!(store.paths.index_db().is_file());
        assert!(store.paths.journal_db().is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn journal_append_increments_count() {
        let dir = fresh_tmp("journal");
        let store = Store::open_in_memory(&dir).await.unwrap();

        assert_eq!(journal::count(&store.journal_pool).await.unwrap(), 0);

        journal::append(
            &store.journal_pool,
            "create_quest",
            &serde_json::json!({"title": "X", "quest_type_id": 1}),
            Some(&serde_json::json!({"id": 1})),
        )
        .await
        .unwrap();

        assert_eq!(journal::count(&store.journal_pool).await.unwrap(), 1);

        journal::append(
            &store.journal_pool,
            "change_status",
            &serde_json::json!({"id": 1, "status_id": 2}),
            None::<&serde_json::Value>,
        )
        .await
        .unwrap();

        let rows = journal::list_all(&store.journal_pool).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].op, "create_quest");
        assert_eq!(rows[1].op, "change_status");
        assert!(rows[1].result.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn journal_truncate_empties_ops() {
        let dir = fresh_tmp("trunc");
        let store = Store::open_in_memory(&dir).await.unwrap();

        for i in 0..5 {
            journal::append(
                &store.journal_pool,
                "test",
                &serde_json::json!({"i": i}),
                None::<&serde_json::Value>,
            )
            .await
            .unwrap();
        }
        assert_eq!(journal::count(&store.journal_pool).await.unwrap(), 5);

        journal::truncate(&store.journal_pool).await.unwrap();
        assert_eq!(journal::count(&store.journal_pool).await.unwrap(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn ensure_schema_is_idempotent() {
        let dir = fresh_tmp("idem");
        let store = Store::open_in_memory(&dir).await.unwrap();
        // 한 번 더 호출 — 에러 없이 끝나야 함
        journal::ensure_schema(&store.journal_pool).await.unwrap();
        journal::ensure_schema(&store.journal_pool).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
