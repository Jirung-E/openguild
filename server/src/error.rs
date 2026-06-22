//! core::AppError 를 axum 의 IntoResponse 로 변환하는 wrapper.
//! orphan rule 우회를 위한 newtype.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub use openguild_core::AppError;

/// handler 의 Err 타입. core::AppError 또는 Into<AppError> 인 어떤 에러든 자동 변환.
pub struct HttpError(pub AppError);

pub type AppResult<T> = Result<T, HttpError>;

impl<E: Into<AppError>> From<E> for HttpError {
    fn from(err: E) -> Self {
        HttpError(err.into())
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, message) = match self.0 {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            // DEV-064: 길드가 서버보다 새 schema — 서버 업데이트 필요.
            AppError::IncompatibleGuild(msg) => (StatusCode::CONFLICT, msg),
            AppError::Internal(err) => {
                tracing::error!("internal error: {err:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn body_text(resp: Response) -> (StatusCode, String) {
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn not_found_maps_to_404_with_message() {
        let resp = HttpError(AppError::NotFound("quest 42 not found".into())).into_response();
        let (status, body) = body_text(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("quest 42 not found"));
        assert!(body.starts_with("{") && body.contains("\"error\""));
    }

    #[tokio::test]
    async fn bad_request_maps_to_400() {
        let resp = HttpError(AppError::BadRequest("urgency must be 1..=4".into())).into_response();
        let (status, body) = body_text(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("urgency"));
    }

    #[tokio::test]
    async fn internal_maps_to_500_and_hides_detail() {
        let inner = anyhow::anyhow!("DB connection refused at 127.0.0.1");
        let resp = HttpError(AppError::Internal(inner)).into_response();
        let (status, body) = body_text(resp).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        // 내부 디테일은 클라이언트에 노출 X
        assert!(!body.contains("127.0.0.1"));
        assert!(body.contains("internal server error"));
    }

    #[test]
    fn from_appshorthand_compiles_for_app_error_and_io_and_sqlx() {
        // 컴파일만 통과해도 의도 검증
        let _: HttpError = AppError::NotFound("x".into()).into();
        let io_err = std::io::Error::other("boom");
        let _: HttpError = io_err.into();
        let sqlx_err = sqlx::Error::RowNotFound;
        let _: HttpError = sqlx_err.into();
    }
}
