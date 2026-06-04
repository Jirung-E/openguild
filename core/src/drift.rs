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
use std::time::SystemTime;

use crate::repo::fs as repo_fs;
use crate::store::Store;

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

/// drift 검출. 파일 mtime > index.db 의 updated_at 으로 판단.
///
/// 단점: ISO 8601 string 의 updated_at 과 OS mtime (SystemTime) 비교 어려움.
/// 대신 단순 휴리스틱:
/// - file mtime 이 index.db file mtime 보다 새것이면 그 파일은 fresh 후보.
/// - 정확히 어느 quest 가 변경됐는지는 alm rough — 모든 newer 파일 fresh 로 표기.
pub async fn detect_drift(store: &Store) -> Result<DriftReport> {
    let paths = &store.paths;
    let pool = &store.index_pool;

    // index.db 자체 mtime — 마지막 mutation 이 SQL 통해 적용된 시각의 lower bound.
    let index_mtime = repo_fs::mtime(paths.index_db()).unwrap_or(SystemTime::UNIX_EPOCH);

    // 파일 → mtime 맵
    // BUG-047: sibling `.comments.md` / `.memo.md` 제외 — 매번 missing_in_index
    // 로 잡혀서 reindex 후에도 drift 가 사라지지 않는 false positive.
    let quest_paths = repo_fs::list_quest_body_files(paths.quests_dir())?;
    let mut file_slugs: HashMap<String, SystemTime> = HashMap::new();
    let mut fresh_files = Vec::new();

    for path in &quest_paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let slug = stem.to_string();
        let mtime = repo_fs::mtime(path).unwrap_or(SystemTime::UNIX_EPOCH);
        file_slugs.insert(slug.clone(), mtime);
        if mtime > index_mtime {
            fresh_files.push(slug);
        }
    }
    fresh_files.sort();

    // index 에 있는 quest slug 들
    let index_slugs: Vec<String> = sqlx::query_scalar(
        "SELECT qt.prefix || '-' || printf('%03d', q.number)
         FROM quests q JOIN quest_types qt ON q.quest_type_id = qt.id",
    )
    .fetch_all(pool)
    .await
    .context("index quest 조회 실패")?;

    let mut missing_in_index: Vec<String> = file_slugs
        .keys()
        .filter(|s| !index_slugs.contains(s))
        .cloned()
        .collect();
    missing_in_index.sort();

    let mut stale_in_index: Vec<String> = index_slugs
        .into_iter()
        .filter(|s| !file_slugs.contains_key(s))
        .collect();
    stale_in_index.sort();

    // DEV-102: sibling 파일도 캐시 (`quest_comments` / `quest_memos`) 를 가지므로
    // 외부 편집 감지 대상. file mtime > index.db mtime 이면 fresh_siblings 에 추가.
    // missing/stale 은 sibling 에 대해선 따로 다루지 않음 — reindex 가 DELETE +
    // INSERT 이므로 fresh 한 번 표기 → auto_resync 가 일괄 갱신.
    let mut fresh_siblings = Vec::new();
    for path in repo_fs::list_quest_comment_files(paths.quests_dir())?
        .into_iter()
        .chain(repo_fs::list_quest_memo_files(paths.quests_dir())?)
    {
        let mtime = repo_fs::mtime(&path).unwrap_or(SystemTime::UNIX_EPOCH);
        if mtime > index_mtime
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
