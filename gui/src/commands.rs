//! Tauri invoke 핸들러 — HTTP route 와 1:1 대응.
//!
//! - **read** (조회): `core::services::*` 호출.
//! - **mutation** (변경): `core::ops::*` 호출 (journal + 파일 + index.db).
//!
//! 각 핸들러는 `Result<T, String>` 반환 — `AppError` 를 `{e}` 로 변환.
//! Tauri 가 frontend 로 JSON 직렬화.

use openguild_core::models::{
    AddPrerequisiteRequest, ChangeParentRequest, ChangeStatusRequest, CreateQuestRequest,
    QuestDependency, QuestDetail, QuestPosition, QuestRow, QuestStatus, QuestType,
    UpdatePositionRequest, UpdateQuestRequest,
};
use openguild_core::ops::quests as ops;
use openguild_core::services::{meta as meta_svc, quests as read};
use openguild_core::{drift, reindex, snapshot, Store};
use serde::{Deserialize, Serialize};
use tauri::State;

/// `AppError` → 문자열 — invoke 에러 직렬화 보일러플레이트 제거.
fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e}")
}

// ─────────────────────── meta ───────────────────────

#[tauri::command]
pub async fn list_quest_types(store: State<'_, Store>) -> Result<Vec<QuestType>, String> {
    meta_svc::list_quest_types(&store.index_pool).await.map_err(err)
}

#[tauri::command]
pub async fn list_quest_statuses(store: State<'_, Store>) -> Result<Vec<QuestStatus>, String> {
    meta_svc::list_quest_statuses(&store.index_pool).await.map_err(err)
}

// ─────────────────────── quests (read) ───────────────────────

#[tauri::command]
pub async fn list_quests(store: State<'_, Store>) -> Result<Vec<QuestRow>, String> {
    read::list(&store.index_pool).await.map_err(err)
}

#[tauri::command]
pub async fn list_deleted_quests(store: State<'_, Store>) -> Result<Vec<QuestRow>, String> {
    read::list_deleted(&store.index_pool).await.map_err(err)
}

#[tauri::command]
pub async fn get_quest(store: State<'_, Store>, id: i64) -> Result<QuestDetail, String> {
    read::get(&store.index_pool, id).await.map_err(err)
}

#[tauri::command]
pub async fn get_quest_by_slug(
    store: State<'_, Store>,
    slug: String,
) -> Result<QuestDetail, String> {
    read::get_by_slug(&store.index_pool, &slug).await.map_err(err)
}

