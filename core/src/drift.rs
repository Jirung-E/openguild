//! 외부 편집 감지 — `.guild/quests/*.md` 의 mtime 과 `index.db` 의 `updated_at` 비교.
//!
//! 사용자가 CLI / GUI 없이 파일을 직접 편집한 경우 (또는 git pull 후 파일이 갱신된 경우)
//! 캐시 (`index.db`) 가 stale.
//!
//! 본 모듈:
//! - `detect_drift(store)` — 어떤 quest 파일이 캐시보다 새것인지 확인.
//! - `auto_resync(store)` — drift 발견 시 자동 reindex.
//!
//! 호출 시점: Store::open 직후 (또는 server / cli 시작 hook).

use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::repo::fs as repo_fs;
use crate::store::Store;

// BUG-067/068: drift 판정이 global `app_meta.last_indexed_at` → per-row
// (quest: cached_mtime) / per-file (sibling: file_mtime_cache) 비교로 바뀌어
// last_indexed_at 기반 fetch 헬퍼는 제거됨. last_indexed_at 마커 자체는 reindex
// 가 계속 기록(다른 용도 가능).

#[derive(Debug, Clone, serde::Serialize)]
pub struct DriftReport {
    pub fresh_files: Vec<String>, // quest_id slug 들
    pub missing_in_index: Vec<String>, // 파일은 있는데 index 에 없음
    pub stale_in_index: Vec<String>, // index 에 있는데 파일이 없음
    /// DEV-102: sibling 파일 (`{slug}.comments.md` / `{slug}.memo.md`) 의 mtime
    /// 이 index.db 보다 새것 — 캐시 (`quest_comments` / `quest_memos`) 가 stale.
    /// 항목 형식: `{slug}.comments.md` / `{slug}.memo.md` 로 file_name 그대로.
    #[serde(default)]
    pub fresh_siblings: Vec<String>,
}

impl DriftReport {
    pub fn is_clean(&self) -> bool {
        self.fresh_files.is_empty()
            && self.missing_in_index.is_empty()
            && self.stale_in_index.is_empty()
            && self.fresh_siblings.is_empty()
    }
}

