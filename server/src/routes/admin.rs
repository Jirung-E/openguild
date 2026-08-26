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
    /// 최근 op 개수 (기본 50). 실제 사용 전 [`JournalQuery::clamped_count`] 로
    /// 범위를 강제한다 — 그냥 쓰면 안 된다.
    #[serde(default = "default_journal_count")]
    pub count: i64,
}

/// REQ-005: journal count 상·하한.
///
/// SQLite 는 **음수 LIMIT 을 "제한 없음" 으로 해석**한다. 검증이 없던 시절
/// `?count=-1` 하나로 기본 50 캡을 우회해 journal 테이블 전체를 한 응답에
/// 쏟아낼 수 있었다(장수명 호스트에서 무제한).
const JOURNAL_COUNT_MIN: i64 = 1;
const JOURNAL_COUNT_MAX: i64 = 1000;

impl JournalQuery {
    /// 1..=1000 으로 클램프한 값. 0 이나 음수는 1 로, 과도한 값은 1000 으로.
    pub fn clamped_count(&self) -> i64 {
        self.count.clamp(JOURNAL_COUNT_MIN, JOURNAL_COUNT_MAX)
    }
}
fn default_journal_count() -> i64 {
    50
}

/// DEV-162: `GET /api/admin/journal?count=N` — journal.db(AOF) 최근 op.
pub async fn journal_tail(
    State(store): State<Store>,
    Query(q): Query<JournalQuery>,
) -> AppResult<Json<JournalTail>> {
    let tail = maintenance::journal_tail(&store.paths, q.clamped_count())
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
/// REQ-005: `Range: bytes=...` 파싱 결과.
#[derive(Debug, PartialEq, Eq)]
pub enum RangeSpec {
    /// 헤더 없음 / 우리가 다루지 않는 형식 → 전체를 보낸다.
    Whole,
    /// `start..=end` (양끝 포함, 파일 크기로 clamp 됨).
    Part { start: u64, end: u64 },
    /// 파일 범위를 벗어남 → 416.
    Unsatisfiable,
}

/// `Range` 헤더를 파싱한다. **단일 범위만** 지원한다 —
/// multipart/byteranges 는 응답 형식이 완전히 달라, 지원하지 않는 편이
/// 잘못 만드는 것보다 낫다(그 경우 전체를 보내면 클라이언트가 알아서 쓴다).
///
/// 지원 형식: `bytes=N-M` / `bytes=N-` / `bytes=-N`(마지막 N 바이트).
pub fn parse_range(header: Option<&str>, size: u64) -> RangeSpec {
    let Some(raw) = header else {
        return RangeSpec::Whole;
    };
    let Some(spec) = raw.trim().strip_prefix("bytes=") else {
        return RangeSpec::Whole;
    };
    // 쉼표가 있으면 다중 범위 — 지원하지 않는다.
    if spec.contains(',') {
        return RangeSpec::Whole;
    }
    let (a, b) = match spec.split_once('-') {
        Some(v) => v,
        None => return RangeSpec::Whole,
    };
    let (a, b) = (a.trim(), b.trim());
    if size == 0 {
        return RangeSpec::Unsatisfiable;
    }
    match (a.is_empty(), b.is_empty()) {
        // `bytes=-N` — 마지막 N 바이트.
        (true, false) => match b.parse::<u64>() {
            Ok(0) => RangeSpec::Unsatisfiable,
            Ok(n) => RangeSpec::Part {
                start: size.saturating_sub(n),
                end: size - 1,
            },
            Err(_) => RangeSpec::Whole,
        },
        // `bytes=N-`
        (false, true) => match a.parse::<u64>() {
            Ok(n) if n < size => RangeSpec::Part { start: n, end: size - 1 },
            Ok(_) => RangeSpec::Unsatisfiable,
            Err(_) => RangeSpec::Whole,
        },
        // `bytes=N-M`
        (false, false) => match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(n), Ok(m)) if n < size && n <= m => RangeSpec::Part {
                start: n,
                // 끝이 파일을 넘으면 잘라준다(스펙 허용).
                end: m.min(size - 1),
            },
            (Ok(_), Ok(_)) => RangeSpec::Unsatisfiable,
            _ => RangeSpec::Whole,
        },
        (true, true) => RangeSpec::Whole,
    }
}