#[tauri::command]
pub async fn list_quest_candidates(
    store: State<'_, Store>,
    id: i64,
    relation: String,
) -> Result<Vec<QuestRow>, String> {
    read::list_candidates(&store.index_pool, id, &relation)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn list_quest_positions(
    store: State<'_, Store>,
) -> Result<Vec<QuestPosition>, String> {
    read::list_positions(&store.index_pool).await.map_err(err)
}

#[tauri::command]
pub async fn list_quest_dependencies(
    store: State<'_, Store>,
) -> Result<Vec<QuestDependency>, String> {
    read::list_dependencies(&store.index_pool).await.map_err(err)
}

// ─────────────────────── quests (mutation) ───────────────────────

#[tauri::command]
pub async fn create_quest(
    store: State<'_, Store>,
    body: CreateQuestRequest,
) -> Result<QuestRow, String> {
    ops::create_quest(&store, body).await.map_err(err)
}

#[tauri::command]
pub async fn update_quest(
    store: State<'_, Store>,
    id: i64,
    body: UpdateQuestRequest,
) -> Result<QuestRow, String> {
    ops::update_quest(&store, id, body).await.map_err(err)
}

#[tauri::command]
pub async fn change_quest_status(
    store: State<'_, Store>,
    id: i64,
    body: ChangeStatusRequest,
) -> Result<QuestRow, String> {
    ops::change_status(&store, id, body).await.map_err(err)
}

#[tauri::command]
pub async fn change_quest_parent(
    store: State<'_, Store>,
    id: i64,
    body: ChangeParentRequest,
) -> Result<QuestRow, String> {
    ops::change_parent(&store, id, body).await.map_err(err)
}

#[tauri::command]
pub async fn delete_quest(
    store: State<'_, Store>,
    id: i64,
    cascade: Option<Vec<i64>>,
) -> Result<(), String> {
    let cascade_ids = cascade.unwrap_or_default();
    ops::delete_quest(&store, id, &cascade_ids).await.map_err(err)
}

#[tauri::command]
pub async fn restore_quest(
    store: State<'_, Store>,
    id: i64,
) -> Result<QuestRow, String> {
    ops::restore_quest(&store, id).await.map_err(err)
}

#[tauri::command]
pub async fn add_prerequisite(
    store: State<'_, Store>,
    id: i64,
    body: AddPrerequisiteRequest,
) -> Result<(), String> {
    ops::add_prerequisite(&store, id, body).await.map_err(err)
}

#[tauri::command]
pub async fn remove_prerequisite(
    store: State<'_, Store>,
    id: i64,
    prereq_id: i64,
) -> Result<(), String> {
    ops::remove_prerequisite(&store, id, prereq_id).await.map_err(err)
}

#[tauri::command]
pub async fn update_quest_position(
    store: State<'_, Store>,
    id: i64,
    body: UpdatePositionRequest,
) -> Result<QuestPosition, String> {
    // update_position 은 UI 상태 — SQL 만 (services 의 read 모듈에 위치).
    read::update_position(&store.index_pool, id, body).await.map_err(err)
}

// ─────────────────────── admin ───────────────────────

#[tauri::command]
pub async fn admin_create_snapshot(
    store: State<'_, Store>,
) -> Result<snapshot::SnapshotInfo, String> {
    snapshot::create_snapshot(&store).await.map_err(err)
}

#[tauri::command]
pub async fn admin_list_snapshots(
    store: State<'_, Store>,
) -> Result<Vec<snapshot::SnapshotInfo>, String> {
    snapshot::list_snapshots(&store.paths).map_err(err)
}

#[derive(Debug, Deserialize)]
pub struct RestoreArgs {
    /// 특정 timestamp (`YYYYMMDD-HHMMSS`). 미지정 시 최신.
    #[serde(default)]
    pub to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RestoreResult {
    pub restored_to: String,
}

#[tauri::command]
pub async fn admin_restore(
    store: State<'_, Store>,
    args: RestoreArgs,
) -> Result<RestoreResult, String> {
    let snapshots = snapshot::list_snapshots(&store.paths).map_err(err)?;
    let target = if let Some(ts) = args.to {
        snapshots
            .iter()
            .find(|s| s.timestamp == ts)
            .cloned()
            .ok_or_else(|| format!("snapshot {ts} 없음"))?
    } else {
        snapshots
            .last()
            .cloned()
            .ok_or_else(|| "사용 가능한 snapshot 이 없습니다".to_string())?
    };
    snapshot::restore_snapshot(&store, &target).await.map_err(err)?;
    Ok(RestoreResult {
        restored_to: target.timestamp,
    })
}

#[tauri::command]
pub async fn admin_check_drift(
    store: State<'_, Store>,
) -> Result<drift::DriftReport, String> {
    drift::detect_drift(&store).await.map_err(err)
}

#[derive(Debug, Serialize)]
pub struct ReindexResult {
    pub types_loaded: usize,
    pub statuses_loaded: usize,
    pub quests_loaded: usize,
    pub dependencies_loaded: usize,
    pub positions_restored: usize,
    pub skipped: Vec<SkippedFile>,
}

#[derive(Debug, Serialize)]
pub struct SkippedFile {
    pub path: String,
    pub reason: String,
}

#[tauri::command]
pub async fn admin_reindex(store: State<'_, Store>) -> Result<ReindexResult, String> {
    let report = reindex::reindex(&store).await.map_err(err)?;
    Ok(ReindexResult {
        types_loaded: report.types_loaded,
        statuses_loaded: report.statuses_loaded,
        quests_loaded: report.quests_loaded,
        dependencies_loaded: report.dependencies_loaded,
        positions_restored: report.positions_restored,
        skipped: report
            .skipped
            .into_iter()
            .map(|(path, reason)| SkippedFile { path, reason })
            .collect(),
    })
}
