//! DEV-167: 작업 기록(Worklog) HTTP 어댑터 — `/api/worklog`.
//!
//! core `ops::worklog` 의 얇은 어댑터. GUI(Tauri invoke)와 1:1.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::error::AppResult;
use openguild_core::ops::worklog as ops;
use openguild_core::ops::worklog::WorklogReport;
use openguild_core::Store;

#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    pub from: String,
    pub to: String,
}

pub async fn get_activities(
    State(store): State<Store>,
    Query(q): Query<RangeQuery>,
) -> AppResult<Json<WorklogReport>> {
    Ok(Json(ops::activities(&store, &q.from, &q.to).await?))
}

#[derive(Debug, serde::Serialize)]
pub struct DailyCount {
    pub date: String,
    pub count: i64,
}

pub async fn get_summary(
    State(store): State<Store>,
    Query(q): Query<RangeQuery>,
) -> AppResult<Json<Vec<DailyCount>>> {
    let rows = ops::daily_summary(&store, &q.from, &q.to).await?;
    Ok(Json(
        rows.into_iter()
            .map(|(date, count)| DailyCount { date, count })
            .collect(),
    ))
}

#[derive(Debug, serde::Serialize)]
pub struct NoteResponse {
    pub date: String,
    /// 파일 부재 시 null.
    pub content: Option<String>,
}

pub async fn get_note(
    State(store): State<Store>,
    Path(date): Path<String>,
) -> AppResult<Json<NoteResponse>> {
    let content = ops::get_note(&store, &date)?;
    Ok(Json(NoteResponse { date, content }))
}

#[derive(Debug, Deserialize)]
pub struct SetNoteRequest {
    pub content: String,
}

pub async fn set_note(
    State(store): State<Store>,
    Path(date): Path<String>,
    Json(body): Json<SetNoteRequest>,
) -> AppResult<Json<NoteResponse>> {
    ops::set_note(&store, &date, body.content).await?;
    let content = ops::get_note(&store, &date)?;
    Ok(Json(NoteResponse { date, content }))
}

/// 기간 내 존재하는 노트 나열 (주/월 뷰).
pub async fn list_notes(
    State(store): State<Store>,
    Query(q): Query<RangeQuery>,
) -> AppResult<Json<Vec<NoteResponse>>> {
    let notes = ops::list_notes(&store, &q.from, &q.to)?;
    Ok(Json(
        notes
            .into_iter()
            .map(|(date, content)| NoteResponse { date, content: Some(content) })
            .collect(),
    ))
}
