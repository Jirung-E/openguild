//! Quest 도메인 서비스. SQL · 검증 · 사이클 체크.
//!
//! 모든 함수는 `&SqlitePool` 과 plain Rust 인자를 받고 `AppResult<T>` 반환.
//! HTTP 형식과 무관. server route 와 cli local 모드가 동일하게 호출.

use sqlx::SqlitePool;
use std::collections::HashSet;

use crate::error::{AppError, AppResult};
use crate::models::{
    AddPrerequisiteRequest, ChangeParentRequest, ChangeStatusRequest, CreateQuestRequest,
    ListQuery, QuestDependency, QuestDetail, QuestPosition, QuestRow, UpdatePositionRequest,
    UpdateQuestRequest,
};

/// type / status 를 JOIN 한 공통 SELECT.
pub const QUEST_SELECT: &str = r#"
    SELECT
        q.id,
        qt.prefix || '-' || printf('%03d', q.number) AS quest_id,
        q.quest_type_id,
        qt.prefix  AS type_prefix,
        qt.color   AS type_color,
        q.number,
        q.title,
        q.description,
        q.status_id,
        qs.name_en AS status_name_en,
        qs.name_ko AS status_name_ko,
        qs.color   AS status_color,
        q.urgency,
        q.parent_quest_id,
        q.created_at,
        q.updated_at
    FROM quests q
    JOIN quest_types   qt ON q.quest_type_id = qt.id
    JOIN quest_statuses qs ON q.status_id    = qs.id
"#;

// ─────────────────────── 조회 ───────────────────────

