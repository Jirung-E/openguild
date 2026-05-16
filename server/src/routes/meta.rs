use axum::{extract::State, Json};

use crate::error::AppResult;
use openguild_core::models::{QuestStatus, QuestType};
use openguild_core::services::meta as svc;
use openguild_core::Store;

pub async fn list_quest_types(State(store): State<Store>) -> AppResult<Json<Vec<QuestType>>> {
    Ok(Json(svc::list_quest_types(&store.index_pool).await?))
}

pub async fn list_quest_statuses(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestStatus>>> {
    Ok(Json(svc::list_quest_statuses(&store.index_pool).await?))
}
