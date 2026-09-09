//! DEV-374: 이벤트에 실리는 **공개 모양**.
//!
//! 내부 구조체(`QuestRow` / `CommentEntry`)를 그대로 직렬화하지 않는다.
//! 그대로 흘리면 이름만 계약이고 **내용은 계속 흔들린다** — 필드를 하나 바꿀
//! 때마다 플러그인이 깨진다. 여기서 고른 필드만 나간다.
//!
//! 고르는 기준: **플러그인이 판단하거나 사람에게 보여줄 것**. 내부 id 나
//! 캐시 계산값은 넣지 않는다.

use crate::models::quest::QuestRow;
use crate::repo::comments::CommentEntry;
use serde_json::{Value, json};

/// 퀘스트 한 건.
pub fn quest(q: &QuestRow) -> Value {
    json!({
        "id": q.quest_id,          // "DEV-007" — 사람이 쓰는 식별자
        "title": q.title,
        "type": q.type_prefix,
        "status": q.status_slug,
        "urgency": q.urgency,
        "tags": q.tags,
        "created_at": q.created_at,
        "updated_at": q.updated_at,
    })
}

/// 상태 변경처럼 "무엇에서 무엇으로" 가 핵심인 이벤트의 부가 정보.
pub fn change(from: impl Into<Value>, to: impl Into<Value>) -> Value {
    json!({ "from": from.into(), "to": to.into() })
}

/// 댓글 한 건. **본문을 싣는다** — AI 전송이 주 용도라 없으면 쓸모가 없다.
pub fn comment(c: &CommentEntry) -> Value {
    json!({
        "id": c.id,
        "author": c.author,
        "body": c.body,
        "ts": c.ts,
        "parent_id": c.parent_id,
        "discussion": c.discussion,
        "resolved": c.resolved,
        "reactions": c.reactions,
        "pinned": c.pinned,
    })
}

/// 삭제처럼 대상이 이미 사라져 전체를 실을 수 없을 때.
pub fn quest_ref(quest_id: &str) -> Value {
    json!({ "id": quest_id })
}

pub fn comment_ref(id: u64) -> Value {
    json!({ "id": id })
}

// ── DEV-388: 나머지 리소스 ──────────────────────────────
//
// 고르는 기준은 위와 같다 — **플러그인이 판단하거나 사람에게 보여줄 것**.
// 내부 id 와 캐시 계산값은 안 넣는다.

/// 캠페인 한 건.
pub fn campaign(c: &crate::models::CampaignRow) -> Value {
    json!({
        "id": c.campaign_slug,     // "C-001" — 사람이 쓰는 식별자
        "title": c.title,
        "status": c.status,
        "started_at": c.started_at,
        "ended_at": c.ended_at,
        "created_at": c.created_at,
        "updated_at": c.updated_at,
    })
}

/// 캠페인이 이미 사라져 전체를 실을 수 없을 때.
pub fn campaign_ref(campaign_slug: &str) -> Value {
    json!({ "id": campaign_slug })
}

/// DEV-388: 댓글이 **어디에 달렸나.**
///
/// 캠페인 댓글을 `campaign_comment.*` 로 따로 내는 대신 `comment.*` 로 합쳤다
/// (admin 확정). "댓글이 달리면 알림" 을 구독 하나로 끝낼 수 있고 이름이 10개
/// 안 는다. 대신 **어디 것인지**는 반드시 실어야 한다.
pub fn target(kind: &str, id: &str) -> Value {
    json!({ "kind": kind, "id": id })
}

/// 도서관 문서 한 건. 본문은 안 싣는다 — 책은 길고, 필요하면 플러그인이 읽는다.
pub fn book(b: &crate::ops::library::LibraryDocRow) -> Value {
    json!({
        "id": b.book_id(),
        "title": b.title,
        "path": b.path,
        "tags": b.tags,
        "created_at": b.created_at,
        "updated_at": b.updated_at,
    })
}

pub fn book_ref(book_id: &str) -> Value {
    json!({ "id": book_id })
}

/// 도서관 폴더 — 경로가 곧 정체성이다.
pub fn folder(path: &str) -> Value {
    json!({ "path": path })
}

/// 규칙 한 건. **본문을 싣는다** — 규칙은 짧고, 바뀐 내용을 보는 것이 요점이다.
pub fn rule(r: &crate::repo::rules::RuleEntry) -> Value {
    json!({
        "slug": r.slug,
        "content": r.content,
        "tags": r.tags,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
    })
}

pub fn rule_ref(slug: &str) -> Value {
    json!({ "slug": slug })
}

/// 첨부 한 건. 바이트는 안 싣는다 — 경로로 접근한다.
pub fn attachment(name: &str, path: &str) -> Value {
    json!({ "name": name, "path": path })
}

/// 퀘스트 타입 정의.
pub fn quest_type(t: &crate::models::QuestType) -> Value {
    json!({
        "prefix": t.prefix,
        "color": t.color,
        "description": t.description,
    })
}

pub fn quest_type_ref(prefix: &str) -> Value {
    json!({ "prefix": prefix })
}

/// 상태 정의. `counts_as_done` 은 자동화가 실제로 보는 값이다.
pub fn status(s: &crate::models::QuestStatus) -> Value {
    json!({
        "slug": s.slug,
        "name_en": s.name_en,
        "name_ko": s.name_ko,
        "color": s.color,
        "sort_order": s.sort_order,
        "counts_as_done": s.counts_as_done,
    })
}

pub fn status_ref(slug: &str) -> Value {
    json!({ "slug": slug })
}

/// 태그 정의.
pub fn tag_def(t: &crate::models::QuestTagDef) -> Value {
    json!({
        "slug": t.slug,
        "color": t.color,
        "description": t.description,
    })
}

pub fn tag_ref(slug: &str) -> Value {
    json!({ "slug": slug })
}
