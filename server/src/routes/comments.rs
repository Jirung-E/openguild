//! DEV-012 / DEV-094: Quest 별 댓글 / 메모 HTTP 어댑터.
//!
//! 댓글은 entry 단위 (`GET 목록 / POST 추가 / PATCH 본문수정 / DELETE`).
//! 메모는 단일 텍스트 그대로.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use openguild_core::ops::comments as ops;
use openguild_core::repo::comments::CommentEntry;
use openguild_core::Store;

// ─── BUG-231: 길드 전체 댓글 검색 ───

#[derive(Debug, Default, Deserialize)]
pub struct GlobalCommentsQuery {
    pub author: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub grep: Option<String>,
    #[serde(default)]
    pub discussion: bool,
    #[serde(default)]
    pub unresolved: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GlobalCommentResponse {
    pub scope: String,
    pub slug: String,
    pub entry_id: i64,
    pub ts: String,
    pub author: String,
    pub body: String,
    pub discussion: bool,
    pub resolved: bool,
    pub parent_id: Option<i64>,
    pub pinned: bool,
    pub reactions: String,
}

/// Quest + campaign 댓글을 한 길드 안에서 횡단 검색한다. 정렬/답글 트리/limit은
/// CLI가 로컬 모드와 동일하게 후처리할 수 있도록 전체 결과를 오래된 순으로 반환.
pub async fn search_comments(
    State(store): State<Store>,
    Query(query): Query<GlobalCommentsQuery>,
) -> AppResult<Json<Vec<GlobalCommentResponse>>> {
    let mut conds_q = String::new();
    let mut conds_c = String::new();
    let mut push_both = |quest: &str, campaign: &str| {
        conds_q.push_str(" AND ");
        conds_q.push_str(quest);
        conds_c.push_str(" AND ");
        conds_c.push_str(campaign);
    };
    let mut binds = Vec::new();
    if let Some(author) = query.author {
        push_both("LOWER(c.author) = LOWER(?)", "LOWER(c.author) = LOWER(?)");
        binds.push(author);
    }
    if let Some(since) = query.since {
        push_both("c.ts >= ?", "c.ts >= ?");
        binds.push(openguild_core::time::normalize_filter_ts(&since));
    }
    if let Some(until) = query.until {
        push_both("c.ts <= ?", "c.ts <= ?");
        binds.push(openguild_core::time::normalize_filter_ts(&until));
    }
    if let Some(grep) = query.grep {
        push_both(
            "LOWER(c.body) LIKE '%' || LOWER(?) || '%'",
            "LOWER(c.body) LIKE '%' || LOWER(?) || '%'",
        );
        binds.push(grep);
    }
    if query.discussion {
        push_both("c.discussion = 1", "0 = 1");
    }
    if query.unresolved {
        push_both("c.discussion = 1 AND c.resolved = 0", "0 = 1");
    }

    let sql = format!(
        "SELECT * FROM (
           SELECT 'quest' AS scope,
                  qt.prefix || '-' || printf('%03d', q.number) AS slug,
                  c.entry_id, c.ts, c.author, c.body,
                  c.discussion, c.resolved, c.parent_id, c.pinned, c.reactions
             FROM quest_comments c
             JOIN quests q ON q.id = c.quest_id
             JOIN quest_types qt ON qt.id = q.quest_type_id
            WHERE 1 = 1{conds_q}
           UNION ALL
           SELECT 'campaign' AS scope, ca.campaign_slug AS slug,
                  c.entry_id, c.ts, c.author, c.body,
                  0 AS discussion, 0 AS resolved, c.parent_id, c.pinned, c.reactions
             FROM campaign_comments c
             JOIN campaigns ca ON ca.id = c.campaign_id
            WHERE 1 = 1{conds_c}
         )
         ORDER BY ts ASC"
    );
    let mut sql_query = sqlx::query_as::<_, GlobalCommentResponse>(&sql);
    for value in &binds {
        sql_query = sql_query.bind(value);
    }
    for value in &binds {
        sql_query = sql_query.bind(value);
    }
    Ok(Json(sql_query.fetch_all(&store.index_pool).await?))
}

// ─── 메모: 단일 텍스트 ───