/// 필터 / 정렬 / 제한 옵션. 미지정 시 기존 동작 (전체 alive, id DESC).
///
/// **fuzzy match** (`quest_statuses` 에 slug 컬럼 없음):
/// - type: `qt.prefix` 와 대소문자 무시 비교.
/// - status: `qs.name_en` 을 lower + space/dash → underscore 정규화 후 비교.
///
/// 다중 값: 콤마 구분 (`"DEV,BUG"`). 빈 문자열은 필터 미지정으로 취급.
/// 정렬 / 방향은 화이트리스트 매핑 — SQL injection 방어.
pub async fn list(pool: &SqlitePool, query: &ListQuery) -> AppResult<Vec<QuestRow>> {
    let mut sql = format!("{QUEST_SELECT} WHERE q.deleted_at IS NULL");

    // ── 다중 값 필터: 콤마 split, 빈 entry 제거 ──
    let types = split_csv(&query.r#type);
    let statuses = split_csv(&query.status);

    if !types.is_empty() {
        let placeholders = vec!["UPPER(?)"; types.len()].join(", ");
        sql.push_str(&format!(" AND UPPER(qt.prefix) IN ({placeholders})"));
    }
    if !statuses.is_empty() {
        let one = "REPLACE(REPLACE(LOWER(?), ' ', '_'), '-', '_')";
        let placeholders = vec![one; statuses.len()].join(", ");
        sql.push_str(&format!(
            " AND REPLACE(REPLACE(LOWER(qs.name_en), ' ', '_'), '-', '_') IN ({placeholders})"
        ));
    }

    // urgency — single / CSV / "a-b" 범위 (모두 1..=4).
    let urgency_values: Vec<i64> = parse_urgency(&query.urgency)?;
    if !urgency_values.is_empty() {
        let placeholders = vec!["?"; urgency_values.len()].join(", ");
        sql.push_str(&format!(" AND q.urgency IN ({placeholders})"));
    }

    // 시간 범위 — ISO 8601 string 비교. SQLite 의 datetime 비교는 lexicographic OK.
    if trimmed_opt(&query.created_after).is_some() {
        sql.push_str(" AND q.created_at >= ?");
    }
    if trimmed_opt(&query.created_before).is_some() {
        sql.push_str(" AND q.created_at <= ?");
    }
    if trimmed_opt(&query.updated_after).is_some() {
        sql.push_str(" AND q.updated_at >= ?");
    }
    if trimmed_opt(&query.updated_before).is_some() {
        sql.push_str(" AND q.updated_at <= ?");
    }

    // 관계 필터 — 상호배타 검증.
    if query.has_prereq && query.no_prereq {
        return Err(AppError::BadRequest(
            "--has-prereq and --no-prereq are mutually exclusive".into(),
        ));
    }
    if query.has_sub && query.no_sub {
        return Err(AppError::BadRequest(
            "--has-sub and --no-sub are mutually exclusive".into(),
        ));
    }
    if query.has_prereq {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM quest_dependencies d WHERE d.quest_id = q.id)",
        );
    }
    if query.no_prereq {
        sql.push_str(
            " AND NOT EXISTS (SELECT 1 FROM quest_dependencies d WHERE d.quest_id = q.id)",
        );
    }
    if query.has_sub {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM quests s WHERE s.parent_quest_id = q.id AND s.deleted_at IS NULL)",
        );
    }
    if query.no_sub {
        sql.push_str(
            " AND NOT EXISTS (SELECT 1 FROM quests s WHERE s.parent_quest_id = q.id AND s.deleted_at IS NULL)",
        );
    }

    // search — 공백 split AND. 각 토큰이 title 또는 description 중 하나엔 LIKE 매치.
    let search_tokens: Vec<String> = trimmed_opt(&query.search)
        .map(|s| {
            s.split_whitespace()
                .filter(|t| !t.is_empty())
                .map(|t| t.to_string())
                .collect()
        })
        .unwrap_or_default();
    for _ in &search_tokens {
        sql.push_str(
            " AND (LOWER(q.title) LIKE LOWER(?) OR LOWER(COALESCE(q.description, '')) LIKE LOWER(?))",
        );
    }

    // child_of / no_parent 상호배타
    if query.child_of.is_some() && query.no_parent {
        return Err(AppError::BadRequest(
            "--child-of and --no-parent are mutually exclusive".into(),
        ));
    }
    if query.no_parent {
        sql.push_str(" AND q.parent_quest_id IS NULL");
    }
    let parent_id: Option<i64> = if let Some(slug) = trimmed_opt(&query.child_of) {
        // slug → id 조회 (그 quest 가 parent 인 자식들 검색).
        let (prefix, num_str) = slug
            .split_once('-')
            .ok_or_else(|| AppError::BadRequest(format!("invalid --child-of slug: {slug}")))?;
        let number: i64 = num_str
            .parse()
            .map_err(|_| AppError::BadRequest(format!("invalid --child-of slug: {slug}")))?;
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT q.id FROM quests q JOIN quest_types qt ON q.quest_type_id = qt.id
             WHERE UPPER(qt.prefix) = UPPER(?) AND q.number = ? AND q.deleted_at IS NULL",
        )
        .bind(prefix)
        .bind(number)
        .fetch_optional(pool)
        .await?;
        let id = id.ok_or_else(|| AppError::NotFound(format!("quest {slug} not found")))?;
        sql.push_str(" AND q.parent_quest_id = ?");
        Some(id)
    } else {
        None
    };

    // ── 정렬: 화이트리스트, 다중 키 지원, 방향 전체 토글 ──
    let sort_keys = split_csv(&query.sort);
    let resolved_keys: Vec<(&'static str, bool)> = if sort_keys.is_empty() {
        vec![("id", false)] // default
    } else {
        sort_keys
            .iter()
            .map(|k| match k.to_lowercase().as_str() {
                "urgency" => Ok(("urgency", true)),
                "status" => Ok(("status", true)),
                "updated" => Ok(("updated", false)),
                "created" => Ok(("created", false)),
                "id" => Ok(("id", false)),
                other => Err(AppError::BadRequest(format!(
                    "unsupported sort key '{other}' — expected one of id/urgency/status/updated/created"
                ))),
            })
            .collect::<AppResult<Vec<_>>>()?
    };

    let order_parts: Vec<String> = resolved_keys
        .iter()
        .map(|(key, asc_default)| {
            let asc = if query.reverse { !asc_default } else { *asc_default };
            let dir = if asc { "ASC" } else { "DESC" };
            let col = match *key {
                "urgency" => "q.urgency",
                "status" => "qs.sort_order",
                "updated" => "q.updated_at",
                "created" => "q.created_at",
                _ => "q.id",
            };
            format!("{col} {dir}")
        })
        .collect();
    // 마지막 tiebreaker 로 q.id (안정 정렬). 이미 sort key 에 id 있으면 중복 안 함.
    let has_id = resolved_keys.iter().any(|(k, _)| *k == "id");
    let order = if has_id {
        order_parts.join(", ")
    } else {
        format!("{}, q.id DESC", order_parts.join(", "))
    };
    sql.push_str(&format!(" ORDER BY {order}"));

    if let Some(limit) = query.limit {
        if limit < 0 {
            return Err(AppError::BadRequest("limit must be non-negative".into()));
        }
        sql.push_str(&format!(" LIMIT {limit}"));
    }
    if let Some(offset) = query.offset {
        if offset < 0 {
            return Err(AppError::BadRequest("offset must be non-negative".into()));
        }
        if query.limit.is_none() {
            // SQLite 의 LIMIT 없는 OFFSET 지원 위해 큰 LIMIT 부여.
            sql.push_str(" LIMIT -1");
        }
        sql.push_str(&format!(" OFFSET {offset}"));
    }

    let mut q = sqlx::query_as::<_, QuestRow>(&sql);
    for t in &types {
        q = q.bind(t);
    }
    for s in &statuses {
        q = q.bind(s);
    }
    for u in &urgency_values {
        q = q.bind(*u);
    }
    // 시간 범위 — bind 순서는 SQL 의 ? 순서와 동일.
    if let Some(v) = trimmed_opt(&query.created_after) {
        q = q.bind(v.to_string());
    }
    if let Some(v) = trimmed_opt(&query.created_before) {
        q = q.bind(v.to_string());
    }
    if let Some(v) = trimmed_opt(&query.updated_after) {
        q = q.bind(v.to_string());
    }
    if let Some(v) = trimmed_opt(&query.updated_before) {
        q = q.bind(v.to_string());
    }
    if let Some(pid) = parent_id {
        q = q.bind(pid);
    }
    // search tokens — 각 토큰마다 (title, description) 두 번 bind.
    for token in &search_tokens {
        let pat = format!("%{token}%");
        q = q.bind(pat.clone());
        q = q.bind(pat);
    }
    let quests = q.fetch_all(pool).await?;
    Ok(quests)
}

