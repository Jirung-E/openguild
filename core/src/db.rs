use anyhow::Result;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};
use std::collections::HashSet;
use std::str::FromStr;

/// BUG-091: DB 파일이 네트워크 파일시스템(UNC 공유) 위에 있는지.
///
/// verbatim(`\\?\UNC\..`) 형태도 방어적으로 처리 — 정규화를 놓친 경로가
/// 들어와도 오판(`\\?\C:\..` 를 네트워크로) 하지 않도록 prefix 를 벗기고 판별.
pub fn is_network_path(path: &std::path::Path) -> bool {
    let raw = path.to_string_lossy();
    let s = crate::recents::strip_verbatim_prefix(&raw);
    s.starts_with(r"\\") || s.starts_with("//")
}

/// 파일 기반 SQLite pool — 경로를 URL 문자열로 변환하지 않고 직접 연다.
///
/// BUG-091(2차): UNC 경로를 `sqlite://server/share/..` URL 로 만들면 sqlx 의
/// URL 파서가 `server` 를 host 로 해석해 파일 경로가 깨진다(code 14: unable to
/// open database file) — UNC 는 URL 로 표현할 수 없다.
/// `SqliteConnectOptions::filename()` 은 경로를 파싱 없이 그대로 SQLite 에
/// 전달하므로 UNC(`\\server\share\..`)도 정상 동작.
pub async fn create_pool_from_path(
    path: &std::path::Path,
    read_only: bool,
) -> Result<SqlitePool> {
    let mut options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(!read_only)
        .read_only(read_only);

    // BUG-091: WAL(sqlx 기본)은 공유메모리(mmap)가 필요해 SMB/UNC 네트워크
    // 파일시스템에서 동작하지 않는다(SQLite 공식 제약) → 네트워크 공유 길드의
    // index.db/journal.db 를 못 열던 문제. 네트워크면 rollback journal(DELETE)로
    // 전환하고, SMB 의 락 지연을 견디도록 busy_timeout 도 넉넉히.
    if is_network_path(path) {
        options = options
            .journal_mode(SqliteJournalMode::Delete)
            .busy_timeout(std::time::Duration::from_secs(15));
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// URL 기반 pool — in-memory(`sqlite::memory:` / `sqlite:file:..?mode=memory`)
/// 전용. 파일 DB 는 `create_pool_from_path` 를 쓸 것 (UNC 경로가 URL 로 깨짐).
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

    /// BUG-091: UNC 네트워크 경로만 network 로 판별(로컬 드라이브는 아님).
    #[test]
    fn is_network_path_detects_unc_only() {
        use std::path::Path;
        // UNC 원형 + verbatim.
        assert!(is_network_path(Path::new(r"\\server\share\guild\.guild\index.db")));
        assert!(is_network_path(Path::new(r"\\?\UNC\server\share\guild\.guild\index.db")));
        // 로컬 드라이브 (원형 + verbatim — verbatim 을 네트워크로 오판하면 안 됨).
        assert!(!is_network_path(Path::new(r"C:\work\guild\.guild\index.db")));
        assert!(!is_network_path(Path::new(r"\\?\C:\work\guild\.guild\index.db")));
    }

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
