//! DEV-121 Phase 1: startup incremental sync.
//!
//! 풀 `reindex()` 대신 변경된 파일만 re-parse + UPSERT. `stat()` 만으로
//! file mtime 을 읽어 SQLite 의 `quests.cached_mtime` (Unix nanos) 와 비교 —
//! microsecond 비용 / 파일.
//!
//! ## Scope (Phase 1)
//!
//! 본 모듈은 **`.guild/quests/*.md` (body files only)** 만 처리.
//! statuses / types / tags / campaigns / sibling (`{slug}.comments.md` /
//! `{slug}.memo.md`) 는 양이 적거나 (수~수십개) parse 비용이 작아 기존
//! `reindex` / `drift` 경로로 처리. 추후 Phase 1b 로 확장 가능.
//!
//! ## 시간 비교 안전성 (timezone)
//!
//! 비교 양쪽이 모두 Unix nanoseconds (절대 시각).
//! - File mtime: `SystemTime::duration_since(UNIX_EPOCH).as_nanos()`.
//! - DB: `INTEGER cached_mtime` (Unix nanos 저장).
//!
//! → local time / TZ / DST / 길드 이동에 무관.
//!
//! ## 알고리즘
//!
//! ```text
//! for each .md file in .guild/quests/ (body files only):
//!     file_mtime = stat(file).mtime_unix_nanos()
//!     db = SELECT slug, cached_mtime FROM quests WHERE slug = ?
//!     if db is None:                          # 신규 파일
//!         (reindex 가 처리 — Phase 1b 까지는 fallback)
//!     elif file_mtime > db.cached_mtime:      # 외부 편집
//!         parse + UPDATE (description, frontmatter 필드, cached_mtime)
//!     # else: skip
//!
//! for each db quest (alive):
//!     if file 사라짐:
//!         (reindex / drift 가 처리 — Phase 1b 까지는 fallback)
//! ```
//!
//! Phase 1 에선 **modified file 만** 처리 — 신규/삭제는 drift::auto_resync 가
//! 잡아 reindex 트리거. 이 조합으로 "외부 편집된 기존 파일이 mtime 비교
//! 실패로 안 잡히던" BUG-049 / BUG-059 의 핵심 시나리오를 해결.

use crate::error::AppResult;
use crate::repo::{fs as repo_fs, QuestFile};
use crate::store::Store;

#[derive(Debug, Default, Clone)]
pub struct IncrementalReport {
    /// 외부 편집 감지되어 re-parse + UPDATE 한 quest slug 수.
    pub updated: usize,
    /// 신규 / 삭제 등 본 모듈 범위 외 — 호출자가 drift::auto_resync 로
    /// 풀 reindex 트리거 권장.
    pub needs_full_reindex: bool,
    /// 파싱 실패 등으로 skip 한 항목.
    pub skipped: Vec<(String, String)>,
}

