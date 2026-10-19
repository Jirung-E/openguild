//! DEV-443: 개인 형광펜 — HTTP 어댑터. 따지는 일과 저장은 전부 core(`ops::highlights`)에 있다.
//!
//! 대상은 경로가 아니라 `?target=` 으로 받는다 — 규칙 slug 에는 공백 · 한글이 들어가고, 도서관 키
//! 같은 것을 경로 조각으로 넣으면 인코딩이 두 번 꼬인다.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use crate::error::AppResult;
use openguild_core::Store;
use openguild_core::ops::highlights::{self as ops, NewHighlight};
use openguild_core::repo::highlights::Highlight;

#[derive(Debug, Deserialize)]
pub struct TargetQuery {
    pub target: String,
}

pub async fn list(
    State(store): State<Store>,
    Query(q): Query<TargetQuery>,
) -> AppResult<Json<Vec<Highlight>>> {
    Ok(Json(ops::list_highlights(&store, &q.target).await?))
}

pub async fn add(
    State(store): State<Store>,
    Query(q): Query<TargetQuery>,
    Json(body): Json<NewHighlight>,
) -> AppResult<Json<Highlight>> {
    Ok(Json(ops::add_highlight(&store, &q.target, body).await?))
}

pub async fn remove(
    State(store): State<Store>,
    Path(id): Path<String>,
    Query(q): Query<TargetQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let removed = ops::remove_highlight(&store, &q.target, &id).await?;
    Ok(Json(serde_json::json!({ "removed": removed })))
}
