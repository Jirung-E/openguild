//! 기존 `<guild>/guild.db` 의 데이터를 `.guild/quests/*.md` 등 파일 진리원으로 이전.
//!
//! 일회성 명령 (`openguild-server migrate-to-files`) 에서 호출.
//! 이전 후에도 `guild.db` 는 그대로 두고 사용자가 검증 후 수동 삭제 (`.gitignore` 됨).
//!
//! 알고리즘:
//! 1. legacy `guild.db` 에서 모든 quest / type / status / dependency 로드.
//! 2. id → slug / id → title 룩업 빌드.
//! 3. 각 quest 마다 `QuestFile` 구성 (frontmatter + auto block) → `.guild/quests/{slug}.md` 작성.
//! 4. `.guild/types/{prefix}.toml` 의 `[counter].last_number` 를 `quest_counters` 에서 가져와 갱신.
//! 5. `.guild/index.db` 가 비어있으면 legacy guild.db 를 복사 (캐시 즉시 정상 상태).

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::db;
use crate::repo::{
    auto, GuildPaths, QuestFile, QuestFrontmatter, QuestRef, QuestRelations, StatusFile, TypeFile,
};

/// 진행 결과 보고.
#[derive(Debug, Default, Clone)]
pub struct MigrationReport {
    pub quests_written: usize,
    pub deleted_quests_included: usize,
    pub types_updated: usize,
    pub index_db_copied: bool,
    pub legacy_db_path: PathBuf,
    pub guild_root: PathBuf,
}

