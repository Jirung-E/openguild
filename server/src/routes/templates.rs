//! Quest 템플릿 HTTP 어댑터 — CLI local/remote 파리티.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use openguild_core::repo::TemplateFile;
use openguild_core::Store;

pub async fn list_templates(
    State(store): State<Store>,
) -> AppResult<Json<Vec<TemplateFile>>> {
    Ok(Json(openguild_core::repo::list_templates(&store.paths)?))
}

pub async fn get_template(
    State(store): State<Store>,
    Path(name): Path<String>,
) -> AppResult<Json<TemplateFile>> {
    let name = openguild_core::repo::validate_template_name(&name)
        .map_err(|error| openguild_core::AppError::BadRequest(error.to_string()))?;
    let path = store.paths.template_path(name);
    if !path.exists() {
        return Err(openguild_core::AppError::NotFound(format!(
            "template '{name}' 없음"
        ))
        .into());
    }
    Ok(Json(TemplateFile::read(path)?))
}

#[derive(Debug, Default, Deserialize)]
pub struct SaveTemplateQuery {
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Serialize)]
pub struct SaveTemplateResponse {
    pub path: String,
}

/// BUG-233: API 응답의 상대 경로는 **항상 `/`** 로 낸다.
///
/// `PathBuf::to_string_lossy()` 를 그대로 쓰면 Windows 호스팅에서만 역슬래시가
/// 섞여 나가, 같은 API 가 서버 OS 에 따라 다른 값을 준다. 이 프로젝트의 상대
/// 경로 관례는 `/`(첨부의 `attachments/…` 와 동일)이고, 클라이언트가 이 값을
/// 다시 경로로 조립하기도 한다.
///
/// CI 테스트 잡이 ubuntu 에서만 돌아 이 차이를 못 잡았다 — Windows 로컬 전체
/// 테스트에서 드러났다.
fn rel_posix(path: &std::path::Path, root: &std::path::Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

pub async fn save_template(
    State(store): State<Store>,
    Query(query): Query<SaveTemplateQuery>,
    Json(template): Json<TemplateFile>,
) -> AppResult<Json<SaveTemplateResponse>> {
    let path = openguild_core::repo::save_template(&store.paths, &template, query.force)
        .map_err(|error| openguild_core::AppError::BadRequest(error.to_string()))?;
    Ok(Json(SaveTemplateResponse {
        path: rel_posix(&path, &store.paths.guild_root),
    }))
}
