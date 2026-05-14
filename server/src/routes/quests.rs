use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use sqlx::SqlitePool;
use std::collections::HashSet;

use crate::error::{AppError, AppResult};
use openguild_core::models::{
    AddPrerequisiteRequest, CandidatesQuery, ChangeParentRequest, ChangeStatusRequest,
    CreateQuestRequest, DeleteQuestQuery, QuestDependency, QuestDetail, QuestPosition,
    QuestRow, UpdatePositionRequest, UpdateQuestRequest,
};

/// type, status를 JOIN해서 QuestRow를 가져오는 공통 SELECT
const QUEST_SELECT: &str = r#"
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

// --- 퀘스트 목록 ---

pub async fn list_quests(State(pool): State<SqlitePool>) -> AppResult<Json<Vec<QuestRow>>> {
    let sql = format!("{QUEST_SELECT} WHERE q.deleted_at IS NULL ORDER BY q.id DESC");
    let quests = sqlx::query_as::<_, QuestRow>(&sql)
        .fetch_all(&pool)
        .await?;
    Ok(Json(quests))
}

// --- 퀘스트 생성 ---

pub async fn create_quest(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateQuestRequest>,
) -> AppResult<(StatusCode, Json<QuestRow>)> {
    // parent_quest_id가 지정된 경우, 해당 부모가 존재(alive)하는지 검증
    if let Some(pid) = body.parent_quest_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM quests WHERE id = ? AND deleted_at IS NULL)",
        )
        .bind(pid)
        .fetch_one(&pool)
        .await?;
        if !exists {
            return Err(AppError::BadRequest(format!(
                "parent quest {pid} not found"
            )).into());
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

    let quest = fetch_quest_by_id(&pool, id).await?;
    Ok((StatusCode::CREATED, Json(quest)))
}

// --- 퀘스트 상세 ---

