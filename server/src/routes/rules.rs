//! DEV-016: 길드 규칙 (`.guild/rules.md`) 의 HTTP 어댑터.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use openguild_core::ops::rules as ops;
use openguild_core::Store;

#[derive(Debug, Serialize)]
pub struct RulesResponse {
    /// 규칙 본문. 파일 없으면 `null` (= 아직 규칙 미설정).
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
