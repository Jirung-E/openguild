//! `.guild/quests/*.md` + `.guild/types/*.toml` + `.guild/statuses/*.toml` 파일들로부터
//! `.guild/index.db` 의 캐시 내용을 재구축.
//!
//! 사용 시나리오:
//! - 외부 편집 (사용자가 .md 파일 직접 수정) 후 캐시 동기화
//! - git pull 후 변경된 파일들 반영
//! - index.db 손상 / 삭제 후 복구
//!
//! 알고리즘:
//! 1. 현재 index.db 의 quests / dependencies / counters 비움 (DELETE).
//! 2. types/ 파일들을 quest_types 에 INSERT (id 는 prefix 알파벳 순서).
//! 3. statuses/ 파일들을 quest_statuses 에 INSERT (sort_order 보존).
//! 4. quests/ 파일들을 quests 에 INSERT (id 는 type + number 로 유추 — 충돌 없게).
//! 5. dependencies 는 quest frontmatter 의 prerequisites 에서 빌드.
//! 6. counters 는 types/{prefix}.toml 의 [counter].last_number 에서 가져옴.

use std::collections::HashMap;

use crate::error::AppResult;
use crate::repo::{auto, fs as repo_fs, QuestFile, StatusFile, TypeFile};
use crate::store::Store;

#[derive(Debug, Default, Clone)]
pub struct ReindexReport {
    pub types_loaded: usize,
    pub statuses_loaded: usize,
    pub quests_loaded: usize,
    pub dependencies_loaded: usize,
    /// 파싱 / 무결성 실패로 skip 된 파일 (경로 + 사유).
    pub skipped: Vec<(String, String)>,
}

