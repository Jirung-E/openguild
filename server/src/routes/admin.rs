//! 관리자용 admin endpoints — 백업 / 복원 / drift.
//!
//! **인증 없음** (현 MVP). 멀티유저 단계 진입 시 토큰 / role 가드 필요.

use axum::{extract::{Path, Query, State}, Json};
use serde::{Deserialize, Serialize};
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
    /// 시점 복원 — 최신 snapshot 복원 후 journal 을 이 시각(ISO8601 UTC, 포함)
    /// 까지 replay. `to` 와 동시 지정 시 `at` 우선.
    #[serde(default)]
    pub at: Option<String>,
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

/// DEV-175: `DELETE /api/admin/snapshots/{ts}` — 특정 snapshot 삭제.
pub async fn delete_snapshot(
    State(store): State<Store>,
    Path(ts): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    snapshot::delete_snapshot(&store.paths, &ts)
        .map_err(openguild_core::AppError::Internal)?;
    Ok(Json(json!({ "ok": true, "deleted": ts })))
}

/// `POST /api/admin/restore` — 지정 snapshot 으로 복원.
pub async fn restore(
    State(store): State<Store>,
    Json(body): Json<RestoreRequest>,
) -> AppResult<Json<serde_json::Value>> {
    // DEV-022: 시점 복원 (journal replay) — 최신 snapshot 기준.
    if let Some(ts) = body.at {
        let snapshots = snapshot::list_snapshots(&store.paths)
            .map_err(openguild_core::AppError::Internal)?;
        let latest = snapshots.last().cloned().ok_or_else(|| {
            openguild_core::AppError::NotFound("사용 가능한 snapshot 이 없습니다".into())
        })?;
        let report = openguild_core::replay::replay_to(&store, &latest, &ts).await?;
        return Ok(Json(json!({
            "replayed_to": report.target_ts,
            "applied": report.applied,
            // DEV-212: restore 직전 자동 백업 스냅샷 ts (journal 비어있었으면 null).
            "pre_backup": report.pre_backup,
        })));
    }
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
        "positions_restored": report.positions_restored,
        "campaigns_loaded": report.campaigns_loaded,
        "comments_loaded": report.comments_loaded,
        "memos_loaded": report.memos_loaded,
        "tags_loaded": report.tags_loaded,
        "history_loaded": report.history_loaded,
        "history_exported": report.history_exported,
        "library_loaded": report.library_loaded,
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

#[derive(Debug, Default, Deserialize)]
pub struct CounterCheckRequest {
    #[serde(default)]
    pub fix: bool,
}

/// BUG-231: file/SQL counter 정합 검사 및 선택적 보정.
pub async fn check_counters(
    State(store): State<Store>,
    Json(body): Json<CounterCheckRequest>,
) -> AppResult<Json<openguild_core::ops::counter::CombinedReport>> {
    Ok(Json(
        openguild_core::ops::check_and_fix_counters(&store, body.fix).await?,
    ))
}

#[derive(Debug, Serialize)]
pub struct InfoResponse {
    pub path: std::path::PathBuf,
    pub guild: openguild_core::guild_file::GuildFile,
    pub summary: openguild_core::maintenance::IndexSummary,
    pub snapshots: Vec<SnapshotInfo>,
    pub journal_total: i64,
}

/// BUG-231: 로컬 `info`와 같은 호스트 길드/캐시/백업 요약.
pub async fn info(State(store): State<Store>) -> AppResult<Json<InfoResponse>> {
    let guild = openguild_core::guild_file::load(&store.paths.guild_root.to_string_lossy())?;
    let summary = maintenance::index_summary(&store).await?;
    let snapshots = snapshot::list_snapshots(&store.paths)?;
    let journal_total = maintenance::journal_tail(&store.paths, 0)
        .await?
        .map(|tail| tail.total)
        .unwrap_or(0);
    Ok(Json(InfoResponse {
        path: store.paths.guild_root.clone(),
        guild,
        summary,
        snapshots,
        journal_total,
    }))
}

// ─── DEV-069: 본문 첨부 / 자산 파일 서빙 (브라우저 모드) ───

/// `.guild/attachments/**` / `.guild/assets/**` 만 서빙. 그 외 prefix /
/// path traversal (`..`) 거부 — quests `.md` 등 내부 파일 노출 방지.
pub async fn get_guild_file(
    State(store): State<Store>,
    Path(rel): Path<String>,
    headers: axum::http::HeaderMap,
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
    let meta = std::fs::metadata(&path)
        .map_err(|_| openguild_core::error::AppError::NotFound(format!("파일 없음: {rel}")))?;

    // DEV-357: 캐시 검증자. 예전엔 cache-control / etag / last-modified 가 전부
    // 없어, 브라우저가 재사용도 조건부 요청도 못 하고 **볼 때마다 전부 다시
    // 받았다**(If-Modified-Since 를 붙여도 200 + 전체 본문). 첨부는 화면에
    // 뜰 때마다 통째로 재다운로드되고 있었다.
    //
    // ETag 는 (크기, mtime) 로 만든다 — 내용 해시는 대용량 첨부(BUG-168 의
    // 1.5GB)에서 매 요청 전체를 읽어야 해 쓸 수 없다. 파일이 바뀌면 둘 중
    // 하나는 바뀌므로 검증자로 충분하다.
    let mtime_secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let etag = format!("\"{:x}-{:x}\"", meta.len(), mtime_secs);

    // 조건부 요청 — 같으면 본문 없이 304. 대용량 첨부에서 특히 크게 아낀다.
    if let Some(inm) = headers.get(axum::http::header::IF_NONE_MATCH)
        && inm.to_str().map(|v| v.split(',').any(|t| t.trim() == etag)).unwrap_or(false)
    {
        return Ok((axum::http::StatusCode::NOT_MODIFIED, [(axum::http::header::ETAG, etag)])
            .into_response());
    }

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

    // DEV-357: 첨부는 업로드마다 고유 접미사가 붙어(BUG-219) 같은 URL 이 다른
    // 내용을 가리키지 않는다 — 사실상 불변이라 장기 캐시가 안전하다.
    // 반면 `assets/` 는 같은 이름으로 교체될 수 있으므로 매번 검증만 시킨다
    // (ETag 가 있으니 안 바뀌었으면 304 로 끝난다).
    let cache_control = if rel.starts_with("attachments/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, mime.to_string()),
            (axum::http::header::CACHE_CONTROL, cache_control.to_string()),
            (axum::http::header::ETAG, etag),
        ],
        bytes,
    )
        .into_response())
}