/// drift 검출.
///
/// - **quest 본문**: 파일 mtime(nanos) > 그 quest 행의 `cached_mtime` 이면 fresh
///   (= 우리 write 경로 밖에서 파일이 바뀜). per-row 라 정확.
/// - **sibling** (`.comments.md` / `.memo.md`): 아직 per-row mtime 이 없어 global
///   `last_indexed_at` 기준.
/// - missing/stale: 파일 ↔ index slug 집합 차집합.
pub async fn detect_drift(store: &Store) -> Result<DriftReport> {
    let paths = &store.paths;
    let pool = &store.index_pool;

    // quest 본문 drift 는 **per-row `cached_mtime`** 로 판단 (DEV-121 / migration
    // 0015). 즉 "파일 mtime 이 그 quest 행에 기록된 cached_mtime 보다 새것인가".
    //
    // 이전엔 global `last_indexed_at` 과 비교했는데, ops mutation / 시동 sync /
    // 상세 lazy(DEV-137) 가 데이터(+cached_mtime)는 갱신해도 last_indexed_at 을
    // 안 올리는 경우, **데이터는 최신인 quest 가 매번 fresh 로 오탐**됐다
    // (사용자 보고: 파일/리스트엔 제대로 보이는데 admin drift 에는 걸림).
    // cached_mtime 은 reindex / incremental sync / lazy refresh / 모든 ops 의
    // write_quest_file 이 파일 mtime 으로 동기화하므로, 외부 편집(우리 write
    // 경로 밖에서 파일이 바뀜)만 정확히 fresh 로 잡힌다.
    //
    // BUG-047: sibling `.comments.md` / `.memo.md` 는 본문 목록에서 제외.
    let index_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT qt.prefix || '-' || printf('%03d', q.number), q.cached_mtime
         FROM quests q JOIN quest_types qt ON q.quest_type_id = qt.id",
    )
    .fetch_all(pool)
    .await
    .context("index quest 조회 실패")?;
    let db_mtime: HashMap<String, i64> = index_rows.into_iter().collect();

    let quest_paths = repo_fs::list_quest_body_files(paths.quests_dir())?;
    let mut file_slugs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut fresh_files = Vec::new();

    for path in &quest_paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let slug = stem.to_string();
        file_slugs.insert(slug.clone());
        // db 에 있는 quest 만 fresh 판정 (없으면 missing_in_index 로).
        if let Some(&cached) = db_mtime.get(&slug) {
            let file_mtime = repo_fs::mtime_unix_nanos(path);
            if file_mtime > cached {
                fresh_files.push(slug);
            }
        }
    }
    fresh_files.sort();

    let mut missing_in_index: Vec<String> = file_slugs
        .iter()
        .filter(|s| !db_mtime.contains_key(*s))
        .cloned()
        .collect();
    missing_in_index.sort();

    let mut stale_in_index: Vec<String> = db_mtime
        .keys()
        .filter(|s| !file_slugs.contains(*s))
        .cloned()
        .collect();
    stale_in_index.sort();

    // DEV-102/134: sibling 파일(quest/campaign 의 댓글·메모)도 캐시
    // (quest_comments / quest_memos / campaign_*) 를 가져 외부 편집 감지 대상.
    // BUG-068: per-file `file_mtime_cache` 와 비교 — quest 본문의 per-row
    // cached_mtime 과 동일 취지. 캐시에 mtime 이 있고 파일이 그보다 새것일 때만
    // fresh (ops 가 댓글/메모를 써도 그때 캐시도 같이 갱신되므로 오탐 X). 캐시에
    // 아직 없는 파일은 미반영으로 보고 fresh (reindex 가 sync_all 로 채움).
    let sib_cache = crate::file_mtime::load_all(store).await;
    let mut fresh_siblings = Vec::new();
    for path in repo_fs::list_quest_comment_files(paths.quests_dir())?
        .into_iter()
        .chain(repo_fs::list_quest_memo_files(paths.quests_dir())?)
        .chain(repo_fs::list_quest_comment_files(paths.campaigns_dir())?)
        .chain(repo_fs::list_quest_memo_files(paths.campaigns_dir())?)
    {
        let rel = crate::file_mtime::rel_key(paths, &path);
        let file_mtime = repo_fs::mtime_unix_nanos(&path);
        let fresh = match sib_cache.get(&rel) {
            Some(&cached) => file_mtime > cached,
            None => true,
        };
        if fresh && let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            fresh_siblings.push(name.to_string());
        }
    }
    // DEV-178: primary cached(캠페인 본문 + types/statuses/tags 정의)도 같은
    // file_mtime_cache 로 외부편집을 감지 — per-row cached_mtime 이 없어서다.
    // 단 sibling 과 달리 "캐시에 없음"을 fresh 로 보지 않는다: 메타는 migration
    // 시드로 DB 엔 있는데 캐시엔 없을 수 있어 None=>fresh 면 오탐(§27 회귀).
    // 캐시에 있고 파일이 더 새것일 때만 fresh — 신규 파일 적재는 reindex 담당.
    // (이름은 fresh_siblings 지만 의미상 "캐시 기반 fresh 파일" — 하나라도 있으면
    // is_clean()=false → auto_resync 가 reindex.)
    for path in crate::file_mtime::list_primary_cached_files(paths) {
        let rel = crate::file_mtime::rel_key(paths, &path);
        if let Some(&cached) = sib_cache.get(&rel)
            && repo_fs::mtime_unix_nanos(&path) > cached
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            fresh_siblings.push(name.to_string());
        }
    }
    fresh_siblings.sort();

    Ok(DriftReport {
        fresh_files,
        missing_in_index,
        stale_in_index,
        fresh_siblings,
    })
}

