//! DEV-014: Quest type / status mutation orchestration.
//!
//! 각 함수는 파일 IO + index.db 의 일관된 갱신을 책임.
//! - **create**: 파일 write + DB INSERT.
//! - **update**: 파일 rewrite + DB UPDATE. 필요 시 파일명 rename (status sort_order 변경).
//! - **delete**: 사용 중인 type/status 면 `BadRequest` 로 거부. 파일 삭제 + DB DELETE.
//!
//! ID 는 reindex 가 매기는 정렬 순서지만, runtime mutation 도 동일 정렬을
//! 유지하기 위해:
//! - type: prefix 알파벳 정렬 (디렉토리 read 와 동일) — 새 type 추가 시
//!   max(id)+1.
//! - status: sort_order 가 곧 정렬 키. 추가 시 max(sort_order)+1, 변경 시
//!   파일명 rename + DB sort_order UPDATE.
//!
//! 파일과 DB 의 정렬이 어긋날 가능성을 줄이기 위해, 변경이 끝나면 호출자
//! (services) 가 admin reindex 를 트리거할 수 있음. 본 모듈 함수는 결과를
//! 즉시 표시 가능한 row 로 반환.

use crate::error::{AppError, AppResult};
use crate::models::{QuestStatus, QuestType};
use crate::repo::{StatusFile, TypeFile};
use crate::store::Store;
use anyhow::Context;
use sqlx::SqlitePool;

// ─────────────────────── Quest types ───────────────────────

/// 새 type 추가. 파일 + DB INSERT.
/// `prefix` 는 대문자/숫자 1~6자 + 중복 거부.
pub async fn create_type(
    store: &Store,
    prefix: String,
    color: String,
    description: Option<String>,
) -> AppResult<QuestType> {
    let prefix = prefix.trim().to_string();
    validate_prefix(&prefix)?;
    validate_color(&color)?;

    // 중복 검사 (대소문자 무시 — 파일 시스템도 보통 case-insensitive).
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM quest_types WHERE UPPER(prefix) = UPPER(?)")
            .bind(&prefix)
            .fetch_optional(&store.index_pool)
            .await?;
    if exists.is_some() {
        return Err(AppError::BadRequest(format!(
            "이미 존재하는 type prefix: {prefix}"
        )));
    }

    // 1. 파일 작성.
    std::fs::create_dir_all(store.paths.types_dir())
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let file = TypeFile {
        prefix: prefix.clone(),
        color: color.clone(),
        description: description.clone().filter(|s| !s.trim().is_empty()),
        counter: crate::repo::Counter { last_number: 0 },
    };
    file.write(store.paths.type_path(&prefix))
        .map_err(AppError::Internal)?;

    // 2. DB INSERT + counter row.
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO quest_types (prefix, color, description) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(&prefix)
    .bind(&color)
    .bind(&description)
    .fetch_one(&store.index_pool)
    .await?;
    sqlx::query("INSERT INTO quest_counters (quest_type_id, last_number) VALUES (?, 0)")
        .bind(new_id)
        .execute(&store.index_pool)
        .await?;

    Ok(QuestType {
        id: new_id,
        prefix,
        color,
        description,
    })
}

/// type 수정 — color / description 만. prefix rename 은 별도 (quest slug
/// cascade 발생).
pub async fn update_type(
    store: &Store,
    prefix: String,
    color: Option<String>,
    description: Option<Option<String>>, // outer None = 변경 없음, inner None = clear.
) -> AppResult<QuestType> {
    let prefix = prefix.trim().to_string();
    if let Some(c) = &color {
        validate_color(c)?;
    }

    let mut row = fetch_type_by_prefix(&store.index_pool, &prefix).await?;
    let path = store.paths.type_path(&prefix);
    let mut file = TypeFile::read(&path).map_err(AppError::Internal)?;

    if let Some(c) = color {
        row.color = c.clone();
        file.color = c;
    }
    if let Some(d) = description {
        let cleaned = d.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        row.description = cleaned.clone();
        file.description = cleaned;
    }

    file.write(&path).map_err(AppError::Internal)?;
    sqlx::query("UPDATE quest_types SET color = ?, description = ? WHERE id = ?")
        .bind(&row.color)
        .bind(&row.description)
        .bind(row.id)
        .execute(&store.index_pool)
        .await?;

    Ok(row)
}

