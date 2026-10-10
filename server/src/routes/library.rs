//! DEV-216: 도서관(Library) HTTP 어댑터 — `/api/library`.
//!
//! core `ops::library` 의 얇은 어댑터. GUI(Tauri invoke)와 1:1 대응 —
//! DEV-193 교훈: GUI 가 쓰는 모든 invoke 에 대응 HTTP 필수.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::error::AppResult;
use openguild_core::models::QuestAttachment;
use openguild_core::ops::library as ops;
use openguild_core::ops::library::{LibraryDocRow, LibraryFolderRow};
use openguild_core::Store;

/// 응답에 book_id("BOOK-NNN")를 함께 실어 클라이언트가 번호 포맷을 몰라도 됨.
/// DEV-237: 첨부 목록도 함께 — quest/campaign 과 동일하게 단건 조회(get_book)
/// 에서만 채우고 list_books 는 빈 배열(payload 절약).
#[derive(Debug, serde::Serialize)]
pub struct BookResponse {
    pub book_id: String,
    #[serde(flatten)]
    pub row: LibraryDocRow,
    pub attachments: Vec<QuestAttachment>,
}

impl From<LibraryDocRow> for BookResponse {
    fn from(row: LibraryDocRow) -> Self {
        Self {
            book_id: row.book_id(),
            row,
            attachments: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateBookRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// DEV-239: 소속 폴더 ("" 또는 미지정 = 최상위).
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBookRequest {
    pub title: Option<String>,
    pub body: Option<String>,
    /// DEV-239: `Some("")` = 최상위로 이동, `None`/미지정 = 변경 없음.
    #[serde(default)]
    pub path: Option<String>,
    /// DEV-435: 편집을 시작할 때 본 본문 — 그사이 바뀐 본문과 합치거나 같은 줄이면 409.
    #[serde(default)]
    pub base_body: Option<String>,
    /// DEV-435: 편집을 시작할 때 본 파일 번호 — 다르면 409.
    #[serde(default)]
    pub base_version: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct FolderPathQuery {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    pub path: String,
}

/// DEV-397: 폴더 옮기기·이름 바꾸기. `to` 는 **새 전체 경로**다 — 부모가 달라지면 옮기기,
/// 마지막 조각이 달라지면 이름 바꾸기이고, 코어에서는 같은 연산이다.
#[derive(Debug, Deserialize)]
pub struct MoveFolderRequest {
    pub from: String,
    pub to: String,
}

/// BUG-281: `?folder=` 로 거른다. 없으면 전부.
///
/// 빈 문자열(`?folder=`)은 "최상위만" 이라 `Option` 으로는 못 가른다 —
/// `None`(파라미터 없음)과 `Some("")`(빈 값)이 다른 뜻이다.
#[derive(Deserialize)]
pub struct ListBooksQuery {
    folder: Option<String>,
}

pub async fn list_books(
    State(store): State<Store>,
    Query(q): Query<ListBooksQuery>,
) -> AppResult<Json<Vec<BookResponse>>> {
    // 거르기는 SQL 이 한다 — 전부 보내 놓고 클라이언트가 버리면 문서가 늘수록
    // 그대로 전송 비용이 된다.
    let rows = ops::list_books_in(&store, q.folder.as_deref()).await?;
    Ok(Json(rows.into_iter().map(BookResponse::from).collect()))
}

pub async fn get_book(
    State(store): State<Store>,
    Path(book_id): Path<String>,
) -> AppResult<Json<BookResponse>> {
    let row = ops::get_book(&store, &book_id).await?.ok_or_else(|| {
        openguild_core::error::AppError::NotFound(format!("book not found: {book_id}"))
    })?;
    let mut resp: BookResponse = row.into();
    resp.attachments = openguild_core::ops::attachments::list_book_attachments(&store, &book_id);
    Ok(Json(resp))
}

/// DEV-290: GET /api/library/{book_id}/history — BOOK 변경 이력(최신→과거).
pub async fn list_history(
    State(store): State<Store>,
    Path(book_id): Path<String>,
) -> AppResult<Json<Vec<openguild_core::repo::history::HistoryEntry>>> {
    Ok(Json(ops::history(&store, &book_id)?))
}

pub async fn create_book(
    State(store): State<Store>,
    Json(body): Json<CreateBookRequest>,
) -> AppResult<(StatusCode, Json<BookResponse>)> {
    let row = ops::create_book(&store, &body.title, &body.body, &body.path).await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

pub async fn update_book(
    State(store): State<Store>,
    Path(book_id): Path<String>,
    Json(body): Json<UpdateBookRequest>,
) -> AppResult<Json<BookResponse>> {
    let row = ops::update_book_with(
        &store,
        &book_id,
        body.title.as_deref(),
        body.body.as_deref(),
        body.path.as_deref(),
        openguild_core::ops::EditBase { text: body.base_body.clone(), version: body.base_version },
    )
    .await?;
    // BUG-124(admin 보고): 이전엔 list_books 처럼 attachments 를 빈 배열로
    // 두었음 — get_quest/update_quest(quests.rs) 는 단건 응답에 항상 채우는데
    // 여기만 예외였음. 클라이언트(library/+page.svelte)가 이 응답으로 book
    // 객체 전체를 교체하다 보니, 본문/제목/폴더 편집 후 저장할 때마다 기존
    // 첨부파일 목록이 화면에서 사라졌다(디스크의 sidecar 파일 자체는 안전 —
    // 표시만 깨짐, 새 첨부를 추가하면 그 응답이 최신 전체 목록을 돌려줘
    // 우연히 복구됐었음).
    let mut resp: BookResponse = row.into();
    resp.attachments = openguild_core::ops::attachments::list_book_attachments(&store, &book_id);
    Ok(Json(resp))
}

pub async fn delete_book(
    State(store): State<Store>,
    Path(book_id): Path<String>,
) -> AppResult<StatusCode> {
    ops::delete_book(&store, &book_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// DEV-243: tag 전체 교체. body: `{ "tags": ["a", "b", ...] }` — quest 의
/// set_tags 와 동일 패턴.
pub async fn set_tags(
    State(store): State<Store>,
    Path(book_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<BookResponse>> {
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
    let row = ops::set_book_tags(&store, &book_id, tags).await?;
    let mut resp: BookResponse = row.into();
    resp.attachments = openguild_core::ops::attachments::list_book_attachments(&store, &book_id);
    Ok(Json(resp))
}

/// BUG-287: 태그 붙이기·떼기. body: `{ "add": [...], "remove": [...] }`.
pub async fn edit_tags(
    State(store): State<Store>,
    Path(book_id): Path<String>,
    Json(edit): Json<openguild_core::ops::TagEdit>,
) -> AppResult<Json<BookResponse>> {
    let row = ops::edit_book_tags(&store, &book_id, edit).await?;
    let mut resp: BookResponse = row.into();
    resp.attachments = openguild_core::ops::attachments::list_book_attachments(&store, &book_id);
    Ok(Json(resp))
}

// ─── DEV-239: 폴더 ───

#[derive(Debug, serde::Serialize)]
pub struct FolderResponse {
    #[serde(flatten)]
    pub row: LibraryFolderRow,
}

impl From<LibraryFolderRow> for FolderResponse {
    fn from(row: LibraryFolderRow) -> Self {
        Self { row }
    }
}

pub async fn list_folders(State(store): State<Store>) -> AppResult<Json<Vec<FolderResponse>>> {
    let rows = ops::list_folders(&store).await?;
    Ok(Json(rows.into_iter().map(FolderResponse::from).collect()))
}

pub async fn create_folder(
    State(store): State<Store>,
    Json(body): Json<CreateFolderRequest>,
) -> AppResult<(StatusCode, Json<FolderResponse>)> {
    let row = ops::create_folder(&store, &body.path).await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

pub async fn move_folder(
    State(store): State<Store>,
    Json(body): Json<MoveFolderRequest>,
) -> AppResult<Json<Vec<FolderResponse>>> {
    let rows = ops::move_folder(&store, &body.from, &body.to).await?;
    Ok(Json(rows.into_iter().map(FolderResponse::from).collect()))
}

pub async fn delete_folder(
    State(store): State<Store>,
    Query(q): Query<FolderPathQuery>,
) -> AppResult<StatusCode> {
    ops::delete_folder(&store, &q.path).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── DEV-441: 도서관 내보내기 — 폴더 구조 그대로 zip ───────────────────
//
// [[REQ-028]] 의 내보내기는 파일을 **이 기계에** 쓰므로 데스크톱 로컬 길드에서만 됐다. 서버(원격 길드 · 웹 화면 ·
// 원격 CLI)에서도 꺼내려면 받는 쪽으로 보내야 하고, 여러 파일 · 폴더를 한 번에 보내는 방법은 zip 하나다.
//
// 펼치는 규칙은 로컬과 **같은 함수**(`ops::library_export::export`)를 쓴다 — 서버의 임시 폴더에 펼친 뒤 그
// 폴더를 그대로 담는다. 그래야 제목 → 파일 이름, 겹치는 이름의 ` (2)`, 첨부 폴더 같은 규칙이 두 곳으로
// 갈라지지 않는다. 다 보낸 뒤 임시 폴더는 지운다.

/// `GET /api/library/export.zip?folder=…` · `?id=BOOK-NNN` · `?pick=…&pick=…`
///
/// `pick` 은 화면이 쓰는 열쇠 그대로 — 폴더는 `folder:<경로>`, 문서는 문서 번호. 여러 번 줄 수 있다. 아무것도
/// 없으면 도서관 전체.
pub async fn export_zip(
    State(store): State<Store>,
    Query(q): Query<Vec<(String, String)>>,
) -> AppResult<axum::response::Response> {
    use openguild_core::ops::library_export::{Pick, export};
    let get = |k: &str| q.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    let mut picks: Vec<Pick> = q
        .iter()
        .filter(|(k, _)| k == "pick")
        .map(|(_, v)| match v.strip_prefix("folder:") {
            Some(f) => Pick::Folder(f.to_string()),
            None => Pick::Doc(v.clone()),
        })
        .collect();
    if picks.is_empty() {
        picks.push(match (get("id"), get("folder")) {
            (Some(i), _) => Pick::Doc(i),
            (None, Some(f)) => Pick::Folder(f),
            (None, None) => Pick::Folder(String::new()),
        });
    }

    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stage = std::env::temp_dir().join(format!(
        "openguild-library-export-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&stage).map_err(|e| {
        openguild_core::error::AppError::Internal(anyhow::anyhow!("임시 폴더를 만들지 못했습니다: {e}"))
    })?;
    let mut count = 0usize;
    for pick in &picks {
        match export(&store, pick, &stage).await {
            Ok(out) => count += out.len(),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&stage);
                return Err(e.into());
            }
        }
    }
    if count == 0 {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(openguild_core::error::AppError::NotFound(openguild_core::tf!(
            "내보낼 문서가 없습니다",
            "no documents to export"
        ))
        .into());
    }

    let mut entries = Vec::new();
    collect_files(&stage, &stage, &mut entries);
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    // 파일 이름: `[길드]도서관.zip`, 폴더 하나면 `[길드]<폴더 끝 조각>.zip`, 문서 하나면 그 번호.
    let guild = super::attachments::safe_name_part(&openguild_core::recents::guess_name(
        &store.paths.guild_root,
    ));
    let what = match picks.as_slice() {
        [Pick::Folder(f)] if !f.is_empty() => f.rsplit('/').next().unwrap_or(f).to_string(),
        [Pick::Doc(id)] => id.clone(),
        _ => "library".to_string(),
    };
    let name = format!("[{guild}]{}.zip", super::attachments::safe_name_part(&what));
    super::attachments::stream_zip(entries, name, Some(stage))
}

/// `dir` 아래 파일 전부를 `(root 기준 상대 경로, 절대 경로)` 로. 구분자는 언제나 `/` — zip 의 규칙이다.
fn collect_files(
    root: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<(String, std::path::PathBuf)>,
) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_files(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            let rel = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel, p));
        }
    }
}
