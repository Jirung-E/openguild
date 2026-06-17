//! 관리자용 admin endpoints — 백업 / 복원 / drift.
//!
//! **인증 없음** (현 MVP). 멀티유저 단계 진입 시 토큰 / role 가드 필요.

use axum::{extract::{Path, Query, State}, Json};
use serde::Deserialize;
use serde_json::json;

use crate::error::AppResult;
use openguild_core::maintenance::{self, JournalTail, VacuumReport};
use openguild_core::snapshot::{self, SnapshotInfo};
use openguild_core::{drift, reindex, Store};

#[derive(Debug, Deserialize)]
pub struct RestoreRequest {
    /// 특정 timestamp (`YYYYMMDD-HHMMSS`). 미지정 시 최신.
    #[serde(default)]
    pub to: Option<String>,
}

/// `POST /api/admin/snapshot` — 즉시 snapshot.
pub async fn create_snapshot(
    State(store): State<Store>,
) -> AppResult<Json<SnapshotInfo>> {
    let info = snapshot::create_snapshot(&store)
        .await
        .map_err(openguild_core::AppError::Internal)?;
    Ok(Json(info))
}

/// `GET /api/admin/snapshots` — 목록.
pub async fn list_snapshots(
    State(store): State<Store>,
) -> AppResult<Json<Vec<SnapshotInfo>>> {
    let list = snapshot::list_snapshots(&store.paths)
        .map_err(openguild_core::AppError::Internal)?;
    Ok(Json(list))
}

/// `POST /api/admin/restore` — 지정 snapshot 으로 복원.
pub async fn restore(
    State(store): State<Store>,
    Json(body): Json<RestoreRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let snapshots = snapshot::list_snapshots(&store.paths)
        .map_err(openguild_core::AppError::Internal)?;
    let target = if let Some(ts) = body.to {
        snapshots
            .iter()
            .find(|s| s.timestamp == ts)
            .cloned()
            .ok_or_else(|| openguild_core::AppError::NotFound(format!("snapshot {ts} 없음")))?
    } else {
        snapshots
            .last()
            .cloned()
            .ok_or_else(|| {
                openguild_core::AppError::NotFound("사용 가능한 snapshot 이 없습니다".into())
            })?
    };
    snapshot::restore_snapshot(&store, &target)
        .await
        .map_err(openguild_core::AppError::Internal)?;
    Ok(Json(json!({
        "restored_to": target.timestamp,
    })))
}

/// `GET /api/admin/drift` — drift 검사.
pub async fn check_drift(
    State(store): State<Store>,
) -> AppResult<Json<drift::DriftReport>> {
    let report = drift::detect_drift(&store)
        .await
        .map_err(openguild_core::AppError::Internal)?;
    Ok(Json(report))
}

/// `POST /api/admin/reindex` — 파일 → index.db 재구축.
pub async fn run_reindex(
    State(store): State<Store>,
) -> AppResult<Json<serde_json::Value>> {
    let report = reindex::reindex(&store).await?;
    Ok(Json(json!({
        "types_loaded": report.types_loaded,
        "statuses_loaded": report.statuses_loaded,
        "quests_loaded": report.quests_loaded,
        "dependencies_loaded": report.dependencies_loaded,
        "skipped": report.skipped.iter().map(|(p, r)| json!({ "path": p, "reason": r })).collect::<Vec<_>>(),
    })))
}

/// DEV-162: `POST /api/admin/vacuum` — index.db VACUUM (런타임 정비).
pub async fn vacuum(State(store): State<Store>) -> AppResult<Json<VacuumReport>> {
    let report = maintenance::vacuum(&store)
        .await
        .map_err(openguild_core::AppError::Internal)?;
    Ok(Json(report))
}

#[derive(Debug, Deserialize)]
pub struct JournalQuery {
    /// 최근 op 개수 (기본 50).
    #[serde(default = "default_journal_count")]
    pub count: i64,
}
fn default_journal_count() -> i64 {
    50
}

/// DEV-162: `GET /api/admin/journal?count=N` — journal.db(AOF) 최근 op.
pub async fn journal_tail(
    State(store): State<Store>,
    Query(q): Query<JournalQuery>,
) -> AppResult<Json<JournalTail>> {
    let tail = maintenance::journal_tail(&store.paths, q.count)
        .await
        .map_err(openguild_core::AppError::Internal)?
        .unwrap_or_default();
    Ok(Json(tail))
}

// ─── DEV-069: 본문 첨부 / 자산 파일 서빙 (브라우저 모드) ───

/// `.guild/attachments/**` / `.guild/assets/**` 만 서빙. 그 외 prefix /
/// path traversal (`..`) 거부 — quests `.md` 등 내부 파일 노출 방지.
pub async fn get_guild_file(
    State(store): State<Store>,
    Path(rel): Path<String>,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let rel = rel.replace('\\', "/");
    let allowed = rel.starts_with("attachments/") || rel.starts_with("assets/");
    if !allowed || rel.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        return Err(openguild_core::error::AppError::BadRequest(format!(
            "허용되지 않은 경로: {rel} (attachments/ 또는 assets/ 하위만)"
        ))
        .into());
    }
    let path = store.paths.dot_guild().join(&rel);
    let bytes = std::fs::read(&path).map_err(|_| {
        openguild_core::error::AppError::NotFound(format!("파일 없음: {rel}"))
    })?;
    let mime = match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    };
    Ok(([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response())
}
