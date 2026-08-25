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
            // DEV-323: 클라이언트가 스스로 중단한 요청 — nginx 관례인 499.
            AppError::Cancelled(msg) => (
                StatusCode::from_u16(499).unwrap_or(StatusCode::BAD_REQUEST),
                msg,
            ),
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

/// DEV-367: 핸들러가 패닉했을 때의 응답.
///
/// catch-panic 계층이 없으면 패닉은 **연결 끊김**으로 나간다 — 500 도 본문도
/// 없다. 클라이언트는 그걸 네트워크 오류로 보고, 강화 검색처럼 "실패하면
/// 기존 동작으로 되돌림" 이 설계인 화면에서는 조용히 잘못된 답(검색 결과
/// 없음)이 된다. BUG-249 를 사용자가 "검색이 잘 안 된다" 로 겪은 이유다.
///
/// 응답 형식과 노출 수위는 `AppError::Internal` 과 **똑같이** 맞춘다 —
/// 로그에는 전부, 응답에는 고정 문구. 이 서버는 인증이 없고 LAN/Tailscale 로
/// 열어 쓰므로 패닉 메시지(내부 경로·변수)를 본문에 실으면 안 된다.
pub fn panic_to_500(err: Box<dyn std::any::Any + Send + 'static>) -> Response {
    // 패닉 페이로드는 보통 &str 이나 String 이다. 둘 다 아니면 타입만이라도.
    let detail = if let Some(s) = err.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else {
        "(문자열이 아닌 패닉 페이로드)".to_string()
    };
    // 위치·백트레이스는 표준 패닉 훅이 이미 stderr 에 찍는다. 여기서는 어떤
    // 요청이 죽었는지 추적 로그와 같은 자리에 남기는 것이 목적이다.
    tracing::error!("handler panicked: {detail}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "internal server error" })),
    )
        .into_response()
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
    // ── DEV-367: 패닉 → 500 ──

    /// 패닉 페이로드가 `&str` 일 때(가장 흔함 — `panic!("...")`).
    #[tokio::test]
    async fn panic_str_maps_to_500_and_hides_detail() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("byte index 2360 is not a char boundary");
        let (status, body) = body_text(panic_to_500(payload)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        // 이 서버는 인증이 없다 — 패닉 메시지(내부 경로·변수)를 실으면 안 된다.
        assert!(!body.contains("char boundary"), "패닉 내용이 응답에 샜다: {body}");
        assert!(body.contains("internal server error"));
        // 형식은 AppError::Internal 과 동일해야 한다 — 프런트가 error 필드를 읽는다.
        assert!(body.starts_with("{") && body.contains("\"error\""));
    }

    /// `panic!("{}", x)` 처럼 포맷된 경우 페이로드는 `String` 이다.
    #[tokio::test]
    async fn panic_string_payload_also_maps_to_500() {
        let payload: Box<dyn std::any::Any + Send> = Box::new(String::from("내부 상태 유출 금지"));
        let (status, body) = body_text(panic_to_500(payload)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!body.contains("유출"), "패닉 내용이 응답에 샜다: {body}");
    }

    /// 문자열이 아닌 페이로드에서도 죽지 않는다(`panic_any` 등).
    #[tokio::test]
    async fn panic_non_string_payload_still_responds() {
        let payload: Box<dyn std::any::Any + Send> = Box::new(42u32);
        let (status, _) = body_text(panic_to_500(payload)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

}