pub async fn get_guild_file(
    State(store): State<Store>,
    Path(rel): Path<String>,
    headers: axum::http::HeaderMap,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;
    // REQ-002: 검증 로직을 core 로 옮겼다 — GUI 의 open/copy 경로가 여기와
    // 다른(약한) 가드를 쓰다가 절대경로 이스케이프가 났다. 한 구현만 남긴다.
    let rel = openguild_core::ops::attachments::validate_guild_rel(&rel)?;
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

    // REQ-005: 예전엔 `std::fs::read` 로 **파일 전체를 메모리에 올렸다.**
    // 압축 계층은 BUG-188 의 1.5GB 첨부를 의식해 octet-stream/zip/video/audio 를
    // 정성껏 제외하고 있는데, 정작 그 첨부를 서빙하는 이쪽이 통째로 버퍼링해
    // 동시 다운로드 몇 건이면 메모리가 고갈되고 async 핸들러 안의 동기 I/O 가
    // tokio 워커를 막았다. Range 도 없어 영상 탐색이 불가능했다.
    //
    // 이제 파일을 열어 **스트리밍**하고 Range 를 지원한다.
    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| openguild_core::error::AppError::NotFound(format!("파일 없음: {rel}")))?;
    let size = meta.len();
    let range = parse_range(
        headers.get(axum::http::header::RANGE).and_then(|v| v.to_str().ok()),
        size,
    );
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
    // 공통 헤더. `Accept-Ranges` 를 항상 알려야 클라이언트가 탐색을 시도한다.
    let base = [
        (axum::http::header::CONTENT_TYPE, mime.to_string()),
        (axum::http::header::CACHE_CONTROL, cache_control.to_string()),
        (axum::http::header::ETAG, etag),
        (axum::http::header::ACCEPT_RANGES, "bytes".to_string()),
    ];

    match range {
        RangeSpec::Unsatisfiable => Ok((
            axum::http::StatusCode::RANGE_NOT_SATISFIABLE,
            [(axum::http::header::CONTENT_RANGE, format!("bytes */{size}"))],
        )
            .into_response()),
        RangeSpec::Whole => {
            let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file));
            Ok((
                base,
                [(axum::http::header::CONTENT_LENGTH, size.to_string())],
                body,
            )
                .into_response())
        }
        RangeSpec::Part { start, end } => {
            use tokio::io::AsyncSeekExt;
            file.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|e| openguild_core::error::AppError::Internal(anyhow::anyhow!("seek: {e}")))?;
            let len = end - start + 1;
            let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(
                tokio::io::AsyncReadExt::take(file, len),
            ));
            Ok((
                axum::http::StatusCode::PARTIAL_CONTENT,
                base,
                [
                    (axum::http::header::CONTENT_LENGTH, len.to_string()),
                    (
                        axum::http::header::CONTENT_RANGE,
                        format!("bytes {start}-{end}/{size}"),
                    ),
                ],
                body,
            )
                .into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REQ-005: SQLite 는 **음수 LIMIT 을 "제한 없음"** 으로 해석한다.
    /// 검증이 없던 시절 `?count=-1` 하나로 기본 50 캡을 우회해 journal 전체를
    /// 한 응답에 쏟을 수 있었다.
    #[test]
    fn journal_count_is_clamped() {
        let q = |c: i64| JournalQuery { count: c }.clamped_count();
        assert_eq!(q(-1), 1, "음수는 무제한이 아니라 최소치로");
        assert_eq!(q(0), 1);
        assert_eq!(q(50), 50, "정상값은 그대로");
        assert_eq!(q(1000), 1000);
        assert_eq!(q(999_999), 1000, "과도한 값은 상한으로");
        assert_eq!(q(i64::MIN), 1);
        assert_eq!(q(i64::MAX), 1000);
    }

    // ── REQ-005: Range 파싱 ──

    #[test]
    fn range_absent_or_unknown_form_means_whole() {
        assert_eq!(parse_range(None, 100), RangeSpec::Whole);
        // 우리가 다루지 않는 단위 / 형식은 전체로 — 잘못 자르는 것보다 낫다.
        assert_eq!(parse_range(Some("items=0-10"), 100), RangeSpec::Whole);
        assert_eq!(parse_range(Some("bytes=abc-def"), 100), RangeSpec::Whole);
        assert_eq!(parse_range(Some("bytes=-"), 100), RangeSpec::Whole);
        // 다중 범위는 응답 형식이 완전히 달라 지원하지 않는다.
        assert_eq!(parse_range(Some("bytes=0-9,20-29"), 100), RangeSpec::Whole);
    }

    #[test]
    fn range_start_end() {
        assert_eq!(parse_range(Some("bytes=0-9"), 100), RangeSpec::Part { start: 0, end: 9 });
        assert_eq!(parse_range(Some("bytes=10-19"), 100), RangeSpec::Part { start: 10, end: 19 });
        // 끝이 파일을 넘으면 잘라준다(스펙 허용) — 416 이 아니다.
        assert_eq!(parse_range(Some("bytes=90-999"), 100), RangeSpec::Part { start: 90, end: 99 });
    }

    #[test]
    fn range_open_ended() {
        assert_eq!(parse_range(Some("bytes=50-"), 100), RangeSpec::Part { start: 50, end: 99 });
    }

    /// `bytes=-N` 은 시작이 아니라 **마지막 N 바이트**다 — 헷갈리기 쉬운 형식.
    #[test]
    fn range_suffix_means_last_n_bytes() {
        assert_eq!(parse_range(Some("bytes=-10"), 100), RangeSpec::Part { start: 90, end: 99 });
        // N 이 파일보다 크면 전체(0 부터).
        assert_eq!(parse_range(Some("bytes=-500"), 100), RangeSpec::Part { start: 0, end: 99 });
        assert_eq!(parse_range(Some("bytes=-0"), 100), RangeSpec::Unsatisfiable);
    }

    #[test]
    fn range_out_of_bounds_is_unsatisfiable() {
        assert_eq!(parse_range(Some("bytes=100-"), 100), RangeSpec::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=200-300"), 100), RangeSpec::Unsatisfiable);
        // 시작 > 끝.
        assert_eq!(parse_range(Some("bytes=50-10"), 100), RangeSpec::Unsatisfiable);
        // 빈 파일엔 어떤 범위도 성립하지 않는다.
        assert_eq!(parse_range(Some("bytes=0-0"), 0), RangeSpec::Unsatisfiable);
    }

    #[test]
    fn range_whitespace_tolerated() {
        assert_eq!(parse_range(Some(" bytes=0-9 "), 100), RangeSpec::Part { start: 0, end: 9 });
    }

    /// 기본값은 예전과 같아야 한다(회귀).
    #[test]
    fn journal_default_count_unchanged() {
        assert_eq!(default_journal_count(), 50);
    }
}
