//! DEV-216: 도서관(Library) HTTP 어댑터 — `/api/library`.
//!
//! core `ops::library` 의 얇은 어댑터. GUI(Tauri invoke)와 1:1 대응 —
//! DEV-193 교훈: GUI 가 쓰는 모든 invoke 에 대응 HTTP 필수.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::error::AppResult;
use openguild_core::ops::library as ops;
use openguild_core::ops::library::LibraryDocRow;
use openguild_core::Store;

/// 응답에 book_id("BOOK-NNN")를 함께 실어 클라이언트가 번호 포맷을 몰라도 됨.
#[derive(Debug, serde::Serialize)]
pub struct BookResponse {
    pub book_id: String,
    #[serde(flatten)]
    pub row: LibraryDocRow,
}

impl From<LibraryDocRow> for BookResponse {
    fn from(row: LibraryDocRow) -> Self {
        Self {
            book_id: row.book_id(),
            row,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateBookRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBookRequest {
    pub title: Option<String>,
    pub body: Option<String>,
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
    Ok(Json(row.into()))
}

pub async fn create_book(
    State(store): State<Store>,
    Json(body): Json<CreateBookRequest>,
) -> AppResult<(StatusCode, Json<BookResponse>)> {
    let row = ops::create_book(&store, &body.title, &body.body).await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

pub async fn update_book(
    State(store): State<Store>,
    Path(book_id): Path<String>,
    Json(body): Json<UpdateBookRequest>,
) -> AppResult<Json<BookResponse>> {
    let row =
        ops::update_book(&store, &book_id, body.title.as_deref(), body.body.as_deref()).await?;
    Ok(Json(row.into()))
}

pub async fn delete_book(
    State(store): State<Store>,
    Path(book_id): Path<String>,
) -> AppResult<StatusCode> {
    ops::delete_book(&store, &book_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
