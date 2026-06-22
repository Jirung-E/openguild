use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

use crate::error::AppResult;
use openguild_core::models::{QuestStatus, QuestTagDef, QuestType};
use openguild_core::ops::meta as meta_ops;
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

/// DEV-068: tag def 목록.
pub async fn list_tag_defs(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestTagDef>>> {
    Ok(Json(svc::list_quest_tag_defs(&store.index_pool).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpsertTagDefBody {
    pub slug: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub description: String,
}

pub async fn upsert_tag_def(
    State(store): State<Store>,
    Json(body): Json<UpsertTagDefBody>,
) -> AppResult<Json<QuestTagDef>> {
    Ok(Json(
        meta_ops::upsert_tag_def(&store, body.slug, body.color, body.description).await?,
    ))
}

pub async fn delete_tag_def(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    meta_ops::delete_tag_def(&store, slug).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