/// urgency string 파싱 — single / CSV / "a-b" 범위. 모두 1..=4 검증.
fn parse_urgency(s: &Option<String>) -> AppResult<Vec<i64>> {
    let Some(raw) = trimmed_opt(s) else { return Ok(Vec::new()) };
    let mut result: Vec<i64> = Vec::new();
    if let Some((lo_s, hi_s)) = raw.split_once('-') {
        // "a-b" 범위 — 양쪽 끝 inclusive.
        let lo: i64 = lo_s.trim().parse().map_err(|_| {
            AppError::BadRequest(format!("invalid urgency range start: {lo_s}"))
        })?;
        let hi: i64 = hi_s.trim().parse().map_err(|_| {
            AppError::BadRequest(format!("invalid urgency range end: {hi_s}"))
        })?;
        if !(1..=4).contains(&lo) || !(1..=4).contains(&hi) {
            return Err(AppError::BadRequest(format!(
                "urgency range out of bounds: {raw} (must be 1..=4)"
            )));
        }
        if lo > hi {
            return Err(AppError::BadRequest(format!(
                "urgency range start > end: {raw}"
            )));
        }
        for u in lo..=hi {
            result.push(u);
        }
    } else {
        // CSV 또는 단일.
        for part in raw.split(',') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            let u: i64 = p
                .parse()
                .map_err(|_| AppError::BadRequest(format!("invalid urgency: {p}")))?;
            if !(1..=4).contains(&u) {
                return Err(AppError::BadRequest(format!(
                    "urgency must be 1..=4, got {u}"
                )));
            }
            result.push(u);
        }
    }
    // 중복 제거.
    result.sort_unstable();
    result.dedup();
    Ok(result)
}

