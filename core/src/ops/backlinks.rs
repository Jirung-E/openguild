//! REQ-008: backlink 조회 — "이 문서를 참조하는 문서".
//!
//! 색인은 reindex 가 만든다(`doc_links`, 6e 단계). 여기서는 읽기만 한다 —
//! [`BOOK-001`] 불변식대로 DB → 파일 역류는 없다.

use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;

/// backlink 한 건 — 참조하는 쪽 문서.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Backlink {
    /// 'quest' | 'campaign' | 'rule' | 'book'
    pub kind: String,
    /// quest_id / campaign_slug / rule slug / BOOK-NNN
    pub id: String,
    /// 표시용 제목. 규칙은 slug 가 곧 제목이라 같은 값이 온다.
    pub title: String,
}

/// `kind`/`id` 문서를 참조하는 문서 목록. 종류 → id 순 정렬.
///
/// `id` 는 대소문자 무시로 맞춘다(색인이 대문자로 정규화해 저장).
pub async fn list_backlinks(pool: &SqlitePool, kind: &str, id: &str) -> AppResult<Vec<Backlink>> {
    // 제목은 각 종류의 캐시 테이블에서 가져온다. 규칙은 index.db 에 캐시가
    // 없어(file-truth-db-cache §4) slug 를 그대로 제목으로 쓴다.
    let rows: Vec<Backlink> = sqlx::query_as(
        r#"
        SELECT l.src_kind AS kind, l.src_id AS id,
               COALESCE(
                   (SELECT q.title FROM quests q
                      JOIN quest_types t ON t.id = q.quest_type_id
                     WHERE l.src_kind = 'quest'
                       AND t.prefix || '-' || printf('%03d', q.number) = l.src_id),
                   (SELECT c.title FROM campaigns c
                     WHERE l.src_kind = 'campaign' AND c.campaign_slug = l.src_id),
                   (SELECT b.title FROM library_docs b
                     WHERE l.src_kind = 'book'
                       AND 'BOOK-' || printf('%03d', b.number) = l.src_id),
                   l.src_id
               ) AS title
          FROM doc_links l
         WHERE l.dst_kind = ? AND l.dst_id = UPPER(?)
         ORDER BY l.src_kind, l.src_id
        "#,
    )
    .bind(kind)
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
