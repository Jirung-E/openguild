//! core 도메인 에러. HTTP / GUI / CLI 인터페이스 무관.
//!
//! - server: `AppError` → `IntoResponse` 변환 (server/src/error.rs)
//! - cli: stderr / exit code 변환
//! - desktop: invoke 의 `Result` 로 변환

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

/// DEV-435: 편집 충돌의 내용 — 밖으로 나가는 모양(앱 · 서버 · CLI 가 같은 것을 본다).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EditConflict {
    /// `text`(같은 줄을 다르게 고침) · `stale`(시작한 뒤 바뀜 — 번호로만 알 때) · `renamed`(번호가 바뀜) · `deleted`.
    pub reason: &'static str,
    /// 사람에게 보일 한 줄.
    pub message: String,
    /// 지금 저장된 본문 — 다시 시작할 출발점. `renamed` · `deleted` 에는 없다.
    pub current: Option<String>,
    /// `text` 일 때 구간들 — 이으면 전체 글(충돌 구간은 고른 쪽으로).
    pub segments: Vec<crate::merge::Segment>,
    /// `renamed` 일 때 새 번호.
    pub new_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    /// DEV-064: 길드가 이 실행파일보다 새 schema 로 만들어져 못 엶. 앱 업데이트 필요.
    #[error("{0}")]
    IncompatibleGuild(String),

    /// DEV-323: 사용자가 중단시킨 작업(첨부 업로드 취소 등). 실패가 아니라
    /// **의도된 중단**이라 호출부가 에러 배너 대신 조용히 정리하도록 구분한다.
    #[error("{0}")]
    Cancelled(String),

    /// DEV-435: 저장이 그사이 남이 한 변경과 부딪혔다 — 조용히 덮지 않고 멈춘다. 받은 쪽은 `current` 에서 다시
    /// 시작하거나(에이전트), 충돌 구간을 보고 고른다(앱). 서버는 409 와 이 본문을 그대로 돌려준다.
    #[error("{}", .0.message)]
    EditConflict(Box<EditConflict>),

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