/// 콤마 구분 string → Vec — 빈 entry / whitespace-only 제거.
/// 빈 문자열 (`?type=`) 또는 None 둘 다 빈 Vec 반환 (= 필터 미지정).
fn split_csv(s: &Option<String>) -> Vec<String> {
    let Some(raw) = s else { return Vec::new() };
    raw.split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

/// Option<String> 의 trim 후 빈 문자열 → None.
fn trimmed_opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

pub async fn list_deleted(pool: &SqlitePool) -> AppResult<Vec<QuestRow>> {
    let sql =
        format!("{QUEST_SELECT} WHERE q.deleted_at IS NOT NULL ORDER BY q.deleted_at DESC");
    let quests = sqlx::query_as::<_, QuestRow>(&sql).fetch_all(pool).await?;
    Ok(quests)
}

pub async fn get(pool: &SqlitePool, id: i64) -> AppResult<QuestDetail> {
    let quest = fetch_by_id(pool, id).await?;
    let (sub_quests, prerequisites, position) = fetch_relations(pool, id).await?;
    Ok(QuestDetail {
        quest,
        sub_quests,
        prerequisites,
        position,
    })
}

pub async fn get_by_slug(pool: &SqlitePool, slug: &str) -> AppResult<QuestDetail> {
    let (prefix, num_str) = slug
        .split_once('-')
        .ok_or_else(|| AppError::BadRequest(format!("invalid quest id: {slug}")))?;

    let number: i64 = num_str
        .parse()
        .map_err(|_| AppError::BadRequest(format!("invalid quest number: {num_str}")))?;

    let sql = format!(
        "{QUEST_SELECT} WHERE q.deleted_at IS NULL AND qt.prefix = ? AND q.number = ?"
    );
    let quest = sqlx::query_as::<_, QuestRow>(&sql)
        .bind(prefix.to_uppercase())
        .bind(number)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("quest {slug} not found")))?;

    let id = quest.id;
    let (sub_quests, prerequisites, position) = fetch_relations(pool, id).await?;

    Ok(QuestDetail {
        quest,
        sub_quests,
        prerequisites,
        position,
    })
}

pub async fn list_positions(pool: &SqlitePool) -> AppResult<Vec<QuestPosition>> {
    // soft-deleted quest 의 position 은 응답에서 제외 — frontend 가 stale 노드를 그리지 않도록
    let positions = sqlx::query_as::<_, QuestPosition>(
        "SELECT p.quest_id, p.x, p.y
         FROM quest_positions p
         JOIN quests q ON q.id = p.quest_id
         WHERE q.deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(positions)
}

pub async fn list_dependencies(pool: &SqlitePool) -> AppResult<Vec<QuestDependency>> {
    // 양 끝 quest 가 모두 alive 인 dependency 만
    let deps = sqlx::query_as::<_, QuestDependency>(
        "SELECT d.quest_id, d.prerequisite_id
         FROM quest_dependencies d
         JOIN quests q1 ON q1.id = d.quest_id
         JOIN quests q2 ON q2.id = d.prerequisite_id
         WHERE q1.deleted_at IS NULL AND q2.deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(deps)
}

// ─────────────────────── 변경 ───────────────────────

pub async fn create(pool: &SqlitePool, body: CreateQuestRequest) -> AppResult<QuestRow> {
    // parent_quest_id 지정 시 부모가 alive 한지 검증
    if let Some(pid) = body.parent_quest_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM quests WHERE id = ? AND deleted_at IS NULL)",
        )
        .bind(pid)
        .fetch_one(pool)
        .await?;
        if !exists {
            return Err(AppError::BadRequest(format!(
                "parent quest {pid} not found"
            )));
        }
    }

    let mut tx = pool.begin().await?;

    let number = sqlx::query_scalar::<_, i64>(
        "UPDATE quest_counters SET last_number = last_number + 1
         WHERE quest_type_id = ? RETURNING last_number",
    )
    .bind(body.quest_type_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::BadRequest("invalid quest_type_id".to_string()))?;

    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO quests (quest_type_id, number, title, description, status_id, urgency, parent_quest_id)
         VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(body.quest_type_id)
    .bind(number)
    .bind(&body.title)
    .bind(&body.description)
    .bind(body.status_id)
    .bind(body.urgency.unwrap_or(3))
    .bind(body.parent_quest_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    fetch_by_id(pool, id).await
}

