use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use openguild_core::models::{QuestStatus, QuestTagDef, QuestType};
use openguild_core::ops::meta as meta_ops;
use openguild_core::services::meta as svc;
use openguild_core::Store;

pub async fn list_quest_types(State(store): State<Store>) -> AppResult<Json<Vec<QuestType>>> {
    Ok(Json(svc::list_quest_types(&store.index_pool).await?))
}

pub async fn list_quest_statuses(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestStatus>>> {
    Ok(Json(svc::list_quest_statuses(&store.index_pool).await?))
}

#[derive(Debug, Serialize)]
pub struct GuildInfo {
    pub name: String,
}

/// `GET /api/guild-info` — DEV-113 후속: 사용자 보고("원격 길드 접속 시
/// 제목이 안 보이거나 잘못 보임") — 브라우저/원격(HTTP) 모드에서 길드
/// 이름을 표시하기 위한 라우트. Tauri 의 `current_guild_name` invoke 와
/// 동일한 fallback 규칙(`recents::guess_name` — marker 파일의 name 또는
/// 디렉토리명)을 재사용해 한쪽만 다른 이름을 보여주는 불일치를 막는다.
pub async fn get_guild_info(State(store): State<Store>) -> Json<GuildInfo> {
    let name = openguild_core::recents::guess_name(&store.paths.guild_root);
    Json(GuildInfo { name })
}

/// DEV-068: tag def 목록.
pub async fn list_tag_defs(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestTagDef>>> {
    Ok(Json(svc::list_quest_tag_defs(&store.index_pool).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpsertTagDefBody {
    pub slug: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub description: String,
}

pub async fn upsert_tag_def(
    State(store): State<Store>,
    Json(body): Json<UpsertTagDefBody>,
) -> AppResult<Json<QuestTagDef>> {
    Ok(Json(
        meta_ops::upsert_tag_def(&store, body.slug, body.color, body.description).await?,
    ))
}

pub async fn delete_tag_def(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    meta_ops::delete_tag_def(&store, slug).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// BUG-231: quest/library frontmatter 캐시에 실제로 사용 중인 태그 distinct 목록.
pub async fn list_tags_in_use(
    State(store): State<Store>,
) -> AppResult<Json<Vec<String>>> {
    Ok(Json(
        openguild_core::services::meta::list_tags_in_use(&store.index_pool).await?,
    ))
}

// ─────────────────────── admin: types/statuses (DEV-193) ───────────────────────
//
// 브라우저/원격(HTTP) 모드의 admin 페이지가 Tauri invoke 와 동일하게 쓸 수
// 있도록 — gui/src/commands.rs 의 admin_list_types 등과 1:1 대응. 경로/요청
// 바디는 transport.ts 의 routeToInvoke 매핑 그대로(`/api/admin/types`,
// `/api/admin/statuses`).

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

pub async fn admin_list_types(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestTypeWithCount>>> {
    let rows = svc::list_quest_types(&store.index_pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let quest_count = meta_ops::count_quests_by_type(&store.index_pool, row.id).await?;
        out.push(QuestTypeWithCount { row, quest_count });
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct CreateTypeBody {
    pub prefix: String,
    pub color: String,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn admin_create_type(
    State(store): State<Store>,
    Json(body): Json<CreateTypeBody>,
) -> AppResult<Json<QuestType>> {
    Ok(Json(
        meta_ops::create_type(&store, body.prefix, body.color, body.description).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct UpdateTypeBody {
    /// BUG-018: prefix rename 통합. 변경하지 않으려면 필드 생략.
    #[serde(default)]
    pub new_prefix: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// `null` outer = 변경 없음, `null` inner (= JS `null` 명시) 는 unset.
    #[serde(default, with = "double_option")]
    pub description: Option<Option<String>>,
}

pub async fn admin_update_type(
    State(store): State<Store>,
    Path(prefix): Path<String>,
    Json(body): Json<UpdateTypeBody>,
) -> AppResult<Json<QuestType>> {
    Ok(Json(
        meta_ops::update_type(&store, prefix, body.new_prefix, body.color, body.description)
            .await?,
    ))
}

pub async fn admin_delete_type(
    State(store): State<Store>,
    Path(prefix): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    meta_ops::delete_type(&store, prefix).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn admin_list_statuses(
    State(store): State<Store>,
) -> AppResult<Json<Vec<QuestStatusWithCount>>> {
    let rows = svc::list_quest_statuses(&store.index_pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let quest_count = meta_ops::count_quests_by_status(&store.index_pool, row.id).await?;
        out.push(QuestStatusWithCount { row, quest_count });
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct CreateStatusBody {
    pub name_en: String,
    pub name_ko: String,
    pub color: String,
    #[serde(default)]
    pub sort_order: Option<i64>,
}

pub async fn admin_create_status(
    State(store): State<Store>,
    Json(body): Json<CreateStatusBody>,
) -> AppResult<Json<QuestStatus>> {
    Ok(Json(
        meta_ops::create_status(&store, body.name_en, body.name_ko, body.color, body.sort_order)
            .await?,
    ))
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

pub async fn admin_update_status(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<UpdateStatusBody>,
) -> AppResult<Json<QuestStatus>> {
    Ok(Json(
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
        .await?,
    ))
}

pub async fn admin_delete_status(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    meta_ops::delete_status(&store, slug).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// serde: `Option<Option<T>>` 필드 생략 vs `null` 구분 — gui/src/commands.rs
/// 의 동명 모듈과 동일.
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

/// REQ-008: `GET /api/backlinks/{kind}/{id}` — 이 문서를 참조하는 문서 목록.
///
/// 색인(`doc_links`)은 reindex 가 만든다. 여기서는 읽기만 한다.
pub async fn list_backlinks(
    State(store): State<Store>,
    Path((kind, id)): Path<(String, String)>,
) -> AppResult<Json<Vec<openguild_core::ops::backlinks::Backlink>>> {
    // 알 수 없는 kind 는 조용히 빈 목록이 아니라 400 — 오타를 삼키면 "backlink 가
    // 없는 문서" 와 구분이 안 된다.
    if !matches!(kind.as_str(), "quest" | "campaign" | "rule" | "book") {
        return Err(openguild_core::error::AppError::BadRequest(format!(
            "알 수 없는 문서 종류: '{kind}' (quest / campaign / rule / book)"
        ))
        .into());
    }
    let rows = openguild_core::ops::backlinks::list_backlinks(&store.index_pool, &kind, &id).await?;
    Ok(Json(rows))
}

/// REQ-009: `GET /api/search?q=…&in=body,comment,attachment[,memo]&limit=N`
///
/// **기본 검색(`/api/quests?search=`)과 별개**다. 그쪽은 title/description/slug
/// 만 보는 빠른 경로로 두고, 이쪽이 댓글·첨부 이름까지 훑는다.
///
/// `in` 을 생략하면 **메모를 뺀** 기본 영역만 본다 — 메모는 gitignore 되는 개인
/// 기록이고 이 서버는 인증이 없다. 명시적으로 요청해야 포함된다.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub r#in: Option<String>,
    pub limit: Option<usize>,
}

pub async fn enhanced_search(
    State(store): State<Store>,
    Query(p): Query<SearchQuery>,
) -> AppResult<Json<Vec<openguild_core::ops::search::SearchHit>>> {
    use openguild_core::ops::search::SearchField;
    let fields: Vec<SearchField> = match p.r#in.as_deref() {
        Some(raw) if !raw.trim().is_empty() => {
            let mut out = Vec::new();
            for part in raw.split(',') {
                match SearchField::parse(part) {
                    Some(f) if !out.contains(&f) => out.push(f),
                    Some(_) => {}
                    // 오타를 조용히 무시하면 "왜 안 나오지" 로 이어진다.
                    None => {
                        return Err(openguild_core::error::AppError::BadRequest(format!(
                            "알 수 없는 검색 영역: '{}' (body / comment / attachment / memo)",
                            part.trim()
                        ))
                        .into());
                    }
                }
            }
            out
        }
        _ => SearchField::defaults(),
    };
    let hits = openguild_core::ops::search::search(&store, &p.q, &fields, p.limit).await?;
    Ok(Json(hits))
}
