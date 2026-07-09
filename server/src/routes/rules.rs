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
    /// DEV-243: 자유 태그. 파일 부재 시 빈 배열.
    #[serde(default)]
    pub tags: Vec<String>,
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
    let entry = ops::get_rule_entry(&store, &slug)?;
    Ok(Json(match entry {
        Some(e) => RuleResponse { slug: e.slug, content: Some(e.content), tags: e.tags },
        None => RuleResponse { slug, content: None, tags: vec![] },
    }))
}

pub async fn set_rule(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<RuleContentRequest>,
) -> AppResult<Json<RuleResponse>> {
    ops::set_rule(&store, &slug, body.content.clone()).await?;
    // BUG-134 패턴: 본문 저장은 tags 를 안 건드리지만(보존), 응답엔 실제
    // 현재 tags 를 정직하게 실어야 함 — 재조회.
    let tags = ops::get_rule_entry(&store, &slug)?.map(|e| e.tags).unwrap_or_default();
    Ok(Json(RuleResponse {
        slug,
        content: Some(body.content),
        tags,
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
            tags: vec![],
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
    let entry = ops::get_rule_entry(&store, &body.new_slug)?;
    Ok(Json(match entry {
        Some(e) => RuleResponse { slug: e.slug, content: Some(e.content), tags: e.tags },
        None => RuleResponse { slug: body.new_slug, content: None, tags: vec![] },
    }))
}

/// DEV-243: tag 전체 교체. body: `{ "tags": ["a", "b", ...] }`.
pub async fn set_tags(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<RuleResponse>> {
    let tags: Vec<String> = body
        .as_object()
        .and_then(|o| o.get("tags"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let entry = ops::set_rule_tags(&store, &slug, tags).await?;
    Ok(Json(RuleResponse {
        slug: entry.slug,
        content: Some(entry.content),
        tags: entry.tags,
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
