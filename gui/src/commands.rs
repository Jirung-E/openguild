//! Tauri invoke 핸들러 — HTTP route 와 1:1 대응.
//!
//! - **read** (조회): `core::services::*` 호출.
//! - **mutation** (변경): `core::ops::*` 호출 (journal + 파일 + index.db).
//!
//! 각 핸들러는 `Result<T, String>` 반환 — `AppError` 를 `{e}` 로 변환.
//! Tauri 가 frontend 로 JSON 직렬화.

use openguild_core::models::{
    AddChecklistRequest, AddPrerequisiteRequest, CampaignChecklistItem, CampaignDetail,
    CampaignRow, CampaignSummary, ChangeParentRequest, ChangeStatusRequest,
    CreateCampaignRequest, CreateQuestRequest, LinkQuestRequest, ListQuery, QuestDependency,
    QuestDetail, QuestHistoryEntry, QuestPosition, QuestRow, QuestStatus, QuestTagDef, QuestType,
    UpdateCampaignRequest, UpdatePositionRequest, UpdateQuestRequest,
};
use openguild_core::ops::{campaigns as camp_ops, meta as meta_ops, quests as ops};
use openguild_core::services::{
    campaigns as camp_svc, meta as meta_svc, quests as read,
};
use openguild_core::{drift, reindex, snapshot, Store};
use serde::{Deserialize, Serialize};
use tauri::State;

/// `AppError` → 문자열 — invoke 에러 직렬화 보일러플레이트 제거.
fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e}")
}

