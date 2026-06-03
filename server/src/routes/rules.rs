//! DEV-016 (multi-file): 길드 규칙 HTTP 어댑터.
//!
//! 다중 파일 — `.guild/rules/{slug}.md`. 기존 단일-파일 endpoint 도 backward
//! compat 으로 유지.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use openguild_core::ops::rules as ops;
use openguild_core::repo::rules::RuleEntry;
use openguild_core::Store;

// ─── multi-file ───

#[derive(Debug, Serialize)]
pub struct RulesListResponse {
    pub entries: Vec<RuleEntry>,
}

#[derive(Debug, Serialize)]
pub struct RuleResponse {
    pub slug: String,
    /// 파일 부재 시 null.
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RuleContentRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRuleRequest {
    pub slug: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct RenameRuleRequest {
    pub new_slug: String,
}

pub async fn list_rules(State(store): State<Store>) -> AppResult<Json<RulesListResponse>> {
    let entries = ops::list_rules(&store)?;
    Ok(Json(RulesListResponse { entries }))
}

pub async fn get_rule(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<RuleResponse>> {
    let content = ops::get_rule(&store, &slug)?;
    Ok(Json(RuleResponse { slug, content }))
}

pub async fn set_rule(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<RuleContentRequest>,
) -> AppResult<Json<RuleResponse>> {
    ops::set_rule(&store, &slug, body.content.clone()).await?;
    Ok(Json(RuleResponse {
        slug,
        content: Some(body.content),
    }))
}

pub async fn create_rule(
    State(store): State<Store>,
    Json(body): Json<CreateRuleRequest>,
) -> AppResult<(StatusCode, Json<RuleResponse>)> {
    ops::create_rule(&store, &body.slug, body.content.clone()).await?;
    Ok((
        StatusCode::CREATED,
        Json(RuleResponse {
            slug: body.slug,
            content: Some(body.content),
        }),
    ))
}

pub async fn delete_rule(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<StatusCode> {
    ops::delete_rule(&store, &slug).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rename_rule(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<RenameRuleRequest>,
) -> AppResult<Json<RuleResponse>> {
    ops::rename_rule(&store, &slug, &body.new_slug).await?;
    let content = ops::get_rule(&store, &body.new_slug)?;
    Ok(Json(RuleResponse {
        slug: body.new_slug,
        content,
    }))
}

// ─── (deprecated) 단일 파일 endpoint — backward compat ───

#[derive(Debug, Serialize)]
pub struct RulesResponse {
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetRulesRequest {
    pub content: String,
}

pub async fn get_rules(State(store): State<Store>) -> AppResult<Json<RulesResponse>> {
    let content = ops::get_rules(&store)?;
    Ok(Json(RulesResponse { content }))
}

pub async fn set_rules(
    State(store): State<Store>,
    Json(body): Json<SetRulesRequest>,
) -> AppResult<Json<RulesResponse>> {
    ops::set_rules(&store, body.content.clone()).await?;
    Ok(Json(RulesResponse {
        content: Some(body.content),
    }))
}