#[derive(Debug, Serialize)]
pub struct ContentResponse {
    /// 파일 부재 시 `null`.
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetContentRequest {
    pub content: String,
}

pub async fn get_memo(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<ContentResponse>> {
    let content = ops::get_memo(&store, &slug)?;
    Ok(Json(ContentResponse { content }))
}

pub async fn set_memo(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<SetContentRequest>,
) -> AppResult<Json<ContentResponse>> {
    ops::set_memo(&store, &slug, body.content.clone()).await?;
    Ok(Json(ContentResponse {
        content: Some(body.content),
    }))
}

// ─── DEV-094: 댓글 entry 단위 ───

#[derive(Debug, Serialize)]
pub struct CommentsListResponse {
    pub entries: Vec<CommentEntry>,
}

#[derive(Debug, Deserialize)]
pub struct AddCommentRequest {
    #[serde(default)]
    pub author: String,
    pub body: String,
    /// DEV-094 후속: 답글이면 부모 entry id. None / 미지정 → top-level.
    #[serde(default)]
    pub parent_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommentRequest {
    pub body: String,
}

pub async fn list_comments(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<CommentsListResponse>> {
    let entries = ops::list_comment_entries(&store, &slug)?;
    Ok(Json(CommentsListResponse { entries }))
}

pub async fn add_comment(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<AddCommentRequest>,
) -> AppResult<Json<CommentEntry>> {
    let entry =
        ops::add_comment_entry(&store, &slug, body.author, body.body, body.parent_id)
            .await?;
    Ok(Json(entry))
}

pub async fn update_comment(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
    Json(body): Json<UpdateCommentRequest>,
) -> AppResult<Json<CommentEntry>> {
    let entry = ops::update_comment_entry(&store, &slug, id, body.body).await?;
    Ok(Json(entry))
}

pub async fn delete_comment(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
) -> AppResult<axum::http::StatusCode> {
    ops::delete_comment_entry(&store, &slug, id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ─── DEV-108: 이모지 반응 ───

#[derive(Debug, Deserialize)]
pub struct ToggleReactionRequest {
    pub emoji: String,
    // DEV-108: 누가 반응했는지 — 미지정 시 빈 문자열 → ops 가 '(익명)' 처리.
    #[serde(default)]
    pub author: String,
}

pub async fn toggle_reaction(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
    Json(body): Json<ToggleReactionRequest>,
) -> AppResult<Json<CommentEntry>> {
    let entry = ops::toggle_comment_reaction(&store, &slug, id, &body.emoji, &body.author).await?;
    Ok(Json(entry))
}

// ─── DEV-142: 토론(discussion) 플래그 / resolve 토글 ───

pub async fn toggle_discussion(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
) -> AppResult<Json<CommentEntry>> {
    let entry = ops::toggle_comment_discussion(&store, &slug, id).await?;
    Ok(Json(entry))
}

pub async fn toggle_resolved(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
) -> AppResult<Json<CommentEntry>> {
    let entry = ops::toggle_comment_resolved(&store, &slug, id).await?;
    Ok(Json(entry))
}

// ─── DEV-234: 상단 고정(pin) 토글 ───

pub async fn toggle_pinned(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
) -> AppResult<Json<CommentEntry>> {
    let entry = ops::toggle_comment_pinned(&store, &slug, id).await?;
    Ok(Json(entry))
}

// ─── DEV-100: Campaign 댓글 / 메모 — quest 와 동일 형식 ───

use openguild_core::ops::campaign_comments as cops;

pub async fn camp_list_comments(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<CommentsListResponse>> {
    let entries = cops::list_entries(&store, &slug)?;
    Ok(Json(CommentsListResponse { entries }))
}

pub async fn camp_add_comment(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<AddCommentRequest>,
) -> AppResult<Json<CommentEntry>> {
    let entry = cops::add_entry(&store, &slug, body.author, body.body, body.parent_id).await?;
    Ok(Json(entry))
}

pub async fn camp_update_comment(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
    Json(body): Json<UpdateCommentRequest>,
) -> AppResult<Json<CommentEntry>> {
    let entry = cops::update_entry(&store, &slug, id, body.body).await?;
    Ok(Json(entry))
}

pub async fn camp_delete_comment(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
) -> AppResult<axum::http::StatusCode> {
    cops::delete_entry(&store, &slug, id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn camp_toggle_reaction(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
    Json(body): Json<ToggleReactionRequest>,
) -> AppResult<Json<CommentEntry>> {
    let entry = cops::toggle_reaction(&store, &slug, id, &body.emoji, &body.author).await?;
    Ok(Json(entry))
}

// DEV-234: 캠페인 댓글도 pin 지원 (discussion 과 달리 quest 전용 아님).
pub async fn camp_toggle_pinned(
    State(store): State<Store>,
    Path((slug, id)): Path<(String, u64)>,
) -> AppResult<Json<CommentEntry>> {
    let entry = cops::toggle_pinned(&store, &slug, id).await?;
    Ok(Json(entry))
}

pub async fn camp_get_memo(
    State(store): State<Store>,
    Path(slug): Path<String>,
) -> AppResult<Json<ContentResponse>> {
    let content = cops::get_memo(&store, &slug)?;
    Ok(Json(ContentResponse { content }))
}

pub async fn camp_set_memo(
    State(store): State<Store>,
    Path(slug): Path<String>,
    Json(body): Json<SetContentRequest>,
) -> AppResult<Json<ContentResponse>> {
    cops::set_memo(&store, &slug, body.content.clone()).await?;
    Ok(Json(ContentResponse {
        content: Some(body.content),
    }))
}
