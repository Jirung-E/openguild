use axum::{extract::State, Json};
use sqlx::SqlitePool;

use crate::error::AppResult;
use openguild_core::models::{QuestStatus, QuestType};
use openguild_core::services::meta as svc;

pub async fn list_quest_types(State(pool): State<SqlitePool>) -> AppResult<Json<Vec<QuestType>>> {
    Ok(Json(svc::list_quest_types(&pool).await?))
}

pub async fn list_quest_statuses(
    State(pool): State<SqlitePool>,
) -> AppResult<Json<Vec<QuestStatus>>> {
    Ok(Json(svc::list_quest_statuses(&pool).await?))
}
