//! DEV-016 (multi-file): 길드 규칙 HTTP 어댑터.
//!
//! 다중 파일 — `.guild/rules/{slug}.md`. 기존 단일-파일 endpoint 도 backward
//! compat 으로 유지.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use openguild_core::Store;
use openguild_core::ops::rules as ops;
use openguild_core::repo::rules::RuleEntry;

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
    /// DEV-182: 생성 / 마지막 본문 저장 시각. 파일 부재 시 빈 문자열.
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// DEV-435: 파일 번호 — `base_version` 에 돌려줄 값. 파일 부재 시 0.
    pub version: u64,
}

#[derive(Debug, Deserialize)]
pub struct RuleContentRequest {
    pub content: String,
    /// DEV-435: 편집을 시작할 때 본 본문 — 그사이 바뀐 본문과 합치거나 같은 줄이면 409.
    #[serde(default)]
    pub base_content: Option<String>,
    /// DEV-435: 편집을 시작할 때 본 파일 번호 — 다르면 409.
    #[serde(default)]
    pub base_version: Option<u64>,
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
    Ok(Json(read_response(&store, slug)?))
}

/// 파일에서 다시 읽은 응답. 파일 부재 시 `content: null`.
///
/// DEV-435: 쓴 뒤에도 보낸 값이 아니라 파일을 돌려준다 — 저장은 그사이 바뀐 본문과 합칠 수 있어, 보낸 본문이 곧 저장된
/// 본문이 아니다. 번호도 여기서 싣는다.
fn read_response(store: &Store, slug: String) -> AppResult<RuleResponse> {
    let version = ops::rule_version(store, &slug)?;
    Ok(match ops::get_rule_entry(store, &slug)? {
        Some(e) => RuleResponse {
            slug: e.slug,
            content: Some(e.content),
            tags: e.tags,
            created_at: e.created_at,
            updated_at: e.updated_at,
            version,
        },
        None => RuleResponse {
            slug,
            content: None,
            tags: vec![],
            created_at: String::new(),
            updated_at: String::new(),
            version,
        },
    })
}

/// DEV-290: GET /api/rules/{slug}/history — 규칙 변경 이력(최신→과거).
pub async fn list_history(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<Vec<openguild_core::repo::history::HistoryEntry>>> {
    Ok(Json(ops::history(&store, &slug)?))
}

pub async fn set_rule(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<RuleContentRequest>,
) -> AppResult<Json<RuleResponse>> {
    ops::set_rule_with(
        &store,
        &slug,
        body.content.clone(),
        openguild_core::ops::EditBase {
            text: body.base_content.clone(),
            version: body.base_version,
        },
    )
    .await?;
    // BUG-134 패턴: 본문 저장은 tags 를 안 건드리지만(보존), 응답엔 실제
    // 현재 tags/시각을 정직하게 실어야 함 — 재조회.
    Ok(Json(read_response(&store, slug)?))
}

pub async fn create_rule(
    State(store): State<Store>,
    Json(body): Json<CreateRuleRequest>,
) -> AppResult<(StatusCode, Json<RuleResponse>)> {
    ops::create_rule(&store, &body.slug, body.content.clone()).await?;
    Ok((StatusCode::CREATED, Json(read_response(&store, body.slug)?)))
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
    Ok(Json(read_response(&store, body.new_slug)?))
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
    ops::set_rule_tags(&store, &slug, tags).await?;
    Ok(Json(read_response(&store, slug)?))
}

/// BUG-287: 태그 붙이기·떼기. body: `{ "add": [...], "remove": [...] }`.
pub async fn edit_tags(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(edit): Json<openguild_core::ops::TagEdit>,
) -> AppResult<Json<RuleResponse>> {
    ops::edit_rule_tags(&store, &slug, edit).await?;
    Ok(Json(read_response(&store, slug)?))
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