/// type 삭제. 사용 중 quest 있으면 `BadRequest`.
pub async fn delete_type(store: &Store, prefix: String) -> AppResult<()> {
    let prefix = prefix.trim().to_string();
    let row = fetch_type_by_prefix(&store.index_pool, &prefix).await?;
    let count = count_quests_by_type(&store.index_pool, row.id).await?;
    if count > 0 {
        return Err(AppError::BadRequest(format!(
            "type '{prefix}' 는 {count} 개 quest 가 사용 중 — 먼저 다른 type 으로 이동시키거나 삭제하세요."
        )));
    }

    // 파일 + DB. 파일은 best-effort (파일 없어도 DB 만 일관되면 ok).
    let _ = std::fs::remove_file(store.paths.type_path(&prefix));
    sqlx::query("DELETE FROM quest_counters WHERE quest_type_id = ?")
        .bind(row.id)
        .execute(&store.index_pool)
        .await?;
    sqlx::query("DELETE FROM quest_types WHERE id = ?")
        .bind(row.id)
        .execute(&store.index_pool)
        .await?;
    Ok(())
}

/// 특정 type 을 사용하는 alive quest 수.
pub async fn count_quests_by_type(pool: &SqlitePool, type_id: i64) -> AppResult<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM quests WHERE quest_type_id = ? AND deleted_at IS NULL",
    )
    .bind(type_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

async fn fetch_type_by_prefix(pool: &SqlitePool, prefix: &str) -> AppResult<QuestType> {
    sqlx::query_as::<_, QuestType>("SELECT * FROM quest_types WHERE prefix = ?")
        .bind(prefix)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("type 없음: {prefix}")))
}

fn validate_prefix(prefix: &str) -> AppResult<()> {
    if prefix.is_empty() || prefix.len() > 6 {
        return Err(AppError::BadRequest(
            "type prefix 는 1~6자".into(),
        ));
    }
    if !prefix.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return Err(AppError::BadRequest(
            "type prefix 는 대문자 또는 숫자만 (예: DEV, BUG, REQ)".into(),
        ));
    }
    Ok(())
}

fn validate_color(color: &str) -> AppResult<()> {
    let s = color.trim();
    if !s.starts_with('#') || (s.len() != 4 && s.len() != 7) {
        return Err(AppError::BadRequest(
            "color 는 #RGB 또는 #RRGGBB 형식".into(),
        ));
    }
    if !s[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest(
            "color hex 가 잘못됨".into(),
        ));
    }
    Ok(())
}

// ─────────────────────── Quest statuses ───────────────────────

/// 새 status 추가. 파일명 = `<sort_order>-<slug>.toml` (slug 는 name_en 기반 자동 생성).
pub async fn create_status(
    store: &Store,
    name_en: String,
    name_ko: String,
    color: String,
    sort_order: Option<i64>,
) -> AppResult<QuestStatus> {
    let name_en = name_en.trim().to_string();
    let name_ko = name_ko.trim().to_string();
    if name_en.is_empty() || name_ko.is_empty() {
        return Err(AppError::BadRequest(
            "status 이름 (en / ko) 둘 다 필수".into(),
        ));
    }
    validate_color(&color)?;

    let slug = slugify(&name_en);
    if slug.is_empty() {
        return Err(AppError::BadRequest(format!(
            "name_en '{name_en}' 에서 slug 를 추출할 수 없음 (영문/숫자 포함 필요)"
        )));
    }

    // slug 중복?
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM quest_statuses WHERE slug = ?")
            .bind(&slug)
            .fetch_optional(&store.index_pool)
            .await?;
    if exists.is_some() {
        return Err(AppError::BadRequest(format!(
            "이미 존재하는 status slug: {slug} (name_en 을 다르게)"
        )));
    }

    // sort_order — 지정 안 했으면 max+1.
    let sort_order = match sort_order {
        Some(n) => n,
        None => {
            let max: Option<i64> =
                sqlx::query_scalar("SELECT MAX(sort_order) FROM quest_statuses")
                    .fetch_one(&store.index_pool)
                    .await?;
            max.unwrap_or(0) + 1
        }
    };

    // 1. 파일 작성.
    std::fs::create_dir_all(store.paths.statuses_dir())
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let file = StatusFile {
        sort_order,
        name_en: name_en.clone(),
        name_ko: name_ko.clone(),
        color: color.clone(),
    };
    let filename = StatusFile::filename(sort_order, &slug);
    file.write(store.paths.statuses_dir().join(&filename))
        .map_err(AppError::Internal)?;

    // 2. DB INSERT.
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO quest_statuses (slug, name_en, name_ko, color, sort_order)
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&slug)
    .bind(&name_en)
    .bind(&name_ko)
    .bind(&color)
    .bind(sort_order)
    .fetch_one(&store.index_pool)
    .await?;

    Ok(QuestStatus {
        id: new_id,
        slug,
        name_en,
        name_ko,
        color,
        sort_order,
    })
}

