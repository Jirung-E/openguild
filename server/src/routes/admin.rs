//! 관리자용 admin endpoints — 백업 / 복원 / drift.
//!
//! **인증 없음** (현 MVP). 멀티유저 단계 진입 시 토큰 / role 가드 필요.

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::json;

use crate::error::AppResult;
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
