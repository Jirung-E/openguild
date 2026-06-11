//! DEV-012 / DEV-094: Quest 별 댓글 / 메모 HTTP 어댑터.
//!
//! 댓글은 entry 단위 (`GET 목록 / POST 추가 / PATCH 본문수정 / DELETE`).
//! 메모는 단일 텍스트 그대로.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use openguild_core::ops::comments as ops;
use openguild_core::repo::comments::CommentEntry;
use openguild_core::Store;

// ─── 메모: 단일 텍스트 ───

#[derive(Debug, Serialize)]
pub struct ContentResponse {
    /// 파일 부재 시 `null`.
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetContentRequest {
    pub content: String,
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

// ─── DEV-094: 댓글 entry 단위 ───

#[derive(Debug, Serialize)]
pub struct CommentsListResponse {
    pub entries: Vec<CommentEntry>,
}

#[derive(Debug, Deserialize)]
pub struct AddCommentRequest {
    #[serde(default)]
    pub author: String,
    pub body: String,
    /// DEV-094 후속: 답글이면 부모 entry id. None / 미지정 → top-level.
    #[serde(default)]
    pub parent_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommentRequest {
    pub body: String,
}

pub async fn list_comments(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<CommentsListResponse>> {
    let entries = ops::list_comment_entries(&store, &slug)?;
    Ok(Json(CommentsListResponse { entries }))
}

pub async fn add_comment(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<AddCommentRequest>,
) -> AppResult<Json<CommentEntry>> {
    let entry =
        ops::add_comment_entry(&store, &slug, body.author, body.body, body.parent_id)
            .await?;
    Ok(Json(entry))
}

pub async fn update_comment(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
    Json(body): Json<UpdateCommentRequest>,
) -> AppResult<Json<CommentEntry>> {
    let entry = ops::update_comment_entry(&store, &slug, id, body.body).await?;
    Ok(Json(entry))
}

pub async fn delete_comment(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
) -> AppResult<axum::http::StatusCode> {
    ops::delete_comment_entry(&store, &slug, id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ─── DEV-108: 이모지 반응 ───

#[derive(Debug, Deserialize)]
pub struct ToggleReactionRequest {
    pub emoji: String,
}

pub async fn toggle_reaction(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
    Json(body): Json<ToggleReactionRequest>,
) -> AppResult<Json<CommentEntry>> {
    let entry = ops::toggle_comment_reaction(&store, &slug, id, &body.emoji).await?;
    Ok(Json(entry))
}