/// `<guild_root>/guild.db` 를 `.guild/` 구조로 마이그레이션.
///
/// 전제:
/// - `<guild_root>/{name}.guild` 마커가 존재 (검증은 호출자 책임).
/// - `<guild_root>/.guild/` 가 이미 시드되어 있음 (`seed_guild_dir` 호출 후 / `openguild init` 사용 시 자동).
/// - `<guild_root>/.guild/quests/` 가 비어있는 게 권장 (충돌 시 overwrite — caller 가 확인).
pub async fn migrate_to_files<P: AsRef<Path>>(guild_root: P) -> Result<MigrationReport> {
    let guild_root = guild_root.as_ref();
    let paths = GuildPaths::new(guild_root);
    let legacy_db = guild_root.join("guild.db");

    if !legacy_db.exists() {
        anyhow::bail!(
            "legacy DB 가 없습니다: {}\n\
             이미 마이그레이션이 끝났거나, 빈 길드일 수 있습니다.",
            legacy_db.display()
        );
    }

    // Windows canonicalize 는 `\\?\` 접두사를 붙임 — sqlx URL 파서가 `?` 를 query 시작으로 오인.
    let raw = legacy_db.to_string_lossy();
    let cleaned = raw
        .trim_start_matches(r"\\?\")
        .trim_start_matches(r"\\\\?\\")
        .replace('\\', "/");
    let legacy_url = format!("sqlite:{cleaned}?mode=ro");
    let pool = db::create_pool(&legacy_url)
        .await
        .with_context(|| format!("legacy DB open 실패: {legacy_url}"))?;

    // 모든 quest (alive + deleted) — frontmatter 의 deleted 플래그로 표현.
    let quests: Vec<QuestRowFlat> = sqlx::query_as::<_, QuestRowFlat>(
        r#"SELECT
            q.id,
            qt.prefix || '-' || printf('%03d', q.number) AS slug,
            qt.prefix AS type_prefix,
            q.number,
            q.title,
            q.description,
            qs.name_en AS status_name_en,
            q.urgency,
            q.parent_quest_id,
            q.created_at,
            q.updated_at,
            q.deleted_at
        FROM quests q
        JOIN quest_types qt ON q.quest_type_id = qt.id
        JOIN quest_statuses qs ON q.status_id = qs.id
        ORDER BY q.id"#,
    )
    .fetch_all(&pool)
    .await
    .context("legacy quests SELECT 실패")?;

    let deps: Vec<(i64, i64)> = sqlx::query_as("SELECT quest_id, prerequisite_id FROM quest_dependencies")
        .fetch_all(&pool)
        .await
        .context("legacy quest_dependencies SELECT 실패")?;

    let counters: Vec<(i64, i64)> = sqlx::query_as("SELECT quest_type_id, last_number FROM quest_counters")
        .fetch_all(&pool)
        .await
        .context("legacy quest_counters SELECT 실패")?;

    let type_map: Vec<(i64, String)> = sqlx::query_as("SELECT id, prefix FROM quest_types")
        .fetch_all(&pool)
        .await
        .context("legacy quest_types SELECT 실패")?;

    // 룩업 빌드
    let id_to_slug: HashMap<i64, String> = quests
        .iter()
        .map(|q| (q.id, q.slug.clone()))
        .collect();
    let id_to_title: HashMap<i64, String> = quests
        .iter()
        .map(|q| (q.id, q.title.clone()))
        .collect();

    let mut prereqs_by_quest: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut dependents_by_prereq: HashMap<i64, Vec<i64>> = HashMap::new();
    for (qid, pid) in &deps {
        prereqs_by_quest.entry(*qid).or_default().push(*pid);
        dependents_by_prereq.entry(*pid).or_default().push(*qid);
    }

    let mut children_by_parent: HashMap<i64, Vec<i64>> = HashMap::new();
    for q in &quests {
        if let Some(pid) = q.parent_quest_id {
            children_by_parent.entry(pid).or_default().push(q.id);
        }
    }

    // 각 quest 파일 작성
    let mut report = MigrationReport {
        legacy_db_path: legacy_db.clone(),
        guild_root: guild_root.to_path_buf(),
        ..Default::default()
    };

    for q in &quests {
        let qf = build_quest_file(
            q,
            &id_to_slug,
            &id_to_title,
            &prereqs_by_quest,
            &children_by_parent,
        )?;
        let path = paths.quest_path(&q.slug);
        qf.write(&path)
            .with_context(|| format!("quest 파일 작성 실패: {}", path.display()))?;

        report.quests_written += 1;
        if q.deleted_at.is_some() {
            report.deleted_quests_included += 1;
        }
    }

    // types/ counters 갱신
    let type_id_to_prefix: HashMap<i64, String> = type_map.into_iter().collect();
    for (type_id, last_number) in counters {
        if let Some(prefix) = type_id_to_prefix.get(&type_id) {
            let path = paths.type_path(prefix);
            if path.exists() {
                let mut t = TypeFile::read(&path)?;
                if t.counter.last_number < last_number {
                    t.counter.last_number = last_number;
                    t.write(&path)?;
                    report.types_updated += 1;
                }
            }
        }
    }

    // index.db 가 없으면 legacy 를 복사 (캐시 시드)
    let index_path = paths.index_db();
    if !index_path.exists() {
        std::fs::create_dir_all(paths.dot_guild())?;
        std::fs::copy(&legacy_db, &index_path)
            .with_context(|| format!("index.db 복사 실패: {}", index_path.display()))?;
        report.index_db_copied = true;
    }

    pool.close().await;
    Ok(report)
}

/// legacy DB row 의 flat 표현.
#[derive(sqlx::FromRow)]
struct QuestRowFlat {
    id: i64,
    slug: String,
    type_prefix: String,
    number: i64,
    title: String,
    description: Option<String>,
    status_name_en: String,
    urgency: i64,
    parent_quest_id: Option<i64>,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
}

/// 한 quest 의 `QuestFile` 빌드 (frontmatter + description + auto 블록).
fn build_quest_file(
    q: &QuestRowFlat,
    id_to_slug: &HashMap<i64, String>,
    id_to_title: &HashMap<i64, String>,
    prereqs_by_quest: &HashMap<i64, Vec<i64>>,
    children_by_parent: &HashMap<i64, Vec<i64>>,
) -> Result<QuestFile> {
    // 참조 헬퍼: id → QuestRef (slug + title). 누락된 id (== legacy DB 무결성 깨짐) 는 panic 대신 skip.
    let to_ref = |id: i64| -> Option<QuestRef> {
        let slug = id_to_slug.get(&id)?.clone();
        let title = id_to_title.get(&id).cloned().unwrap_or_default();
        Some(QuestRef::new(slug, title))
    };

    let parent_ref = q.parent_quest_id.and_then(to_ref);
    let sub_refs: Vec<QuestRef> = children_by_parent
        .get(&q.id)
        .map(|v| v.iter().filter_map(|id| to_ref(*id)).collect())
        .unwrap_or_default();
    let prereq_refs: Vec<QuestRef> = prereqs_by_quest
        .get(&q.id)
        .map(|v| v.iter().filter_map(|id| to_ref(*id)).collect())
        .unwrap_or_default();

    let relations = QuestRelations {
        parent: parent_ref.clone(),
        sub_quests: sub_refs,
        prerequisites: prereq_refs.clone(),
    };
    let auto_block = auto::render(&relations).trim().to_string();

    let frontmatter = QuestFrontmatter {
        quest_id: q.slug.clone(),
        title: q.title.clone(),
        status: status_name_to_slug(&q.status_name_en),
        urgency: q.urgency,
        parent: parent_ref.map(|r| r.quest_id),
        prerequisites: prereq_refs.into_iter().map(|r| r.quest_id).collect(),
        created_at: q.created_at.clone(),
        updated_at: q.updated_at.clone(),
        deleted: q.deleted_at.is_some(),
    };

    // _ 처리: type_prefix 와 number 는 frontmatter 에 직접 안 들어감 (slug 에서 derive).
    let _ = (&q.type_prefix, &q.number);

    Ok(QuestFile {
        frontmatter,
        description: q.description.clone().unwrap_or_default(),
        auto_block,
    })
}

/// status name_en → slug (`In Progress` → `in_progress`).
fn status_name_to_slug(name_en: &str) -> String {
    name_en.to_lowercase().replace(' ', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tmp(label: &str) -> PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-migrate-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn status_slug_normalization() {
        assert_eq!(status_name_to_slug("Open"), "open");
        assert_eq!(status_name_to_slug("In Progress"), "in_progress");
        assert_eq!(status_name_to_slug("On Hold"), "on_hold");
        assert_eq!(status_name_to_slug("Done"), "done");
    }

    #[tokio::test]
    async fn migrate_empty_db_fails() {
        let dir = fresh_tmp("empty");
        let err = migrate_to_files(&dir).await.unwrap_err();
        assert!(err.to_string().contains("legacy DB 가 없습니다"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn migrate_minimal_real_db() {
        // 최소 SQLite — types/statuses 시드 후 quest 하나 INSERT.
        let dir = fresh_tmp("minimal");
        crate::repo::seed_guild_dir(&dir).unwrap();

        let legacy = dir.join("guild.db");
        let url = format!(
            "sqlite:{}?mode=rwc",
            legacy.to_string_lossy().replace('\\', "/")
        );
        let pool = crate::db::create_pool(&url).await.unwrap();
        crate::db::run_migrations(&pool).await.unwrap();

        // DEV-001 quest 하나 직접 INSERT.
        sqlx::query("UPDATE quest_counters SET last_number = 1 WHERE quest_type_id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO quests (quest_type_id, number, title, status_id, urgency)
             VALUES (1, 1, 'first quest', 1, 3)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let report = migrate_to_files(&dir).await.unwrap();
        assert_eq!(report.quests_written, 1);
        assert_eq!(report.deleted_quests_included, 0);
        assert!(report.index_db_copied);

        // 파일 검증
        let path = dir.join(".guild/quests/DEV-001.md");
        assert!(path.is_file());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("quest_id = \"DEV-001\""));
        assert!(content.contains("title = \"first quest\""));
        assert!(content.contains("status = \"open\""));

        // 카운터 갱신
        let dev_type = crate::repo::TypeFile::read(dir.join(".guild/types/DEV.toml")).unwrap();
        assert_eq!(dev_type.counter.last_number, 1);

        // index.db 복사됨
        assert!(dir.join(".guild/index.db").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn migrate_preserves_relations() {
        let dir = fresh_tmp("relations");
        crate::repo::seed_guild_dir(&dir).unwrap();

        let url = format!(
            "sqlite:{}?mode=rwc",
            dir.join("guild.db").to_string_lossy().replace('\\', "/")
        );
        let pool = crate::db::create_pool(&url).await.unwrap();
        crate::db::run_migrations(&pool).await.unwrap();

        // 3 quests: DEV-001 root, DEV-002 child of 001, DEV-003 prereq of 002.
        sqlx::query("UPDATE quest_counters SET last_number = 3 WHERE quest_type_id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO quests (id, quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES
             (1, 1, 1, 'parent', 1, 2, NULL),
             (2, 1, 2, 'child',  1, 3, 1),
             (3, 1, 3, 'dep',    1, 3, NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO quest_dependencies (quest_id, prerequisite_id) VALUES (2, 3)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let report = migrate_to_files(&dir).await.unwrap();
        assert_eq!(report.quests_written, 3);

        // 자식 파일은 parent + prereq 둘 다 가져야
        let child = std::fs::read_to_string(dir.join(".guild/quests/DEV-002.md")).unwrap();
        assert!(child.contains("parent = \"DEV-001\""));
        assert!(child.contains("prerequisites = [\"DEV-003\"]"));
        // auto block 도 표시
        assert!(child.contains("[DEV-001](DEV-001.md)"));
        assert!(child.contains("[DEV-003](DEV-003.md)"));

        // 부모 파일은 sub-quest 로 child 표시
        let parent = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(parent.contains("[DEV-002](DEV-002.md)"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
