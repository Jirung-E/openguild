use anyhow::Result;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::collections::HashSet;
use std::str::FromStr;

pub async fn create_pool(database_url: &str) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    Ok(pool)
}

/// 이 binary 가 컴파일 타임에 알고 있는 가장 큰 migration version.
/// BUG-041: SchemaAheadBanner 가 "내 GUI 가 mig N 까지 아는데 DB 는 N+k 까지
/// 적용됨" 식의 명확한 안내를 위해 사용.
pub fn latest_known_migration_version() -> Option<i64> {
    let migrator = sqlx::migrate!("./migrations");
    migrator.iter().map(|m| m.version).max()
}

/// migration 실행 + "DB 에는 있는데 binary 가 모르는" version 셋 반환.
///
/// BUG-041 사용자 보고: 새 빌드가 migration N+1 적용한 길드 DB 를 더 이전
/// binary 가 열면 "VersionMissing(N+1)" panic → 모든 옛 release brick.
///
/// `set_ignore_missing(true)` 로 panic 자체는 막고, 추가로 "내가 모르는 mig"
/// 목록을 반환 → 호출자 (GUI) 가 사용자에게 "binary 가 DB schema 보다 뒤처짐
/// — 업데이트 권장" banner 를 띄울 수 있게.
pub async fn run_migrations(pool: &SqlitePool) -> Result<Vec<i64>> {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator.run(pool).await?;

    // 적용된 (DB 안에 기록된) version 들.
    let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    // binary 가 컴파일 타임에 알고 있는 version 셋.
    let known: HashSet<i64> = migrator.iter().map(|m| m.version).collect();

    // DB 에만 있는 것 = binary 가 모르는 future migration.
    let mut ahead: Vec<i64> = applied.into_iter().filter(|v| !known.contains(v)).collect();
    ahead.sort_unstable();
    Ok(ahead)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BUG-041 regression: DB 에 binary 가 모르는 mig record 가 있어도
    /// `run_migrations` 가 panic 없이 통과 + 그 version 을 ahead 로 보고.
    #[tokio::test]
    async fn run_migrations_tolerates_unknown_applied_version() {
        let pool = create_pool("sqlite::memory:").await.unwrap();
        // 한 번 migrate — `_sqlx_migrations` 테이블 생성 + 모든 known mig record 적용.
        let _ = run_migrations(&pool).await.unwrap();

        // 위조: binary 가 모르는 version 9999 를 _sqlx_migrations 에 직접 INSERT.
        // sqlx 의 _sqlx_migrations schema: version / description / installed_on /
        // success / checksum / execution_time.
        sqlx::query(
            "INSERT INTO _sqlx_migrations
             (version, description, installed_on, success, checksum, execution_time)
             VALUES (9999, 'future stub', datetime('now'), 1, X'00', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // 두 번째 호출 — 이전 buggy 동작이면 panic.
        let ahead = run_migrations(&pool).await.unwrap();
        assert!(ahead.contains(&9999), "ahead should include the unknown version: {ahead:?}");
    }

    /// 모든 mig 가 알려진 정상 DB → ahead 는 빈 vec.
    #[tokio::test]
    async fn run_migrations_returns_empty_ahead_on_clean_db() {
        let pool = create_pool("sqlite::memory:").await.unwrap();
        let ahead = run_migrations(&pool).await.unwrap();
        assert!(ahead.is_empty(), "clean DB should have no ahead: {ahead:?}");
    }
}