/// 변경된 파일만 동기화. 신규 / 삭제는 본 함수가 안 하고 `needs_full_reindex`
/// flag 만 set — 호출자가 drift::auto_resync 로 풀 reindex.
pub async fn sync_changed_quest_files(store: &Store) -> AppResult<IncrementalReport> {
    let mut report = IncrementalReport::default();
    let paths = &store.paths;
    let pool = &store.index_pool;

    // 파일 목록.
    let quest_paths = repo_fs::list_quest_body_files(paths.quests_dir())
        .map_err(crate::error::AppError::Internal)?;

    // DB 의 slug → (id, cached_mtime) 맵.
    let db_rows: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT q.id, qt.prefix || '-' || printf('%03d', q.number) AS slug, q.cached_mtime
         FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id
         WHERE q.deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    let db_map: std::collections::HashMap<String, (i64, i64)> = db_rows
        .into_iter()
        .map(|(id, slug, mtime)| (slug, (id, mtime)))
        .collect();

    // 파일 → DB row 매칭 + mtime 비교.
    let mut file_slugs = std::collections::HashSet::new();
    for path in &quest_paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let slug = stem.to_string();
        file_slugs.insert(slug.clone());

        let file_mtime = repo_fs::mtime_unix_nanos(path);

        match db_map.get(&slug) {
            None => {
                // 신규 파일 — 본 함수 범위 외. drift::auto_resync 가 처리.
                report.needs_full_reindex = true;
            }
            Some(&(id, cached_mtime)) => {
                if file_mtime > cached_mtime {
                    // 외부 편집. parse + UPDATE.
                    match QuestFile::read(path) {
                        Ok(qf) => {
                            // status_id 결정.
                            let status_id: Option<i64> = sqlx::query_scalar(
                                "SELECT id FROM quest_statuses WHERE slug = ?",
                            )
                            .bind(&qf.frontmatter.status)
                            .fetch_optional(pool)
                            .await?;
                            let Some(status_id) = status_id else {
                                report.needs_full_reindex = true; // status 가 없으면 풀 reindex 필요
                                report.skipped.push((
                                    path.display().to_string(),
                                    format!("unknown status slug: {}", qf.frontmatter.status),
                                ));
                                continue;
                            };

                            // parent / prereq 관계는 신규 컬럼 갱신만. 본격 cascade
                            // (quest_dependencies 재계산) 는 drift::auto_resync 가
                            // 풀 reindex 로 처리 — Phase 1 의 의도적 단순화.
                            let parent_slug = qf.frontmatter.parent.clone();
                            let parent_id: Option<i64> = match parent_slug {
                                Some(s) => sqlx::query_scalar(
                                    "SELECT q.id FROM quests q
                                     JOIN quest_types qt ON qt.id = q.quest_type_id
                                     WHERE qt.prefix || '-' || printf('%03d', q.number) = ?",
                                )
                                .bind(&s)
                                .fetch_optional(pool)
                                .await?,
                                None => None,
                            };

                            let created_at =
                                crate::time::normalize_legacy_ts(&qf.frontmatter.created_at);
                            let updated_at =
                                crate::time::normalize_legacy_ts(&qf.frontmatter.updated_at);
                            let deleted_at: Option<String> =
                                qf.frontmatter.deleted.then(|| updated_at.clone());

                            sqlx::query(
                                "UPDATE quests SET
                                   title = ?, description = ?, status_id = ?, urgency = ?,
                                   parent_quest_id = ?, created_at = ?, updated_at = ?,
                                   deleted_at = ?, desired_due = ?, required_due = ?,
                                   cached_mtime = ?
                                 WHERE id = ?",
                            )
                            .bind(&qf.frontmatter.title)
                            .bind(&qf.description)
                            .bind(status_id)
                            .bind(qf.frontmatter.urgency)
                            .bind(parent_id)
                            .bind(&created_at)
                            .bind(&updated_at)
                            .bind(deleted_at)
                            .bind(qf.frontmatter.desired_due.as_deref())
                            .bind(qf.frontmatter.required_due.as_deref())
                            .bind(file_mtime)
                            .bind(id)
                            .execute(pool)
                            .await?;

                            // prereq / tag cascade 는 Phase 1 범위 X — 풀 reindex 권장.
                            // (사용자가 frontmatter 의 prereq / tag 만 바꾼 경우엔 풀 reindex 필요.)
                            report.needs_full_reindex = true;

                            report.updated += 1;
                        }
                        Err(e) => {
                            report
                                .skipped
                                .push((path.display().to_string(), format!("{e:#}")));
                        }
                    }
                }
                // else: file mtime <= cached → no-op.
            }
        }
    }

    // DB 에만 있고 파일 사라진 — drift::auto_resync 가 처리.
    for slug in db_map.keys() {
        if !file_slugs.contains(slug) {
            report.needs_full_reindex = true;
            break;
        }
    }

    Ok(report)
}

/// Store::open 후 통합 sync. Phase 1: incremental + 필요 시 fallback reindex.
///
/// 흐름:
/// 1. `sync_changed_quest_files` — modified file 들 cheap 처리.
/// 2. needs_full_reindex 면 `drift::auto_resync` — 신규/삭제/다른 테이블 처리.
///
/// 통합 호출자는 `Store::open_with_sync` (store.rs).
pub async fn sync_on_open(
    store: &Store,
) -> AppResult<(IncrementalReport, Option<crate::reindex::ReindexReport>)> {
    let inc = sync_changed_quest_files(store).await?;
    let reindex_report = if inc.needs_full_reindex {
        crate::drift::auto_resync(store)
            .await
            .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?
    } else {
        None
    };
    Ok((inc, reindex_report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{seed_guild_dir, QuestFile, QuestFrontmatter};

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-inc-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        let store = Store::open(dir).await.unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        store
    }

    /// 외부 편집된 파일이 정확히 UPDATE 되고 cached_mtime 이 갱신.
    #[tokio::test]
    async fn modified_file_detected_and_updated() {
        let dir = fresh_tmp("modify");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        // 시드 + reindex 후 quest 하나 추가 + 풀 reindex.
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "original".into(),
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
            description: "body v1".into(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path("DEV-001")).unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        // 외부 편집 시뮬레이션 — 살짝 기다린 후 새 mtime 으로 덮어씀.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let qf2 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "edited externally".into(),
                status: "open".into(),
                urgency: 2,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-02T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: "body v2".into(),
            auto_block: String::new(),
        };
        qf2.write(paths.quest_path("DEV-001")).unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report.updated, 1, "modified file 1건 UPDATE 되어야");

        // DB 확인 — 새 title / urgency 가 반영.
        let row: (String, i64) =
            sqlx::query_as("SELECT title, urgency FROM quests WHERE id = 1")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(row.0, "edited externally");
        assert_eq!(row.1, 2);

        // 두 번째 호출 — 변경 없음.
        let report2 = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report2.updated, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 변경 없으면 cached_mtime 비교만 — UPDATE 0건.
    #[tokio::test]
    async fn no_change_no_update() {
        let dir = fresh_tmp("noop");
        let store = setup(&dir).await;

        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "stable".into(),
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
            description: "stable body".into(),
            auto_block: String::new(),
        };
        qf.write(store.paths.quest_path("DEV-001")).unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report.updated, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 신규 파일은 본 함수 범위 X — needs_full_reindex flag.
    #[tokio::test]
    async fn new_file_triggers_full_reindex_flag() {
        let dir = fresh_tmp("new");
        let store = setup(&dir).await;

        // 시드만 — quest 0건.
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "new quest".into(),
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

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report.updated, 0);
        assert!(report.needs_full_reindex, "신규 파일은 풀 reindex flag");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
