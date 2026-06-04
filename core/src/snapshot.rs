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
use std::time::Duration;

use crate::repo::GuildPaths;
use crate::store::{journal, Store};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotInfo {
    pub timestamp: String, // "YYYYMMDD-HHMMSS"
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// 자동 백업 정책. 둘 중 **하나라도** 도달하면 snapshot 실행.
///
/// 기본값: ops 50 OR 24 시간. env `OPENGUILD_AUTO_BACKUP_OPS`,
/// `OPENGUILD_AUTO_BACKUP_HOURS` 로 override.
#[derive(Debug, Clone, Copy)]
pub struct AutoSnapshotPolicy {
    pub max_ops_since_last: i64,
    pub max_age_hours: u64,
}

impl Default for AutoSnapshotPolicy {
    fn default() -> Self {
        Self {
            max_ops_since_last: 50,
            max_age_hours: 24,
        }
    }
}

impl AutoSnapshotPolicy {
    /// env override 적용된 기본값.
    pub fn from_env() -> Self {
        let mut p = Self::default();
        if let Ok(v) = std::env::var("OPENGUILD_AUTO_BACKUP_OPS")
            && let Ok(n) = v.parse()
        {
            p.max_ops_since_last = n;
        }
        if let Ok(v) = std::env::var("OPENGUILD_AUTO_BACKUP_HOURS")
            && let Ok(n) = v.parse()
        {
            p.max_age_hours = n;
        }
        p
    }
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

/// 정책에 따른 자동 snapshot.
/// 임계치 도달 안 했으면 None 반환 (정상 — no-op).
/// 도달 했으면 snapshot 생성 + journal truncate + Some(SnapshotInfo) 반환.
///
/// 호출자 책임: 결과가 Some 이면 사용자에게 알림 (stderr 출력 등).
pub async fn maybe_auto_snapshot(
    store: &Store,
    policy: AutoSnapshotPolicy,
) -> Result<Option<SnapshotInfo>> {
    // 1. journal ops 수 확인
    let ops_count = journal::count(&store.journal_pool)
        .await
        .context("journal count 조회 실패")?;
    let ops_trigger = ops_count >= policy.max_ops_since_last;

    // 2. age trigger: 마지막 snapshot 으로부터 N 시간 + ops > 0 이면 fire.
    //    snapshot 한 번도 없는 경우엔 age trigger 안 함 — ops 임계치 도달까지 대기
    //    (사용자가 첫 mutation 마다 즉시 snapshot 생기는 게 아닌, 의미 있는 양 쌓인 뒤에).
    let latest = latest_snapshot(&store.paths)?;
    let age_trigger = match &latest {
        None => false,
        Some(s) => {
            let age = std::time::SystemTime::now()
                .duration_since(snapshot_time(&s.timestamp).unwrap_or(std::time::UNIX_EPOCH))
                .unwrap_or(Duration::ZERO);
            age >= Duration::from_secs(policy.max_age_hours * 3600) && ops_count > 0
        }
    };

    if !ops_trigger && !age_trigger {
        return Ok(None);
    }

    let info = create_snapshot(store).await?;
    Ok(Some(info))
}

/// "YYYYMMDD-HHMMSS" → SystemTime (UTC).
fn snapshot_time(timestamp: &str) -> Option<std::time::SystemTime> {
    // 형식: YYYYMMDD-HHMMSS (15글자)
    if timestamp.len() != 15 || &timestamp[8..9] != "-" {
        return None;
    }
    let y: u64 = timestamp[0..4].parse().ok()?;
    let mo: u64 = timestamp[4..6].parse().ok()?;
    let d: u64 = timestamp[6..8].parse().ok()?;
    let h: u64 = timestamp[9..11].parse().ok()?;
    let mi: u64 = timestamp[11..13].parse().ok()?;
    let s: u64 = timestamp[13..15].parse().ok()?;

    // UTC 기준 epoch 변환 (단순 — 윤년 처리 정확).
    let secs = ymdhms_to_epoch(y, mo, d, h, mi, s)?;
    Some(std::time::UNIX_EPOCH + Duration::from_secs(secs))
}

fn ymdhms_to_epoch(y: u64, mo: u64, d: u64, h: u64, mi: u64, s: u64) -> Option<u64> {
    if y < 1970 || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let mut days: u64 = 0;
    for yr in 1970..y {
        days += if is_leap_y(yr as i64) { 366 } else { 365 };
    }
    let months = days_in_months_y(y as i64);
    for &m in months.iter().take((mo - 1) as usize) {
        days += m as u64;
    }
    days += d - 1;
    Some(days * 86400 + h * 3600 + mi * 60 + s)
}

fn is_leap_y(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
fn days_in_months_y(y: i64) -> [u32; 12] {
    [
        31,
        if is_leap_y(y) { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ]
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
    async fn maybe_auto_snapshot_noop_when_no_ops() {
        let dir = fresh_tmp("auto-noop");
        let store = setup(&dir).await;
        // ops 0 → trigger 안 함
        let result = maybe_auto_snapshot(&store, AutoSnapshotPolicy::default())
            .await
            .unwrap();
        assert!(result.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn maybe_auto_snapshot_fires_on_ops_threshold() {
        let dir = fresh_tmp("auto-fire");
        let store = setup(&dir).await;
        // 임계치 낮춰서 빠르게 trigger
        let policy = AutoSnapshotPolicy {
            max_ops_since_last: 2,
            max_age_hours: 24,
        };
        // ops 1 — 아직 trigger 안 함
        journal::append(
            &store.journal_pool,
            "t",
            &serde_json::json!({"i": 1}),
            None::<&serde_json::Value>,
        )
        .await
        .unwrap();
        assert!(maybe_auto_snapshot(&store, policy).await.unwrap().is_none());

        // ops 2 — trigger
        journal::append(
            &store.journal_pool,
            "t",
            &serde_json::json!({"i": 2}),
            None::<&serde_json::Value>,
        )
        .await
        .unwrap();
        let snap = maybe_auto_snapshot(&store, policy).await.unwrap();
        assert!(snap.is_some());

        // snapshot 후 journal truncate 됐는지
        assert_eq!(journal::count(&store.journal_pool).await.unwrap(), 0);

        // 다음 호출은 다시 trigger 안 함 (ops 0)
        assert!(maybe_auto_snapshot(&store, policy).await.unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn maybe_auto_snapshot_no_fire_on_first_ops_alone() {
        // snapshot 없는 상태에서 첫 ops 만으로는 fire 안 함 — ops 임계치 도달까지 대기.
        // (사용자가 매 mutation 마다 snapshot 쌓이는 것 방지)
        let dir = fresh_tmp("auto-first");
        let store = setup(&dir).await;
        let policy = AutoSnapshotPolicy {
            max_ops_since_last: 1000,
            max_age_hours: 1,
        };
        journal::append(
            &store.journal_pool,
            "t",
            &serde_json::json!({"i": 1}),
            None::<&serde_json::Value>,
        )
        .await
        .unwrap();
        let snap = maybe_auto_snapshot(&store, policy).await.unwrap();
        assert!(snap.is_none(), "첫 ops 만으로는 fire 안 함");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn policy_from_env_uses_default_when_unset() {
        // SAFETY: 테스트 단일 스레드. env 직접 set/unset.
        unsafe {
            std::env::remove_var("OPENGUILD_AUTO_BACKUP_OPS");
            std::env::remove_var("OPENGUILD_AUTO_BACKUP_HOURS");
        }
        let p = AutoSnapshotPolicy::from_env();
        assert_eq!(p.max_ops_since_last, 50);
        assert_eq!(p.max_age_hours, 24);
    }

    #[test]
    fn snapshot_time_parses_format() {
        let t = snapshot_time("20260516-103341").unwrap();
        // 2026-05-16 10:33:41 UTC = unknown exact secs without lookup, but check it's after 1970
        assert!(t > std::time::UNIX_EPOCH);
    }

    #[test]
    fn snapshot_time_rejects_bad_format() {
        assert!(snapshot_time("invalid").is_none());
        assert!(snapshot_time("20260516_103341").is_none()); // _ instead of -
        assert!(snapshot_time("2026-05-16T10:33:41").is_none()); // ISO format
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

    /// DEV-102: 댓글 / 메모가 snapshot 에 살아남고 restore 후에도 그대로인지.
    /// 이게 본 quest 의 핵심 목적 — git 없이도 댓글 / 메모를 잃지 않음.
    #[tokio::test]
    async fn snapshot_preserves_comments_and_memos() {
        use crate::models::CreateQuestRequest;
        use crate::ops::{comments as comments_ops, quests as quest_ops};

        let dir = fresh_tmp("snap-c-m");
        let store = setup(&dir).await;

        // 1. quest 1 개 생성.
        let q = quest_ops::create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "for backup".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();

        // 2. 댓글 + 메모 작성 (file + DB 양쪽 sync).
        let entry = comments_ops::add_comment_entry(
            &store,
            &q.quest_id,
            "alice".into(),
            "hello".into(),
            None,
        )
        .await
        .unwrap();
        comments_ops::set_memo(&store, &q.quest_id, "private note".into())
            .await
            .unwrap();

        // 3. snapshot 생성.
        let snap = create_snapshot(&store).await.unwrap();
        assert!(snap.path.exists());

        // 4. file 진리원도 같이 살린다는 시나리오 가정 — DB 캐시만 손실 시나리오 시뮬레이션:
        //    quest_comments / quest_memos 행을 의도적으로 비우고, restore 가 살리는지.
        sqlx::query("DELETE FROM quest_comments")
            .execute(&store.index_pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM quest_memos")
            .execute(&store.index_pool)
            .await
            .unwrap();
        let c_after_wipe: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_comments")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(c_after_wipe, 0);

        // 5. snapshot 으로 restore.
        restore_snapshot(&store, &snap).await.unwrap();

        // 6. 새 store 로 열어서 댓글 / 메모 복원 확인.
        let store2 = Store::open(&dir).await.unwrap();
        let c_after_restore: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_comments")
            .fetch_one(&store2.index_pool)
            .await
            .unwrap();
        assert_eq!(c_after_restore, 1, "댓글이 snapshot 에서 복원되어야");

        let (entry_id, author, body): (i64, String, String) = sqlx::query_as(
            "SELECT entry_id, author, body FROM quest_comments",
        )
        .fetch_one(&store2.index_pool)
        .await
        .unwrap();
        assert_eq!(entry_id, entry.id as i64);
        assert_eq!(author, "alice");
        assert_eq!(body, "hello");

        let memo_content: String = sqlx::query_scalar(
            "SELECT content FROM quest_memos WHERE user_id = 0",
        )
        .fetch_one(&store2.index_pool)
        .await
        .unwrap();
        assert_eq!(memo_content, "private note");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
