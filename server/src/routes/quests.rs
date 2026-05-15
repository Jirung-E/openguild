//! HTTP 어댑터 — axum extractor → core::services::quests → JSON 응답.
//!
//! 비즈니스 로직 / SQL / 검증은 전부 core::services::quests 에 있다.
//! 이 파일은 입력 추출 + 출력 직렬화만 담당.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use sqlx::SqlitePool;

use crate::error::AppResult;
use openguild_core::models::{
    AddPrerequisiteRequest, CandidatesQuery, ChangeParentRequest, ChangeStatusRequest,
    CreateQuestRequest, DeleteQuestQuery, QuestDependency, QuestDetail, QuestPosition,
    QuestRow, UpdatePositionRequest, UpdateQuestRequest,
};
use openguild_core::services::quests as svc;

pub async fn list_quests(State(pool): State<SqlitePool>) -> AppResult<Json<Vec<QuestRow>>> {
    Ok(Json(svc::list(&pool).await?))
}

pub async fn create_quest(
    State(pool): State<SqlitePool>,
    Json(body): Json<CreateQuestRequest>,
) -> AppResult<(StatusCode, Json<QuestRow>)> {
    let quest = svc::create(&pool, body).await?;
    Ok((StatusCode::CREATED, Json(quest)))
}

pub async fn get_quest(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> AppResult<Json<QuestDetail>> {
    Ok(Json(svc::get(&pool, id).await?))
}

pub async fn update_quest(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateQuestRequest>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(svc::update(&pool, id, body).await?))
}

pub async fn change_parent(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<ChangeParentRequest>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(svc::change_parent(&pool, id, body).await?))
}

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
    svc::delete(&pool, id, &cascade_ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_deleted_quests(
    State(pool): State<SqlitePool>,
) -> AppResult<Json<Vec<QuestRow>>> {
    Ok(Json(svc::list_deleted(&pool).await?))
}

pub async fn restore_quest(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(svc::restore(&pool, id).await?))
}

pub async fn change_status(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<ChangeStatusRequest>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(svc::change_status(&pool, id, body).await?))
}

pub async fn add_prerequisite(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<AddPrerequisiteRequest>,
) -> AppResult<StatusCode> {
    svc::add_prerequisite(&pool, id, body).await?;
    Ok(StatusCode::CREATED)
}

pub async fn remove_prerequisite(
    State(pool): State<SqlitePool>,
    Path((id, prereq_id)): Path<(i64, i64)>,
) -> AppResult<StatusCode> {
    svc::remove_prerequisite(&pool, id, prereq_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_candidates(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Query(q): Query<CandidatesQuery>,
) -> AppResult<Json<Vec<QuestRow>>> {
    Ok(Json(svc::list_candidates(&pool, id, &q.relation).await?))
}

pub async fn update_position(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(body): Json<UpdatePositionRequest>,
) -> AppResult<Json<QuestPosition>> {
    Ok(Json(svc::update_position(&pool, id, body).await?))
}

pub async fn get_quest_by_slug(
    State(pool): State<SqlitePool>,
    Path(slug): Path<String>,
) -> AppResult<Json<QuestDetail>> {
    Ok(Json(svc::get_by_slug(&pool, &slug).await?))
}

pub async fn list_positions(
    State(pool): State<SqlitePool>,
) -> AppResult<Json<Vec<QuestPosition>>> {
    Ok(Json(svc::list_positions(&pool).await?))
}

pub async fn list_dependencies(
    State(pool): State<SqlitePool>,
) -> AppResult<Json<Vec<QuestDependency>>> {
    Ok(Json(svc::list_dependencies(&pool).await?))
}
