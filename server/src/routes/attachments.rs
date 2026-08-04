//! DEV-152: 첨부 업로드 HTTP 어댑터 — remote(서버/브라우저) 모드.
//!
//! 이전엔 Tauri `invoke` 전용(GUI desktop)이라 브라우저 모드에서 첨부가
//! 불가했음. core::ops::attachments 의 함수들은 원래 Tauri 와 무관하므로
//! 여기서 그대로 재사용 — base64 JSON body 로 Tauri invoke 와 동일 시그니처를
//! 유지해 frontend `transport.ts` 의 routeToInvoke 매핑이 1:1 로 대응된다.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    Json,
};
use base64::Engine as _;
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use openguild_core::models::QuestAttachment;
use openguild_core::ops::attachments as ops;
use openguild_core::Store;

/// BUG-168: 이 라우트가 허용하는 **원본 파일** 최대 크기.
///
/// axum 기본 body limit 은 2 MiB 라, base64(4/3 팽창)를 감싸면 원본 1.5 MB
/// 정도에서 413 이 났다(사진 한 장에도 걸리는 수준 — 실측 경계 2,097,152 B).
/// bytes 를 받는 라우트는 이 라우트뿐이므로 여기에만 한도를 명시하고 나머지는
/// 기본값(2 MiB)을 유지한다.
///
/// 무제한으로 두지 않는 이유: 첨부는 `attachment_blobs` 로 index.db 에도
/// 복사되므로(DEV-284) 스냅샷 용량이 같이 커진다.
pub const MAX_ATTACHMENT_BYTES: usize = 64 * 1024 * 1024;

/// 위 원본 한도를 base64 로 감싼 JSON body 의 상한 — base64 는 3바이트를
/// 4바이트로 부풀리고, 여기에 JSON 래퍼(`{"data_base64":"…","ext":"…"}`)와
/// 여유를 더한다.
pub const MAX_ATTACHMENT_BODY_BYTES: usize = MAX_ATTACHMENT_BYTES / 3 * 4 + 64 * 1024;

/// DEV-337: 스트리밍 업로드 — `POST /api/attachments/stream`.
///
/// 위 base64 라우트의 64MB 상한은 **원래 근거가 사라진 값**이다(첨부를 index.db
/// 에 blob 으로 복사하던 것 — BUG-188 에서 제거). 남은 진짜 제약은 크기가
/// 아니라 "body 를 통째로 메모리에 올린다" 는 방식이었다.
///
/// 이 라우트는 body 를 청크 단위로 받아 파일로 바로 흘려쓴다. 메모리는 파일
/// 크기와 무관하게 상수고, base64 왕복이 없어 전송량도 25% 줄어든다. 그래서
/// 크기 상한을 두지 않는다(데스크톱 경로가 이미 그렇다 — BUG-168).
///
/// 파일명/확장자는 body 가 원문이라 쿼리로 받는다: `?ext=zip&name=foo.zip`.
///
/// 예전 base64 라우트는 그대로 남긴다 — 구버전 클라이언트 호환.
#[derive(Debug, Deserialize)]
pub struct StreamAttachmentQuery {
    pub ext: String,
    #[serde(default)]
    pub name: Option<String>,
}

pub async fn save_attachment_stream(
    State(store): State<Store>,
    Query(q): Query<StreamAttachmentQuery>,
    body: Body,
) -> AppResult<Json<String>> {
    use futures_util::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;

    let (rel, abs) = ops::new_attachment_dest(&store, &q.ext, q.name.as_deref()).await?;
    let mut file = tokio::fs::File::create(&abs)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("첨부 파일 생성 실패: {e}")))?;

    let mut stream = body.into_data_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        // 도중에 끊기면 **조각 파일을 남기지 않는다** — 아무도 참조하지 않는
        // 쓰레기가 reindex/스냅샷에 계속 끌려다닌다(DEV-323 과 같은 이유).
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tokio::fs::remove_file(&abs).await;
                return Err(AppError::Cancelled(format!("업로드 중단됨: {e}")).into());
            }
        };
        if let Err(e) = file.write_all(&chunk).await {
            let _ = tokio::fs::remove_file(&abs).await;
            return Err(AppError::Internal(anyhow::anyhow!("첨부 write 실패: {e}")).into());
        }
        written += chunk.len() as u64;
    }
    if let Err(e) = file.flush().await {
        let _ = tokio::fs::remove_file(&abs).await;
        return Err(AppError::Internal(anyhow::anyhow!("첨부 flush 실패: {e}")).into());
    }
    drop(file);
    if written == 0 {
        let _ = tokio::fs::remove_file(&abs).await;
        return Err(AppError::BadRequest(openguild_core::tf!("빈 첨부", "empty attachment")).into());
    }
    Ok(Json(rel))
}

#[derive(Debug, Deserialize)]
pub struct SaveAttachmentRequest {
    pub data_base64: String,
    pub ext: String,
    /// DEV-324: 원본 파일명 — 저장 파일명에 남겨 나중에 알아볼 수 있게.
    /// 예전 클라이언트는 안 보내므로 optional.
    #[serde(default)]
    pub name: Option<String>,
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
    // BUG-168: body limit(=base64 기준)만으로는 413 원문("Failed to buffer the
    // request body")이 그대로 노출된다. 원본 기준으로 한 번 더 확인해 한도를
    // 밝힌 메시지를 준다.
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        // handler 의 Err 타입은 HttpError — `?` 는 From 으로 자동 변환되지만
        // 명시적 return 은 변환이 없어 .into() 가 필요하다.
        return Err(AppError::BadRequest(openguild_core::tf!(
            "첨부 파일이 너무 큽니다 ({} MB) — 최대 {} MB",
            "attachment too large ({} MB) — maximum {} MB",
            bytes.len() / (1024 * 1024),
            MAX_ATTACHMENT_BYTES / (1024 * 1024)
        ))
        .into());
    }
    let rel = ops::save_attachment(&store, &bytes, &body.ext, body.name.as_deref()).await?;
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
