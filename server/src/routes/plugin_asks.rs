//! DEV-479: 서버 플러그인의 확인 창 — 그 일을 일으킨 창이 질문을 가져가고 답을 보낸다. 중계는 core(`plugins::asks`).
//!
//! 창은 자기 이름을 요청마다 `x-openguild-client` 헤더로 보낸다. 여기서도 같은 헤더로 누구인지 안다.

use axum::{
    Json,
    extract::Path,
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use openguild_core::events::origin::{CLIENT_HEADER, Origin};
use openguild_core::plugins::asks::{self, Question};

/// 헤더의 창 이름 — 모양이 틀리면 없는 것으로 본다(이벤트에 싣는 것과 같은 검사).
fn client(headers: &HeaderMap) -> AppResult<String> {
    let raw = headers.get(CLIENT_HEADER).and_then(|v| v.to_str().ok());
    Origin::user()
        .with_client(raw)
        .client()
        .map(str::to_string)
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "`{CLIENT_HEADER}` 헤더가 없거나 모양이 틀립니다 (영문 · 숫자 · '-', 64자까지)"
            ))
            .into()
        })
}

#[derive(Debug, Deserialize)]
pub struct WaitQuery {
    /// 창이 이미 보여 주고 있는 질문 번호들(쉼표로). 이것과 같으면 바뀔 때까지(최대 25초) 기다린다.
    #[serde(default)]
    pub known: String,
}

/// 이 창에 온 질문. 창은 이걸 계속 다시 부른다.
pub async fn wait(
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<WaitQuery>,
) -> AppResult<Json<Vec<Question>>> {
    let c = client(&headers)?;
    let known: Vec<u64> = q
        .known
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let qs = tokio::task::spawn_blocking(move || asks::wait_for(&c, &known))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(qs))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerBody {
    pub yes: bool,
}

pub async fn answer(
    headers: HeaderMap,
    Path(id): Path<u64>,
    Json(body): Json<AnswerBody>,
) -> AppResult<StatusCode> {
    let c = client(&headers)?;
    if asks::answer(&c, id, body.yes) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!(
            "질문 {id} 이 없습니다 — 이미 답했거나 시간이 지났습니다"
        ))
        .into())
    }
}