/// 메인 진입점.
pub async fn reindex(store: &Store) -> AppResult<ReindexReport> {
    let mut report = ReindexReport::default();
    let pool = &store.index_pool;
    let paths = &store.paths;

    // 1. 기존 내용 비움 (트랜잭션 안에서 — partial 실패 시 rollback).
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM quest_dependencies").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quest_positions").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quests").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quest_counters").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quest_statuses").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quest_types").execute(&mut *tx).await?;

    // 2. types — id 는 파일 정렬 순.
    let type_paths = repo_fs::list_with_extension(paths.types_dir(), "toml")
        .map_err(crate::error::AppError::Internal)?;
    let mut prefix_to_id: HashMap<String, i64> = HashMap::new();
    for (i, path) in type_paths.iter().enumerate() {
        let id = (i + 1) as i64;
        match TypeFile::read(path) {
            Ok(t) => {
                sqlx::query(
                    "INSERT INTO quest_types (id, prefix, color, description) VALUES (?, ?, ?, ?)",
                )
                .bind(id)
                .bind(&t.prefix)
                .bind(&t.color)
                .bind(&t.description)
                .execute(&mut *tx)
                .await?;
                // counter
                sqlx::query(
                    "INSERT INTO quest_counters (quest_type_id, last_number) VALUES (?, ?)",
                )
                .bind(id)
                .bind(t.counter.last_number)
                .execute(&mut *tx)
                .await?;
                prefix_to_id.insert(t.prefix.clone(), id);
                report.types_loaded += 1;
            }
            Err(e) => {
                report.skipped.push((path.display().to_string(), format!("{e:#}")));
            }
        }
    }

    // 3. statuses — id 는 파일 정렬 순 (파일명 prefix 가 정렬 기준 = sort_order 동일).
    let status_paths = repo_fs::list_with_extension(paths.statuses_dir(), "toml")
        .map_err(crate::error::AppError::Internal)?;
    let mut slug_to_status_id: HashMap<String, i64> = HashMap::new();
    for (i, path) in status_paths.iter().enumerate() {
        let id = (i + 1) as i64;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let slug = StatusFile::slug_from_filename(filename).unwrap_or(filename);
        match StatusFile::read(path) {
            Ok(s) => {
                sqlx::query(
                    "INSERT INTO quest_statuses (id, name_en, name_ko, color, sort_order)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(id)
                .bind(&s.name_en)
                .bind(&s.name_ko)
                .bind(&s.color)
                .bind(s.sort_order)
                .execute(&mut *tx)
                .await?;
                slug_to_status_id.insert(slug.to_string(), id);
                report.statuses_loaded += 1;
            }
            Err(e) => {
                report.skipped.push((path.display().to_string(), format!("{e:#}")));
            }
        }
    }

    // 4. quests — 파일 한 번 로드해서 모두 메모리에. id 는 파일 정렬 순.
    let quest_paths = repo_fs::list_with_extension(paths.quests_dir(), "md")
        .map_err(crate::error::AppError::Internal)?;
    let mut quest_files: Vec<(std::path::PathBuf, QuestFile)> = Vec::new();
    for path in &quest_paths {
        match QuestFile::read(path) {
            Ok(qf) => quest_files.push((path.clone(), qf)),
            Err(e) => {
                report.skipped.push((path.display().to_string(), format!("{e:#}")));
            }
        }
    }

    // slug → (id, parent_quest_id Option<i64>, prereq slugs)
    let mut slug_to_id: HashMap<String, i64> = HashMap::new();
    for (i, (_, qf)) in quest_files.iter().enumerate() {
        slug_to_id.insert(qf.frontmatter.quest_id.clone(), (i + 1) as i64);
    }

    for (i, (path, qf)) in quest_files.iter().enumerate() {
        let id = (i + 1) as i64;
        let prefix = qf.type_prefix().unwrap_or("").to_string();
        let Some(type_id) = prefix_to_id.get(&prefix).copied() else {
            report.skipped.push((
                path.display().to_string(),
                format!("unknown type prefix: {prefix}"),
            ));
            continue;
        };
        let number = match qf.number() {
            Ok(n) => n,
            Err(e) => {
                report.skipped.push((path.display().to_string(), format!("{e:#}")));
                continue;
            }
        };
        let Some(status_id) = slug_to_status_id.get(&qf.frontmatter.status).copied() else {
            report.skipped.push((
                path.display().to_string(),
                format!("unknown status slug: {}", qf.frontmatter.status),
            ));
            continue;
        };
        let parent_id = qf
            .frontmatter
            .parent
            .as_ref()
            .and_then(|s| slug_to_id.get(s).copied());

        let deleted_at: Option<String> = qf
            .frontmatter
            .deleted
            .then(|| qf.frontmatter.updated_at.clone());

        sqlx::query(
            "INSERT INTO quests
             (id, quest_type_id, number, title, description, status_id, urgency, parent_quest_id,
              created_at, updated_at, deleted_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(type_id)
        .bind(number)
        .bind(&qf.frontmatter.title)
        .bind(&qf.description)
        .bind(status_id)
        .bind(qf.frontmatter.urgency)
        .bind(parent_id)
        .bind(&qf.frontmatter.created_at)
        .bind(&qf.frontmatter.updated_at)
        .bind(deleted_at)
        .execute(&mut *tx)
        .await?;

        report.quests_loaded += 1;
    }

    // 5. dependencies — 각 quest 의 prerequisites 에서.
    for (_, qf) in &quest_files {
        let Some(qid) = slug_to_id.get(&qf.frontmatter.quest_id).copied() else {
            continue;
        };
        for pslug in &qf.frontmatter.prerequisites {
            let Some(pid) = slug_to_id.get(pslug).copied() else {
                continue;
            };
            sqlx::query(
                "INSERT OR IGNORE INTO quest_dependencies (quest_id, prerequisite_id) VALUES (?, ?)",
            )
            .bind(qid)
            .bind(pid)
            .execute(&mut *tx)
            .await?;
            report.dependencies_loaded += 1;
        }
    }

    tx.commit().await?;

    // 6. auto 블록을 SQL 기준으로 다시 그려서 파일에 쓰기 — 외부 편집 결과
    //    auto 블록이 stale 일 수 있음. write_consistent_auto_blocks 가 옵션.
    //    (현재 turn 에선 단순 reindex 만, auto 갱신은 호출자가 별도 호출 가능)
    let _ = auto::render; // keep import alive
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{seed_guild_dir, GuildPaths, QuestFile, QuestFrontmatter};

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-reindex-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup_store(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        Store::open(dir).await.unwrap()
    }

    #[tokio::test]
    async fn reindex_seeded_guild_no_quests() {
        let dir = fresh_tmp("empty");
        let store = setup_store(&dir).await;

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.types_loaded, 3);
        assert_eq!(report.statuses_loaded, 6);
        assert_eq!(report.quests_loaded, 0);
        assert!(report.skipped.is_empty());

        // index.db 에 types/statuses 들어가있음
        let n_types: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM quest_types").fetch_one(&store.index_pool).await.unwrap();
        assert_eq!(n_types, 3);
        let n_statuses: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_statuses").fetch_one(&store.index_pool).await.unwrap();
        assert_eq!(n_statuses, 6);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reindex_from_quest_files() {
        let dir = fresh_tmp("from-files");
        setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        // 파일 직접 작성 (외부 편집 시뮬레이션)
        let q1 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "first".into(),
                status: "open".into(),
                urgency: 2,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-05-16T15:00:00Z".into(),
                updated_at: "2026-05-16T15:00:00Z".into(),
                deleted: false,
            },
            description: "body".into(),
            auto_block: String::new(),
        };
        q1.write(paths.quest_path("DEV-001")).unwrap();

        let q2 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-002".into(),
                title: "child".into(),
                status: "in_progress".into(),
                urgency: 3,
                parent: Some("DEV-001".into()),
                prerequisites: vec![],
                created_at: "2026-05-16T15:01:00Z".into(),
                updated_at: "2026-05-16T15:01:00Z".into(),
                deleted: false,
            },
            description: String::new(),
            auto_block: String::new(),
        };
        q2.write(paths.quest_path("DEV-002")).unwrap();

        // counter 갱신 (사용자가 직접 — last_number 2)
        let mut dev = TypeFile::read(paths.type_path("DEV")).unwrap();
        dev.counter.last_number = 2;
        dev.write(paths.type_path("DEV")).unwrap();

        // 새 Store — index.db 빈 상태에서 reindex
        let store = Store::open(&dir).await.unwrap();
        let report = reindex(&store).await.unwrap();
        assert_eq!(report.quests_loaded, 2);

        // index.db 검증
        let titles: Vec<String> = sqlx::query_scalar("SELECT title FROM quests ORDER BY id")
            .fetch_all(&store.index_pool).await.unwrap();
        assert_eq!(titles, vec!["first".to_string(), "child".to_string()]);

        // parent 링크 보존
        let parent_id: Option<i64> = sqlx::query_scalar(
            "SELECT parent_quest_id FROM quests WHERE title = 'child'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert!(parent_id.is_some(), "child should have parent");

        // counter 보존 — DEV 의 type_id 를 prefix 로 조회.
        let counter: i64 = sqlx::query_scalar(
            "SELECT c.last_number FROM quest_counters c
             JOIN quest_types t ON c.quest_type_id = t.id
             WHERE t.prefix = 'DEV'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(counter, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reindex_skips_invalid_files() {
        let dir = fresh_tmp("invalid");
        let store = setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        // 정상 quest + 손상 quest
        let good = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "ok".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
            },
            description: String::new(),
            auto_block: String::new(),
        };
        good.write(paths.quest_path("DEV-001")).unwrap();
        std::fs::write(paths.quest_path("BROKEN"), "not a quest file").unwrap();

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.quests_loaded, 1);
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].1.contains("opening"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reindex_preserves_dependencies() {
        let dir = fresh_tmp("deps");
        let store = setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        let q1 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "a".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
            },
            description: String::new(),
            auto_block: String::new(),
        };
        q1.write(paths.quest_path("DEV-001")).unwrap();

        let q2 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-002".into(),
                title: "b".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec!["DEV-001".into()],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
            },
            description: String::new(),
            auto_block: String::new(),
        };
        q2.write(paths.quest_path("DEV-002")).unwrap();

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.dependencies_loaded, 1);

        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_dependencies")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
