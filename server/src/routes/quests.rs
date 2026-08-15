//! HTTP 어댑터 — axum extractor → core::ops::quests (mutation) / core::services::quests (read) → JSON 응답.
//!
//! 비즈니스 로직 / SQL / 파일 IO / journal 은 전부 core 에 있다.
//! 이 파일은 입력 추출 + 출력 직렬화만 담당.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use crate::error::AppResult;
use openguild_core::models::{
    AddPrerequisiteRequest, CandidatesQuery, ChangeParentRequest, ChangeStatusRequest,
    CreateQuestRequest, DeleteQuestQuery, ListQuery, QuestDependency, QuestDetail,
    QuestHistoryEntry, QuestPosition, QuestRow, UpdatePositionRequest, UpdateQuestRequest,
};
use openguild_core::ops::quests as ops;
use openguild_core::services::quests as read;
use openguild_core::Store;

/// DEV-358: 목록 응답에 ETag + 조건부 요청.
///
/// 상세 → 목록 왕복마다 이 응답(퀘스트 531건 기준 **275KB 해제**)을 다시 받고
/// 클라이언트가 500여 개 객체로 다시 파싱했다. 상세 문서 자체는 13KB 뿐이라,
/// 목록을 오가는 사용 패턴에서는 이게 비용의 대부분이었다.
///
/// 클라이언트가 캐시 무효화를 스스로 관리하는 방식은 택하지 않았다 — **원격
/// 모드에서 다른 클라이언트가 바꾼 변경을 알 수 없어** "고쳤는데 목록에 안
/// 보인다" 가 된다. 대신 매 요청 서버가 신선도를 판정하고, 안 바뀌었으면
/// 304 + 빈 본문으로 끝낸다(전송 0, 클라이언트 파싱 0).
///
/// ETag 는 **직렬화된 응답 본문의 해시**다. 쿼리 조합(필터·정렬·slim)마다
/// 결과가 다르므로 본문에서 뽑는 게 가장 안전하다. 목록 SQL 은 어차피 돌므로
/// 추가 비용은 해시 한 번뿐이다.
pub async fn list_quests(
    State(store): State<Store>,
    Query(q): Query<ListQuery>,
    headers: axum::http::HeaderMap,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let rows = read::list(&store.index_pool, &q).await?;
    let body = serde_json::to_vec(&rows)
        .map_err(|e| openguild_core::AppError::Internal(anyhow::anyhow!(e)))?;

    // FNV-1a 64 — 의존성 없이 충분히 빠르고 충돌은 실사용상 무시 가능.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in &body {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    let etag = format!("\"{hash:x}\"");

    if let Some(inm) = headers.get(axum::http::header::IF_NONE_MATCH)
        && inm
            .to_str()
            .map(|v| v.split(',').any(|t| t.trim() == etag))
            .unwrap_or(false)
    {
        return Ok((
            axum::http::StatusCode::NOT_MODIFIED,
            [
                (axum::http::header::ETAG, etag),
                // 매번 서버에 물어보되, 안 바뀌었으면 본문을 안 받는다.
                (axum::http::header::CACHE_CONTROL, "no-cache".to_string()),
            ],
        )
            .into_response());
    }

    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/json".to_string(),
            ),
            (axum::http::header::ETAG, etag),
            (axum::http::header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        body,
    )
        .into_response())
}

pub async fn create_quest(
    State(store): State<Store>,
    Json(body): Json<CreateQuestRequest>,
) -> AppResult<(StatusCode, Json<QuestRow>)> {
    let quest = ops::create_quest(&store, body).await?;
    Ok((StatusCode::CREATED, Json(quest)))
}

pub async fn get_quest(
    State(store): State<Store>,
    Path(id): Path<i64>,
) -> AppResult<Json<QuestDetail>> {
    let mut detail = read::get(&store.index_pool, id).await?;
    // DEV-152: 첨부 목록(sidecar) — GUI Tauri 커맨드와 동일하게 여기서 채움.
    detail.attachments =
        openguild_core::ops::attachments::list_quest_attachments(&store, &detail.quest.quest_id);
    Ok(Json(detail))
}

pub async fn update_quest(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateQuestRequest>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(ops::update_quest(&store, id, body).await?))
}

pub async fn change_parent(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<ChangeParentRequest>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(ops::change_parent(&store, id, body).await?))
}

