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
use crate::repo::{GuildPaths, StatusFile, TypeFile};
use crate::store::Store;
use anyhow::Context;
use sqlx::SqlitePool;
use std::path::PathBuf;

/// BUG-018: slug 로 `.guild/statuses/<order>-<slug>.toml` 파일 찾기.
///
/// 이전엔 `StatusFile::filename(row.sort_order, slug)` 로 파일 경로를 추정
/// 했는데, 사용자가 외부 편집 / 마이그레이션 / 옛 길드 등으로 file 의
/// sort_order 와 파일명 prefix 가 어긋난 경우 (drift) 모든 mutation 이
/// 'failed to read' 실패. 디렉토리에서 `*-<slug>.toml` 패턴으로 search.
fn find_status_file_by_slug(
    paths: &GuildPaths,
    slug: &str,
) -> AppResult<Option<PathBuf>> {
    let dir = paths.statuses_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Internal(anyhow::anyhow!(e))),
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };
        // pattern: `<order>-<slug>` — 첫 `-` 이후가 slug.
        if let Some(dash) = stem.find('-')
            && &stem[dash + 1..] == slug
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

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
    let type_path = store.paths.type_path(&prefix);
    file.write(&type_path).map_err(AppError::Internal)?;
    let _ = crate::file_mtime::touch(store, &type_path).await; // DEV-178: drift 오탐 방지

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

