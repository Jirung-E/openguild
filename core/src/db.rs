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