/// DEV-076: 희망 / 필수 기한 설정 / 해제.
///
/// JSON body: 키 존재 여부로 변경 의도 구분.
///   { "desired_due": "2026-06-15" }  → 설정
///   { "desired_due": null }          → 해제
///   {}                                → 변경 없음 (no-op)
/// 두 필드 동시 가능.
pub async fn set_due_dates(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<QuestRow>> {
    use serde_json::Value;
    fn parse_field(body: &Value, key: &str) -> Option<Option<String>> {
        // 키가 없으면 None (no-op). 있고 null 이면 Some(None) (해제).
        // 있고 string 이면 Some(Some(s)).
        let obj = body.as_object()?;
        let v = obj.get(key)?;
        Some(match v {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            _ => return None, // 타입 오류면 그냥 무시 (no-op) — 엄밀한 검증은 service.
        })
    }
    let desired = parse_field(&body, "desired_due");
    let required = parse_field(&body, "required_due");
    Ok(Json(ops::set_due_dates(&store, id, desired, required).await?))
}

/// DEV-068: tag 전체 교체. body: `{ "tags": ["a", "b", ...] }`.
/// 정규화 (trim + dedupe + 빈 제거) 는 service 위임. 인자 빈 vec = 전체 삭제.
pub async fn set_tags(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<QuestRow>> {
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
    Ok(Json(ops::set_quest_tags(&store, id, tags).await?))
}

pub async fn delete_quest(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Query(q): Query<DeleteQuestQuery>,
) -> AppResult<StatusCode> {
    let cascade_ids: Vec<i64> = q
        .cascade
        .as_deref()
        .map(|s| {
            s.split(',')
                .filter(|x| !x.trim().is_empty())
                .filter_map(|x| x.trim().parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_default();
    ops::delete_quest(&store, id, &cascade_ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_deleted_quests(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestRow>>> {
    Ok(Json(read::list_deleted(&store.index_pool).await?))
}

pub async fn restore_quest(
    State(store): State<Store>,
    Path(id): Path<i64>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(ops::restore_quest(&store, id).await?))
}

pub async fn change_status(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<ChangeStatusRequest>,
) -> AppResult<Json<QuestRow>> {
    Ok(Json(ops::change_status(&store, id, body).await?))
}

pub async fn add_prerequisite(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<AddPrerequisiteRequest>,
) -> AppResult<StatusCode> {
    ops::add_prerequisite(&store, id, body).await?;
    Ok(StatusCode::CREATED)
}

pub async fn remove_prerequisite(
    State(store): State<Store>,
    Path((id, prereq_id)): Path<(i64, i64)>,
) -> AppResult<StatusCode> {
    ops::remove_prerequisite(&store, id, prereq_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_candidates(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Query(q): Query<CandidatesQuery>,
) -> AppResult<Json<Vec<QuestRow>>> {
    Ok(Json(read::list_candidates(&store.index_pool, id, &q.relation).await?))
}

pub async fn update_position(
    State(store): State<Store>,
    Path(id): Path<i64>,
    Json(body): Json<UpdatePositionRequest>,
) -> AppResult<Json<QuestPosition>> {
    // update_position 은 UI 상태 — 파일 IO 없음. SQL 만 직접.
    Ok(Json(read::update_position(&store.index_pool, id, body).await?))
}

pub async fn get_quest_by_slug(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<QuestDetail>> {
    let mut detail = read::get_by_slug(&store.index_pool, &slug).await?;
    // frontmatter가 진리원이며 DB projection에는 순서 컬럼이 없다.
    detail.tags = openguild_core::ops::quests::list_quest_tags(&store, &slug)?;
    // DEV-152: 첨부 목록(sidecar) — GUI Tauri 커맨드와 동일하게 여기서 채움.
    detail.attachments = openguild_core::ops::attachments::list_quest_attachments(&store, &slug);
    Ok(Json(detail))
}

pub async fn list_positions(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestPosition>>> {
    Ok(Json(read::list_positions(&store.index_pool).await?))
}

pub async fn list_dependencies(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestDependency>>> {
    Ok(Json(read::list_dependencies(&store.index_pool).await?))
}

/// DEV-013: GET /api/quests/{id}/history
pub async fn list_history(
    State(store): State<Store>,
    Path(id): Path<i64>,
) -> AppResult<Json<Vec<QuestHistoryEntry>>> {
    Ok(Json(read::list_history(&store.index_pool, id).await?))
}