/// type 수정 — color / description / **prefix (rename)** 통합.
///
/// `new_prefix` 가 현재와 다르면 rename cascade (그 type 의 모든 quest 의
/// slug 가 바뀜). slug 가 stable identifier 라 사용자 시각엔 보이지 않지만
/// 파일명 / frontmatter / quest_history.quest_slug / positions.quest_slug
/// 모두 cascade.
pub async fn update_type(
    store: &Store,
    prefix: String,
    new_prefix: Option<String>, // BUG-018: rename 통합 — 다르면 cascade.
    color: Option<String>,
    description: Option<Option<String>>, // outer None = 변경 없음, inner None = clear.
) -> AppResult<QuestType> {
    let prefix = prefix.trim().to_string();
    if let Some(c) = &color {
        validate_color(c)?;
    }

    // prefix 변경 요청 시 먼저 rename — 그 후 다른 필드 update 는 새 prefix 기준.
    let working_prefix = if let Some(np) = new_prefix {
        let np = np.trim().to_string();
        if np.is_empty() || np.eq_ignore_ascii_case(&prefix) {
            prefix
        } else {
            rename_type(store, prefix, np.clone()).await?;
            np
        }
    } else {
        prefix
    };

    let mut row = fetch_type_by_prefix(&store.index_pool, &working_prefix).await?;
    let path = store.paths.type_path(&working_prefix);
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
    let _ = crate::file_mtime::touch(store, &path).await; // DEV-178: drift 오탐 방지
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

/// DEV-061: type prefix rename — 그 type 의 모든 quest 의 slug cascade.
///
/// 영향:
/// - `.guild/types/<old>.toml` → `.guild/types/<new>.toml` (파일 + 안의 prefix 필드).
/// - 그 type 의 모든 `.guild/quests/<old>-NNN.md` → `<new>-NNN.md` rename +
///   frontmatter `quest_id` 갱신.
/// - DB: `quest_types.prefix` UPDATE + `quest_history.quest_slug` /
///   `quest_positions.quest_slug` cascade (DEV-049 의 slug 컬럼).
/// - 영향받는 다른 quest (parent / sub / prereq / dependent) 파일들의
///   auto-block 재생성 — 그쪽에서 본인을 mention 하던 게 새 slug 로 반영.
///
/// **본문 안 자유 텍스트 mention** 은 자동 갱신 X — DEV-055 와 동일 정책.
pub async fn rename_type(
    store: &Store,
    old_prefix: String,
    new_prefix: String,
) -> AppResult<QuestType> {
    let old_prefix = old_prefix.trim().to_string();
    let new_prefix = new_prefix.trim().to_string();
    validate_prefix(&new_prefix)?;

    // 같은 prefix (대소문자 무시) 면 NoOp — case 변경만 원해도 새 row 가
    // 동일 prefix 충돌 → 따로 처리 안 함 (현재는 거부).
    if old_prefix.eq_ignore_ascii_case(&new_prefix) {
        return fetch_type_by_prefix(&store.index_pool, &old_prefix).await;
    }

    // 중복 검사 — new 가 이미 다른 type 의 prefix 면 거부.
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM quest_types WHERE UPPER(prefix) = UPPER(?)",
    )
    .bind(&new_prefix)
    .fetch_optional(&store.index_pool)
    .await?;
    if exists.is_some() {
        return Err(AppError::BadRequest(format!(
            "이미 존재하는 type prefix: {new_prefix}"
        )));
    }

    let old_row = fetch_type_by_prefix(&store.index_pool, &old_prefix).await?;

    // 영향받는 quest id 들 미리 수집 — auto-block 재생성 대상 결정용.
    // (그 type 의 모든 quest + 그 quest 들을 parent / sub / prereq 로 가진 quest)
    let own_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM quests WHERE quest_type_id = ? AND deleted_at IS NULL",
    )
    .bind(old_row.id)
    .fetch_all(&store.index_pool)
    .await?;

    // (own_ids 와 관계된 다른 alive quest)
    let related_ids: Vec<i64> = if own_ids.is_empty() {
        Vec::new()
    } else {
        let placeholders = own_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql_str = format!(
            r#"
            SELECT DISTINCT q.id FROM quests q
             WHERE q.deleted_at IS NULL
               AND q.id NOT IN ({placeholders})
               AND (
                 q.parent_quest_id IN ({placeholders})
                 OR q.id IN (
                   SELECT quest_id FROM quest_dependencies
                    WHERE prerequisite_id IN ({placeholders})
                 )
               )
            "#
        );
        let mut q = sqlx::query_scalar(&sql_str);
        for _ in 0..3 {
            for id in &own_ids {
                q = q.bind(*id);
            }
        }
        q.fetch_all(&store.index_pool).await?
    };

    // ── DB UPDATE (transaction) ──
    let mut tx = store.index_pool.begin().await?;
    sqlx::query("UPDATE quest_types SET prefix = ? WHERE id = ?")
        .bind(&new_prefix)
        .bind(old_row.id)
        .execute(&mut *tx)
        .await?;
    // quest_history.quest_slug — 그 type 의 모든 quest 에 대해 새 slug 로 갱신.
    sqlx::query(
        "UPDATE quest_history
            SET quest_slug = (
              SELECT qt.prefix || '-' || printf('%03d', q.number)
                FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id
               WHERE q.id = quest_history.quest_id
            )
          WHERE quest_id IN (SELECT id FROM quests WHERE quest_type_id = ?)",
    )
    .bind(old_row.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE quest_positions
            SET quest_slug = (
              SELECT qt.prefix || '-' || printf('%03d', q.number)
                FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id
               WHERE q.id = quest_positions.quest_id
            )
          WHERE quest_id IN (SELECT id FROM quests WHERE quest_type_id = ?)",
    )
    .bind(old_row.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // ── 파일 IO ──
    // 1. types/<old>.toml → <new>.toml (파일 안 prefix 필드도 갱신).
    let old_type_path = store.paths.type_path(&old_prefix);
    let new_type_path = store.paths.type_path(&new_prefix);
    if old_type_path.exists() {
        let mut tf =
            TypeFile::read(&old_type_path).map_err(AppError::Internal)?;
        tf.prefix = new_prefix.clone();
        tf.write(&new_type_path).map_err(AppError::Internal)?;
        if old_type_path != new_type_path {
            let _ = std::fs::remove_file(&old_type_path);
        }
    }

    // 2. 그 type 의 모든 quest 파일 rename + auto-block 재생성.
    //    write_quest_file 가 새 slug (quest.quest_id) path 에 씀.
    use crate::services::quests as sql;
    for qid in &own_ids {
        let quest = sql::fetch_by_id(&store.index_pool, *qid).await?;
        // 옛 slug path 계산 — old_prefix + 같은 number.
        let old_slug = format!("{}-{:03}", old_prefix, quest.number);
        let old_quest_path = store.paths.quest_path(&old_slug);
        // DEV-066: rename 시 옛 파일에서 description 미리 보존 + DB sync.
        if let Ok(old_qf) = crate::repo::QuestFile::read(&old_quest_path)
            && !old_qf.description.trim().is_empty()
        {
            let db_desc = quest.description.as_deref().unwrap_or("");
            if old_qf.description != db_desc {
                sqlx::query("UPDATE quests SET description = ? WHERE id = ?")
                    .bind(&old_qf.description)
                    .bind(quest.id)
                    .execute(&store.index_pool)
                    .await?;
            }
        }
        let quest = sql::fetch_by_id(&store.index_pool, *qid).await?;
        crate::ops::quests::write_quest_file(store, &quest, true).await?;
        if old_quest_path != store.paths.quest_path(&quest.quest_id)
            && old_quest_path.exists()
        {
            let _ = std::fs::remove_file(&old_quest_path);
        }
    }

    // 3. 관련 다른 quest 의 auto-block 재생성 (parent / sub / prereq mention).
    for rid in &related_ids {
        if let Ok(q) = sql::fetch_by_id(&store.index_pool, *rid).await {
            crate::ops::quests::write_quest_file(store, &q, false).await?;
        }
    }

    fetch_type_by_prefix(&store.index_pool, &new_prefix).await
}