pub async fn get_quest(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> AppResult<Json<QuestDetail>> {
    let quest = fetch_quest_by_id(&pool, id).await?;
    let (sub_quests, prerequisites, position) = fetch_relations(&pool, id).await?;
    Ok(Json(QuestDetail {
        quest,
        sub_quests,
        prerequisites,
        position,
    }))
}

// --- 퀘스트 수정 (parent_quest_id 제외) ---

pub async fn update_quest(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateQuestRequest>,
) -> AppResult<Json<QuestRow>> {
    fetch_quest_by_id(&pool, id).await?;

    if let Some(title) = &body.title {
        sqlx::query("UPDATE quests SET title = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(title)
            .bind(id)
            .execute(&pool)
            .await?;
    }
    if body.description.is_some() {
        sqlx::query(
            "UPDATE quests SET description = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&body.description)
        .bind(id)
        .execute(&pool)
        .await?;
    }
    if let Some(urgency) = body.urgency {
        sqlx::query("UPDATE quests SET urgency = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(urgency)
            .bind(id)
            .execute(&pool)
            .await?;
    }

    let quest = fetch_quest_by_id(&pool, id).await?;
    Ok(Json(quest))
}

// --- 부모 퀘스트 변경 / 분리 ---
//
// 별도 엔드포인트로 분리한 이유: PATCH /quests/:id 의 UpdateQuestRequest로는
// `parent_quest_id: null`(분리)를 표현하기가 까다로움 (Option<Option<T>> 직렬화).

pub async fn change_parent(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<ChangeParentRequest>,
) -> AppResult<Json<QuestRow>> {
    fetch_quest_by_id(&pool, id).await?;

    if let Some(new_pid) = body.parent_quest_id {
        if new_pid == id {
            return Err(AppError::BadRequest(
                "a quest cannot be its own parent".to_string(),
            ).into());
        }
        // 새 부모가 자기 자신의 자손이면 사이클
        if is_descendant_of(&pool, new_pid, id).await? {
            return Err(AppError::BadRequest(
                "would create a parent cycle".to_string(),
            ).into());
        }
        // 상호 배제: 이 퀘스트가 새 부모의 직접 선행이면 sub 으로 들어갈 수 없음.
        // (즉 P 의 선행에 C 가 있는데 C 의 부모를 P 로 만들면 sub 과 prereq 동시 점유)
        let already_prereq: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM quest_dependencies
                            WHERE quest_id = ? AND prerequisite_id = ?)",
        )
        .bind(new_pid)
        .bind(id)
        .fetch_one(&pool)
        .await?;
        if already_prereq {
            return Err(AppError::BadRequest(
                "this quest is already a prerequisite of the target — cannot also be its sub-quest"
                    .to_string(),
            ).into());
        }
    }

    sqlx::query("UPDATE quests SET parent_quest_id = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(body.parent_quest_id)
        .bind(id)
        .execute(&pool)
        .await?;

    let quest = fetch_quest_by_id(&pool, id).await?;
    Ok(Json(quest))
}

// --- 퀘스트 삭제 (선택적 cascade) ---

pub async fn delete_quest(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Query(q): Query<DeleteQuestQuery>,
) -> AppResult<StatusCode> {
    let cascade_ids: Vec<i64> = q
        .cascade
        .as_deref()
        .map(|s| {
            s.split(',')
                .filter(|x| !x.trim().is_empty())
                .filter_map(|x| x.trim().parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_default();

    if cascade_ids.len() > 100 {
        return Err(AppError::BadRequest(
            "too many cascade ids (max 100)".to_string(),
        ).into());
    }

    let mut tx = pool.begin().await?;

    // cascade 로 명시된 ID 들이 실제 alive 직계 자식인지 검증
    for cid in &cascade_ids {
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
            )).into());
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
    for cid in &cascade_ids {
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
        return Err(AppError::NotFound(format!("quest {id} not found")).into());
    }

    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- soft deleted 퀘스트 목록 ---

pub async fn list_deleted_quests(
    State(pool): State<SqlitePool>,
) -> AppResult<Json<Vec<QuestRow>>> {
    let sql = format!(
        "{QUEST_SELECT} WHERE q.deleted_at IS NOT NULL ORDER BY q.deleted_at DESC"
    );
    let quests = sqlx::query_as::<_, QuestRow>(&sql)
        .fetch_all(&pool)
        .await?;
    Ok(Json(quests))
}

// --- 퀘스트 복원 (soft delete 취소) ---

pub async fn restore_quest(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> AppResult<Json<QuestRow>> {
    let rows = sqlx::query(
        "UPDATE quests SET deleted_at = NULL, updated_at = datetime('now')
         WHERE id = ? AND deleted_at IS NOT NULL",
    )
    .bind(id)
    .execute(&pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!(
            "quest {id} is not deleted (or does not exist)"
        ))
        .into());
    }
    let quest = fetch_quest_by_id(&pool, id).await?;
    Ok(Json(quest))
}

// --- 상태 변경 ---

pub async fn change_status(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<ChangeStatusRequest>,
) -> AppResult<Json<QuestRow>> {
    let rows = sqlx::query(
        "UPDATE quests SET status_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(body.status_id)
    .bind(id)
    .execute(&pool)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("quest {id} not found")).into());
    }

    let quest = fetch_quest_by_id(&pool, id).await?;
    Ok(Json(quest))
}

// --- 선행 퀘스트 추가 (사이클 방지) ---

pub async fn add_prerequisite(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<AddPrerequisiteRequest>,
) -> AppResult<StatusCode> {
    if id == body.prerequisite_id {
        return Err(AppError::BadRequest(
            "a quest cannot be its own prerequisite".to_string(),
        ).into());
    }
    // 둘 다 존재 검증
    let target = fetch_quest_by_id(&pool, id).await?;
    let prereq = fetch_quest_by_id(&pool, body.prerequisite_id).await?;

    // 상호 배제: 후보가 이미 이 퀘스트의 직접 자식이면 prereq 로 추가 불가
    if prereq.parent_quest_id == Some(id) {
        return Err(AppError::BadRequest(
            "target is already a sub-quest — cannot also be a prerequisite".to_string(),
        ).into());
    }
    // 직계 부모는 prereq 로 추가 불가 — 부모-자식 관계는 의존(선행) 관계와 별개.
    if target.parent_quest_id == Some(prereq.id) {
        return Err(AppError::BadRequest(
            "parent quest cannot be added as a prerequisite".to_string(),
        ).into());
    }

    // 사이클 방지: prereq의 선행 체인에 id가 있으면 사이클
    if has_prerequisite_path(&pool, body.prerequisite_id, id).await? {
        return Err(AppError::BadRequest(
            "would create a dependency cycle".to_string(),
        ).into());
    }

    sqlx::query(
        "INSERT OR IGNORE INTO quest_dependencies (quest_id, prerequisite_id) VALUES (?, ?)",
    )
    .bind(id)
    .bind(body.prerequisite_id)
    .execute(&pool)
    .await?;

    Ok(StatusCode::CREATED)
}

// --- 선행 퀘스트 제거 ---

pub async fn remove_prerequisite(
    State(pool): State<SqlitePool>,
    Path((id, prereq_id)): Path<(i64, i64)>,
) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM quest_dependencies WHERE quest_id = ? AND prerequisite_id = ?")
        .bind(id)
        .bind(prereq_id)
        .execute(&pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// --- 후보 조회 (사이클/자기/이미 부모 있는 것 제외) ---
//
// relation:
//   - "parent" : 이 퀘스트의 부모로 지정 가능한 후보 (자기 + 자손 제외)
//   - "sub"    : 이 퀘스트의 서브로 지정 가능한 후보 (자기 + 조상 + 이미 부모 있는 것 제외)
//   - "prereq" : 이 퀘스트의 선행으로 추가 가능한 후보 (자기 + 사이클 제외)

pub async fn list_candidates(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Query(q): Query<CandidatesQuery>,
) -> AppResult<Json<Vec<QuestRow>>> {
    let target = fetch_quest_by_id(&pool, id).await?;
    let all = sqlx::query_as::<_, QuestRow>(&format!(
        "{QUEST_SELECT} WHERE q.deleted_at IS NULL ORDER BY q.id DESC"
    ))
        .fetch_all(&pool)
        .await?;

    // 이 퀘스트가 이미 가진 직접 선행 / 직접 자식 ID 목록
    let direct_prereqs: HashSet<i64> = sqlx::query_scalar(
        "SELECT prerequisite_id FROM quest_dependencies WHERE quest_id = ?",
    )
    .bind(id)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .collect();

    let direct_subs: HashSet<i64> =
        sqlx::query_scalar(
            "SELECT id FROM quests WHERE parent_quest_id = ? AND deleted_at IS NULL",
        )
            .bind(id)
            .fetch_all(&pool)
            .await?
            .into_iter()
            .collect();

    let mut result = Vec::new();
    match q.relation.as_str() {
        "parent" => {
            for c in all {
                if c.id == id {
                    continue;
                }
                // 후보가 자기 자신의 자손이면 제외 (부모를 자손으로 만들면 사이클)
                if is_descendant_of(&pool, c.id, id).await? {
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
                    continue; // 이미 부모 있음
                }
                // 이미 이 퀘스트의 선행이라면 sub 으로도 지정 불가 (상호 배제)
                if direct_prereqs.contains(&c.id) {
                    continue;
                }
                // 자기 자신이 후보의 자손이면 (= 후보가 자기 조상이면) 제외
                if is_descendant_of(&pool, id, c.id).await? {
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
                // 이미 이 퀘스트의 서브라면 prereq 로도 지정 불가 (상호 배제)
                if direct_subs.contains(&c.id) {
                    continue;
                }
                // 직계 부모도 prereq 로 지정 불가
                if target.parent_quest_id == Some(c.id) {
                    continue;
                }
                // 후보의 선행 체인에 id가 있으면 사이클
                if has_prerequisite_path(&pool, c.id, id).await? {
                    continue;
                }
                result.push(c);
            }
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "invalid relation: {other} (expected parent|sub|prereq)"
            ))
            .into());
        }
    }

    Ok(Json(result))
}

// --- 노드 위치 저장 ---

pub async fn update_position(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<UpdatePositionRequest>,
) -> AppResult<Json<QuestPosition>> {
    sqlx::query(
        "INSERT INTO quest_positions (quest_id, x, y) VALUES (?, ?, ?)
         ON CONFLICT(quest_id) DO UPDATE SET x = excluded.x, y = excluded.y",
    )
    .bind(id)
    .bind(body.x)
    .bind(body.y)
    .execute(&pool)
    .await?;

    Ok(Json(QuestPosition {
        quest_id: id,
        x: body.x,
        y: body.y,
    }))
}

// --- quest_id 슬러그 조회 ---

pub async fn get_quest_by_slug(
    State(pool): State<SqlitePool>,
    Path(slug): Path<String>,
) -> AppResult<Json<QuestDetail>> {
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
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("quest {slug} not found")))?;

    let id = quest.id;
    let (sub_quests, prerequisites, position) = fetch_relations(&pool, id).await?;

    Ok(Json(QuestDetail {
        quest,
        sub_quests,
        prerequisites,
        position,
    }))
}