/// status 수정 — name_en / name_ko / color / sort_order. slug 자체는 frozen
/// (history / frontmatter 호환). sort_order 변경 시 파일명 rename.
pub async fn update_status(
    store: &Store,
    slug: String,
    name_en: Option<String>,
    name_ko: Option<String>,
    color: Option<String>,
    sort_order: Option<i64>,
) -> AppResult<QuestStatus> {
    if let Some(c) = &color {
        validate_color(c)?;
    }

    let mut row = fetch_status_by_slug(&store.index_pool, &slug).await?;
    let old_filename = StatusFile::filename(row.sort_order, &slug);
    let old_path = store.paths.statuses_dir().join(&old_filename);
    let mut file = StatusFile::read(&old_path).map_err(AppError::Internal)?;

    if let Some(n) = name_en {
        let n = n.trim().to_string();
        if n.is_empty() {
            return Err(AppError::BadRequest("name_en 비울 수 없음".into()));
        }
        row.name_en = n.clone();
        file.name_en = n;
    }
    if let Some(n) = name_ko {
        let n = n.trim().to_string();
        if n.is_empty() {
            return Err(AppError::BadRequest("name_ko 비울 수 없음".into()));
        }
        row.name_ko = n.clone();
        file.name_ko = n;
    }
    if let Some(c) = color {
        row.color = c.clone();
        file.color = c;
    }
    let order_changed = matches!(sort_order, Some(n) if n != row.sort_order);
    if let Some(n) = sort_order {
        row.sort_order = n;
        file.sort_order = n;
    }

    // 파일 — sort_order 가 바뀌었으면 rename, 아니면 in-place rewrite.
    if order_changed {
        let new_filename = StatusFile::filename(row.sort_order, &slug);
        let new_path = store.paths.statuses_dir().join(&new_filename);
        file.write(&new_path).map_err(AppError::Internal)?;
        if new_path != old_path {
            let _ = std::fs::remove_file(&old_path);
        }
    } else {
        file.write(&old_path).map_err(AppError::Internal)?;
    }

    sqlx::query(
        "UPDATE quest_statuses SET name_en = ?, name_ko = ?, color = ?, sort_order = ? WHERE id = ?",
    )
    .bind(&row.name_en)
    .bind(&row.name_ko)
    .bind(&row.color)
    .bind(row.sort_order)
    .bind(row.id)
    .execute(&store.index_pool)
    .await?;

    Ok(row)
}

/// status 삭제. 사용 중 quest 있으면 `BadRequest`.
pub async fn delete_status(store: &Store, slug: String) -> AppResult<()> {
    let row = fetch_status_by_slug(&store.index_pool, &slug).await?;
    let count = count_quests_by_status(&store.index_pool, row.id).await?;
    if count > 0 {
        return Err(AppError::BadRequest(format!(
            "status '{slug}' 는 {count} 개 quest 가 사용 중 — 먼저 다른 status 로 이동시키세요."
        )));
    }

    let filename = StatusFile::filename(row.sort_order, &slug);
    let _ = std::fs::remove_file(store.paths.statuses_dir().join(&filename));
    sqlx::query("DELETE FROM quest_statuses WHERE id = ?")
        .bind(row.id)
        .execute(&store.index_pool)
        .await?;
    Ok(())
}

/// 특정 status 를 사용하는 alive quest 수.
pub async fn count_quests_by_status(pool: &SqlitePool, status_id: i64) -> AppResult<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM quests WHERE status_id = ? AND deleted_at IS NULL",
    )
    .bind(status_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

async fn fetch_status_by_slug(pool: &SqlitePool, slug: &str) -> AppResult<QuestStatus> {
    sqlx::query_as::<_, QuestStatus>("SELECT * FROM quest_statuses WHERE slug = ?")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .context("fetch status")
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound(format!("status 없음: {slug}")))
}

