//! core::AppError 를 axum 의 IntoResponse 로 변환하는 wrapper.
//! orphan rule 우회를 위한 newtype.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
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
            // DEV-435: 편집 충돌 — 409 와 함께 무엇이 부딪혔는지(지금 본문 · 충돌 구간 · 새 번호)를 싣는다.
            // 받는 쪽이 그걸로 고르거나 다시 시작한다.
            AppError::EditConflict(c) => {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({ "error": c.message, "conflict": c })),
                )
                    .into_response();
            }
            // DEV-465: 플러그인이 대신 처리했다 — 만든 것은 없지만 실패가 아니다. 받는 쪽은 `instead` 를 보고
            // 안내로 보인다(본문이 만든 것의 모양이 아니므로 그대로 읽으면 안 된다).
            AppError::Instead(msg) => {
                return (StatusCode::ACCEPTED, Json(json!({ "instead": msg }))).into_response();
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            // DEV-064: 길드가 서버보다 새 schema — 서버 업데이트 필요.
            AppError::IncompatibleGuild(msg) => (StatusCode::CONFLICT, msg),
            // DEV-323: 클라이언트가 스스로 중단한 요청 — nginx 관례인 499.
            AppError::Cancelled(msg) => (
                StatusCode::from_u16(499).unwrap_or(StatusCode::BAD_REQUEST),
                msg,
            ),
            // BUG-395: 문서 저장 실패 — 어떤 문서인지와 오류 번호만 싣는다. 원인 · 경로는 로그에만(같은 번호로 찾는다).
            AppError::SaveFailed(f) => {
                let id = new_error_id();
                tracing::error!(error_id = %id, doc = %f.doc, "save failed: {:#}", f.source);
                (StatusCode::INTERNAL_SERVER_ERROR, f.public_message(&id))
            }
            AppError::Internal(err) => {
                let id = new_error_id();
                tracing::error!(error_id = %id, "internal error: {err:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, internal_message(&id))
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
    let id = new_error_id();
    tracing::error!(error_id = %id, "handler panicked: {detail}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": internal_message(&id) })),
    )
        .into_response()
}

/// BUG-395: 내부 오류의 고정 문구 + 오류 번호. 원인은 싣지 않는다(이 서버는 인증이 없다 — DEV-367).
fn internal_message(id: &str) -> String {
    format!("internal server error (error ID {id})")
}

/// BUG-395: 응답과 서버 로그를 잇는 짧은 번호 — 사용자가 서버 운영자에게 알려 주면 로그에서 그 줄을 찾는다
/// (`grep error_id=3f9c2a1b`). 상용 서비스의 request/correlation ID 와 같은 쓰임. 보안 토큰이 아니라 찾기용이라
/// 표준 라이브러리의 무작위 해시 키(+ 시각 · 순번)로 충분하다 — 의존성을 늘리지 않는다.
pub fn new_error_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(SEQ.fetch_add(1, Ordering::Relaxed));
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    format!("{:08x}", h.finish() as u32)
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

    /// DEV-465: 대신 처리함 — 실패가 아니라 202, 본문은 `error` 가 아니라 `instead`.
    #[tokio::test]
    async fn instead_maps_to_202_with_the_sentence() {
        let resp = HttpError(AppError::Instead("세션을 엽니다".into())).into_response();
        let (status, body) = body_text(resp).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["instead"], "세션을 엽니다");
        assert!(v.get("error").is_none(), "{body}");
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

    /// BUG-395: 저장 실패는 문서 번호와 오류 번호만 — 경로 · 원인은 응답에 없다.
    #[tokio::test]
    async fn save_failed_shows_doc_and_error_id_but_hides_cause() {
        let io = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let inner = anyhow::Error::new(io)
            .context("failed to write temp file: /home/me/g/.guild/quests/DEV-001.md.tmp");
        let resp = HttpError(AppError::save_failed("DEV-001", inner)).into_response();
        let (status, body) = body_text(resp).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body.contains("DEV-001"), "{body}");
        assert!(!body.contains("/home/me"), "경로가 샜다: {body}");
        assert!(
            !body.contains("권한") && !body.to_lowercase().contains("permission"),
            "원인이 샜다: {body}"
        );
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let msg = v["error"].as_str().unwrap();
        assert!(
            msg.split_whitespace()
                .any(|w| w.len() == 8 && w.chars().all(|c| c.is_ascii_hexdigit())),
            "오류 번호가 없다: {msg}"
        );
    }

    #[test]
    fn error_ids_differ() {
        let a = new_error_id();
        let b = new_error_id();
        assert_eq!(a.len(), 8);
        assert_ne!(a, b);
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
        let payload: Box<dyn std::any::Any + Send> =
            Box::new("byte index 2360 is not a char boundary");
        let (status, body) = body_text(panic_to_500(payload)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        // 이 서버는 인증이 없다 — 패닉 메시지(내부 경로·변수)를 실으면 안 된다.
        assert!(
            !body.contains("char boundary"),
            "패닉 내용이 응답에 샜다: {body}"
        );
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
