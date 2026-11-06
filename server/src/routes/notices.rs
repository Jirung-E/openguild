//! DEV-478: 알림함 — HTTP 어댑터. 저장은 core(`notices`)에 있다.
//!
//! 이 서버에서 돈 플러그인의 알림과 실패다. 원격으로 붙은 앱 · CLI 가 여기서 읽는다.

use axum::{Json, extract::State};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use openguild_core::Store;
use openguild_core::notices::{self, Inbox};

pub async fn get(State(store): State<Store>) -> AppResult<Json<Inbox>> {
    Ok(Json(notices::inbox(&store.paths)?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeenBody {
    /// 여기까지 읽었다 — 알림의 `at` 그대로.
    pub at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DismissBody {
    /// 지울 알림의 `at` · `text` 그대로 — 둘이 같은 줄을 지운다.
    pub at: String,
    pub text: String,
}

pub async fn dismiss(
    State(store): State<Store>,
    Json(body): Json<DismissBody>,
) -> AppResult<Json<serde_json::Value>> {
    let removed = notices::dismiss(&store.paths, &body.at, &body.text)?;
    Ok(Json(serde_json::json!({ "ok": true, "removed": removed })))
}

pub async fn seen(
    State(store): State<Store>,
    Json(body): Json<SeenBody>,
) -> AppResult<Json<serde_json::Value>> {
    notices::mark_seen(&store.paths, &body.at)
        .map_err(|e| AppError::BadRequest(format!("{e:#}")))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
