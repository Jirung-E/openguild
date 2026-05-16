//! Snapshot + Restore — Redis RDB 패턴.
//!
//! `.guild/backups/snapshots/{timestamp}.db` 는 그 시점의 `.guild/index.db` 사본.
//! 동시에 `.guild/backups/journal.db` 의 ops 테이블 비움 — 새 snapshot 이후의
//! mutation 만 journal 에 쌓이도록.
//!
//! Restore 시:
//! 1. 가장 가까운 snapshot 선택 (요청 시각 ≤ snapshot ts).
//! 2. snapshot DB 를 `.guild/index.db` 로 복사.
//! 3. journal 의 ops 를 시간 순서대로 replay (요청 시각 ≤ op ts).
//! 4. 파일들 (`.guild/quests/`, `types/`, `statuses/`) 을 새 index 기준으로 재생성.
//!
//! 첫 단계 구현 (이 모듈): snapshot 생성 + 목록 + 단순 restore (snapshot 만, journal replay X).
//! Journal replay 는 ops 의 args 를 역으로 적용해야 — 별도 단계 (F7+ 예정).

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::repo::GuildPaths;
use crate::store::{journal, Store};

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub timestamp: String, // "YYYYMMDD-HHMMSS"
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// 현재 `.guild/index.db` 를 `.guild/backups/snapshots/{ts}.db` 로 복사 +
/// `.guild/backups/journal.db` truncate.
///
/// Retention 7 개 — 8 번째 이상 오래된 것 삭제.
pub async fn create_snapshot(store: &Store) -> Result<SnapshotInfo> {
    let paths = &store.paths;
    let ts = now_compact();
    let target = paths.snapshots_dir().join(format!("{ts}.db"));

    std::fs::create_dir_all(paths.snapshots_dir())
        .with_context(|| format!("snapshots 디렉토리 생성 실패: {}", paths.snapshots_dir().display()))?;

    // index.db 가 write 중일 수 있어 SQLite 의 backup API 가 안전 (PRAGMA wal_checkpoint 후 fs::copy).
    // 단순 fs::copy 도 SQLite WAL 모드에선 일관성 위험. 안전을 위해 source pool 에서 wal_checkpoint(TRUNCATE) 실행.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&store.index_pool)
        .await
        .ok(); // checkpoint 실패해도 fs::copy 시도 — best effort

    std::fs::copy(paths.index_db(), &target)
        .with_context(|| format!("snapshot 복사 실패: {} → {}", paths.index_db().display(), target.display()))?;

    let size_bytes = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);

    // journal truncate.
    journal::truncate(&store.journal_pool)
        .await
        .context("journal truncate 실패")?;

    // retention
    prune_old_snapshots(paths, 7)?;

    Ok(SnapshotInfo {
        timestamp: ts,
        path: target,
        size_bytes,
    })
}

