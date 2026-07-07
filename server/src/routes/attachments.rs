//! DEV-152: 첨부 업로드 HTTP 어댑터 — remote(서버/브라우저) 모드.
//!
//! 이전엔 Tauri `invoke` 전용(GUI desktop)이라 브라우저 모드에서 첨부가
//! 불가했음. core::ops::attachments 의 함수들은 원래 Tauri 와 무관하므로
//! 여기서 그대로 재사용 — base64 JSON body 로 Tauri invoke 와 동일 시그니처를
//! 유지해 frontend `transport.ts` 의 routeToInvoke 매핑이 1:1 로 대응된다.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use base64::Engine as _;
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use openguild_core::models::QuestAttachment;
use openguild_core::ops::attachments as ops;
use openguild_core::Store;

#[derive(Debug, Deserialize)]
pub struct SaveAttachmentRequest {
    pub data_base64: String,
    pub ext: String,
}

/// `POST /api/attachments` — bytes(base64) 를 `.guild/attachments/` 에 저장.
/// 반환된 rel 경로를 quest/campaign 의 `attachments` endpoint 에 등록해야
/// 목록에 보인다(2단계 — Tauri 의 save_attachment + add_*_attachment 와 동일
/// 흐름). 응답을 순수 JSON 문자열로 둔 건(`Json<String>`, 객체로 안 감쌈)
/// frontend `transport.ts` 가 Tauri invoke(`Result<String,_>`)와 HTTP 응답을
/// 같은 타입(`string`)으로 다루기 위함 — 둘 중 어느 transport 를 타든 호출부가
/// 동일 코드로 처리 가능.
pub async fn save_attachment(
    State(store): State<Store>,
    Json(body): Json<SaveAttachmentRequest>,
) -> AppResult<Json<String>> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&body.data_base64)
        .map_err(|e| AppError::BadRequest(format!("base64 디코드 실패: {e}")))?;
    let rel = ops::save_attachment(&store, &bytes, &body.ext).await?;
    Ok(Json(rel))
}

#[derive(Debug, Deserialize)]
pub struct AddAttachmentRequest {
    pub path: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct AttachmentPathQuery {
    /// DELETE 는 body 를 안 쓰는 client(`api.delete`) 와 맞추기 위해 query string.
    pub path: String,
}

pub async fn add_quest_attachment(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<AddAttachmentRequest>,
) -> AppResult<Json<Vec<QuestAttachment>>> {
    Ok(Json(
        ops::add_quest_attachment(&store, &slug, &body.path, &body.name).await?,
    ))
}

pub async fn remove_quest_attachment(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Query(q): Query<AttachmentPathQuery>,
) -> AppResult<Json<Vec<QuestAttachment>>> {
    Ok(Json(
        ops::remove_quest_attachment(&store, &slug, &q.path).await?,
    ))
}

pub async fn add_campaign_attachment(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<AddAttachmentRequest>,
) -> AppResult<Json<Vec<QuestAttachment>>> {
    Ok(Json(
        ops::add_campaign_attachment(&store, &slug, &body.path, &body.name).await?,
    ))
}

pub async fn remove_campaign_attachment(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Query(q): Query<AttachmentPathQuery>,
) -> AppResult<Json<Vec<QuestAttachment>>> {
    Ok(Json(
        ops::remove_campaign_attachment(&store, &slug, &q.path).await?,
    ))
}

// DEV-237: 도서관 문서 첨부 — quest/campaign 과 동일 형식.
pub async fn add_book_attachment(
    State(store): State<Store>,
    Path(book_id): Path<String>,
    Json(body): Json<AddAttachmentRequest>,
) -> AppResult<Json<Vec<QuestAttachment>>> {
    Ok(Json(
        ops::add_book_attachment(&store, &book_id, &body.path, &body.name).await?,
    ))
}

pub async fn remove_book_attachment(
    State(store): State<Store>,
    Path(book_id): Path<String>,
    Query(q): Query<AttachmentPathQuery>,
) -> AppResult<Json<Vec<QuestAttachment>>> {
    Ok(Json(
        ops::remove_book_attachment(&store, &book_id, &q.path).await?,
    ))
}
