//! core 도메인 에러. HTTP / GUI / CLI 인터페이스 무관.
//!
//! - server: `AppError` → `IntoResponse` 변환 (server/src/error.rs)
//! - cli: stderr / exit code 변환
//! - desktop: invoke 의 `Result` 로 변환

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("internal error: {0:#}")]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format() {
        assert_eq!(AppError::NotFound("X".into()).to_string(), "X");
        assert_eq!(AppError::BadRequest("Y".into()).to_string(), "Y");
    }

    #[test]
    fn from_sqlx_error() {
        let sqlx_err = sqlx::Error::RowNotFound;
        let app: AppError = sqlx_err.into();
        // RowNotFound 는 internal 로 매핑됨 (현재 단순 구현)
        assert!(matches!(app, AppError::Internal(_)));
    }

    #[test]
    fn from_io_error() {
        let io = std::io::Error::other("boom");
        let app: AppError = io.into();
        assert!(matches!(app, AppError::Internal(_)));
    }

    #[test]
    fn from_anyhow_error() {
        let any = anyhow::anyhow!("custom");
        let app: AppError = any.into();
        assert!(matches!(app, AppError::Internal(_)));
        assert!(app.to_string().contains("custom"));
    }
}
