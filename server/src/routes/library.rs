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
}

#[derive(Debug, Deserialize)]
pub struct FolderPathQuery {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    pub path: String,
}

pub async fn list_books(State(store): State<Store>) -> AppResult<Json<Vec<BookResponse>>> {
    let rows = ops::list_books(&store).await?;
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
    let row = ops::update_book(
        &store,
        &book_id,
        body.title.as_deref(),
        body.body.as_deref(),
        body.path.as_deref(),
    )
    .await?;
    Ok(Json(row.into()))
}

pub async fn delete_book(
    State(store): State<Store>,
    Path(book_id): Path<String>,
) -> AppResult<StatusCode> {
    ops::delete_book(&store, &book_id).await?;
    Ok(StatusCode::NO_CONTENT)
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

pub async fn delete_folder(
    State(store): State<Store>,
    Query(q): Query<FolderPathQuery>,
) -> AppResult<StatusCode> {
    ops::delete_folder(&store, &q.path).await?;
    Ok(StatusCode::NO_CONTENT)
}
