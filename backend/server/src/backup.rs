//! DB 자동 백업.
//!
//! - 서버 시작 시 1회 백업
//! - 이후 BACKUP_INTERVAL_SECS 간격으로 백업
//! - 최근 BACKUP_KEEP 개만 보관 (오래된 것 자동 삭제)
//!
//! 백업 위치: `<guild_path>/backups/guild.db.<timestamp>`
//! SQLite `VACUUM INTO` 사용 — 서버 동작 중에도 안전하게 일관된 스냅샷 생성.

use anyhow::{Context, Result};
use sqlx::{Executor, SqlitePool};
use std::path::{Path, PathBuf};
use std::time::Duration;

const BACKUP_INTERVAL_SECS: u64 = 60 * 60; // 1시간
const BACKUP_KEEP: usize = 24 * 7; // 7일치 (시간당 1개 × 24 × 7)

/// guild_path 안의 backups/ 디렉토리.
fn backup_dir(guild_path: &str) -> PathBuf {
    Path::new(guild_path).join("backups")
}

/// 1회 백업: guild.db → backups/guild.db.<timestamp>
async fn backup_once(pool: &SqlitePool, guild_path: &str) -> Result<PathBuf> {
    let dir = backup_dir(guild_path);
    std::fs::create_dir_all(&dir).context("failed to create backup dir")?;

    // 파일명: guild.db.YYYYMMDD-HHMMSS
    let now = chrono_now_utc_compact();
    let target = dir.join(format!("guild.db.{now}"));

    // VACUUM INTO 는 안전한 스냅샷. 서버 동작 중에도 일관된 백업.
    // SQLite 는 path 를 SQL string literal 로 받는다.
    let target_str = target.to_string_lossy().replace('\'', "''");
    let sql = format!("VACUUM INTO '{}'", target_str);
    pool.execute(sql.as_str())
        .await
        .with_context(|| format!("VACUUM INTO failed: {target_str}"))?;

    Ok(target)
}

/// `backups/` 안에서 BACKUP_KEEP 개만 남기고 오래된 것 삭제.
fn prune_old_backups(guild_path: &str) -> Result<()> {
    let dir = backup_dir(guild_path);
    if !dir.exists() {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|r| r.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("guild.db.")
        })
        .collect();
    // 파일명에 timestamp 가 들어있어 정렬하면 시간순. 오래된 것이 앞.
    entries.sort_by_key(|e| e.file_name());
    while entries.len() > BACKUP_KEEP {
        let old = entries.remove(0);
        let _ = std::fs::remove_file(old.path());
    }
    Ok(())
}

/// 백그라운드 task 로 자동 백업 실행. 서버 main 에서 spawn.
pub fn spawn_backup_task(pool: SqlitePool, guild_path: String) {
    tokio::spawn(async move {
        // 시작 시 즉시 백업
        match backup_once(&pool, &guild_path).await {
            Ok(p) => tracing::info!("initial backup created: {}", p.display()),
            Err(e) => tracing::warn!("initial backup failed: {e:#}"),
        }
        let _ = prune_old_backups(&guild_path);

        let mut ticker = tokio::time::interval(Duration::from_secs(BACKUP_INTERVAL_SECS));
        ticker.tick().await; // 첫 tick 즉시 발생 — 위에서 이미 했으니 소비
        loop {
            ticker.tick().await;
            match backup_once(&pool, &guild_path).await {
                Ok(p) => tracing::info!("periodic backup: {}", p.display()),
                Err(e) => tracing::warn!("periodic backup failed: {e:#}"),
            }
            if let Err(e) = prune_old_backups(&guild_path) {
                tracing::warn!("backup prune failed: {e:#}");
            }
        }
    });
}

/// chrono 의존 없이 timestamp 문자열 생성: YYYYMMDD-HHMMSS (UTC).
/// SystemTime → Unix epoch → 직접 분해.
fn chrono_now_utc_compact() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 매우 단순 분해: 1970-01-01 기준 초 → 날짜
    // 윤년 등 정확. 외부 crate 없이 구현.
    let (y, mo, d, h, mi, s) = epoch_to_ymdhms(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

fn epoch_to_ymdhms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (secs % 60) as u32;
    let mi = ((secs / 60) % 60) as u32;
    let h = ((secs / 3600) % 24) as u32;
    let mut days = (secs / 86400) as i64;
    // 1970-01-01 부터의 일수 → 연/월/일 변환 (proleptic Gregorian)
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
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ]
}