/// drift 발견 시 자동 reindex.
/// drift 없으면 no-op.
pub async fn auto_resync(store: &Store) -> Result<Option<crate::reindex::ReindexReport>> {
    let drift = detect_drift(store).await?;
    if drift.is_clean() {
        return Ok(None);
    }
    tracing::info!(
        "drift detected — fresh {} / missing {} / stale {} / fresh siblings {}. Running reindex...",
        drift.fresh_files.len(),
        drift.missing_in_index.len(),
        drift.stale_in_index.len(),
        drift.fresh_siblings.len()
    );
    let report = crate::reindex::reindex(store).await?;
    Ok(Some(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use crate::repo::{seed_guild_dir, QuestFile, QuestFrontmatter};

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-drift-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        Store::open(dir).await.unwrap()
    }

    #[tokio::test]
    async fn no_drift_when_index_in_sync() {
        let dir = fresh_tmp("clean");
        let store = setup(&dir).await;
        let _ = ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "t".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();

        let report = detect_drift(&store).await.unwrap();
        // create 가 파일 mtime 을 index.db mtime 직후로 만들었을 수 있음 — 그 경우 fresh 표시될 수도.
        // 본 테스트의 핵심: missing/stale 은 비어있어야.
        assert!(report.missing_in_index.is_empty());
        assert!(report.stale_in_index.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-047 regression: sibling `.comments.md` / `.memo.md` 가 있어도
    /// `missing_in_index` 에 안 들어가야 함. 이전엔 quest 본문으로 오인해서
    /// 매번 false positive.
    #[tokio::test]
    async fn detect_drift_excludes_sibling_files() {
        let dir = fresh_tmp("siblings");
        let store = setup(&dir).await;

        // 1) 정상 quest 하나 — ops 경로로 만들어 index 와 정합.
        let q = ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "with siblings".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();

        // 2) sibling 파일 작성 — quest_id slug 옆에 직접.
        // (DEV-012 / DEV-094 / DEV-099 가 사용자 작업으로 만들어 둘 만한 패턴.)
        let slug = &q.quest_id;
        let comments = store.paths.quests_dir().join(format!("{slug}.comments.md"));
        let memo = store.paths.quests_dir().join(format!("{slug}.memo.md"));
        std::fs::write(&comments, "<!-- og-comment id=\"1\" ts=\"\" author=\"\" -->\n사용자 댓글\n").unwrap();
        std::fs::write(&memo, "personal note").unwrap();

        let report = detect_drift(&store).await.unwrap();
        // sibling 들은 file_slugs 에 안 들어가야 하므로 missing_in_index 비어야.
        assert!(
            report.missing_in_index.is_empty(),
            "sibling 이 missing_in_index 에 누적되면 안 됨: {:?}",
            report.missing_in_index
        );
        // stale_in_index 도 비어야 (위에서 만든 quest 는 index 에 있음).
        assert!(report.stale_in_index.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-121 #8 회귀: ops/sync 로 데이터는 최신인데 global last_indexed_at 이
    /// 안 올라가 drift 가 오탐되던 문제. per-row cached_mtime 비교라, ops 로
    /// 만든/고친 quest 는 fresh 로 안 잡히고, 우리 write 경로 밖에서 파일이 더
    /// 새로 바뀐 경우(외부 편집)만 fresh 로 잡힌다.
    #[tokio::test]
    async fn drift_fresh_only_for_external_edit_not_ops() {
        let dir = fresh_tmp("per-row");
        let store = setup(&dir).await;
        let q = ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "t".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let slug = q.quest_id.clone();
        let path = store.paths.quest_path(&slug);

        // ops 로 막 만든 quest → write_quest_file 이 cached_mtime 동기화 → fresh 아님.
        let r1 = detect_drift(&store).await.unwrap();
        assert!(
            !r1.fresh_files.contains(&slug),
            "ops write 직후 fresh 오탐: {:?}",
            r1.fresh_files
        );

        // 외부 편집 시뮬: ops 밖에서 파일 mtime 을 더 새로 (내용 그대로 재기록).
        std::thread::sleep(std::time::Duration::from_millis(20));
        let content = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, content).unwrap();

        let r2 = detect_drift(&store).await.unwrap();
        assert!(
            r2.fresh_files.contains(&slug),
            "외부 편집은 fresh 로 잡혀야: {:?}",
            r2.fresh_files
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-177: `quest new` 는 `.guild/types/{prefix}.toml` 의 counter 도 쓴다
    /// (DEV-242). 그때 mtime 캐시를 갱신하지 않으면 **퀘스트를 만들 때마다**
    /// drift 거짓 경고가 떠서 사용자가 `--resync`(=전체 reindex) 를 돌리게 된다.
    #[tokio::test]
    async fn type_counter_write_does_not_cause_drift() {
        let dir = fresh_tmp("type-counter-drift");
        let store = setup(&dir).await;

        let mk = |title: &str| crate::models::CreateQuestRequest {
            quest_type_id: 1,
            title: title.into(),
            description: None,
            status_slug: "open".into(),
            urgency: Some(3),
            parent_quest_id: None,
        };

        // 첫 quest — counter 파일이 실제로 갱신되는(last_number 0 → 1) 경로.
        ops::create_quest(&store, mk("첫 퀘스트")).await.unwrap();
        let r1 = detect_drift(&store).await.unwrap();
        assert!(
            r1.is_clean(),
            "quest new 직후 drift 오탐: fresh={:?} siblings={:?} missing={:?}",
            r1.fresh_files,
            r1.fresh_siblings,
            r1.missing_in_index
        );

        // 두 번째도 동일 — counter 가 매번 올라가므로 매번 파일이 다시 쓰인다.
        std::thread::sleep(std::time::Duration::from_millis(20));
        ops::create_quest(&store, mk("둘째 퀘스트")).await.unwrap();
        let r2 = detect_drift(&store).await.unwrap();
        assert!(
            r2.is_clean(),
            "두 번째 quest new 후 drift 오탐: fresh={:?} siblings={:?}",
            r2.fresh_files,
            r2.fresh_siblings
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-068: ops 로 쓴 sibling(댓글) 은 file_mtime_cache 가 같이 갱신돼
    /// fresh_siblings 오탐 X. ops 밖에서 파일을 더 새로 바꾼 경우만 fresh.
    #[tokio::test]
    async fn sibling_drift_only_for_external_edit_not_ops() {
        let dir = fresh_tmp("sib-per-file");
        let store = setup(&dir).await;
        let q = ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "t".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let slug = q.quest_id.clone();
        // ops 로 댓글 추가 → 파일 write + file_mtime_cache touch.
        ops::comments::add_comment_entry(&store, &slug, "a".into(), "x".into(), None)
            .await
            .unwrap();

        let r1 = detect_drift(&store).await.unwrap();
        assert!(
            r1.fresh_siblings.is_empty(),
            "ops 로 쓴 댓글은 fresh_siblings 오탐 X: {:?}",
            r1.fresh_siblings
        );

        // 외부 편집 시뮬: ops 밖에서 comments 파일 mtime 더 새로.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let cpath = store.paths.comments_path(&slug);
        let content = std::fs::read_to_string(&cpath).unwrap();
        std::fs::write(&cpath, content).unwrap();

        let r2 = detect_drift(&store).await.unwrap();
        assert!(
            r2.fresh_siblings.iter().any(|n| n.ends_with(".comments.md")),
            "외부 편집한 sibling 은 fresh: {:?}",
            r2.fresh_siblings
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-178: 캠페인 본문 + 메타(status 정의)의 외부 편집이 drift 로 잡혀야.
    /// reindex 직후엔 clean (sync_all 이 캐시를 채움) → 외부 편집만 fresh.
    #[tokio::test]
    async fn drift_detects_external_campaign_and_meta_edits() {
        let dir = fresh_tmp("camp-meta");
        let store = setup(&dir).await;
        std::fs::create_dir_all(store.paths.campaigns_dir()).unwrap();
        let cpath = store.paths.campaigns_dir().join("C-001.md");
        std::fs::write(
            &cpath,
            "+++\ncampaign_id = \"C-001\"\ntitle = \"c\"\nstatus = \"active\"\ncreated_at = \"x\"\nupdated_at = \"x\"\n+++\nbody v1\n",
        )
        .unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        // reindex 직후엔 clean (캠페인 + 메타 모두 캐시와 일치).
        assert!(
            detect_drift(&store).await.unwrap().is_clean(),
            "reindex 직후엔 clean 이어야"
        );

        // 1) 캠페인 본문 외부 편집 → drift.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            &cpath,
            "+++\ncampaign_id = \"C-001\"\ntitle = \"c2\"\nstatus = \"active\"\ncreated_at = \"x\"\nupdated_at = \"x\"\n+++\nbody v2\n",
        )
        .unwrap();
        assert!(
            !detect_drift(&store).await.unwrap().is_clean(),
            "캠페인 본문 외부편집이 drift 로 잡혀야"
        );

        // reindex 로 clean 회복.
        crate::reindex::reindex(&store).await.unwrap();
        assert!(detect_drift(&store).await.unwrap().is_clean());

        // 2) 메타(status 정의 toml) 외부 편집 → drift.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let status_files =
            crate::repo::fs::list_with_extension(store.paths.statuses_dir(), "toml").unwrap();
        let sf = status_files.first().expect("seed status 존재");
        let content = std::fs::read_to_string(sf).unwrap();
        std::fs::write(sf, content).unwrap(); // 내용 동일, mtime 만 갱신.
        assert!(
            !detect_drift(&store).await.unwrap().is_clean(),
            "status 정의 외부편집이 drift 로 잡혀야"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn detects_file_not_in_index() {
        let dir = fresh_tmp("missing");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        // index 에 없는 quest 파일 직접 작성
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "manual".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path("DEV-001")).unwrap();

        let report = detect_drift(&store).await.unwrap();
        assert!(report.missing_in_index.contains(&"DEV-001".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn auto_resync_clean_returns_none() {
        let dir = fresh_tmp("resync-clean");
        let store = setup(&dir).await;
        let result = auto_resync(&store).await.unwrap();
        assert!(result.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-102: sibling 파일이 캐시보다 새것이면 fresh_siblings 에 잡힘.
    /// (BUG-047 의 sibling 제외 회귀와 별개 — 본문 vs sibling 의 역할 분리.)
    #[tokio::test]
    async fn detect_drift_picks_up_fresh_sibling_files() {
        let dir = fresh_tmp("sibling-fresh");
        let store = setup(&dir).await;

        // 정상 quest 생성 + DB 적재.
        let q = ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "for sibling drift".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();

        // index.db 의 mtime 보다 sibling 파일이 새것이 되도록, 잠시 후 sibling 작성.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let slug = &q.quest_id;
        let comments = store.paths.quests_dir().join(format!("{slug}.comments.md"));
        let memo = store.paths.quests_dir().join(format!("{slug}.memo.md"));
        std::fs::write(&comments, "<!-- og-comment id=\"1\" ts=\"\" author=\"\" -->\nx\n").unwrap();
        std::fs::write(&memo, "private").unwrap();

        let report = detect_drift(&store).await.unwrap();
        // sibling 두 파일이 fresh_siblings 에 잡혀야.
        assert!(
            report.fresh_siblings.iter().any(|n| n.ends_with(".comments.md")),
            "comments sibling not in fresh: {:?}",
            report.fresh_siblings
        );
        assert!(
            report.fresh_siblings.iter().any(|n| n.ends_with(".memo.md")),
            "memo sibling not in fresh: {:?}",
            report.fresh_siblings
        );
        // 기존 missing/stale 검사는 깨끗해야.
        assert!(report.missing_in_index.is_empty());
        assert!(report.stale_in_index.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-059: `last_indexed_at` 마커를 명시적으로 과거 시각으로 설정하고
    /// 파일은 그 이후에 작성 → `fresh_files` 에 잡혀야. (이전엔 index.db
    /// mtime 만 봤기 때문에 sqlite 가 NOW 로 mtime 을 튕기면 false negative.)
    #[tokio::test]
    async fn drift_uses_last_indexed_at_marker_not_index_db_mtime() {
        let dir = fresh_tmp("bug-059");
        let store = setup(&dir).await;

        // 1) reindex 한 번 → 정상적으로 last_indexed_at 마커 기록.
        crate::reindex::reindex(&store).await.unwrap();

        // 2) 마커를 일부러 24h 이전 ISO 시각으로 덮어씀 — "오래된 reindex" 시뮬레이션.
        let yesterday = (chrono::Local::now() - chrono::Duration::hours(24))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        sqlx::query(
            "INSERT INTO app_meta (key, value) VALUES ('last_indexed_at', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(&yesterday)
        .execute(&store.index_pool)
        .await
        .unwrap();

        // 3) 그 후 quest 파일 작성 — mtime = NOW > yesterday.
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "external edit".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        qf.write(store.paths.quest_path("DEV-001")).unwrap();

        // 4) detect_drift: yesterday < file_mtime → fresh 또는 missing 에 잡혀야.
        //    (file 은 index 에 없으므로 missing_in_index 에도 잡힘.)
        let report = detect_drift(&store).await.unwrap();
        let detected = report.fresh_files.iter().any(|s| s == "DEV-001")
            || report.missing_in_index.iter().any(|s| s == "DEV-001");
        assert!(
            detected,
            "DEV-001 이 fresh 또는 missing_in_index 에 잡혀야 — 마커 기반 비교: \
             fresh={:?}, missing={:?}",
            report.fresh_files, report.missing_in_index
        );
    }

    /// BUG-059 (fix2): 마커가 비어있으면 epoch 으로 fallback → 모든 기존 파일이
    /// fresh 로 잡혀 첫 부트스트랩 reindex 가 강제되어야 한다. 이전 fix1 은 빈
    /// 마커일 때 index.db mtime 으로 fallback 해서 동일한 false negative 가
    /// 재발 → reindex skip → 마커 영원히 empty 의 데드락.
    #[tokio::test]
    async fn drift_with_empty_marker_treats_all_files_as_fresh() {
        let dir = fresh_tmp("bug-059-bootstrap");
        let store = setup(&dir).await;

        // setup 직후 — 마커는 빈 값 (migration seed), index.db mtime ≈ NOW.
        // quest 파일을 하나 작성 (외부 편집 시뮬레이션). mtime 도 NOW.
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "first run".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        qf.write(store.paths.quest_path("DEV-001")).unwrap();

        // fix1 같으면: 빈 마커 → index.db mtime fallback → file_mtime ≈ index_mtime
        // → strict > false → fresh_files 비어있음 → drift clean → reindex skip.
        // fix2: 빈 마커 → epoch → file_mtime > epoch → fresh 잡힘.
        let report = detect_drift(&store).await.unwrap();
        let detected = report.fresh_files.iter().any(|s| s == "DEV-001")
            || report.missing_in_index.iter().any(|s| s == "DEV-001");
        assert!(
            detected,
            "빈 마커일 때 첫 startup 에서 모든 파일이 fresh 로 잡혀야 (부트스트랩): \
             fresh={:?}, missing={:?}",
            report.fresh_files, report.missing_in_index
        );
    }

    #[tokio::test]
    async fn auto_resync_fixes_drift() {
        let dir = fresh_tmp("resync-fix");
        let store = setup(&dir).await;

        // index 에 없는 파일 작성
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "drift target".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        qf.write(store.paths.quest_path("DEV-001")).unwrap();

        // 자동 resync
        let report = auto_resync(&store).await.unwrap();
        assert!(report.is_some(), "drift 시 reindex 실행됨");

        // 이제 index 에 들어와 있음
        let title: String = sqlx::query_scalar(
            "SELECT title FROM quests WHERE id = ?",
        )
        .bind(1_i64)
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(title, "drift target");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
