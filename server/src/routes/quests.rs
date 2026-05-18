//! HTTP 어댑터 — axum extractor → core::ops::quests (mutation) / core::services::quests (read) → JSON 응답.
//!
//! 비즈니스 로직 / SQL / 파일 IO / journal 은 전부 core 에 있다.
//! 이 파일은 입력 추출 + 출력 직렬화만 담당.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use crate::error::AppResult;
use openguild_core::models::{
    AddPrerequisiteRequest, CandidatesQuery, ChangeParentRequest, ChangeStatusRequest,
    CreateQuestRequest, DeleteQuestQuery, ListQuery, QuestDependency, QuestDetail,
    QuestPosition, QuestRow, UpdatePositionRequest, UpdateQuestRequest,
};
use openguild_core::ops::quests as ops;
use openguild_core::services::quests as read;
use openguild_core::Store;

pub async fn list_quests(
    State(store): State<Store>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<QuestRow>>> {
    Ok(Json(read::list(&store.index_pool, &q).await?))
}

pub async fn create_quest(
    State(store): State<Store>,
    Json(body): Json<CreateQuestRequest>,
) -> AppResult<(StatusCode, Json<QuestRow>)> {
    let quest = ops::create_quest(&store, body).await?;
    Ok((StatusCode::CREATED, Json(quest)))
}

pub async fn get_quest(
    State(store): State<Store>,
    Path(id): Path<i64>,
) -> AppResult<Json<QuestDetail>> {
    Ok(Json(read::get(&store.index_pool, id).await?))
}

pub async fn update_quest(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateQuestRequest>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(ops::update_quest(&store, id, body).await?))
}

pub async fn change_parent(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<ChangeParentRequest>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(ops::change_parent(&store, id, body).await?))
}

pub async fn delete_quest(
    State(store): State<Store>,
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
    ops::delete_quest(&store, id, &cascade_ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_deleted_quests(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestRow>>> {
    Ok(Json(read::list_deleted(&store.index_pool).await?))
}

pub async fn restore_quest(
    State(store): State<Store>,
    Path(id): Path<i64>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(ops::restore_quest(&store, id).await?))
}

pub async fn change_status(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<ChangeStatusRequest>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(ops::change_status(&store, id, body).await?))
}

pub async fn add_prerequisite(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<AddPrerequisiteRequest>,
) -> AppResult<StatusCode> {
    ops::add_prerequisite(&store, id, body).await?;
    Ok(StatusCode::CREATED)
}

pub async fn remove_prerequisite(
    State(store): State<Store>,
    Path((id, prereq_id)): Path<(i64, i64)>,
) -> AppResult<StatusCode> {
    ops::remove_prerequisite(&store, id, prereq_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_candidates(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Query(q): Query<CandidatesQuery>,
) -> AppResult<Json<Vec<QuestRow>>> {
    Ok(Json(read::list_candidates(&store.index_pool, id, &q.relation).await?))
}

pub async fn update_position(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<UpdatePositionRequest>,
) -> AppResult<Json<QuestPosition>> {
    // update_position 은 UI 상태 — 파일 IO 없음. SQL 만 직접.
    Ok(Json(read::update_position(&store.index_pool, id, body).await?))
}

pub async fn get_quest_by_slug(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<QuestDetail>> {
    Ok(Json(read::get_by_slug(&store.index_pool, &slug).await?))
}

pub async fn list_positions(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestPosition>>> {
    Ok(Json(read::list_positions(&store.index_pool).await?))
}

pub async fn list_dependencies(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestDependency>>> {
    Ok(Json(read::list_dependencies(&store.index_pool).await?))
}