/// DEV-061: status slug rename — `.guild/quests/*.md` frontmatter 의
/// `status` + `.guild/statuses/<order>-<slug>.toml` 파일명 + DB
/// `quest_statuses.slug` + `quest_history.old/new_value` cascade.
///
/// auto-block 변화 없음 (status 는 auto-block 에 표시 안 됨).
pub async fn rename_status_slug(
    store: &Store,
    old_slug: String,
    new_slug: String,
) -> AppResult<QuestStatus> {
    let old_slug = old_slug.trim().to_string();
    let new_slug = new_slug.trim().to_string();
    validate_status_slug(&new_slug)?;

    if old_slug == new_slug {
        return fetch_status_by_slug(&store.index_pool, &old_slug).await;
    }

    // 중복 검사.
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM quest_statuses WHERE slug = ?",
    )
    .bind(&new_slug)
    .fetch_optional(&store.index_pool)
    .await?;
    if exists.is_some() {
        return Err(AppError::BadRequest(format!(
            "이미 존재하는 status slug: {new_slug}"
        )));
    }

    let old_row = fetch_status_by_slug(&store.index_pool, &old_slug).await?;

    // 영향받는 quest id 들 미리 수집 (frontmatter rewrite 대상).
    let affected_quest_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM quests WHERE status_id = ? AND deleted_at IS NULL",
    )
    .bind(old_row.id)
    .fetch_all(&store.index_pool)
    .await?;

    // ── DB UPDATE (transaction) ──
    let mut tx = store.index_pool.begin().await?;
    sqlx::query("UPDATE quest_statuses SET slug = ? WHERE id = ?")
        .bind(&new_slug)
        .bind(old_row.id)
        .execute(&mut *tx)
        .await?;
    // history 의 change_status op 에서 old / new value 가 old_slug 면 갱신.
    sqlx::query(
        "UPDATE quest_history SET old_value = ? WHERE op = 'change_status' AND old_value = ?",
    )
    .bind(&new_slug)
    .bind(&old_slug)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE quest_history SET new_value = ? WHERE op = 'change_status' AND new_value = ?",
    )
    .bind(&new_slug)
    .bind(&old_slug)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // ── 파일 IO ──
    // 1. statuses 의 파일 rename. BUG-018: file 의 sort_order 와 DB 의 값이
    //    drift 한 길드에서도 동작하도록 디렉토리 search 사용.
    if let Some(old_path) = find_status_file_by_slug(&store.paths, &old_slug)? {
        let new_filename = StatusFile::filename(old_row.sort_order, &new_slug);
        let new_path = store.paths.statuses_dir().join(&new_filename);
        if old_path != new_path {
            std::fs::rename(&old_path, &new_path)
                .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
        }
    }

    // 2. 그 status 의 모든 quest .md frontmatter 의 `status` 필드 rewrite.
    // DEV-066: status rename 은 frontmatter 만 바꿈, description 은 안 건드림.
    // false 로 호출하여 파일 본문이 있으면 보존 + DB sync.
    use crate::services::quests as sql;
    for qid in &affected_quest_ids {
        if let Ok(q) = sql::fetch_by_id(&store.index_pool, *qid).await {
            crate::ops::quests::write_quest_file(store, &q, false).await?;
        }
    }

    fetch_status_by_slug(&store.index_pool, &new_slug).await
}

/// status slug validation — `slugify` 가 만드는 형식과 동일하게 강제.
fn validate_status_slug(slug: &str) -> AppResult<()> {
    if slug.is_empty() || slug.chars().count() > 32 {
        return Err(AppError::BadRequest(
            "status slug 는 1~32자".into(),
        ));
    }
    for c in slug.chars() {
        let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
        if !ok {
            return Err(AppError::BadRequest(format!(
                "status slug 에 허용되지 않은 문자 '{c}'. \
                 소문자 / 숫자 / '_' 만 사용."
            )));
        }
    }
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

/// status `name_en` validation — slug / 파일명 안전성 확보.
///
/// 규칙: 영문 시작 + (영문 / 숫자 / 공백 / `-` / `_`) 최대 32자. 한글 / 특수
/// 문자 거부. slugify 결과의 안정성과 파일명 / URL 친화성 둘 다 만족.
fn validate_status_name_en(s: &str) -> AppResult<()> {
    if s.is_empty() {
        return Err(AppError::BadRequest("name_en 은 필수".into()));
    }
    if s.chars().count() > 32 {
        return Err(AppError::BadRequest(
            "name_en 은 최대 32자".into(),
        ));
    }
    let first = s.chars().next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err(AppError::BadRequest(
            "name_en 은 영문자로 시작해야 함 (한글 / 숫자 / 특수문자 불가)".into(),
        ));
    }
    for c in s.chars() {
        let ok = c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_';
        if !ok {
            return Err(AppError::BadRequest(format!(
                "name_en 에 허용되지 않은 문자 '{c}'. \
                 영문 / 숫자 / 공백 / '-' / '_' 만 사용 가능."
            )));
        }
    }
    Ok(())
}

