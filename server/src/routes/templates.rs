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

pub async fn save_template(
    State(store): State<Store>,
    Query(query): Query<SaveTemplateQuery>,
    Json(template): Json<TemplateFile>,
) -> AppResult<Json<SaveTemplateResponse>> {
    let path = openguild_core::repo::save_template(&store.paths, &template, query.force)
        .map_err(|error| openguild_core::AppError::BadRequest(error.to_string()))?;
    let relative = path
        .strip_prefix(&store.paths.guild_root)
        .unwrap_or(&path)
        .to_string_lossy()
        .to_string();
    Ok(Json(SaveTemplateResponse { path: relative }))
}