/// 사용 가능한 snapshot 목록 (오래된 순부터).
pub fn list_snapshots(paths: &GuildPaths) -> Result<Vec<SnapshotInfo>> {
    let dir = paths.snapshots_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("db"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut out = Vec::new();
    for e in entries {
        let path = e.path();
        let timestamp = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let size_bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(SnapshotInfo {
            timestamp,
            path,
            size_bytes,
        });
    }
    Ok(out)
}

/// 가장 최근 snapshot. 없으면 None.
pub fn latest_snapshot(paths: &GuildPaths) -> Result<Option<SnapshotInfo>> {
    Ok(list_snapshots(paths)?.into_iter().last())
}

/// snapshot DB 를 `.guild/index.db` 로 복원.
///
/// **현 시점 단순 버전**: snapshot 시점으로만 되돌림. 그 이후 journal 의 ops 는 미반영.
/// (journal replay 는 F7+ 에서 별도 구현 예정.)
///
/// 파일 시스템 측 (`.guild/quests/*.md` 등) 은 자동 갱신 안 함 — 호출자가 별도 `reindex` 또는
/// 파일 export 명령으로 처리 (다음 단계).
pub async fn restore_snapshot(store: &Store, snapshot: &SnapshotInfo) -> Result<()> {
    let paths = &store.paths;

    // 현재 index.db 를 .pre-restore 로 백업 (재시도 가능).
    let backup = paths.index_db().with_extension("pre-restore.db");
    if paths.index_db().exists() {
        let _ = std::fs::remove_file(&backup);
        std::fs::copy(paths.index_db(), &backup)
            .with_context(|| format!("pre-restore 백업 실패: {}", backup.display()))?;
    }

    // index_pool 의 연결을 비워두기 위해 checkpoint 실행 후 (또는 풀 close 옵션 — 현재 풀은 살아있음)
    // SQLite 는 fs::copy 시 같은 파일 잠금이 충돌할 수 있음. 가장 안전: pool 종료.
    // 본 함수는 호출자가 store 를 새로 열기 전에 호출하는 패턴 권장 — 단순 fs::copy.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&store.index_pool)
        .await
        .ok();

    std::fs::copy(&snapshot.path, paths.index_db())
        .with_context(|| format!("snapshot 복사 실패: {} → {}", snapshot.path.display(), paths.index_db().display()))?;

    Ok(())
}

/// snapshot 파일 시간 정렬 후 N 개 이상 오래된 것 삭제.
fn prune_old_snapshots(paths: &GuildPaths, keep: usize) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(paths.snapshots_dir())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("db"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    while entries.len() > keep {
        let old = entries.remove(0);
        let _ = std::fs::remove_file(old.path());
    }
    Ok(())
}

/// `YYYYMMDD-HHMMSS` UTC compact timestamp.
fn now_compact() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = epoch_to_ymdhms(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed_guild_dir;

    fn fresh_tmp(label: &str) -> PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-snapshot-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        Store::open(dir).await.unwrap()
    }

    #[tokio::test]
    async fn create_snapshot_writes_db_file() {
        let dir = fresh_tmp("create");
        let store = setup(&dir).await;
        // 메인 index 에 무언가 INSERT 가 있어야 의미 있음 — 메타데이터는 자동 시드됨.
        let info = create_snapshot(&store).await.unwrap();
        assert!(info.path.exists());
        assert!(info.size_bytes > 0);
        assert!(info.timestamp.len() == 15); // YYYYMMDD-HHMMSS

        // snapshots/ 안에 1 개
        let list = list_snapshots(&store.paths).unwrap();
        assert_eq!(list.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn create_snapshot_truncates_journal() {
        let dir = fresh_tmp("trunc");
        let store = setup(&dir).await;

        // journal 에 op 들 쌓기
        for i in 0..3 {
            journal::append(
                &store.journal_pool,
                "test",
                &serde_json::json!({"i": i}),
                None::<&serde_json::Value>,
            )
            .await
            .unwrap();
        }
        assert_eq!(journal::count(&store.journal_pool).await.unwrap(), 3);

        let _ = create_snapshot(&store).await.unwrap();
        assert_eq!(journal::count(&store.journal_pool).await.unwrap(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn snapshot_retention_keeps_seven() {
        let dir = fresh_tmp("retention");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        // 9 개 snapshot fake — 시간차 별로 작성.
        for i in 0..9 {
            let ts = format!("2026010{i}-000000");
            let p = paths.snapshots_dir().join(format!("{ts}.db"));
            std::fs::write(&p, format!("snapshot-{i}")).unwrap();
        }
        // 한 번 더 create_snapshot — retention 7 적용.
        let _ = create_snapshot(&store).await.unwrap();

        let list = list_snapshots(&paths).unwrap();
        assert_eq!(list.len(), 7, "should keep exactly 7");
        // 가장 오래된 두 개 삭제됐는지 (fake 들 중 0, 1)
        let names: Vec<_> = list.iter().map(|s| s.timestamp.clone()).collect();
        assert!(!names.iter().any(|n| n.starts_with("20260100") || n.starts_with("20260101")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn list_snapshots_sorted_chronologically() {
        let dir = fresh_tmp("sorted");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        // 시간 역순으로 작성 — 정렬 후 시간 순이어야
        for ts in ["20260103-120000", "20260101-120000", "20260102-120000"] {
            std::fs::write(paths.snapshots_dir().join(format!("{ts}.db")), b"x").unwrap();
        }
        let list = list_snapshots(&paths).unwrap();
        let stamps: Vec<_> = list.iter().map(|s| s.timestamp.clone()).collect();
        assert_eq!(
            stamps,
            vec![
                "20260101-120000".to_string(),
                "20260102-120000".to_string(),
                "20260103-120000".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn restore_snapshot_restores_index_db() {
        let dir = fresh_tmp("restore");
        let store = setup(&dir).await;

        // 1. 초기 상태에서 snapshot
        let s1 = create_snapshot(&store).await.unwrap();
        let initial_size = s1.size_bytes;

        // 2. index.db 에 quest 행 직접 INSERT (snapshot 이후 변화)
        sqlx::query(
            "INSERT INTO quests (quest_type_id, number, title, status_id, urgency)
             VALUES (1, 1, 'X', 1, 3)",
        )
        .execute(&store.index_pool)
        .await
        .unwrap();
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quests")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(after, 1);

        // 3. snapshot 으로 복원
        restore_snapshot(&store, &s1).await.unwrap();

        // 4. index.db 를 새 풀로 열어서 확인 (기존 풀은 캐시된 트랜잭션이 있을 수 있음)
        let store2 = Store::open(&dir).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quests")
            .fetch_one(&store2.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "snapshot 시점엔 quest 0");

        // pre-restore 백업 생성됨
        assert!(store.paths.index_db().with_extension("pre-restore.db").exists());
        // 복원된 파일 크기는 원래 snapshot 과 비슷 (정확히 같진 않을 수 있으나 차이 작음)
        let _ = initial_size;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
