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

    /// DEV-465: 플러그인이 변경 대신 다른 일을 했다(예: 댓글 명령). 저장된 것은 없지만 **실패가 아니다** —
    /// 받은 쪽은 오류 대신 안내로 보이고 입력을 비운다. 서버는 202 와 `{"instead": 문장}` 을 돌려준다.
    #[error("{0}")]
    Instead(String),

    /// DEV-435: 저장이 그사이 남이 한 변경과 부딪혔다 — 조용히 덮지 않고 멈춘다. 받은 쪽은 `current` 에서 다시
    /// 시작하거나(에이전트), 충돌 구간을 보고 고른다(앱). 서버는 409 와 이 본문을 그대로 돌려준다.
    #[error("{}", .0.message)]
    EditConflict(Box<EditConflict>),

    /// BUG-395: 문서 파일을 저장하지 못했다(디스크 쪽 실패). 앱 · CLI 는 자기 컴퓨터라 짧은 원인까지 보이고
    /// (`Display`), 서버는 원인 없이 오류 번호만 보인다 — 원문은 서버 로그에(`server::error`).
    #[error("{}", .0.local_message())]
    SaveFailed(Box<SaveFailure>),

    #[error("internal error: {0:#}")]
    Internal(#[from] anyhow::Error),
}

/// BUG-395: 어떤 문서를 왜 저장하지 못했나.
#[derive(Debug)]
pub struct SaveFailure {
    /// 문서 번호(`DEV-001`, `C-003`, `BOOK-002`) — 보는 사람이 이미 아는 것만.
    pub doc: String,
    pub cause: SaveCause,
    /// 원문(경로 포함) — 로그용. 서버 응답에는 싣지 않는다.
    pub source: anyhow::Error,
}

/// 저장 실패의 짧은 분류 — 사용자가 손쓸 수 있는 것만 나눈다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveCause {
    /// 읽기 전용이거나 쓸 권한이 없다.
    ReadOnly,
    /// 다른 프로그램이 파일을 잡고 있다(윈도우의 공유 위반).
    InUse,
    /// 디스크가 가득 찼다.
    DiskFull,
    Other,
}

impl SaveCause {
    /// 오류 사슬에서 첫 I/O 오류로 가른다. I/O 오류가 없으면 `None` — 저장 실패가 아니다(DB 오류 등).
    pub fn of(e: &anyhow::Error) -> Option<Self> {
        use std::io::ErrorKind;
        let io = e.chain().find_map(|c| c.downcast_ref::<std::io::Error>())?;
        let code = io.raw_os_error();
        Some(match io.kind() {
            ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem => {
                // 윈도우는 다른 프로그램이 잡은 파일의 rename 도 "접근 거부"(5)로 돌려준다 — 그쪽은 권한 문제와
                // 구별할 수 없어 둘 다 말한다(아래 문구).
                SaveCause::ReadOnly
            }
            ErrorKind::StorageFull | ErrorKind::QuotaExceeded => SaveCause::DiskFull,
            _ if cfg!(windows) && matches!(code, Some(32 | 33)) => SaveCause::InUse,
            _ if cfg!(windows) && matches!(code, Some(39 | 112)) => SaveCause::DiskFull,
            _ => SaveCause::Other,
        })
    }

    fn label(self) -> String {
        match self {
            SaveCause::ReadOnly if cfg!(windows) => crate::tf!(
                "읽기 전용이거나 쓸 권한이 없거나, 다른 프로그램이 파일을 쓰고 있습니다",
                "it is read-only, not writable, or in use by another program"
            ),
            SaveCause::ReadOnly => crate::tf!(
                "읽기 전용이거나 쓸 권한이 없습니다",
                "it is read-only or not writable"
            ),
            SaveCause::InUse => crate::tf!(
                "다른 프로그램이 파일을 쓰고 있습니다",
                "another program is using the file"
            ),
            SaveCause::DiskFull => crate::tf!("디스크 공간이 모자랍니다", "the disk is full"),
            SaveCause::Other => crate::tf!("파일 시스템 오류", "file system error"),
        }
    }
}

impl SaveFailure {
    /// 앱 · CLI 에 보이는 문구 — 짧은 원인까지, 경로는 없이.
    pub fn local_message(&self) -> String {
        crate::tf!(
            "{} 파일을 저장하지 못했습니다 — {}. 원인을 없앤 뒤 다시 하세요",
            "Could not save {} — {}. Fix the cause and try again",
            self.doc,
            self.cause.label()
        )
    }

    /// 서버 응답 문구 — 원인 없이 오류 번호만. 서버 운영자가 같은 번호로 로그를 찾는다.
    pub fn public_message(&self, error_id: &str) -> String {
        crate::tf!(
            "{} 파일을 저장하지 못했습니다. 잠시 뒤 다시 해 보고, 계속 안 되면 서버 운영자에게 오류 번호 {} 를 알려 주세요",
            "Could not save {}. Try again later; if it keeps failing, give error ID {} to the server operator",
            self.doc,
            error_id
        )
    }
}

impl AppError {
    /// BUG-395: 문서 파일 쓰기의 실패 — 디스크 쪽 실패(I/O 오류)면 [`AppError::SaveFailed`], 아니면 그대로 내부 오류.
    pub fn save_failed(doc: &str, e: anyhow::Error) -> AppError {
        match SaveCause::of(&e) {
            Some(cause) => AppError::SaveFailed(Box::new(SaveFailure {
                doc: doc.to_string(),
                cause,
                source: e,
            })),
            None => AppError::Internal(e),
        }
    }

    /// 이미 `AppError` 인 결과에 문서 번호를 붙인다 — 내부 오류 중 디스크 쪽 실패만 바꾸고 나머지는 그대로.
    pub fn on_save(self, doc: &str) -> AppError {
        match self {
            AppError::Internal(e) => AppError::save_failed(doc, e),
            other => other,
        }
    }
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