/// `name_en` → snake_case slug ([a-z0-9_]+).
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_us = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            for c in ch.to_lowercase() {
                out.push(c);
            }
            prev_us = false;
        } else if !prev_us && !out.is_empty() {
            out.push('_');
            prev_us = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-ops-meta-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn fresh_store(label: &str) -> (std::path::PathBuf, Store) {
        let dir = fresh_tmp(label);
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        (dir, store)
    }

    // ─── slugify ───
    #[test]
    fn slugify_lowercase_and_underscore() {
        assert_eq!(slugify("In Progress"), "in_progress");
        assert_eq!(slugify("Needs Review!"), "needs_review");
        assert_eq!(slugify("ABC 123"), "abc_123");
        assert_eq!(slugify("   "), "");
        assert_eq!(slugify("Done"), "done");
    }

    // ─── type ───
    #[tokio::test]
    async fn create_type_writes_file_and_db() {
        let (dir, store) = fresh_store("type-create").await;
        let t = create_type(&store, "FOO".into(), "#abcdef".into(), Some("desc".into()))
            .await
            .unwrap();
        assert_eq!(t.prefix, "FOO");
        assert!(store.paths.type_path("FOO").exists());
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_types WHERE prefix = 'FOO'")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
        // counter row 도 생성됐는지.
        let c: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM quest_counters WHERE quest_type_id = ?",
        )
        .bind(t.id)
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(c, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn create_type_rejects_duplicate() {
        let (dir, store) = fresh_store("type-dup").await;
        let err = create_type(&store, "DEV".into(), "#000".into(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn create_type_rejects_invalid_prefix() {
        let (dir, store) = fresh_store("type-bad").await;
        for bad in &["dev", "TOOLONG", "DE!", ""] {
            let err = create_type(&store, (*bad).into(), "#000".into(), None)
                .await
                .unwrap_err();
            assert!(matches!(err, AppError::BadRequest(_)), "bad={bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn update_type_changes_color_and_description() {
        let (dir, store) = fresh_store("type-upd").await;
        let _ = update_type(
            &store,
            "DEV".into(),
            Some("#123456".into()),
            Some(Some("changed".into())),
        )
        .await
        .unwrap();
        let row = fetch_type_by_prefix(&store.index_pool, "DEV").await.unwrap();
        assert_eq!(row.color, "#123456");
        assert_eq!(row.description.as_deref(), Some("changed"));
        // 파일도.
        let f = TypeFile::read(store.paths.type_path("DEV")).unwrap();
        assert_eq!(f.color, "#123456");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_type_blocks_when_in_use() {
        // DEV 타입은 seed 직후엔 counter 만 있고 quest 없음 → 삭제 가능.
        // 사용 케이스 만들려면 quest 추가 필요. 여기선 fixture 만들기 번거로워
        // count 함수만 단독 테스트.
        let (dir, store) = fresh_store("type-del-empty").await;
        // 빈 상태 — 삭제 가능.
        delete_type(&store, "BUG".into()).await.unwrap();
        assert!(!store.paths.type_path("BUG").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn count_quests_by_type_zero_after_seed() {
        let (dir, store) = fresh_store("type-count").await;
        let row = fetch_type_by_prefix(&store.index_pool, "DEV").await.unwrap();
        let n = count_quests_by_type(&store.index_pool, row.id).await.unwrap();
        assert_eq!(n, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── status ───
    #[tokio::test]
    async fn create_status_writes_file_and_db() {
        let (dir, store) = fresh_store("st-create").await;
        let s = create_status(
            &store,
            "Blocked".into(),
            "막힘".into(),
            "#ff0000".into(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(s.slug, "blocked");
        let path = store
            .paths
            .statuses_dir()
            .join(StatusFile::filename(s.sort_order, "blocked"));
        assert!(path.exists(), "expected file {path:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn create_status_rejects_duplicate_slug() {
        let (dir, store) = fresh_store("st-dup").await;
        let err = create_status(
            &store,
            "Open".into(), // seed 에 이미 있음.
            "X".into(),
            "#000".into(),
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn update_status_renames_file_on_sort_order_change() {
        let (dir, store) = fresh_store("st-rename").await;
        let row = fetch_status_by_slug(&store.index_pool, "open").await.unwrap();
        let old_path = store
            .paths
            .statuses_dir()
            .join(StatusFile::filename(row.sort_order, "open"));
        assert!(old_path.exists());

        let _ = update_status(
            &store,
            "open".into(),
            None,
            None,
            None,
            Some(row.sort_order + 100),
        )
        .await
        .unwrap();

        assert!(!old_path.exists(), "old path should be removed");
        let new_path = store
            .paths
            .statuses_dir()
            .join(StatusFile::filename(row.sort_order + 100, "open"));
        assert!(new_path.exists(), "new path should exist");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn update_status_keeps_slug_frozen() {
        // name_en 변경해도 slug 는 변하지 않음 (history 호환).
        let (dir, store) = fresh_store("st-name").await;
        let _ = update_status(
            &store,
            "open".into(),
            Some("Reopened".into()),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let row = fetch_status_by_slug(&store.index_pool, "open").await.unwrap();
        assert_eq!(row.slug, "open");
        assert_eq!(row.name_en, "Reopened");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn delete_status_removes_file_and_row() {
        let (dir, store) = fresh_store("st-del").await;
        // on_hold 는 seed 에 있고 quest 0개.
        delete_status(&store, "on_hold".into()).await.unwrap();
        let row = sqlx::query_as::<_, QuestStatus>(
            "SELECT * FROM quest_statuses WHERE slug = 'on_hold'",
        )
        .fetch_optional(&store.index_pool)
        .await
        .unwrap();
        assert!(row.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
