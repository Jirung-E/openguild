//! Campaign HTTP routes (DEV-011).
//!
//! 패턴은 `quests.rs` 와 동일 — axum extractor → core::ops / services → JSON.
//! 입력 추출 + 응답 직렬화만, 비즈니스 로직은 전부 core 가짐.
//!
//! Slug-first endpoint — `/api/campaigns/{slug}` 가 권장. id 노출 안 함
//! (CLI / GUI 가 slug 기반으로 호출).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use openguild_core::models::{
    AddChecklistRequest, CampaignChecklistItem, CampaignDetail, CampaignRow,
    CampaignSummary, CreateCampaignRequest, LinkQuestRequest, UpdateCampaignRequest,
    UpdateChecklistRequest,
};
use openguild_core::ops::campaigns as ops;
use openguild_core::services::campaigns as svc;
use openguild_core::Store;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// "active" | "done" 미지정 시 전체 alive.
    pub status: Option<String>,
}

pub async fn list_campaigns(
    State(store): State<Store>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<CampaignRow>>> {
    let rows = match q.status.as_deref() {
        Some(s) => {
            if s != "active" && s != "done" {
                return Err(AppError::BadRequest(format!(
                    "invalid status '{s}' (expected 'active' or 'done')"
                ))
                .into());
            }
            svc::list_by_status(&store.index_pool, s).await?
        }
        None => svc::list_alive(&store.index_pool).await?,
    };
    Ok(Json(rows))
}

pub async fn create_campaign(
    State(store): State<Store>,
    Json(body): Json<CreateCampaignRequest>,
) -> AppResult<(StatusCode, Json<CampaignRow>)> {
    let row = ops::create_campaign(&store, body).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn get_campaign(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<CampaignDetail>> {
    Ok(Json(ops::fetch_detail(&store, &slug).await?))
}

pub async fn update_campaign(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<UpdateCampaignRequest>,
) -> AppResult<Json<CampaignRow>> {
    let row = svc::fetch_by_slug(&store.index_pool, &slug).await?;
    Ok(Json(ops::update_campaign(&store, row.id, body).await?))
}

pub async fn delete_campaign(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<StatusCode> {
    let row = svc::fetch_by_slug(&store.index_pool, &slug).await?;
    ops::delete_campaign(&store, row.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Quest link ──────────────────────────────────────

pub async fn link_quest(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<LinkQuestRequest>,
) -> AppResult<StatusCode> {
    let row = svc::fetch_by_slug(&store.index_pool, &slug).await?;
    ops::link_quest_by_slug(&store, row.id, &body.quest_slug).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unlink_quest(
    State(store): State<Store>,
    Path((slug, quest_slug)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    let row = svc::fetch_by_slug(&store.index_pool, &slug).await?;
    ops::unlink_quest_by_slug(&store, row.id, &quest_slug).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Checklist ───────────────────────────────────────

pub async fn add_checklist(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<AddChecklistRequest>,
) -> AppResult<(StatusCode, Json<CampaignChecklistItem>)> {
    let row = svc::fetch_by_slug(&store.index_pool, &slug).await?;
    let item = ops::add_checklist_line(&store, row.id, &body.text).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

/// PATCH 의 body 는 `{ "checked": true | false }`. 인덱스는 path.
pub async fn set_checklist(
    State(store): State<Store>,
    Path((slug, index)): Path<(String, usize)>,
    Json(body): Json<UpdateChecklistRequest>,
) -> AppResult<StatusCode> {
    let row = svc::fetch_by_slug(&store.index_pool, &slug).await?;
    let Some(checked) = body.checked else {
        return Err(AppError::BadRequest(
            "PATCH checklist 본문에 `checked: bool` 필요".into(),
        )
        .into());
    };
    ops::set_checklist_checked_by_index(&store, row.id, index, checked).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_checklist(
    State(store): State<Store>,
    Path((slug, index)): Path<(String, usize)>,
) -> AppResult<StatusCode> {
    let row = svc::fetch_by_slug(&store.index_pool, &slug).await?;
    ops::remove_checklist_by_index(&store, row.id, index).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Home summaries (DEV-011 GUI 용) ─────────────────

#[derive(Debug, Deserialize)]
pub struct UpcomingQuery {
    /// 향후 N 일 이내 시작하는 캠페인. 기본 7일.
    pub days: Option<i64>,
}

pub async fn list_active_summaries(
    State(store): State<Store>,
) -> AppResult<Json<Vec<CampaignSummary>>> {
    Ok(Json(svc::list_active_summaries(&store.index_pool).await?))
}

pub async fn list_upcoming_summaries(
    State(store): State<Store>,
    Query(q): Query<UpcomingQuery>,
) -> AppResult<Json<Vec<CampaignSummary>>> {
    let today = openguild_core::time::today_local_iso_date();
    let days = q.days.unwrap_or(7);
    Ok(Json(
        svc::list_upcoming_summaries(&store.index_pool, &today, days).await?,
    ))
}

/// Quest 가 속한 모든 캠페인. Quest Detail 의 Campaign 섹션 표시용.
/// id 는 quest 의 numeric id (DEV-049: slug 가 stable identifier 지만 quest
/// routes 가 이미 id 기반이라 일관 유지).
pub async fn list_for_quest(
    State(store): State<Store>,
    Path(quest_id): Path<i64>,
) -> AppResult<Json<Vec<CampaignRow>>> {
    Ok(Json(svc::list_for_quest(&store.index_pool, quest_id).await?))
}

/// DEV-087: 배너 이미지 bytes 서빙 — HTTP / 브라우저 모드 표시용.
/// Tauri 모드는 asset protocol (convertFileSrc) 사용이라 이 endpoint 안 탐.
pub async fn get_banner_image(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let row = svc::fetch_by_slug(&store.index_pool, &slug).await?;
    let Some(rel) = row.image_path else {
        return Err(openguild_core::error::AppError::NotFound(format!(
            "campaign {slug} 에 배너 없음"
        ))
        .into());
    };
    let path = store.paths.dot_guild().join(&rel);
    let bytes = std::fs::read(&path).map_err(|e| {
        openguild_core::error::AppError::Internal(anyhow::anyhow!(
            "배너 파일 읽기 실패 {}: {e}",
            path.display()
        ))
    })?;
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    };
    Ok(([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response())
}