// --- 전체 위치 / 의존 관계 조회 ---

pub async fn list_positions(
    State(pool): State<SqlitePool>,
) -> AppResult<Json<Vec<QuestPosition>>> {
    // soft-deleted quest 의 position 은 응답에서 제외 — frontend 가 stale 노드를 그리지 않도록
    let positions = sqlx::query_as::<_, QuestPosition>(
        "SELECT p.quest_id, p.x, p.y
         FROM quest_positions p
         JOIN quests q ON q.id = p.quest_id
         WHERE q.deleted_at IS NULL",
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(positions))
}

pub async fn list_dependencies(
    State(pool): State<SqlitePool>,
) -> AppResult<Json<Vec<QuestDependency>>> {
    // 양 끝 quest 가 모두 alive 인 dependency 만
    let deps = sqlx::query_as::<_, QuestDependency>(
        "SELECT d.quest_id, d.prerequisite_id
         FROM quest_dependencies d
         JOIN quests q1 ON q1.id = d.quest_id
         JOIN quests q2 ON q2.id = d.prerequisite_id
         WHERE q1.deleted_at IS NULL AND q2.deleted_at IS NULL",
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(deps))
}

// --- 공통 헬퍼 ---

async fn fetch_quest_by_id(pool: &SqlitePool, id: i64) -> AppResult<QuestRow> {
    let sql = format!("{QUEST_SELECT} WHERE q.deleted_at IS NULL AND q.id = ?");
    sqlx::query_as::<_, QuestRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("quest {id} not found")).into())
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

/// `quest_id`가 `ancestor_id`의 자손인지 (parent_quest_id 체인을 거슬러 올라가며 검사).
/// `quest_id == ancestor_id`이면 true.
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
        let parent: Option<Option<i64>> =
            sqlx::query_scalar(
                "SELECT parent_quest_id FROM quests WHERE id = ? AND deleted_at IS NULL",
            )
                .bind(cid)
                .fetch_optional(pool)
                .await?;
        current = parent.flatten();
    }
    Ok(false)
}

/// `quest_id`의 선행 체인(transitively)에 `target_id`가 포함되는지 BFS로 확인.
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
        let prereqs: Vec<i64> = sqlx::query_scalar(
            "SELECT prerequisite_id FROM quest_dependencies WHERE quest_id = ?",
        )
        .bind(cid)
        .fetch_all(pool)
        .await?;
        to_visit.extend(prereqs);
    }
    Ok(false)
}
