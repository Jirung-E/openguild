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
/// **이 상한은 이 라우트(base64 JSON)에만 남아 있다.** 원래 근거였던 "첨부가
/// index.db 에 blob 으로 복사돼 스냅샷이 커진다"(DEV-284)는 BUG-188 에서 blob
/// 백업을 없애며 사라졌다. 지금 남은 제약은 크기가 아니라 **body 를 통째로
/// 버퍼링하는 방식**이다(base64 문자열 + 디코드 버퍼로 피크에 파일 크기의 2배
/// 이상). 크기 제한 없는 업로드는 DEV-337 의 `POST /api/attachments/stream` —
/// 이 라우트는 구버전 클라이언트 호환으로 유지한다.
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

// ─── BUG-241: 첨부 일괄 다운로드 (zip 스트리밍) ───────────────────────────
//
// 브라우저 모드의 '전체 다운로드' 는 첨부마다 `<a download>` 를 클릭했는데,
// 브라우저는 제스처와 붙어 있는 다운로드만 허용하므로 첫 파일만 저장됐다.
//
// 데스크톱/보안 컨텍스트에서는 프론트가 `showDirectoryPicker` 로 폴더에 직접
// 써서 압축 자체가 필요 없다. 하지만 **폰에서 `http://<LAN IP>` 로 접속하면**
// 그 API 가 아예 없다 — 평문 HTTP 라 보안 컨텍스트가 아니고, 모바일 브라우저는
// File System Access 를 지원하지도 않는다. 그 경로에서 여러 파일을 받게 하는
// 방법은 **다운로드를 1건으로 만드는 것** 뿐이라 zip 으로 묶는다.
//
// 두 가지를 지킨다:
//   1. **스트리밍** — 전체를 메모리에 올리지 않는다. BUG-188 의 1.5GB 첨부가
//      있고, 폰은 메모리가 더 빠듯하다.
//   2. **무압축(store)** — 첨부는 이미 png/zip/mp4 같은 압축 포맷이 대부분이라
//      재압축은 CPU 만 쓰고 크기는 거의 그대로다.

use async_zip::{tokio::write::ZipFileWriter, Compression, ZipEntryBuilder};
use axum::http::header;

/// 한 문서의 첨부 전체를 zip 으로 스트리밍한다.
async fn stream_attachments_zip(
    store: Store,
    items: Vec<openguild_core::models::QuestAttachment>,
    download_name: String,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;

    if items.is_empty() {
        return Err(openguild_core::error::AppError::NotFound("첨부 없음".into()).into());
    }

    // duplex 로 writer↔reader 를 잇는다. zip 을 쓰는 쪽은 별도 task 에서 돌고,
    // 응답 본문은 reader 를 그대로 흘려보낸다 — 어느 쪽도 전체를 들고 있지 않다.
    let (w, r) = tokio::io::duplex(64 * 1024);

    tokio::spawn(async move {
        let mut zip = ZipFileWriter::with_tokio(w);
        for a in items {
            // REQ-002 와 같은 allowlist 검증 — zip 경로로 우회되면 안 된다.
            let Ok(rel) = openguild_core::ops::attachments::validate_guild_rel(&a.path) else {
                continue;
            };
            let path = store.paths.dot_guild().join(&rel);
            let Ok(mut f) = tokio::fs::File::open(&path).await else {
                continue; // 사이드카엔 있으나 파일이 사라진 경우 — 건너뛴다.
            };
            let entry = ZipEntryBuilder::new(a.name.clone().into(), Compression::Stored);
            let Ok(mut ew) = zip.write_entry_stream(entry).await else {
                break;
            };
            // `EntryStreamWriter` 는 futures 쪽 AsyncWrite 라, tokio File 을
            // compat 으로 감싸 futures 계열 copy 로 흘린다.
            use tokio_util::compat::TokioAsyncReadCompatExt;
            let mut src = (&mut f).compat();
            if futures_util::io::copy(&mut src, &mut ew).await.is_err() {
                break;
            }
            if ew.close().await.is_err() {
                break;
            }
        }
        // 실패해도 여기서 할 수 있는 건 스트림을 닫는 것뿐이다. 클라이언트는
        // 잘린 zip 을 받고 압축 해제 시 알게 된다.
        let _ = zip.close().await;
    });

    let body = Body::from_stream(tokio_util::io::ReaderStream::new(r));
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, content_disposition(&download_name)),
        ],
        body,
    )
        .into_response())
}

/// 파일명 조각을 파일 시스템에 안전하게 만든다 — 경로 구분자와 예약 문자를 `_` 로.
///
/// 길드 이름은 사용자가 정하므로 공백·슬래시·따옴표가 들어올 수 있다. 한글 등
/// 비ASCII 는 **그대로 둔다**(아래 RFC 5987 로 전달되고, 파일명으로도 정상이다).
fn safe_name_part(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() { "guild".to_string() } else { trimmed.to_string() }
}

/// `Content-Disposition` 값. 한글 길드/문서 이름을 위해 **RFC 5987** 형식을 함께
/// 낸다 — `filename=` 만 쓰면 비ASCII 가 브라우저마다 다르게 깨진다. 구형
/// 클라이언트를 위해 ASCII 로 접은 이름을 `filename=` 에 남긴다.
fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '[' | ']') { c } else { '_' })
        .collect();
    let encoded: String = name
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

/// BUG-241 후속: zip 파일명은 **길드 이름을 포함**한다. 여러 길드에서 받은
/// 첨부 묶음이 다운로드 폴더에 섞이면 `DEV-007-attachments.zip` 만으로는 어느
/// 길드 것인지 알 수 없다.
fn zip_download_name(store: &Store, doc_id: &str) -> String {
    let guild = safe_name_part(&openguild_core::recents::guess_name(&store.paths.guild_root));
    let doc = safe_name_part(doc_id);
    format!("[{guild}]_{doc}_attachments.zip")
}

/// `GET /api/quests/by/{slug}/attachments.zip`
pub async fn quest_attachments_zip(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<axum::response::Response> {
    let items = ops::list_quest_attachments(&store, &slug);
    let name = zip_download_name(&store, &slug);
    stream_attachments_zip(store.clone(), items, name).await
}

/// `GET /api/campaigns/{slug}/attachments.zip`
pub async fn campaign_attachments_zip(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<axum::response::Response> {
    let items = ops::list_campaign_attachments(&store, &slug);
    let name = zip_download_name(&store, &slug);
    stream_attachments_zip(store.clone(), items, name).await
}

/// `GET /api/library/{book_id}/attachments.zip`
pub async fn book_attachments_zip(
    State(store): State<Store>,
    Path(book_id): Path<String>,
) -> AppResult<axum::response::Response> {
    let items = ops::list_book_attachments(&store, &book_id);
    let name = zip_download_name(&store, &book_id);
    stream_attachments_zip(store.clone(), items, name).await
}
