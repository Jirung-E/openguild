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
    QuestHistoryEntry, QuestPosition, QuestRow, UpdatePositionRequest, UpdateQuestRequest,
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

/// DEV-076: 희망 / 필수 기한 설정 / 해제.
///
/// JSON body: 키 존재 여부로 변경 의도 구분.
///   { "desired_due": "2026-06-15" }  → 설정
///   { "desired_due": null }          → 해제
///   {}                                → 변경 없음 (no-op)
/// 두 필드 동시 가능.
pub async fn set_due_dates(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<QuestRow>> {
    use serde_json::Value;
    fn parse_field(body: &Value, key: &str) -> Option<Option<String>> {
        // 키가 없으면 None (no-op). 있고 null 이면 Some(None) (해제).
        // 있고 string 이면 Some(Some(s)).
        let obj = body.as_object()?;
        let v = obj.get(key)?;
        Some(match v {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            _ => return None, // 타입 오류면 그냥 무시 (no-op) — 엄밀한 검증은 service.
        })
    }
    let desired = parse_field(&body, "desired_due");
    let required = parse_field(&body, "required_due");
    Ok(Json(ops::set_due_dates(&store, id, desired, required).await?))
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

/// DEV-013: GET /api/quests/{id}/history
pub async fn list_history(
    State(store): State<Store>,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<QuestHistoryEntry>>> {
    Ok(Json(read::list_history(&store.index_pool, id).await?))
}