/// DEV-154: Store::open 에러를 프론트가 구분할 수 있게 태깅. 더 새 schema 길드
/// (IncompatibleGuild) 는 sentinel 접두어로 — welcome 이 전용 안내 + 업데이트
/// 버튼을 띄운다. 그 외는 기존 메시지.
pub const INCOMPATIBLE_GUILD_TAG: &str = "INCOMPATIBLE_GUILD::";
fn open_err(e: anyhow::Error) -> String {
    if let Some(openguild_core::AppError::IncompatibleGuild(msg)) =
        e.downcast_ref::<openguild_core::AppError>()
    {
        format!("{INCOMPATIBLE_GUILD_TAG}{msg}")
    } else {
        format!("Store::open 실패: {e:#}")
    }
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
pub async fn list_quests(
    store: State<'_, Store>,
    query: Option<ListQuery>,
) -> Result<Vec<QuestRow>, String> {
    let q = query.unwrap_or_default();
    read::list(&store.index_pool, &q).await.map_err(err)
}

#[tauri::command]
pub async fn list_deleted_quests(store: State<'_, Store>) -> Result<Vec<QuestRow>, String> {
    read::list_deleted(&store.index_pool).await.map_err(err)
}

#[tauri::command]
pub async fn get_quest(store: State<'_, Store>, id: i64) -> Result<QuestDetail, String> {
    let mut detail = read::get(&store.index_pool, id).await.map_err(err)?;
    // DEV-156: 첨부 목록(sidecar)은 Store 가 필요 — 여기서 채운다.
    detail.attachments =
        openguild_core::ops::attachments::list_quest_attachments(&store, &detail.quest.quest_id);
    Ok(detail)
}

#[tauri::command]
pub async fn get_quest_by_slug(
    store: State<'_, Store>,
    slug: String,
) -> Result<QuestDetail, String> {
    // DEV-137 (Phase 2): 상세 진입 시 그 파일만 lazy mtime 체크 — GUI 를 켜둔
    // 채 외부 편집한 경우에도 상세 화면은 최신. 실패는 무시 (stale 표시가
    // 에러보다 낫다).
    let _ = openguild_core::incremental::refresh_quest_if_stale(&store, &slug).await;
    let mut detail = read::get_by_slug(&store.index_pool, &slug).await.map_err(err)?;
    // DEV-156: 첨부 목록(sidecar) 채우기.
    detail.attachments = openguild_core::ops::attachments::list_quest_attachments(&store, &slug);
    Ok(detail)
}

// ─────────────────────── DEV-156: quest/campaign 첨부 목록 ───────────────────────

/// quest 첨부 추가 (이미 저장된 .guild/attachments 경로 + 원본 파일명). 갱신 목록.
#[tauri::command]
pub async fn add_quest_attachment(
    store: State<'_, Store>,
    slug: String,
    path: String,
    name: String,
) -> Result<Vec<openguild_core::models::QuestAttachment>, String> {
    openguild_core::ops::attachments::add_quest_attachment(&store, &slug, &path, &name)
        .await
        .map_err(err)
}

/// quest 첨부 제거 (목록에서만). 갱신 목록.
#[tauri::command]
pub async fn remove_quest_attachment(
    store: State<'_, Store>,
    slug: String,
    path: String,
) -> Result<Vec<openguild_core::models::QuestAttachment>, String> {
    openguild_core::ops::attachments::remove_quest_attachment(&store, &slug, &path)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn add_campaign_attachment(
    store: State<'_, Store>,
    slug: String,
    path: String,
    name: String,
) -> Result<Vec<openguild_core::models::QuestAttachment>, String> {
    openguild_core::ops::attachments::add_campaign_attachment(&store, &slug, &path, &name)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn remove_campaign_attachment(
    store: State<'_, Store>,
    slug: String,
    path: String,
) -> Result<Vec<openguild_core::models::QuestAttachment>, String> {
    openguild_core::ops::attachments::remove_campaign_attachment(&store, &slug, &path)
        .await
        .map_err(err)
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

/// DEV-013: quest 의 변경 이력.
#[tauri::command]
pub async fn list_quest_history(
    store: State<'_, Store>,
    id: i64,
) -> Result<Vec<QuestHistoryEntry>, String> {
    read::list_history(&store.index_pool, id).await.map_err(err)
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

/// DEV-076 / BUG-031: 희망 / 필수 기한 설정 / 해제.
///
/// body JSON 키 존재 여부로 변경 의도 구분:
///   { "desired_due": "2026-06-15" }  → 설정
///   { "desired_due": null }          → 해제
///   {}                                → no-op
/// (서버 routes/quests.rs::set_due_dates 와 동일 contract.)
#[tauri::command]
pub async fn set_quest_due_dates(
    store: State<'_, Store>,
    id: i64,
    body: serde_json::Value,
) -> Result<QuestRow, String> {
    use serde_json::Value;
    fn parse_field(body: &Value, key: &str) -> Option<Option<String>> {
        let obj = body.as_object()?;
        let v = obj.get(key)?;
        Some(match v {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            _ => return None,
        })
    }
    let desired = parse_field(&body, "desired_due");
    let required = parse_field(&body, "required_due");
    ops::set_due_dates(&store, id, desired, required)
        .await
        .map_err(err)
}

/// DEV-068: tag 전체 교체. 정규화 (trim + dedupe + 빈 제거) 는 service 위임.
#[tauri::command]
pub async fn set_quest_tags(
    store: State<'_, Store>,
    id: i64,
    tags: Vec<String>,
) -> Result<QuestRow, String> {
    ops::set_quest_tags(&store, id, tags).await.map_err(err)
}

/// DEV-055: quest 의 type 변경 — slug 가 바뀜.
#[tauri::command]
pub async fn change_quest_type(
    store: State<'_, Store>,
    id: i64,
    body: openguild_core::models::ChangeTypeRequest,
) -> Result<QuestRow, String> {
    ops::change_quest_type(&store, id, body).await.map_err(err)
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

/// 비정상 quest 파일 (정의되지 않은 status / 파싱 실패) 목록. GUI 시동 알림 +
/// admin 재검사용. read-only (DB 안 건드림).
#[tauri::command]
pub async fn list_problem_files(store: State<'_, Store>) -> Result<Vec<SkippedFile>, String> {
    Ok(openguild_core::health::list_problem_quest_files(&store)
        .await
        .into_iter()
        .map(|(path, reason)| SkippedFile { path, reason })
        .collect())
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

/// DEV-162: index.db VACUUM (런타임 정비). admin 페이지 '정리' 버튼.
#[tauri::command]
pub async fn admin_vacuum(
    store: State<'_, Store>,
) -> Result<openguild_core::maintenance::VacuumReport, String> {
    openguild_core::maintenance::vacuum(&store).await.map_err(err)
}

/// DEV-162: journal.db(AOF) 최근 op. admin 페이지 '최근 작업' 뷰.
#[tauri::command]
pub async fn admin_journal_tail(
    store: State<'_, Store>,
    count: Option<i64>,
) -> Result<openguild_core::maintenance::JournalTail, String> {
    let tail = openguild_core::maintenance::journal_tail(&store.paths, count.unwrap_or(50))
        .await
        .map_err(err)?;
    Ok(tail.unwrap_or_default())
}

// ─────────────────────── admin meta (DEV-014) ───────────────────────

/// 사용 중 quest 수를 포함한 type DTO — UI 의 "삭제 가능?" 판단 용도.
#[derive(Debug, Serialize)]
pub struct QuestTypeWithCount {
    #[serde(flatten)]
    pub row: QuestType,
    pub quest_count: i64,
}

#[derive(Debug, Serialize)]
pub struct QuestStatusWithCount {
    #[serde(flatten)]
    pub row: QuestStatus,
    pub quest_count: i64,
}

#[tauri::command]
pub async fn admin_list_types(
    store: State<'_, Store>,
) -> Result<Vec<QuestTypeWithCount>, String> {
    let rows = meta_svc::list_quest_types(&store.index_pool)
        .await
        .map_err(err)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let quest_count = meta_ops::count_quests_by_type(&store.index_pool, row.id)
            .await
            .map_err(err)?;
        out.push(QuestTypeWithCount { row, quest_count });
    }
    Ok(out)
}

#[tauri::command]
pub async fn admin_list_statuses(
    store: State<'_, Store>,
) -> Result<Vec<QuestStatusWithCount>, String> {
    let rows = meta_svc::list_quest_statuses(&store.index_pool)
        .await
        .map_err(err)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let quest_count = meta_ops::count_quests_by_status(&store.index_pool, row.id)
            .await
            .map_err(err)?;
        out.push(QuestStatusWithCount { row, quest_count });
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct CreateTypeBody {
    pub prefix: String,
    pub color: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[tauri::command]
pub async fn admin_create_type(
    store: State<'_, Store>,
    body: CreateTypeBody,
) -> Result<QuestType, String> {
    meta_ops::create_type(&store, body.prefix, body.color, body.description)
        .await
        .map_err(err)
}

#[derive(Debug, Deserialize)]
pub struct UpdateTypeBody {
    /// BUG-018: prefix rename 통합. 변경하지 않으려면 필드 생략.
    #[serde(default)]
    pub new_prefix: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// `null` outer = 변경 없음, `null` inner (= JS `null` 명시) 는 unset.
    /// JSON 으로는 `{"description": null}` vs 필드 자체 생략 으로 구분.
    #[serde(default, with = "double_option")]
    pub description: Option<Option<String>>,
}

#[tauri::command]
pub async fn admin_update_type(
    store: State<'_, Store>,
    prefix: String,
    body: UpdateTypeBody,
) -> Result<QuestType, String> {
    meta_ops::update_type(
        &store,
        prefix,
        body.new_prefix,
        body.color,
        body.description,
    )
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn admin_delete_type(
    store: State<'_, Store>,
    prefix: String,
) -> Result<(), String> {
    meta_ops::delete_type(&store, prefix).await.map_err(err)
}

#[derive(Debug, Deserialize)]
pub struct CreateStatusBody {
    pub name_en: String,
    pub name_ko: String,
    pub color: String,
    #[serde(default)]
    pub sort_order: Option<i64>,
}

#[tauri::command]
pub async fn admin_create_status(
    store: State<'_, Store>,
    body: CreateStatusBody,
) -> Result<QuestStatus, String> {
    meta_ops::create_status(
        &store,
        body.name_en,
        body.name_ko,
        body.color,
        body.sort_order,
    )
    .await
    .map_err(err)
}

#[derive(Debug, Deserialize)]
pub struct UpdateStatusBody {
    /// BUG-018: slug rename 통합. 변경하지 않으려면 필드 생략.
    #[serde(default)]
    pub new_slug: Option<String>,
    #[serde(default)]
    pub name_en: Option<String>,
    #[serde(default)]
    pub name_ko: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    // DEV-093: 캠페인 진행도용 "완료" 카운트 토글.
    #[serde(default)]
    pub counts_as_done: Option<bool>,
}

#[tauri::command]
pub async fn admin_update_status(
    store: State<'_, Store>,
    slug: String,
    body: UpdateStatusBody,
) -> Result<QuestStatus, String> {
    meta_ops::update_status(
        &store,
        slug,
        body.new_slug,
        body.name_en,
        body.name_ko,
        body.color,
        body.sort_order,
        body.counts_as_done,
    )
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn admin_delete_status(
    store: State<'_, Store>,
    slug: String,
) -> Result<(), String> {
    meta_ops::delete_status(&store, slug).await.map_err(err)
}

// ─────────────────────── tag defs (DEV-068) ───────────────────────

#[tauri::command]
pub async fn admin_list_tag_defs(
    store: State<'_, Store>,
) -> Result<Vec<QuestTagDef>, String> {
    meta_svc::list_quest_tag_defs(&store.index_pool)
        .await
        .map_err(err)
}

#[derive(Debug, Deserialize)]
pub struct UpsertTagDefBody {
    pub slug: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub description: String,
}

#[tauri::command]
pub async fn admin_upsert_tag_def(
    store: State<'_, Store>,
    body: UpsertTagDefBody,
) -> Result<QuestTagDef, String> {
    meta_ops::upsert_tag_def(&store, body.slug, body.color, body.description)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn admin_delete_tag_def(
    store: State<'_, Store>,
    slug: String,
) -> Result<(), String> {
    meta_ops::delete_tag_def(&store, slug).await.map_err(err)
}

/// serde: `Option<Option<T>>` 필드 생략 vs `null` 구분 — `update_type`
/// 의 `description` 이 unset (None) 인지 변경 없음 (Option::None) 인지
/// 구분하기 위함.
mod double_option {
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(d).map(Some)
    }
}

// ─────────────────────── recents (DEV-006) ───────────────────────

use openguild_core::recents;

/// DEV-052 후속 (4회차): 존재 여부를 enrichment 해서 frontend 가 사라진
/// 길드를 회색 처리 / 제거 액션을 제공할 수 있게.
#[derive(Serialize)]
pub struct RecentDto {
    pub path: String,
    pub name: String,
    pub last_opened: String,
    /// path 가 더 이상 존재하지 않으면 `true`. 외장 드라이브 일시적 unmount
    /// 일 수도 있으므로 자동 제거는 안 함.
    pub missing: bool,
}

#[tauri::command]
pub async fn list_recents() -> Result<Vec<RecentDto>, String> {
    let raw = recents::list().map_err(|e| format!("{e:#}"))?;
    Ok(raw
        .into_iter()
        .map(|r| {
            let p = std::path::Path::new(&r.path);
            // DEV-052 후속 (5회차): path 가 살아있어도 `.guild` 마커가 없으면
            // 더 이상 유효한 길드 아님 (사용자가 .guild 폴더만 삭제했거나
            // 디렉토리 자체를 재활용한 경우). missing 으로 취급해서 자동
            // 초기화 (실수로 빈 길드 생성) 방지.
            let missing = !p.exists() || !crate::has_guild_marker(p);
            RecentDto {
                missing,
                path: r.path,
                name: r.name,
                last_opened: r.last_opened,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn clear_recents() -> Result<(), String> {
    recents::clear().map_err(|e| format!("{e:#}"))
}

/// DEV-052 후속 (4회차): 단일 항목 제거 — "사라진 길드" 목록 정리용.
#[tauri::command]
pub async fn remove_recent(path: String) -> Result<(), String> {
    recents::remove(&path).map_err(|e| format!("{e:#}"))
}

// ─────────────────────── launch (DEV-052) ───────────────────────

#[derive(Serialize)]
pub struct LaunchInfoDto {
    pub mode: &'static str, // "guild" | "welcome" | "uninit"
    pub uninit_path: Option<String>,
}

/// DEV-053: dialog 로 선택한 디렉토리가 유효한 길드인지 (마커 존재 여부) 검사.
///
/// 반환:
/// - `exists`: path 가 존재 (디렉토리 / 파일).
/// - `is_dir`: 디렉토리 여부 (`*.guild` 파일을 골랐을 수도 있음 — 부모 디렉토리).
/// - `has_marker`: `.guild/` 폴더 또는 `*.guild` 파일이 있는지.
/// - `resolved_path`: `*.guild` 파일이면 부모 디렉토리, 디렉토리면 그대로
///   (절대경로 + Windows `\\?\` 제거).
///
/// frontend 가 이 응답을 보고:
/// - `has_marker == true` → `open_guild_in_current_window(resolved_path)`.
/// - `has_marker == false` → uninit prompt 활성화 (`uninitPath = resolved_path`).
#[derive(Serialize)]
pub struct GuildPathInspect {
    pub exists: bool,
    pub is_dir: bool,
    pub has_marker: bool,
    pub resolved_path: String,
}

#[tauri::command]
pub fn inspect_guild_path(path: String) -> GuildPathInspect {
    let p = std::path::Path::new(&path);
    let exists = p.exists();
    if !exists {
        return GuildPathInspect {
            exists: false,
            is_dir: false,
            has_marker: false,
            resolved_path: path,
        };
    }
    // `.guild` 파일이면 parent 를 길드 root 로.
    let resolved = if p.is_file()
        && p.extension().and_then(|e| e.to_str()) == Some("guild")
    {
        p.parent().map(|x| x.to_path_buf()).unwrap_or_else(|| p.to_path_buf())
    } else {
        p.to_path_buf()
    };
    // 절대화 + Windows `\\?\` 제거.
    let abs = openguild_core::recents::normalize_abs(&resolved);
    let abs_path = std::path::Path::new(&abs);
    GuildPathInspect {
        exists: true,
        is_dir: abs_path.is_dir(),
        has_marker: abs_path.is_dir() && crate::has_guild_marker(abs_path),
        resolved_path: abs,
    }
}

/// DEV-052: frontend 가 첫 진입 URL 결정용 — { mode, uninit_path }.
#[tauri::command]
pub fn launch_mode(state: State<'_, crate::LaunchInfo>) -> LaunchInfoDto {
    LaunchInfoDto {
        mode: state.mode,
        uninit_path: state
            .uninit_path
            .as_ref()
            .map(|p| p.display().to_string()),
    }
}

/// BUG-019: 현재 활성 길드 경로 (절대 경로). frontend 의 localStorage
/// namespace 분리 / 길드별 UI 상태 키 prefix 에 사용. Welcome / Uninit
/// 모드일 땐 placeholder 경로가 그대로 반환되므로 frontend 는
/// `launch_mode.mode === "guild"` 일 때만 의미 있게 사용해야 함.
#[tauri::command]
pub fn current_guild_path(store: State<'_, Store>) -> String {
    store.paths.guild_root.display().to_string()
}

/// DEV-141: 현재 길드 이름 — `{name}.guild` 마커의 stem 또는 디렉토리명
/// (recents 의 표시명과 동일 규칙). Nav 에서 어느 길드에 들어와 있는지 표시용.
/// Welcome / Uninit 모드의 placeholder 경로면 의미 없는 값일 수 있으므로
/// frontend 는 `launch_mode.mode === "guild"` 일 때만 사용.
#[tauri::command]
pub fn current_guild_name(store: State<'_, Store>) -> String {
    openguild_core::recents::guess_name(&store.paths.guild_root)
}

/// BUG-041: DB schema 가 현재 binary 가 모르는 migration 까지 적용된 상태인지.
///
/// 응답:
/// - `is_ahead`: true 면 frontend 가 "업데이트 필요" 알림 표시.
/// - `ahead_versions`: DB 의 `_sqlx_migrations` 중 binary 가 모르는 version 목록.
/// - `binary_version`: 이 binary 의 CARGO_PKG_VERSION — 사용자가 "내 GUI 가
///   몇 버전" 인지 한 눈에 확인용.
/// - `latest_known_migration`: 이 binary 가 알고 있는 가장 큰 migration version
///   (= 빌드 시 `core/migrations` 의 max). DB 가 이보다 앞서 있으면 banner.
///
/// frontend 의 banner 가 이 값으로 사용자 행동 가이드 표시. Welcome/Uninit 모드
/// (in-memory) 는 항상 `is_ahead: false`.
#[derive(serde::Serialize)]
pub struct DbSchemaStatus {
    pub is_ahead: bool,
    pub ahead_versions: Vec<i64>,
    pub binary_version: String,
    pub latest_known_migration: Option<i64>,
}

#[tauri::command]
pub fn get_db_schema_status(store: State<'_, Store>) -> DbSchemaStatus {
    let ahead = store.db_ahead_versions.clone();
    // binary 가 알고 있는 max migration — core 가 expose.
    let latest_known = openguild_core::db::latest_known_migration_version();
    DbSchemaStatus {
        is_ahead: !ahead.is_empty(),
        ahead_versions: ahead,
        binary_version: env!("CARGO_PKG_VERSION").to_string(),
        latest_known_migration: latest_known,
    }
}

/// DEV-052 후속: 길드 마커 없는 디렉토리에서 사용자가 "초기화" 승인 시
/// 호출. .guild 시드 생성 + Store::open + recents 등록 + Store / LaunchInfo
/// 를 swap. `unmanage` 의 deprecation 이유는 open_guild_in_current_window 주석 참고.
#[tauri::command]
#[allow(deprecated)]
pub fn init_and_open_guild(
    app: tauri::AppHandle,
    path: String,
    name: Option<String>,
) -> Result<(), String> {
    use tauri::Manager;

    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("path 가 존재하지 않습니다: {path}"));
    }

    // 1. .guild 디렉토리 시드. seed_guild_dir 는 idempotent 이라 안전.
    openguild_core::repo::seed_guild_dir(p)
        .map_err(|e| format!("seed_guild_dir 실패: {e:#}"))?;

    // 2. <name>.guild 마커 파일 생성 (없으면). 이름은 인자 또는 디렉토리명.
    let guild_name = name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("guild")
                .to_string()
        });
    let marker = p.join(format!("{guild_name}.guild"));
    if !marker.exists() {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        // DEV-064: 마커 포맷은 core 공용 헬퍼 — schema_version 포함.
        let toml = openguild_core::guild_file::marker_content(&guild_name, &today);
        std::fs::write(&marker, toml).map_err(|e| format!("marker 파일 생성 실패: {e}"))?;
    }

    // 3. Store 열고 swap.
    let store = tauri::async_runtime::block_on(openguild_core::Store::open(p))
        .map_err(open_err)?;
    // BUG-049/DEV-121 fix: 초기화/연 길드도 시동 sync (open_guild_in_current_window 와 동일).
    if let Err(e) =
        tauri::async_runtime::block_on(openguild_core::incremental::sync_on_open(&store))
    {
        eprintln!("[openguild-gui] warn: sync_on_open 실패 — {e:#}");
    }
    if let Err(e) = openguild_core::recents::add(p) {
        eprintln!("[openguild-gui] warn: recents 갱신 실패 — {e:#}");
    }
    app.unmanage::<openguild_core::Store>();
    app.manage(store);
    // DEV-087 fix2: 새/초기화한 길드 디렉토리를 asset protocol scope 에 추가
    // (open_guild_in_current_window 와 동일 이유 — 배너/첨부 asset:// 차단 방지).
    if let Err(e) = app.asset_protocol_scope().allow_directory(p, true) {
        eprintln!("[openguild-gui] warn: asset scope allow 실패 — {e:#}");
    }
    app.unmanage::<crate::LaunchInfo>();
    app.manage(crate::LaunchInfo {
        mode: "guild",
        uninit_path: None,
    });

    Ok(())
}

/// DEV-052 후속 (2회차): Welcome 의 recent 클릭 → 현재 프로세스의 Store 를
/// 새 path 로 교체 (Tauri 의 `unmanage` → `manage` 패턴). 새 창 spawn 안 함.
///
/// 동작:
/// 1. 새 path 의 Store 생성 (sqlx pool 등 미리 확보).
/// 2. recents 자동 등록.
/// 3. `unmanage::<Store>()` 로 기존 store 해제 (사용 중인 reference 는 Arc
///    refcount 로 자연 종료).
/// 4. `manage(new_store)` 로 새 store 등록.
/// 5. `LaunchInfo.mode` 도 "guild" 로 갱신.
///
/// frontend 는 응답 받은 직후 `goto('/')` 로 보드 진입.
///
/// `unmanage` 는 Tauri 2 에서 deprecated (dangling ref 우려) 지만 본 앱은
/// 단일 사용자 + swap 이 사용자 명시 액션이라 다른 command 와 동시 실행이
/// 사실상 없음. 차후 Mutex<Store> 리팩터 시 제거.
#[tauri::command]
#[allow(deprecated)]
pub fn open_guild_in_current_window(
    app: tauri::AppHandle,
    path: String,
) -> Result<(), String> {
    use tauri::Manager;

    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("path 가 존재하지 않습니다: {path}"));
    }
    // DEV-052 후속 (5회차): `.guild` 마커가 없으면 Store::open 이 빈 길드를
    // 새로 만들어버림 → 사용자 실수로 보일 수 있음. 명시적 에러로 막고,
    // "이 위치 초기화" 흐름 (init_and_open_guild) 을 거치게 유도.
    if !crate::has_guild_marker(p) {
        return Err(format!(
            "'{path}' 에 길드 마커 (.guild/ 폴더 또는 *.guild 파일) 가 없습니다. \
             목록에서 제거하거나 새 위치를 지정하세요."
        ));
    }

    // 1. 새 Store 미리 열기 (실패 시 swap 안 함).
    let new_store = tauri::async_runtime::block_on(openguild_core::Store::open(p))
        .map_err(open_err)?;

    // BUG-049/DEV-121 fix: Welcome 에서 연 길드도 시동 sync 적용. 기존엔
    // sync_on_open 이 앱 부팅 시 '초기 길드' 에만 돌아, Welcome → 길드 열기
    // 흐름에서는 외부 편집이 동기화 안 돼 admin drift 가 남았다 (asset scope 와
    // 동일한 swap 누락 패턴). swap 전 새 store 에 sync.
    if let Err(e) =
        tauri::async_runtime::block_on(openguild_core::incremental::sync_on_open(&new_store))
    {
        eprintln!("[openguild-gui] warn: sync_on_open 실패 — {e:#}");
    }

    // 2. recents 등록 (실패해도 swap 진행).
    if let Err(e) = openguild_core::recents::add(p) {
        eprintln!("[openguild-gui] warn: recents 갱신 실패 — {e:#}");
    }

    // 3-4. 기존 store 해제 + 새 store 등록.
    app.unmanage::<openguild_core::Store>();
    app.manage(new_store);

    // DEV-087 fix2: 새 길드의 디렉토리를 asset protocol scope 에 추가.
    // startup 의 asset scope 는 초기 길드만 allow 하므로, Welcome 에서 다른
    // 길드를 열면 그 길드의 `.guild/assets|attachments` 가 scope 밖이 되어
    // 배너/첨부의 asset:// URL 이 차단됐다 (이미지 안 뜸). swap 마다 재적용.
    if let Err(e) = app.asset_protocol_scope().allow_directory(p, true) {
        eprintln!("[openguild-gui] warn: asset scope allow 실패 — {e:#}");
    }

    // 5. launch mode → guild. unmanage/manage 동일 패턴.
    app.unmanage::<crate::LaunchInfo>();
    app.manage(crate::LaunchInfo {
        mode: "guild",
        uninit_path: None,
    });

    Ok(())
}

// ─────────────────────── Campaign (DEV-011) ───────────────────────

#[tauri::command]
pub async fn list_campaigns(
    store: State<'_, Store>,
    status: Option<String>,
) -> Result<Vec<CampaignRow>, String> {
    match status.as_deref() {
        Some(s) => {
            if s != "active" && s != "done" {
                return Err(format!(
                    "invalid status '{s}' (expected 'active' or 'done')"
                ));
            }
            camp_svc::list_by_status(&store.index_pool, s)
                .await
                .map_err(err)
        }
        None => camp_svc::list_alive(&store.index_pool).await.map_err(err),
    }
}

#[tauri::command]
pub async fn create_campaign(
    store: State<'_, Store>,
    body: CreateCampaignRequest,
) -> Result<CampaignRow, String> {
    camp_ops::create_campaign(&store, body).await.map_err(err)
}

#[tauri::command]
pub async fn get_campaign(
    store: State<'_, Store>,
    slug: String,
) -> Result<CampaignDetail, String> {
    let mut detail = camp_ops::fetch_detail(&store, &slug).await.map_err(err)?;
    // DEV-156: 첨부 목록(sidecar) 채우기.
    detail.attachments = openguild_core::ops::attachments::list_campaign_attachments(&store, &slug);
    Ok(detail)
}

#[tauri::command]
pub async fn update_campaign(
    store: State<'_, Store>,
    slug: String,
    body: UpdateCampaignRequest,
) -> Result<CampaignRow, String> {
    let row = camp_svc::fetch_by_slug(&store.index_pool, &slug)
        .await
        .map_err(err)?;
    camp_ops::update_campaign(&store, row.id, body)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_campaign(
    store: State<'_, Store>,
    slug: String,
) -> Result<(), String> {
    let row = camp_svc::fetch_by_slug(&store.index_pool, &slug)
        .await
        .map_err(err)?;
    camp_ops::delete_campaign(&store, row.id).await.map_err(err)
}

#[tauri::command]
pub async fn campaign_link_quest(
    store: State<'_, Store>,
    slug: String,
    body: LinkQuestRequest,
) -> Result<(), String> {
    let row = camp_svc::fetch_by_slug(&store.index_pool, &slug)
        .await
        .map_err(err)?;
    camp_ops::link_quest_by_slug(&store, row.id, &body.quest_slug)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn campaign_unlink_quest(
    store: State<'_, Store>,
    slug: String,
    quest_slug: String,
) -> Result<(), String> {
    let row = camp_svc::fetch_by_slug(&store.index_pool, &slug)
        .await
        .map_err(err)?;
    camp_ops::unlink_quest_by_slug(&store, row.id, &quest_slug)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn campaign_checklist_add(
    store: State<'_, Store>,
    slug: String,
    body: AddChecklistRequest,
) -> Result<CampaignChecklistItem, String> {
    let row = camp_svc::fetch_by_slug(&store.index_pool, &slug)
        .await
        .map_err(err)?;
    camp_ops::add_checklist_line(&store, row.id, &body.text)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn campaign_checklist_set(
    store: State<'_, Store>,
    slug: String,
    index: usize,
    checked: bool,
) -> Result<(), String> {
    let row = camp_svc::fetch_by_slug(&store.index_pool, &slug)
        .await
        .map_err(err)?;
    camp_ops::set_checklist_checked_by_index(&store, row.id, index, checked)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn campaign_checklist_rm(
    store: State<'_, Store>,
    slug: String,
    index: usize,
) -> Result<(), String> {
    let row = camp_svc::fetch_by_slug(&store.index_pool, &slug)
        .await
        .map_err(err)?;
    camp_ops::remove_checklist_by_index(&store, row.id, index)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn list_campaign_active_summaries(
    store: State<'_, Store>,
) -> Result<Vec<CampaignSummary>, String> {
    camp_svc::list_active_summaries(&store.index_pool)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn list_campaign_upcoming_summaries(
    store: State<'_, Store>,
    days: Option<i64>,
) -> Result<Vec<CampaignSummary>, String> {
    let today = openguild_core::time::today_local_iso_date();
    let d = days.unwrap_or(7);
    camp_svc::list_upcoming_summaries(&store.index_pool, &today, d)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn list_campaigns_for_quest(
    store: State<'_, Store>,
    quest_id: i64,
) -> Result<Vec<CampaignRow>, String> {
    camp_svc::list_for_quest(&store.index_pool, quest_id)
        .await
        .map_err(err)
}

// ─────────────────────── DEV-012/016: content 응답 공용 shape ───────────────────────

/// `.guild/rules.md` / `.guild/quests/{slug}.{comments,memo}.md` 의 GET 응답.
/// 파일 부재 시 `content: null`.
#[derive(serde::Serialize)]
pub struct ContentResponse {
    pub content: Option<String>,
}

// DEV-016 호환 alias.
pub type RulesResponse = ContentResponse;

#[tauri::command]
pub fn get_rules(store: State<'_, Store>) -> Result<RulesResponse, String> {
    let content = openguild_core::ops::rules::get_rules(&store).map_err(err)?;
    Ok(RulesResponse { content })
}

#[tauri::command]
pub async fn set_rules(
    store: State<'_, Store>,
    content: String,
) -> Result<RulesResponse, String> {
    openguild_core::ops::rules::set_rules(&store, content.clone())
        .await
        .map_err(err)?;
    Ok(RulesResponse {
        content: Some(content),
    })
}

// ─── DEV-016 (multi-file): 다중 길드 규칙 ───

use openguild_core::repo::rules::RuleEntry;

#[derive(serde::Serialize)]
pub struct RulesListResponse {
    pub entries: Vec<RuleEntry>,
}

#[derive(serde::Serialize)]
pub struct RuleResponse {
    pub slug: String,
    pub content: Option<String>,
}

#[tauri::command]
pub fn list_rules(store: State<'_, Store>) -> Result<RulesListResponse, String> {
    let entries = openguild_core::ops::rules::list_rules(&store).map_err(err)?;
    Ok(RulesListResponse { entries })
}

#[tauri::command]
pub fn get_rule(store: State<'_, Store>, slug: String) -> Result<RuleResponse, String> {
    let content = openguild_core::ops::rules::get_rule(&store, &slug).map_err(err)?;
    Ok(RuleResponse { slug, content })
}

#[tauri::command]
pub async fn set_rule(
    store: State<'_, Store>,
    slug: String,
    content: String,
) -> Result<RuleResponse, String> {
    openguild_core::ops::rules::set_rule(&store, &slug, content.clone())
        .await
        .map_err(err)?;
    Ok(RuleResponse {
        slug,
        content: Some(content),
    })
}

#[tauri::command]
pub async fn create_rule(
    store: State<'_, Store>,
    slug: String,
    content: Option<String>,
) -> Result<RuleResponse, String> {
    let c = content.unwrap_or_default();
    openguild_core::ops::rules::create_rule(&store, &slug, c.clone())
        .await
        .map_err(err)?;
    Ok(RuleResponse {
        slug,
        content: Some(c),
    })
}

#[tauri::command]
pub async fn delete_rule(store: State<'_, Store>, slug: String) -> Result<(), String> {
    openguild_core::ops::rules::delete_rule(&store, &slug)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn rename_rule(
    store: State<'_, Store>,
    slug: String,
    new_slug: String,
) -> Result<RuleResponse, String> {
    openguild_core::ops::rules::rename_rule(&store, &slug, &new_slug)
        .await
        .map_err(err)?;
    let content = openguild_core::ops::rules::get_rule(&store, &new_slug).map_err(err)?;
    Ok(RuleResponse {
        slug: new_slug,
        content,
    })
}

// ─────────────────────── DEV-012 / DEV-094: 댓글 / 메모 ───────────────────────

// 메모는 단일 텍스트 그대로 (DEV-012).
#[tauri::command]
pub fn get_memo(
    store: State<'_, Store>,
    slug: String,
) -> Result<ContentResponse, String> {
    let content = openguild_core::ops::comments::get_memo(&store, &slug).map_err(err)?;
    Ok(ContentResponse { content })
}

#[tauri::command]
pub async fn set_memo(
    store: State<'_, Store>,
    slug: String,
    content: String,
) -> Result<ContentResponse, String> {
    openguild_core::ops::comments::set_memo(&store, &slug, content.clone())
        .await
        .map_err(err)?;
    Ok(ContentResponse {
        content: Some(content),
    })
}

// DEV-094: 댓글은 entry 단위. 응답은 항상 `{ entries: [...] }` 또는 단일 entry.
use openguild_core::repo::comments::CommentEntry;

#[derive(serde::Serialize)]
pub struct CommentsListResponse {
    pub entries: Vec<CommentEntry>,
}

#[tauri::command]
pub fn list_comments(
    store: State<'_, Store>,
    slug: String,
) -> Result<CommentsListResponse, String> {
    let entries =
        openguild_core::ops::comments::list_comment_entries(&store, &slug).map_err(err)?;
    Ok(CommentsListResponse { entries })
}

#[tauri::command]
pub async fn add_comment(
    store: State<'_, Store>,
    slug: String,
    author: Option<String>,
    body: String,
    parent_id: Option<u64>,
) -> Result<CommentEntry, String> {
    openguild_core::ops::comments::add_comment_entry(
        &store,
        &slug,
        author.unwrap_or_default(),
        body,
        parent_id,
    )
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn update_comment(
    store: State<'_, Store>,
    slug: String,
    id: u64,
    body: String,
) -> Result<CommentEntry, String> {
    openguild_core::ops::comments::update_comment_entry(&store, &slug, id, body)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_comment(
    store: State<'_, Store>,
    slug: String,
    id: u64,
) -> Result<(), String> {
    openguild_core::ops::comments::delete_comment_entry(&store, &slug, id)
        .await
        .map_err(err)
}

/// DEV-108: 이모지 반응 토글.
#[tauri::command]
pub async fn toggle_comment_reaction(
    store: State<'_, Store>,
    slug: String,
    id: u64,
    emoji: String,
    author: String,
) -> Result<CommentEntry, String> {
    openguild_core::ops::comments::toggle_comment_reaction(&store, &slug, id, &emoji, &author)
        .await
        .map_err(err)
}

/// DEV-142: 토론(discussion) 플래그 토글. discussion 댓글이 미해결이면 quest
/// 완료 전환이 차단된다.
#[tauri::command]
pub async fn toggle_comment_discussion(
    store: State<'_, Store>,
    slug: String,
    id: u64,
) -> Result<CommentEntry, String> {
    openguild_core::ops::comments::toggle_comment_discussion(&store, &slug, id)
        .await
        .map_err(err)
}

/// DEV-142: discussion 댓글의 resolved 토글.
#[tauri::command]
pub async fn toggle_comment_resolved(
    store: State<'_, Store>,
    slug: String,
    id: u64,
) -> Result<CommentEntry, String> {
    openguild_core::ops::comments::toggle_comment_resolved(&store, &slug, id)
        .await
        .map_err(err)
}

// ─── DEV-100: Campaign 댓글 / 메모 — quest 패턴 미러 ───

#[tauri::command]
pub fn list_campaign_comments(
    store: State<'_, Store>,
    slug: String,
) -> Result<CommentsListResponse, String> {
    let entries =
        openguild_core::ops::campaign_comments::list_entries(&store, &slug).map_err(err)?;
    Ok(CommentsListResponse { entries })
}

#[tauri::command]
pub async fn add_campaign_comment(
    store: State<'_, Store>,
    slug: String,
    author: Option<String>,
    body: String,
    parent_id: Option<u64>,
) -> Result<CommentEntry, String> {
    openguild_core::ops::campaign_comments::add_entry(
        &store,
        &slug,
        author.unwrap_or_default(),
        body,
        parent_id,
    )
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn update_campaign_comment(
    store: State<'_, Store>,
    slug: String,
    id: u64,
    body: String,
) -> Result<CommentEntry, String> {
    openguild_core::ops::campaign_comments::update_entry(&store, &slug, id, body)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_campaign_comment(
    store: State<'_, Store>,
    slug: String,
    id: u64,
) -> Result<(), String> {
    openguild_core::ops::campaign_comments::delete_entry(&store, &slug, id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn toggle_campaign_comment_reaction(
    store: State<'_, Store>,
    slug: String,
    id: u64,
    emoji: String,
    author: String,
) -> Result<CommentEntry, String> {
    openguild_core::ops::campaign_comments::toggle_reaction(&store, &slug, id, &emoji, &author)
        .await
        .map_err(err)
}

#[tauri::command]
pub fn get_campaign_memo(
    store: State<'_, Store>,
    slug: String,
) -> Result<ContentResponse, String> {
    let content =
        openguild_core::ops::campaign_comments::get_memo(&store, &slug).map_err(err)?;
    Ok(ContentResponse { content })
}

// ─── DEV-060: 퀘스트 템플릿 ───

/// 템플릿 DTO — repo::TemplateFile 은 Serialize 미구현이라 평탄화해서 반환.
#[derive(serde::Serialize)]
pub struct TemplateDto {
    pub name: String,
    pub title: Option<String>,
    /// type prefix (예 "BUG").
    pub r#type: Option<String>,
    pub urgency: Option<i64>,
    pub tags: Vec<String>,
    pub body: String,
}

#[tauri::command]
pub fn list_templates(store: State<'_, Store>) -> Result<Vec<TemplateDto>, String> {
    let templates =
        openguild_core::repo::list_templates(&store.paths).map_err(|e| format!("{e:#}"))?;
    Ok(templates
        .into_iter()
        .map(|t| TemplateDto {
            name: t.name,
            title: t.frontmatter.title,
            r#type: t.frontmatter.type_prefix,
            urgency: t.frontmatter.urgency,
            tags: t.frontmatter.tags,
            body: t.body,
        })
        .collect())
}

/// DEV-158: 현재 입력을 템플릿으로 저장 — `.guild/templates/{name}.md`.
/// `force=false` 인데 같은 이름이 있으면 에러 (프론트에서 덮어쓰기 확인 후 재호출).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_template(
    store: State<'_, Store>,
    name: String,
    title: Option<String>,
    r#type: Option<String>,
    urgency: Option<i64>,
    tags: Vec<String>,
    body: String,
    force: bool,
) -> Result<String, String> {
    let tpl = openguild_core::repo::TemplateFile {
        name,
        frontmatter: openguild_core::repo::TemplateFrontmatter {
            title,
            type_prefix: r#type,
            urgency,
            tags,
        },
        body,
    };
    let path = openguild_core::repo::save_template(&store.paths, &tpl, force)
        .map_err(|e| format!("{e:#}"))?;
    Ok(path.display().to_string())
}

// ─── DEV-087: 캠페인 배너 이미지 ───

/// source 파일을 `.guild/assets/` 로 복사 + frontmatter / DB 갱신.
/// 갱신된 campaign row 반환 (image_path 포함).
#[tauri::command]
pub async fn set_campaign_banner(
    store: State<'_, Store>,
    slug: String,
    source_path: String,
) -> Result<openguild_core::models::CampaignRow, String> {
    openguild_core::ops::campaigns::set_banner_image(
        &store,
        &slug,
        std::path::Path::new(&source_path),
    )
    .await
    .map_err(err)
}

/// DEV-069: 본문 첨부 저장 — 클립보드 paste / 드래그&드랍 파일을 base64 로
/// 받아 `.guild/attachments/` 에 write + blob 백업. 반환: 본문 참조용
/// 상대 경로 (`attachments/...`).
#[tauri::command]
pub async fn save_attachment(
    store: State<'_, Store>,
    data_base64: String,
    ext: String,
) -> Result<String, String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("base64 decode 실패: {e}"))?;
    openguild_core::ops::attachments::save_attachment(&store, &bytes, &ext)
        .await
        .map_err(err)
}

/// DEV-171/BUG-081: `.guild` 상대 경로를 절대 경로로 해석 (traversal 가드 + 존재 확인).
fn resolve_guild_rel(store: &Store, rel: &str) -> Result<std::path::PathBuf, String> {
    if rel.contains("..") {
        return Err("잘못된 첨부 경로".into());
    }
    let path = store.paths.dot_guild().join(rel);
    if !path.exists() {
        return Err(format!("파일 없음: {rel}"));
    }
    Ok(path)
}

/// BUG-081: 첨부 파일을 OS 기본 앱으로 열기 (로컬 미리보기/열기).
#[tauri::command]
pub fn open_guild_file(
    app: tauri::AppHandle,
    store: State<'_, Store>,
    rel: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let path = resolve_guild_rel(&store, &rel)?;
    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("열기 실패: {e}"))
}

/// BUG-081: 첨부 파일을 dest 로 복사 (개별/전체 다운로드 = 다른 위치로 저장).
#[tauri::command]
pub fn copy_guild_file(store: State<'_, Store>, rel: String, dest: String) -> Result<(), String> {
    let src = resolve_guild_rel(&store, &rel)?;
    std::fs::copy(&src, &dest).map_err(|e| format!("복사 실패: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn clear_campaign_banner(
    store: State<'_, Store>,
    slug: String,
) -> Result<openguild_core::models::CampaignRow, String> {
    openguild_core::ops::campaigns::clear_banner_image(&store, &slug)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn set_campaign_memo(
    store: State<'_, Store>,
    slug: String,
    content: String,
) -> Result<ContentResponse, String> {
    openguild_core::ops::campaign_comments::set_memo(&store, &slug, content.clone())
        .await
        .map_err(err)?;
    Ok(ContentResponse {
        content: Some(content),
    })
}
