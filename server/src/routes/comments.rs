//! DEV-012: Quest 별 댓글 / 메모 HTTP 어댑터.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use openguild_core::ops::comments as ops;
use openguild_core::Store;

#[derive(Debug, Serialize)]
pub struct ContentResponse {
    /// 파일 부재 시 `null`.
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetContentRequest {
    pub content: String,
}

pub async fn get_comments(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<ContentResponse>> {
    let content = ops::get_comments(&store, &slug)?;
    Ok(Json(ContentResponse { content }))
}

pub async fn set_comments(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<SetContentRequest>,
) -> AppResult<Json<ContentResponse>> {
    ops::set_comments(&store, &slug, body.content.clone()).await?;
    Ok(Json(ContentResponse {
        content: Some(body.content),
    }))
}

pub async fn get_memo(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<ContentResponse>> {
    let content = ops::get_memo(&store, &slug)?;
    Ok(Json(ContentResponse { content }))
}

pub async fn set_memo(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<SetContentRequest>,
) -> AppResult<Json<ContentResponse>> {
    ops::set_memo(&store, &slug, body.content.clone()).await?;
    Ok(Json(ContentResponse {
        content: Some(body.content),
    }))
}