pub async fn update(pool: &SqlitePool, id: i64, body: UpdateQuestRequest) -> AppResult<QuestRow> {
    fetch_by_id(pool, id).await?;

    if let Some(title) = &body.title {
        sqlx::query("UPDATE quests SET title = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(title)
            .bind(id)
            .execute(pool)
            .await?;
    }
    if body.description.is_some() {
        sqlx::query(
            "UPDATE quests SET description = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&body.description)
        .bind(id)
        .execute(pool)
        .await?;
    }
    if let Some(urgency) = body.urgency {
        sqlx::query("UPDATE quests SET urgency = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(urgency)
            .bind(id)
            .execute(pool)
            .await?;
    }

    fetch_by_id(pool, id).await
}

pub async fn change_parent(
    pool: &SqlitePool,
    id: i64,
    body: ChangeParentRequest,
) -> AppResult<QuestRow> {
    fetch_by_id(pool, id).await?;

    if let Some(new_pid) = body.parent_quest_id {
        if new_pid == id {
            return Err(AppError::BadRequest(
                "a quest cannot be its own parent".to_string(),
            ));
        }
        // 새 부모가 자기 자신의 자손이면 사이클
        if is_descendant_of(pool, new_pid, id).await? {
            return Err(AppError::BadRequest(
                "would create a parent cycle".to_string(),
            ));
        }
        // 상호 배제: 이 퀘스트가 새 부모의 직접 선행이면 sub 으로 들어갈 수 없음.
        let already_prereq: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM quest_dependencies
                            WHERE quest_id = ? AND prerequisite_id = ?)",
        )
        .bind(new_pid)
        .bind(id)
        .fetch_one(pool)
        .await?;
        if already_prereq {
            return Err(AppError::BadRequest(
                "this quest is already a prerequisite of the target — cannot also be its sub-quest"
                    .to_string(),
            ));
        }
    }

    sqlx::query("UPDATE quests SET parent_quest_id = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(body.parent_quest_id)
        .bind(id)
        .execute(pool)
        .await?;

    fetch_by_id(pool, id).await
}

pub async fn change_status(
    pool: &SqlitePool,
    id: i64,
    body: ChangeStatusRequest,
) -> AppResult<QuestRow> {
    let rows = sqlx::query(
        "UPDATE quests SET status_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(body.status_id)
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("quest {id} not found")));
    }

    fetch_by_id(pool, id).await
}

/// 본 퀘스트 soft delete. `cascade_ids` 가 있으면 해당 직계 자식도 같이 soft delete,
/// 그 외 alive 직계 자식은 parent_quest_id = NULL 로 분리.
pub async fn delete(
    pool: &SqlitePool,
    id: i64,
    cascade_ids: &[i64],
) -> AppResult<()> {
    if cascade_ids.len() > 100 {
        return Err(AppError::BadRequest(
            "too many cascade ids (max 100)".to_string(),
        ));
    }

    let mut tx = pool.begin().await?;

    // cascade 로 명시된 ID 들이 실제 alive 직계 자식인지 검증
    for cid in cascade_ids {
        let is_child: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM quests WHERE id = ? AND parent_quest_id = ? AND deleted_at IS NULL)",
        )
        .bind(cid)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if !is_child {
            return Err(AppError::BadRequest(format!(
                "quest {cid} is not a direct child of {id}"
            )));
        }
    }

    // cascade 안 한 alive 직계 자식들 → parent_quest_id = NULL (분리)
    let cascade_filter = if cascade_ids.is_empty() {
        String::new()
    } else {
        format!(
            " AND id NOT IN ({})",
            cascade_ids
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    sqlx::query(&format!(
        "UPDATE quests SET parent_quest_id = NULL, updated_at = datetime('now')
         WHERE parent_quest_id = ? AND deleted_at IS NULL{cascade_filter}"
    ))
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // 명시된 자식들 soft delete
    for cid in cascade_ids {
        sqlx::query(
            "UPDATE quests SET deleted_at = datetime('now'), updated_at = datetime('now')
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(cid)
        .execute(&mut *tx)
        .await?;
    }

    // 본 퀘스트 soft delete
    let rows = sqlx::query(
        "UPDATE quests SET deleted_at = datetime('now'), updated_at = datetime('now')
         WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("quest {id} not found")));
    }

    tx.commit().await?;
    Ok(())
}

