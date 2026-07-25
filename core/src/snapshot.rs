//! Snapshot + Restore — Redis RDB 패턴 (BUG-076: 파일 기반).
//!
//! **RDB = `.guild/` 소스 파일(진실)의 사본**, index.db(캐시)가 아니다.
//! `.guild/backups/snapshots/{timestamp}/` 디렉토리에 루트 마커 + 소스 하위
//! 디렉토리(quests/campaigns/rules/tags/types/statuses/attachments)를 복사한다.
//! index.db/journal.db/backups 는 제외(캐시·자기참조). 동시에 journal(ops, AOF)
//! truncate — 이 snapshot 이후 mutation 만 쌓이도록.
//!
//! Restore 시:
//! 1. 현재 소스 + index.db 를 `.pre-restore/` 로 백업 (되돌리기 가능).
//! 2. 현재 소스 제거 후 snapshot 의 파일을 `.guild/` 로 복원.
//! 3. reindex 로 index.db(캐시) 재구축. → rules/댓글/메모/첨부 등 전부 복원.
//!
//! journal replay(시점 복원, DEV-022 = AOF)는 이 위에 별도로 얹는다 — 현재는
//! snapshot 시점으로 복원.

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

/// DEV-022/BUG-076: snapshot 이 담는 `.guild/` 소스 하위 디렉토리 (진실 = 파일).
/// index.db/journal.db (캐시) 와 backups/ (자기 자신) 는 제외.
const SOURCE_SUBDIRS: &[&str] = &[
    "quests",
    "campaigns",
    "rules",
    "tags",
    "types",
    "statuses",
    "attachments",
    // DEV-180: 이력 사이드카가 quests/ 밖 전용 디렉토리로 분리돼 별도 등록
    // 필요 — 빠뜨리면 snapshot/restore 가 조용히 history/ 를 건너뛴다.
    "history",
    // DEV-215: 도서관 문서 (+.counter.toml — copy_tree 가 숨김 파일도 복사).
    "library",
    // DEV-167: 작업 기록 노트.
    "worklog",
];

/// dir 트리를 통째로 복사 (대상에 병합 생성). 파일/하위디렉토리 재귀.
fn copy_tree(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)
        .with_context(|| format!("디렉토리 생성 실패: {}", dst.display()))?;
    for entry in std::fs::read_dir(src)
        .with_context(|| format!("디렉토리 읽기 실패: {}", src.display()))?
    {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)
                .with_context(|| format!("복사 실패: {} → {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

fn tree_size(dir: &std::path::Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.filter_map(|e| e.ok()).fold(0u64, |acc, e| {
        let p = e.path();
        acc + if p.is_dir() {
            tree_size(&p)
        } else {
            std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
        }
    })
}

/// 길드의 소스 파일(진실)을 `dst` 로 복사 — 루트 마커 `*.guild` + `.guild/` 의
/// SOURCE_SUBDIRS. index.db/journal.db/backups 는 제외. 레이아웃 보존:
/// `dst/{marker}.guild`, `dst/.guild/{subdir}/...`.
fn copy_guild_source(paths: &GuildPaths, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    // 루트 마커 `*.guild`.
    if let Ok(rd) = std::fs::read_dir(&paths.guild_root) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("guild") {
                let _ = std::fs::copy(&p, dst.join(e.file_name()));
            }
        }
    }
    // `.guild/<subdir>`.
    let dot_dst = dst.join(".guild");
    for sub in SOURCE_SUBDIRS {
        copy_tree(&paths.dot_guild().join(sub), &dot_dst.join(sub))?;
    }
    Ok(())
}