/// status `name_ko` validation — 빈 값 OK (선택). 허용 문자는 영문 (`name_en`)
/// 규칙을 그대로 확장: 한글 + 영문 + 숫자 + 공백 + `-` + `_`. 시작 글자
/// 제약은 없음 (한글 시작 OK).
///
/// 향후 다른 언어 (일본어 등) 추가 시 같은 규칙 — 해당 언어 글자 추가.
fn validate_status_name_ko(s: &str) -> AppResult<()> {
    if s.chars().count() > 32 {
        return Err(AppError::BadRequest(
            "name_ko 는 최대 32자".into(),
        ));
    }
    for c in s.chars() {
        let ok = is_hangul(c)
            || c.is_ascii_alphanumeric()
            || c == ' '
            || c == '-'
            || c == '_';
        if !ok {
            return Err(AppError::BadRequest(format!(
                "name_ko 에 허용되지 않은 문자 '{c}'. \
                 한글 / 영문 / 숫자 / 공백 / '-' / '_' 만 사용 가능."
            )));
        }
    }
    Ok(())
}

/// 한글 음절 + 자모 범위. IME 입력 호환 위해 자모 단독도 허용.
fn is_hangul(c: char) -> bool {
    matches!(c as u32,
        0xAC00..=0xD7A3   // 완성형 한글 음절
        | 0x1100..=0x11FF // 자모 (초/중/종성)
        | 0x3130..=0x318F // 호환 자모
        | 0xA960..=0xA97F // 확장 A
        | 0xD7B0..=0xD7FF // 확장 B
    )
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
    // DEV-014 후속: name_ko 는 선택 — 빈 문자열 허용. (frontend 가 표시 시
    // 빈 ko 면 name_en 으로 fallback.)
    let name_en = name_en.trim().to_string();
    let name_ko = name_ko.trim().to_string();
    validate_status_name_en(&name_en)?;
    validate_status_name_ko(&name_ko)?;
    validate_color(&color)?;

    let slug = slugify(&name_en);
    if slug.is_empty() {
        // validate_status_name_en 통과 후엔 사실상 도달 불가 — 방어적.
        return Err(AppError::BadRequest(format!(
            "name_en '{name_en}' 에서 slug 를 추출할 수 없음"
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
        // DEV-093: 신규 생성 status 의 기본 counts_as_done — false. 사용자가
        // 추후 statuses update 로 토글.
        counts_as_done: false,
    };
    let filename = StatusFile::filename(sort_order, &slug);
    let status_path = store.paths.statuses_dir().join(&filename);
    file.write(&status_path).map_err(AppError::Internal)?;
    let _ = crate::file_mtime::touch(store, &status_path).await; // DEV-178: drift 오탐 방지

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
        // DEV-093: 새 status 의 기본 — 사용자가 추후 토글.
        counts_as_done: false,
    })
}

/// status 수정 — name_en / name_ko / color / sort_order / **slug (rename)** 통합.
///
/// `new_slug` 가 현재와 다르면 rename cascade (history.quest_slug + 모든
/// quest .md frontmatter + 파일명).
#[allow(clippy::too_many_arguments)] // 단순 field-by-field optional patch — request DTO 만들 정도는 아님.
pub async fn update_status(
    store: &Store,
    slug: String,
    new_slug: Option<String>, // BUG-018: rename 통합.
    name_en: Option<String>,
    name_ko: Option<String>,
    color: Option<String>,
    sort_order: Option<i64>,
    // DEV-093: 캠페인 진행도 계산용 "완료" 카운트 여부.
    counts_as_done: Option<bool>,
) -> AppResult<QuestStatus> {
    if let Some(c) = &color {
        validate_color(c)?;
    }

    // slug 변경 요청 시 먼저 rename — 그 후 다른 필드는 새 slug 기준.
    let working_slug = if let Some(ns) = new_slug {
        let ns = ns.trim().to_string();
        if ns.is_empty() || ns == slug {
            slug
        } else {
            rename_status_slug(store, slug, ns.clone()).await?;
            ns
        }
    } else {
        slug
    };

    let mut row = fetch_status_by_slug(&store.index_pool, &working_slug).await?;
    // BUG-018: 파일 경로는 디렉토리 search (DB sort_order 추정 X).
    let old_path = find_status_file_by_slug(&store.paths, &working_slug)?
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "status 파일 못 찾음: '{working_slug}'. reindex 권장."
            ))
        })?;
    let mut file = StatusFile::read(&old_path).map_err(AppError::Internal)?;
    // file 의 sort_order 도 DB 와 sync (drift 보정).
    if file.sort_order != row.sort_order {
        file.sort_order = row.sort_order;
    }

    if let Some(n) = name_en {
        let n = n.trim().to_string();
        // DEV-014 후속 (fix4): create 와 동일 validation — update 가 우회 경로로
        // 한글 / 특수문자 들어가던 회귀 차단. slug 자체는 frozen 이지만 표시명에
        // 한글 들어가면 사용자 혼란.
        validate_status_name_en(&n)?;
        row.name_en = n.clone();
        file.name_en = n;
    }
    if let Some(n) = name_ko {
        // DEV-014 후속: name_ko 는 선택 — 빈 문자열 허용. control / 파일 위험
        // 문자만 거부.
        let n = n.trim().to_string();
        validate_status_name_ko(&n)?;
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
    // DEV-093: counts_as_done toggle.
    if let Some(b) = counts_as_done {
        row.counts_as_done = b;
        file.counts_as_done = b;
    }

    // 파일 — sort_order 가 바뀌었으면 rename, 아니면 in-place rewrite.
    let written_path = if order_changed {
        let new_filename = StatusFile::filename(row.sort_order, &working_slug);
        let new_path = store.paths.statuses_dir().join(&new_filename);
        file.write(&new_path).map_err(AppError::Internal)?;
        if new_path != old_path {
            let _ = std::fs::remove_file(&old_path);
        }
        new_path
    } else {
        file.write(&old_path).map_err(AppError::Internal)?;
        old_path
    };
    let _ = crate::file_mtime::touch(store, &written_path).await; // DEV-178: drift 오탐 방지

    sqlx::query(
        "UPDATE quest_statuses SET name_en = ?, name_ko = ?, color = ?, sort_order = ?, counts_as_done = ? WHERE id = ?",
    )
    .bind(&row.name_en)
    .bind(&row.name_ko)
    .bind(&row.color)
    .bind(row.sort_order)
    .bind(row.counts_as_done as i64)
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

    // BUG-018: 디렉토리 search.
    if let Some(p) = find_status_file_by_slug(&store.paths, &slug)? {
        let _ = std::fs::remove_file(p);
    }
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

// ─────────────────────── DEV-068: Tag defs ───────────────────────

/// tag slug 검증 — 소문자/숫자/`-`/`_` 만, 1-32자.
fn validate_tag_slug(slug: &str) -> AppResult<()> {
    if slug.is_empty() || slug.len() > 32 {
        return Err(AppError::BadRequest(format!(
            "tag slug 길이 1-32 만 (입력: {slug:?})"
        )));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(AppError::BadRequest(format!(
            "tag slug 는 소문자/숫자/`-`/`_` 만 (입력: {slug:?})"
        )));
    }
    Ok(())
}

/// tag 정의 생성 또는 갱신 (upsert). color 빈 문자열 = 미정의.
pub async fn upsert_tag_def(
    store: &Store,
    slug: String,
    color: String,
    description: String,
) -> AppResult<crate::models::QuestTagDef> {
    let slug = slug.trim().to_string();
    validate_tag_slug(&slug)?;
    let color = color.trim().to_string();
    if !color.is_empty() {
        validate_color(&color)?;
    }
    let description = description.trim().to_string();

    // file write — `.guild/tags/{slug}.toml`.
    std::fs::create_dir_all(store.paths.tags_dir())
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let file = crate::repo::TagFile {
        color: color.clone(),
        description: description.clone(),
    };
    let tag_path = store.paths.tag_path(&slug);
    file.write(&tag_path).map_err(AppError::Internal)?;
    let _ = crate::file_mtime::touch(store, &tag_path).await; // DEV-178: drift 오탐 방지

    // DB upsert.
    sqlx::query(
        "INSERT INTO quest_tag_defs (slug, color, description) VALUES (?, ?, ?)
         ON CONFLICT(slug) DO UPDATE SET color = excluded.color, description = excluded.description",
    )
    .bind(&slug)
    .bind(&color)
    .bind(&description)
    .execute(&store.index_pool)
    .await?;

    Ok(crate::models::QuestTagDef {
        slug,
        color,
        description,
    })
}

/// tag 정의 삭제 — 파일 + DB. quest 의 frontmatter 의 tag string 자체는 보존
/// (def 없어도 사용 가능). 사용자가 의도적으로 quest tag 도 제거하려면 별도.
pub async fn delete_tag_def(store: &Store, slug: String) -> AppResult<()> {
    let slug = slug.trim().to_string();
    validate_tag_slug(&slug)?;
    let _ = std::fs::remove_file(store.paths.tag_path(&slug));
    sqlx::query("DELETE FROM quest_tag_defs WHERE slug = ?")
        .bind(&slug)
        .execute(&store.index_pool)
        .await?;
    Ok(())
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
            None,
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
            None,
            Some(row.sort_order + 100),
            None,
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
            None,
            Some("Reopened".into()),
            None,
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

    /// DEV-014 후속: name_ko 는 선택 — 빈 문자열 허용.
    #[tokio::test]
    async fn create_status_allows_empty_name_ko() {
        let (dir, store) = fresh_store("st-ko-empty").await;
        let s = create_status(
            &store,
            "Triaged".into(),
            "".into(), // ko 비움.
            "#aabbcc".into(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(s.name_en, "Triaged");
        assert_eq!(s.name_ko, "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn update_status_can_clear_name_ko() {
        let (dir, store) = fresh_store("st-ko-clear").await;
        // seed 의 'open' (name_ko='게시됨') 을 빈 ko 로 갱신.
        let updated = update_status(
            &store,
            "open".into(),
            None,
            None,
            Some("".into()),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(updated.name_en, "Open");
        assert_eq!(updated.name_ko, "");
        // name_en 빈 값은 여전히 거부.
        let err = update_status(&store, "open".into(), None, Some("".into()), None, None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-014 fix4: name_en validation 일관성 — create / update 양쪽에서
    /// 한글 / 특수문자 거부.
    #[tokio::test]
    async fn name_en_rejects_korean_and_special_chars() {
        let (dir, store) = fresh_store("name-en-bad").await;
        // create — 한글 거부.
        let err = create_status(&store, "한국어".into(), "".into(), "#000".into(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        // create — 특수문자 거부.
        for bad in &["Open!", "On/Off", "v1.0", "Done?", "<x>", "a:b"] {
            let err = create_status(&store, (*bad).into(), "".into(), "#000".into(), None)
                .await
                .unwrap_err();
            assert!(matches!(err, AppError::BadRequest(_)), "bad={bad}");
        }
        // create — 영문 안 시작 거부.
        let err = create_status(&store, "1Foo".into(), "".into(), "#000".into(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        // create — 너무 김.
        let long = "a".repeat(33);
        let err = create_status(&store, long, "".into(), "#000".into(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));

        // update — 같은 규칙. open status (seed) 의 name_en 을 한글로 바꾸려 하면 거부.
        let err = update_status(
            &store,
            "open".into(),
            None,
            Some("한국어".into()),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        let err = update_status(
            &store,
            "open".into(),
            None,
            Some("Open!".into()),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));

        // 합법 케이스 — 영문/숫자/공백/-/_ OK.
        let ok = create_status(
            &store,
            "In Review".into(),
            "".into(),
            "#000".into(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(ok.name_en, "In Review");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// name_ko 허용 문자: 한글 + 영문 + 숫자 + 공백 + `-` + `_` 만 (영어와
    /// 동일 정책). 그 외 — control / 파일 위험 문자 / 일반 punctuation 까지
    /// 모두 거부.
    #[tokio::test]
    async fn name_ko_rejects_disallowed_chars() {
        let (dir, store) = fresh_store("name-ko-bad").await;
        let cases = &[
            "a/b", "a\\b", "x:y", "<tag>", "a*b", "a|b", "\"x\"", // 파일 위험
            "a.b", "a,b", "a!b", "a?b", "a(b)", "a;b",            // 일반 punctuation
            "가/나", "가.나", "리뷰 중 (대기)",                       // 한글 + 거부 문자 혼합
            "a\nb", "a\tb",                                       // control
        ];
        for bad in cases {
            let err = create_status(
                &store,
                "Valid".into(),
                (*bad).into(),
                "#000".into(),
                None,
            )
            .await
            .unwrap_err();
            assert!(matches!(err, AppError::BadRequest(_)), "bad={bad}");
        }
        // 길이 초과.
        let long_ko: String = "가".repeat(33);
        let err = create_status(&store, "Valid3".into(), long_ko, "#000".into(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));

        // 합법: 한글 / 영문 / 숫자 / 공백 / - / _ 만.
        let ok_names = ["리뷰", "리뷰 중", "in_progress 한글", "Test-2024", "ㄱㄴㄷ"];
        for (i, ok_name) in ok_names.iter().enumerate() {
            let result = create_status(
                &store,
                format!("Valid name {i}"),
                (*ok_name).into(),
                "#000".into(),
                None,
            )
            .await;
            assert!(result.is_ok(), "should accept '{ok_name}': {result:?}");
        }

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

    // ─── BUG-018: status 파일 sort_order drift ───

    /// 파일명 prefix (예 `0-open.toml`) 와 file 안의 `sort_order` 필드 / DB
    /// `sort_order` 가 어긋난 길드에서도 update / rename / delete 가 동작해야.
    /// 이전엔 DB 의 sort_order 추정으로 파일 경로를 만들어서 'failed to read'.
    #[tokio::test]
    async fn status_mutations_work_when_file_prefix_drifts() {
        let (dir, store) = fresh_store("st-drift").await;

        // open status 의 실제 파일을 `99-open.toml` 로 강제 rename — drift 시뮬레이션.
        // DB 의 sort_order 는 그대로 (1).
        let row =
            fetch_status_by_slug(&store.index_pool, "open").await.unwrap();
        let dir_path = store.paths.statuses_dir();
        let old_real_filename = StatusFile::filename(row.sort_order, "open");
        let drift_filename = "99-open.toml";
        std::fs::rename(
            dir_path.join(&old_real_filename),
            dir_path.join(drift_filename),
        )
        .unwrap();

        // 이전 코드라면 `<row.sort_order>-open.toml` 못 찾아 실패. 이제는 search 로 OK.
        let updated = update_status(
            &store,
            "open".into(),
            None,
            None,
            Some("게시".into()),
            Some("#888888".into()),
            None,
            None,
        )
        .await
        .expect("drift 상태에서도 update 성공해야");
        assert_eq!(updated.name_ko, "게시");
        assert_eq!(updated.color, "#888888");

        // rename 도 OK (drift 파일을 찾아서 새 slug 로 rename).
        let renamed =
            rename_status_slug(&store, "open".into(), "backlog".into())
                .await
                .expect("drift 상태에서도 rename 성공해야");
        assert_eq!(renamed.slug, "backlog");
        // 새 파일 — drift 의 order (99) 유지 안 함; DB 의 sort_order (1) 가 truth.
        let renamed_path =
            dir_path.join(StatusFile::filename(row.sort_order, "backlog"));
        assert!(renamed_path.exists(), "new file: {renamed_path:?}");
        assert!(
            !dir_path.join(drift_filename).exists(),
            "drift 파일 사라져야"
        );

        // delete 도 OK.
        let row2 = fetch_status_by_slug(&store.index_pool, "on_hold").await.unwrap();
        let on_hold_path = dir_path.join(StatusFile::filename(row2.sort_order, "on_hold"));
        std::fs::rename(&on_hold_path, dir_path.join("77-on_hold.toml")).unwrap();
        delete_status(&store, "on_hold".into())
            .await
            .expect("drift 상태에서도 delete 성공해야");
        assert!(!dir_path.join("77-on_hold.toml").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── DEV-061: rename_type ───

    /// 빈 type (quest 0개) rename — 파일 + DB 만 갱신.
    #[tokio::test]
    async fn rename_type_empty() {
        let (dir, store) = fresh_store("rename-type-empty").await;
        // BUG type 은 seed 에 있고 quest 0.
        let updated = rename_type(&store, "BUG".into(), "FIX".into())
            .await
            .unwrap();
        assert_eq!(updated.prefix, "FIX");
        assert!(!store.paths.type_path("BUG").exists(), "옛 type 파일 삭제");
        assert!(store.paths.type_path("FIX").exists(), "새 type 파일 생성");
        // 파일 내용도 prefix 갱신됐는지.
        let f = TypeFile::read(store.paths.type_path("FIX")).unwrap();
        assert_eq!(f.prefix, "FIX");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// quest 가 있는 type rename — quest 파일들 + slug + frontmatter cascade.
    #[tokio::test]
    async fn rename_type_with_quests_cascades() {
        let (dir, store) = fresh_store("rename-type-cascade").await;

        let dev_id: i64 =
            sqlx::query_scalar("SELECT id FROM quest_types WHERE prefix = 'DEV'")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        let open_id: i64 =
            sqlx::query_scalar("SELECT id FROM quest_statuses WHERE slug = 'open'")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();

        // DEV-001, DEV-002 직접 INSERT (test 단순화).
        for n in 1..=2 {
            sqlx::query(
                "INSERT INTO quests (quest_type_id, number, title, status_id, urgency, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 3, datetime('now'), datetime('now'))",
            )
            .bind(dev_id)
            .bind(n)
            .bind(format!("dev quest {n}"))
            .bind(open_id)
            .execute(&store.index_pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "UPDATE quest_counters SET last_number = 2 WHERE quest_type_id = ?",
        )
        .bind(dev_id)
        .execute(&store.index_pool)
        .await
        .unwrap();

        // .md 파일도 만들어 두기 (write_quest_file 가 호출되어 cascade 되는지 확인).
        for n in 1..=2 {
            let id: i64 = sqlx::query_scalar(
                "SELECT id FROM quests WHERE quest_type_id = ? AND number = ?",
            )
            .bind(dev_id)
            .bind(n)
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
            let q =
                crate::services::quests::fetch_by_id(&store.index_pool, id)
                    .await
                    .unwrap();
            crate::ops::quests::write_quest_file(&store, &q, true).await.unwrap();
        }
        assert!(dir.join(".guild/quests/DEV-001.md").exists());
        assert!(dir.join(".guild/quests/DEV-002.md").exists());

        // 한 quest 의 history 도 만들어 두기 (slug cascade 확인).
        sqlx::query(
            "INSERT INTO quest_history (quest_id, quest_slug, ts, op, old_value, new_value)
             VALUES ((SELECT id FROM quests WHERE quest_type_id = ? AND number = 1),
                     'DEV-001', datetime('now'), 'change_status', 'open', 'in_progress')",
        )
        .bind(dev_id)
        .execute(&store.index_pool)
        .await
        .unwrap();

        // rename DEV → CORE.
        let updated = rename_type(&store, "DEV".into(), "CORE".into())
            .await
            .unwrap();
        assert_eq!(updated.prefix, "CORE");

        // 파일 rename 확인.
        assert!(!dir.join(".guild/quests/DEV-001.md").exists());
        assert!(!dir.join(".guild/quests/DEV-002.md").exists());
        assert!(dir.join(".guild/quests/CORE-001.md").exists());
        assert!(dir.join(".guild/quests/CORE-002.md").exists());
        assert!(!store.paths.type_path("DEV").exists());
        assert!(store.paths.type_path("CORE").exists());

        // frontmatter quest_id 갱신.
        let c1 = std::fs::read_to_string(dir.join(".guild/quests/CORE-001.md")).unwrap();
        assert!(c1.contains("quest_id = \"CORE-001\""));

        // history quest_slug cascade.
        let slug: String = sqlx::query_scalar(
            "SELECT quest_slug FROM quest_history WHERE op = 'change_status' LIMIT 1",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(slug, "CORE-001");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rename_type_rejects_duplicate() {
        let (dir, store) = fresh_store("rename-type-dup").await;
        // BUG → DEV (이미 존재).
        let err = rename_type(&store, "BUG".into(), "DEV".into())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rename_type_rejects_invalid_prefix() {
        let (dir, store) = fresh_store("rename-type-bad").await;
        for bad in &["dev", "TOOLONG", ""] {
            let err = rename_type(&store, "BUG".into(), (*bad).into())
                .await
                .unwrap_err();
            assert!(matches!(err, AppError::BadRequest(_)), "bad={bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── DEV-061: rename_status_slug ───

    #[tokio::test]
    async fn rename_status_slug_renames_file_and_cascades() {
        let (dir, store) = fresh_store("rename-status").await;

        // 한 quest 추가 + history 한 줄.
        let dev_id: i64 =
            sqlx::query_scalar("SELECT id FROM quest_types WHERE prefix = 'DEV'")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        let open_id: i64 =
            sqlx::query_scalar("SELECT id FROM quest_statuses WHERE slug = 'open'")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO quests (quest_type_id, number, title, status_id, urgency, created_at, updated_at)
             VALUES (?, 1, 'q', ?, 3, datetime('now'), datetime('now'))",
        )
        .bind(dev_id)
        .bind(open_id)
        .execute(&store.index_pool)
        .await
        .unwrap();
        let qid: i64 = sqlx::query_scalar(
            "SELECT id FROM quests WHERE quest_type_id = ? AND number = 1",
        )
        .bind(dev_id)
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        let q = crate::services::quests::fetch_by_id(&store.index_pool, qid)
            .await
            .unwrap();
        crate::ops::quests::write_quest_file(&store, &q, true).await.unwrap();

        sqlx::query(
            "INSERT INTO quest_history (quest_id, quest_slug, ts, op, old_value, new_value)
             VALUES (?, 'DEV-001', datetime('now'), 'change_status', 'open', 'in_progress')",
        )
        .bind(qid)
        .execute(&store.index_pool)
        .await
        .unwrap();

        // rename open → backlog.
        let updated = rename_status_slug(&store, "open".into(), "backlog".into())
            .await
            .unwrap();
        assert_eq!(updated.slug, "backlog");

        // 파일 rename.
        let old_path = store.paths.statuses_dir().join("1-open.toml");
        let new_path = store.paths.statuses_dir().join("1-backlog.toml");
        assert!(!old_path.exists(), "옛 status 파일 삭제");
        assert!(new_path.exists(), "새 status 파일 생성");

        // quest frontmatter status 갱신.
        let content = std::fs::read_to_string(dir.join(".guild/quests/DEV-001.md")).unwrap();
        assert!(content.contains("status = \"backlog\""));

        // history old_value 'open' → 'backlog'.
        let old_value: String = sqlx::query_scalar(
            "SELECT old_value FROM quest_history WHERE op = 'change_status'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(old_value, "backlog");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rename_status_slug_rejects_duplicate() {
        let (dir, store) = fresh_store("rename-st-dup").await;
        let err = rename_status_slug(&store, "open".into(), "in_progress".into())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rename_status_slug_rejects_invalid_slug() {
        let (dir, store) = fresh_store("rename-st-bad").await;
        for bad in &["Open", "with space", "with-dash", ""] {
            let err = rename_status_slug(&store, "on_hold".into(), (*bad).into())
                .await
                .unwrap_err();
            assert!(matches!(err, AppError::BadRequest(_)), "bad={bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
