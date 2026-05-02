use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use sqlx::SqlitePool;

use crate::{
    error::{AppError, AppResult},
    models::{
        AddPrerequisiteRequest, ChangeStatusRequest, CreateQuestRequest, QuestDetail, QuestPosition,
        QuestRow, UpdatePositionRequest, UpdateQuestRequest,
    },
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
    let sql = format!("{QUEST_SELECT} ORDER BY q.id DESC");
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
    let mut tx = pool.begin().await?;

    // 타입별 카운터 증가 후 번호 획득
    let number = sqlx::query_scalar::<_, i64>(
        "UPDATE quest_counters SET last_number = last_number + 1
         WHERE quest_type_id = ? RETURNING last_number",
    )
    .bind(body.quest_type_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::BadRequest("invalid quest_type_id".to_string()))?;

    // 퀘스트 삽입
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

    // 서브퀘스트
    let sql = format!("{QUEST_SELECT} WHERE q.parent_quest_id = ? ORDER BY q.id");
    let sub_quests = sqlx::query_as::<_, QuestRow>(&sql)
        .bind(id)
        .fetch_all(&pool)
        .await?;

    // 선행 퀘스트
    let sql = format!(
        "{QUEST_SELECT}
         JOIN quest_dependencies dep ON q.id = dep.prerequisite_id
         WHERE dep.quest_id = ? ORDER BY q.id"
    );
    let prerequisites = sqlx::query_as::<_, QuestRow>(&sql)
        .bind(id)
        .fetch_all(&pool)
        .await?;

    // 노드 위치
    let position = sqlx::query_as::<_, QuestPosition>(
        "SELECT quest_id, x, y FROM quest_positions WHERE quest_id = ?",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?;

    Ok(Json(QuestDetail {
        quest,
        sub_quests,
        prerequisites,
        position,
    }))
}

// --- 퀘스트 수정 ---

pub async fn update_quest(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateQuestRequest>,
) -> AppResult<Json<QuestRow>> {
    // 존재 여부 확인
    fetch_quest_by_id(&pool, id).await?;

    if let Some(title) = &body.title {
        sqlx::query(
            "UPDATE quests SET title = ?, updated_at = datetime('now') WHERE id = ?",
        )
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
        sqlx::query(
            "UPDATE quests SET urgency = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(urgency)
        .bind(id)
        .execute(&pool)
        .await?;
    }

    if body.parent_quest_id.is_some() {
        sqlx::query(
            "UPDATE quests SET parent_quest_id = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(body.parent_quest_id)
        .bind(id)
        .execute(&pool)
        .await?;
    }

    let quest = fetch_quest_by_id(&pool, id).await?;
    Ok(Json(quest))
}

// --- 퀘스트 삭제 ---

pub async fn delete_quest(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    let rows = sqlx::query("DELETE FROM quests WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound(format!("quest {id} not found")));
    }

    Ok(StatusCode::NO_CONTENT)
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
        return Err(AppError::NotFound(format!("quest {id} not found")));
    }

    let quest = fetch_quest_by_id(&pool, id).await?;
    Ok(Json(quest))
}

// --- 선행 퀘스트 추가 ---

pub async fn add_prerequisite(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<AddPrerequisiteRequest>,
) -> AppResult<StatusCode> {
    if id == body.prerequisite_id {
        return Err(AppError::BadRequest(
            "a quest cannot be its own prerequisite".to_string(),
        ));
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
    sqlx::query(
        "DELETE FROM quest_dependencies WHERE quest_id = ? AND prerequisite_id = ?",
    )
    .bind(id)
    .bind(prereq_id)
    .execute(&pool)
    .await?;

    Ok(StatusCode::NO_CONTENT)
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

// --- 공통 헬퍼 ---

async fn fetch_quest_by_id(pool: &SqlitePool, id: i64) -> AppResult<QuestRow> {
    let sql = format!("{QUEST_SELECT} WHERE q.id = ?");
    sqlx::query_as::<_, QuestRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("quest {id} not found")))
}