pub async fn restore(pool: &SqlitePool, id: i64) -> AppResult<QuestRow> {
    let rows = sqlx::query(
        "UPDATE quests SET deleted_at = NULL, updated_at = datetime('now')
         WHERE id = ? AND deleted_at IS NOT NULL",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!(
            "quest {id} is not deleted (or does not exist)"
        )));
    }
    fetch_by_id(pool, id).await
}

pub async fn add_prerequisite(
    pool: &SqlitePool,
    id: i64,
    body: AddPrerequisiteRequest,
) -> AppResult<()> {
    if id == body.prerequisite_id {
        return Err(AppError::BadRequest(
            "a quest cannot be its own prerequisite".to_string(),
        ));
    }
    // 둘 다 존재 검증
    let target = fetch_by_id(pool, id).await?;
    let prereq = fetch_by_id(pool, body.prerequisite_id).await?;

    // 상호 배제: 후보가 이미 이 퀘스트의 직접 자식이면 prereq 로 추가 불가
    if prereq.parent_quest_id == Some(id) {
        return Err(AppError::BadRequest(
            "target is already a sub-quest — cannot also be a prerequisite".to_string(),
        ));
    }
    // 직계 부모는 prereq 로 추가 불가 — 부모-자식 관계는 의존(선행) 관계와 별개.
    if target.parent_quest_id == Some(prereq.id) {
        return Err(AppError::BadRequest(
            "parent quest cannot be added as a prerequisite".to_string(),
        ));
    }

    // 사이클 방지: prereq의 선행 체인에 id가 있으면 사이클
    if has_prerequisite_path(pool, body.prerequisite_id, id).await? {
        return Err(AppError::BadRequest(
            "would create a dependency cycle".to_string(),
        ));
    }

    sqlx::query(
        "INSERT OR IGNORE INTO quest_dependencies (quest_id, prerequisite_id) VALUES (?, ?)",
    )
    .bind(id)
    .bind(body.prerequisite_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn remove_prerequisite(
    pool: &SqlitePool,
    id: i64,
    prereq_id: i64,
) -> AppResult<()> {
    sqlx::query("DELETE FROM quest_dependencies WHERE quest_id = ? AND prerequisite_id = ?")
        .bind(id)
        .bind(prereq_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_candidates(
    pool: &SqlitePool,
    id: i64,
    relation: &str,
) -> AppResult<Vec<QuestRow>> {
    let target = fetch_by_id(pool, id).await?;
    let all = sqlx::query_as::<_, QuestRow>(&format!(
        "{QUEST_SELECT} WHERE q.deleted_at IS NULL ORDER BY q.id DESC"
    ))
    .fetch_all(pool)
    .await?;

    let direct_prereqs: HashSet<i64> =
        sqlx::query_scalar("SELECT prerequisite_id FROM quest_dependencies WHERE quest_id = ?")
            .bind(id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();

    let direct_subs: HashSet<i64> = sqlx::query_scalar(
        "SELECT id FROM quests WHERE parent_quest_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let mut result = Vec::new();
    match relation {
        "parent" => {
            for c in all {
                if c.id == id {
                    continue;
                }
                if is_descendant_of(pool, c.id, id).await? {
                    continue;
                }
                result.push(c);
            }
        }
        "sub" => {
            for c in all {
                if c.id == id {
                    continue;
                }
                if c.parent_quest_id.is_some() {
                    continue;
                }
                if direct_prereqs.contains(&c.id) {
                    continue;
                }
                if is_descendant_of(pool, id, c.id).await? {
                    continue;
                }
                result.push(c);
            }
        }
        "prereq" => {
            for c in all {
                if c.id == id {
                    continue;
                }
                if direct_subs.contains(&c.id) {
                    continue;
                }
                if target.parent_quest_id == Some(c.id) {
                    continue;
                }
                if has_prerequisite_path(pool, c.id, id).await? {
                    continue;
                }
                result.push(c);
            }
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "invalid relation: {other} (expected parent|sub|prereq)"
            )));
        }
    }

    Ok(result)
}

pub async fn update_position(
    pool: &SqlitePool,
    id: i64,
    body: UpdatePositionRequest,
) -> AppResult<QuestPosition> {
    sqlx::query(
        "INSERT INTO quest_positions (quest_id, x, y) VALUES (?, ?, ?)
         ON CONFLICT(quest_id) DO UPDATE SET x = excluded.x, y = excluded.y",
    )
    .bind(id)
    .bind(body.x)
    .bind(body.y)
    .execute(pool)
    .await?;

    Ok(QuestPosition {
        quest_id: id,
        x: body.x,
        y: body.y,
    })
}

// ─────────────────────── 내부 헬퍼 ───────────────────────

/// id 로 alive 한 퀘스트 1건 조회. 없으면 NotFound.
pub async fn fetch_by_id(pool: &SqlitePool, id: i64) -> AppResult<QuestRow> {
    let sql = format!("{QUEST_SELECT} WHERE q.deleted_at IS NULL AND q.id = ?");
    sqlx::query_as::<_, QuestRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("quest {id} not found")))
}

async fn fetch_relations(
    pool: &SqlitePool,
    id: i64,
) -> AppResult<(Vec<QuestRow>, Vec<QuestRow>, Option<QuestPosition>)> {
    let sub_sql = format!(
        "{QUEST_SELECT} WHERE q.deleted_at IS NULL AND q.parent_quest_id = ? ORDER BY q.id"
    );
    let sub_quests = sqlx::query_as::<_, QuestRow>(&sub_sql)
        .bind(id)
        .fetch_all(pool)
        .await?;

    let prereq_sql = format!(
        "{QUEST_SELECT}
         JOIN quest_dependencies dep ON q.id = dep.prerequisite_id
         WHERE q.deleted_at IS NULL AND dep.quest_id = ? ORDER BY q.id"
    );
    let prerequisites = sqlx::query_as::<_, QuestRow>(&prereq_sql)
        .bind(id)
        .fetch_all(pool)
        .await?;

    let position = sqlx::query_as::<_, QuestPosition>(
        "SELECT quest_id, x, y FROM quest_positions WHERE quest_id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok((sub_quests, prerequisites, position))
}

/// `quest_id` 가 `ancestor_id` 의 자손인지 (parent_quest_id 체인 BFS).
/// `quest_id == ancestor_id` 이면 true.
async fn is_descendant_of(
    pool: &SqlitePool,
    quest_id: i64,
    ancestor_id: i64,
) -> AppResult<bool> {
    let mut current = Some(quest_id);
    let mut visited: HashSet<i64> = HashSet::new();
    while let Some(cid) = current {
        if !visited.insert(cid) {
            // 데이터 오염 사이클. 더 이상 진행 불가.
            break;
        }
        if cid == ancestor_id {
            return Ok(true);
        }
        let parent: Option<Option<i64>> = sqlx::query_scalar(
            "SELECT parent_quest_id FROM quests WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(cid)
        .fetch_optional(pool)
        .await?;
        current = parent.flatten();
    }
    Ok(false)
}

/// `quest_id` 의 선행 체인(transitively) 에 `target_id` 가 포함되는지 BFS.
async fn has_prerequisite_path(
    pool: &SqlitePool,
    quest_id: i64,
    target_id: i64,
) -> AppResult<bool> {
    let mut to_visit = vec![quest_id];
    let mut visited: HashSet<i64> = HashSet::new();
    while let Some(cid) = to_visit.pop() {
        if !visited.insert(cid) {
            continue;
        }
        if cid == target_id {
            return Ok(true);
        }
        let prereqs: Vec<i64> =
            sqlx::query_scalar("SELECT prerequisite_id FROM quest_dependencies WHERE quest_id = ?")
                .bind(cid)
                .fetch_all(pool)
                .await?;
        to_visit.extend(prereqs);
    }
    Ok(false)
}