/// 현재 길드의 소스(SOURCE_SUBDIRS + 루트 마커) 제거 — restore 직전 클린업.
/// index.db/journal.db/backups 는 절대 건드리지 않는다.
fn clear_guild_source(paths: &GuildPaths) -> Result<()> {
    for sub in SOURCE_SUBDIRS {
        let d = paths.dot_guild().join(sub);
        if d.exists() {
            std::fs::remove_dir_all(&d)
                .with_context(|| format!("소스 제거 실패: {}", d.display()))?;
        }
    }
    if let Ok(rd) = std::fs::read_dir(&paths.guild_root) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("guild") {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    Ok(())
}

/// BUG-076: snapshot = `.guild/` 소스 파일(진실)의 사본. `index.db`(캐시)가 아님.
/// `.guild/backups/snapshots/{ts}/` 디렉토리에 루트 마커 + 소스 하위디렉토리 복사
/// + journal truncate. restore 는 이 파일들을 되돌린 뒤 reindex 로 index.db 재구축.
///
/// Retention 7 개 — 8 번째 이상 오래된 것 삭제.
pub async fn create_snapshot(store: &Store) -> Result<SnapshotInfo> {
    let paths = &store.paths;

    std::fs::create_dir_all(paths.snapshots_dir())
        .with_context(|| format!("snapshots 디렉토리 생성 실패: {}", paths.snapshots_dir().display()))?;

    // BUG-108: now_compact() 는 초 단위라 짧은 시간 안에 snapshot 이 두 번
    // 생성되면(테스트, 또는 replay_to 의 pre_backup 처럼 한 호출 안에서
    // 연달아) 같은 디렉토리 이름이 나온다. copy_tree 는 대상을 비우지 않고
    // 병합만 하므로, 충돌 시 이전 snapshot 이 이후 상태로 오염된다
    // (replay_to 가 과거 snapshot 을 복원해도 그 사이 생성된 quest 가 남는
    // 버그로 발현). 디렉토리가 이미 있으면 `-01`, `-02` ... 접미사로 유니크화.
    let base_ts = now_compact();
    let mut ts = base_ts.clone();
    let mut target = paths.snapshots_dir().join(&ts);
    let mut n = 1u32;
    while target.exists() {
        ts = format!("{base_ts}-{n:02}");
        target = paths.snapshots_dir().join(&ts);
        n += 1;
    }

    copy_guild_source(paths, &target).context("snapshot 소스 복사 실패")?;
    let size_bytes = tree_size(&target);

    // journal truncate (AOF 리셋 — 이 snapshot 이후 ops 만 쌓이도록).
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
    // BUG-167: latest_snapshot() 은 스냅샷 전체 tree_size(재귀 stat)를 계산해
    // 매 mutation 이 ~0.5s 까지 느려졌다 — age trigger 는 timestamp 만 필요.
    let latest = latest_snapshot_timestamp(&store.paths)?;
    let age_trigger = match &latest {
        None => false,
        Some(ts) => {
            let age = std::time::SystemTime::now()
                .duration_since(snapshot_time(ts).unwrap_or(std::time::UNIX_EPOCH))
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

/// "YYYYMMDD-HHMMSS" (UTC) → SystemTime.
///
/// BUG-086(후속): 타임스탬프는 **정규형 UTC** 로 저장(now_compact). 디렉토리명이
/// offset 마커를 못 담으므로 UTC 규약으로 통일 — tz/DST 무관하게 정렬 단조 +
/// 모호성 없음. 사람에게 보일 때만 로컬 변환(ts_to_local_display).
///
/// BUG-108: 같은 초 충돌 시 `-01` 접미사가 붙을 수 있어 앞 15자(고정폭
/// "YYYYMMDD-HHMMSS")만 파싱 대상으로 삼는다.
fn snapshot_time(timestamp: &str) -> Option<std::time::SystemTime> {
    use chrono::TimeZone;
    let base = timestamp.get(0..15)?;
    let naive = chrono::NaiveDateTime::parse_from_str(base, "%Y%m%d-%H%M%S").ok()?;
    Some(chrono::Utc.from_utc_datetime(&naive).into())
}

/// BUG-086(후속): 저장된 UTC compact 타임스탬프(`YYYYMMDD-HHMMSS`, BUG-108 이후
/// 충돌 시 `-01` 접미사 가능)를 사람용 로컬 표시 문자열로 변환. 파싱 실패 시
/// 원본 그대로. CLI / GUI 의 표시 계층에서 사용 (저장값은 UTC 정규형 유지).
pub fn ts_to_local_display(ts: &str) -> String {
    use chrono::TimeZone;
    let Some(base) = ts.get(0..15) else {
        return ts.to_string();
    };
    let suffix = &ts[15.min(ts.len())..];
    match chrono::NaiveDateTime::parse_from_str(base, "%Y%m%d-%H%M%S") {
        Ok(naive) => {
            let display = chrono::Utc
                .from_utc_datetime(&naive)
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
            format!("{display}{suffix}")
        }
        Err(_) => ts.to_string(),
    }
}

/// 사용 가능한 snapshot 목록 (오래된 순부터).
pub fn list_snapshots(paths: &GuildPaths) -> Result<Vec<SnapshotInfo>> {
    let dir = paths.snapshots_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    // BUG-076: snapshot 은 이제 `{ts}/` 디렉토리 (소스 파일 묶음).
    let mut entries: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut out = Vec::new();
    for e in entries {
        let path = e.path();
        let timestamp = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let size_bytes = tree_size(&path);
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

/// BUG-167: 가장 최근 snapshot 의 timestamp 만 — size 계산 없이.
///
/// `latest_snapshot`(→`list_snapshots`)은 스냅샷마다 `tree_size` 로 **전체
/// 파일을 재귀 stat** 한다. 스냅샷은 길드 소스 전체 복사본(수백 파일)이라
/// 이 비용이 (스냅샷 수 × 길드 파일 수)에 비례 — `after_mutation` 이 매
/// mutation 마다 이걸 불러 상태변경/관계변경이 실측 ~0.5s 까지 느려졌다.
/// age trigger 는 timestamp(디렉토리 이름)만 필요하므로 이름 스캔만 한다.
pub fn latest_snapshot_timestamp(paths: &GuildPaths) -> Result<Option<String>> {
    let dir = paths.snapshots_dir();
    if !dir.exists() {
        return Ok(None);
    }
    let mut latest: Option<String> = None;
    for e in std::fs::read_dir(&dir)?.filter_map(|e| e.ok()) {
        if !e.path().is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        // 이름이 "YYYYMMDD-HHMMSS[-NN]" 정렬 단조(UTC 정규형, BUG-086) — 문자열 max = 최신.
        if latest.as_deref().is_none_or(|cur| name.as_str() > cur) {
            latest = Some(name);
        }
    }
    Ok(latest)
}

/// DEV-175: 특정 snapshot 삭제 — `snapshots/{timestamp}/` 디렉토리 제거.
/// timestamp 는 디렉토리 이름 한 토막이어야 (traversal 방지).
pub fn delete_snapshot(paths: &GuildPaths, timestamp: &str) -> Result<()> {
    if timestamp.is_empty()
        || timestamp.contains('/')
        || timestamp.contains('\\')
        || timestamp.contains("..")
    {
        anyhow::bail!("잘못된 snapshot timestamp: {timestamp}");
    }
    let target = paths.snapshots_dir().join(timestamp);
    if !target.is_dir() {
        anyhow::bail!("snapshot 없음: {timestamp}");
    }
    std::fs::remove_dir_all(&target)
        .with_context(|| format!("snapshot 삭제 실패: {}", target.display()))?;
    Ok(())
}

/// BUG-076: snapshot(소스 파일 묶음)을 `.guild/` 로 되돌리고 index.db 를 reindex 로
/// 재구축. 파일이 진실이므로 rules/댓글/메모/첨부 등 모두 복원된다.
///
/// 흐름:
/// 1. 현재 소스 + index.db 를 `.pre-restore/` 로 백업 (재시도 가능).
/// 2. 현재 소스(SOURCE_SUBDIRS + 루트 마커) 제거.
/// 3. snapshot 의 소스를 길드로 복사.
/// 4. reindex 로 index.db 재구축 (live pool — fs::copy 로 덮어쓰지 않으므로 안전).
///
/// journal replay(시점 복원, DEV-022)는 이 위에 별도로 얹는다 — 현재는 snapshot
/// 시점으로 복원.
pub async fn restore_snapshot(store: &Store, snapshot: &SnapshotInfo) -> Result<()> {
    let paths = &store.paths;

    // 1. pre-restore 백업 (소스 파일 + index.db) — 되돌리기 가능.
    let pre = paths.backups_dir().join(".pre-restore");
    let _ = std::fs::remove_dir_all(&pre);
    copy_guild_source(paths, &pre).context("pre-restore 소스 백업 실패")?;
    if paths.index_db().exists() {
        let _ = std::fs::copy(paths.index_db(), pre.join("index.db"));
    }

    // 2. 현재 소스 제거 (index.db/journal.db/backups 는 보존).
    clear_guild_source(paths)?;

    // 3. snapshot 소스를 길드 루트로 복원 (마커 + .guild/<subdir>).
    let dot_dst = paths.dot_guild();
    std::fs::create_dir_all(&dot_dst)?;
    // 루트 마커.
    if let Ok(rd) = std::fs::read_dir(&snapshot.path) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("guild") {
                let _ = std::fs::copy(&p, paths.guild_root.join(e.file_name()));
            }
        }
    }
    // .guild/<subdir>.
    let snap_dot = snapshot.path.join(".guild");
    for sub in SOURCE_SUBDIRS {
        copy_tree(&snap_dot.join(sub), &dot_dst.join(sub))?;
    }

    // 4. index.db 재구축 (파일 → DB). live pool 사용 — fs::copy 로 덮어쓰지
    //    않으므로 연결 불일치(깜빡임) 없음.
    crate::reindex::reindex(store)
        .await
        .map_err(|e| anyhow::anyhow!("restore 후 reindex 실패: {e}"))?;

    Ok(())
}

/// snapshot 디렉토리 시간 정렬 후 N 개 이상 오래된 것 삭제.
fn prune_old_snapshots(paths: &GuildPaths, keep: usize) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(paths.snapshots_dir())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    while entries.len() > keep {
        let old = entries.remove(0);
        let _ = std::fs::remove_dir_all(old.path());
    }
    Ok(())
}

/// `YYYYMMDD-HHMMSS` UTC compact timestamp (디렉토리명 = 정규형 식별자).
///
/// BUG-086(후속): 저장은 UTC 정규형(tz/DST 무관 정렬 단조 + offset 없는 포맷의
/// 모호성 제거). 사람에게 보일 때만 ts_to_local_display 로 로컬 변환.
fn now_compact() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
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

    /// BUG-076: 소스 파일을 지운 뒤 restore 가 파일을 복구 + index.db 재구축.
    #[tokio::test]
    async fn restore_recovers_deleted_source_files() {
        let dir = fresh_tmp("restore");
        let store = setup(&dir).await;
        let types_dir = store.paths.dot_guild().join("types");
        assert!(types_dir.exists(), "seed 가 types 파일 생성");

        let info = create_snapshot(&store).await.unwrap();

        // 사용자 시나리오: 소스 파일 삭제.
        std::fs::remove_dir_all(&types_dir).unwrap();
        assert!(!types_dir.exists());

        restore_snapshot(&store, &info).await.unwrap();

        // 파일 복구.
        assert!(types_dir.exists(), "restore 가 파일을 되돌려야");
        assert!(std::fs::read_dir(&types_dir).unwrap().count() > 0);
        // index.db 도 reindex 로 재구축 (캐시).
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_types")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert!(n > 0, "reindex 로 quest_types 재구축");
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

        // 9 개 snapshot fake — 시간차 별로 작성 (이제 디렉토리).
        for i in 0..9 {
            let ts = format!("2026010{i}-000000");
            let p = paths.snapshots_dir().join(&ts);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("marker.guild"), format!("snapshot-{i}")).unwrap();
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
            let p = paths.snapshots_dir().join(ts);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("marker.guild"), b"x").unwrap();
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

    /// DEV-175: delete_snapshot 이 디렉토리 제거 + 미존재/traversal 가드.
    #[tokio::test]
    async fn delete_snapshot_removes_and_guards() {
        let dir = fresh_tmp("del");
        let store = setup(&dir).await;
        let paths = store.paths.clone();
        for ts in ["20260101-120000", "20260102-120000"] {
            let p = paths.snapshots_dir().join(ts);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("marker.guild"), b"x").unwrap();
        }
        // 존재하는 것 삭제 → 디렉토리 사라지고 목록 1개.
        delete_snapshot(&paths, "20260101-120000").unwrap();
        assert!(!paths.snapshots_dir().join("20260101-120000").exists());
        assert_eq!(list_snapshots(&paths).unwrap().len(), 1);
        // 미존재 → 에러.
        assert!(delete_snapshot(&paths, "29990101-000000").is_err());
        // traversal 가드.
        assert!(delete_snapshot(&paths, "../evil").is_err());
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

    /// BUG-086: now_compact ↔ snapshot_time 왕복이 현재 시각과 근접 (둘 다 UTC).
    /// 생성/파싱 규약이 어긋나면 TZ offset(예: 9h)만큼 벌어져 실패한다.
    #[test]
    fn now_compact_roundtrips() {
        let ts = now_compact();
        let parsed = snapshot_time(&ts).expect("now_compact 출력은 snapshot_time 으로 파싱돼야");
        let now = std::time::SystemTime::now();
        let diff = now
            .duration_since(parsed)
            .or_else(|e| Ok::<_, std::time::SystemTimeError>(e.duration()))
            .unwrap();
        assert!(
            diff < Duration::from_secs(5),
            "now_compact↔snapshot_time 왕복이 현재와 5초 내여야: diff={diff:?}"
        );
    }

    /// BUG-086(후속): UTC 저장값을 로컬 표시로 변환 — KST(+09:00)면 +9h.
    #[test]
    fn ts_to_local_display_converts_utc() {
        // 파싱 실패는 원본 그대로.
        assert_eq!(ts_to_local_display("invalid"), "invalid");
        // 형식 변환 확인 (로컬 offset 은 환경마다 달라 형식만 검증).
        let out = ts_to_local_display("20260101-000000");
        assert_eq!(out.len(), 19, "YYYY-MM-DD HH:MM:SS 형식: {out}");
        assert_eq!(&out[4..5], "-");
        assert_eq!(&out[13..14], ":");
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

        // BUG-076: pre-restore 백업은 이제 backups/.pre-restore/ 디렉토리 + 그 안 index.db.
        assert!(store.paths.backups_dir().join(".pre-restore").exists());
        assert!(store.paths.backups_dir().join(".pre-restore").join("index.db").exists());
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

    /// DEV-180: history 사이드카(`.guild/history/`)가 quests/ 밖으로 분리된
    /// 뒤 SOURCE_SUBDIRS 에 등록을 빠뜨리면 snapshot/restore 가 조용히
    /// history/ 를 건너뛴다 — 회귀 방지.
    #[tokio::test]
    async fn snapshot_preserves_history_sidecar() {
        use crate::models::CreateQuestRequest;
        use crate::ops::quests as quest_ops;

        let dir = fresh_tmp("snap-history");
        let store = setup(&dir).await;
        // change_status 대상 "testing" 이 DB 에 있어야 — 시드된 status 파일을
        // index.db 로 반영.
        crate::reindex::reindex(&store).await.unwrap();

        let q = quest_ops::create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "for history backup".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        // change_status 가 사이드카에 append.
        quest_ops::change_status(
            &store,
            q.id,
            crate::models::ChangeStatusRequest {
                status_slug: "testing".into(),
            },
        )
        .await
        .unwrap();

        let sidecar = store.paths.quest_history_sidecar_path(&q.quest_id);
        assert!(sidecar.exists(), "change_status 가 사이드카를 생성해야");

        let snap = create_snapshot(&store).await.unwrap();
        assert!(
            snap.path.join(".guild/history").join(format!("{}.jsonl", q.quest_id)).exists(),
            "snapshot 이 .guild/history/ 를 포함해야 (SOURCE_SUBDIRS 등록 확인)"
        );

        // DB 캐시만 손실 시나리오: quest_history 를 비우고 restore 가 사이드카에서
        // 되살리는지 확인.
        sqlx::query("DELETE FROM quest_history")
            .execute(&store.index_pool)
            .await
            .unwrap();

        restore_snapshot(&store, &snap).await.unwrap();

        let store2 = Store::open(&dir).await.unwrap();
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM quest_history WHERE quest_slug = ?",
        )
        .bind(&q.quest_id)
        .fetch_one(&store2.index_pool)
        .await
        .unwrap();
        assert_eq!(n, 1, "restore 후 사이드카 기반으로 quest_history 복원돼야");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 도서관(library) 백업 확인 — 문서(폴더 안) + 첨부(DEV-237)까지 snapshot/
    /// restore 후 살아남고, DB 캐시(library_docs/library_folders)도
    /// reindex 로 재구축되는지. SOURCE_SUBDIRS 에 "library" 는 있지만
    /// "attachments"(첨부 실제 bytes) 까지 같이 살아야 첨부가 진짜 복원.
    #[tokio::test]
    async fn snapshot_preserves_library_docs_folders_and_attachments() {
        use crate::ops::{attachments as att_ops, library as lib_ops};

        let dir = fresh_tmp("snap-library");
        let store = setup(&dir).await;

        lib_ops::create_folder(&store, "아키텍처").await.unwrap();
        let book = lib_ops::create_book(&store, "라우터 설계", "본문", "아키텍처")
            .await
            .unwrap();
        let rel = att_ops::save_attachment(&store, b"SPEC-BYTES", "pdf")
            .await
            .unwrap();
        att_ops::add_book_attachment(&store, &book.book_id(), &rel, "spec.pdf")
            .await
            .unwrap();

        let snap = create_snapshot(&store).await.unwrap();
        assert!(
            snap.path.join(".guild/library").join(format!("{}.md", book.book_id())).exists(),
            "snapshot 이 도서관 문서를 포함해야"
        );
        assert!(
            snap.path
                .join(".guild/library")
                .join(format!("{}.attachments.json", book.book_id()))
                .exists(),
            "snapshot 이 도서관 첨부 sidecar 를 포함해야"
        );
        assert!(
            snap.path.join(".guild/attachments").join(rel.trim_start_matches("attachments/")).exists(),
            "snapshot 이 첨부 실제 bytes 도 포함해야"
        );

        // DB 캐시만 손실 시나리오.
        sqlx::query("DELETE FROM library_docs").execute(&store.index_pool).await.unwrap();
        sqlx::query("DELETE FROM library_folders").execute(&store.index_pool).await.unwrap();

        restore_snapshot(&store, &snap).await.unwrap();

        let store2 = Store::open(&dir).await.unwrap();
        let restored = lib_ops::get_book(&store2, &book.book_id()).await.unwrap().unwrap();
        assert_eq!(restored.title, "라우터 설계");
        assert_eq!(restored.path, "아키텍처", "폴더 소속도 복원돼야");

        let folders = lib_ops::list_folders(&store2).await.unwrap();
        assert!(folders.iter().any(|f| f.path == "아키텍처"));

        let atts = att_ops::list_book_attachments(&store2, &book.book_id());
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0].name, "spec.pdf");
        let abs = store2.paths.dot_guild().join(&rel);
        assert_eq!(std::fs::read(&abs).unwrap(), b"SPEC-BYTES", "첨부 bytes 자체도 복원돼야");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
