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
