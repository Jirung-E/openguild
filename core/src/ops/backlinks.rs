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

// ─── REQ-008 후속: 증분 갱신 ───────────────────────────────────────────────
//
// doc_links 는 reindex 가 통째로 다시 만든다. 하지만 사용자는 reindex 를 상시
// 돌리지 않으므로, 그것만으로는 방금 단 cross-link 가 backlink 에 안 나온다
// (실제로 그렇게 보고됐다 — 댓글에 `[[BOOK-002]]` 를 달아도 reindex 전엔 빈
// 목록). 그래서 문서 본문/댓글이 바뀔 때 **그 문서가 내보내는 링크만** 다시
// 계산한다.
//
// 여전히 파일 → DB 단방향이다([[BOOK-001]]). 이 테이블은 파생물이고, 언제
// 지워도 reindex 로 100% 재구축된다 — 증분 갱신은 그 사이를 메우는 것뿐이다.

use crate::repo::crosslink::{self, DocKind};
use crate::store::Store;

/// 링크 대상 해석에 쓰는 실재 문서 집합(대문자 정규화).
async fn existing_docs(store: &Store) -> AppResult<std::collections::HashSet<(DocKind, String)>> {
    use std::collections::HashSet;
    let mut set: HashSet<(DocKind, String)> = HashSet::new();

    let q: Vec<(String,)> = sqlx::query_as(
        "SELECT t.prefix || '-' || printf('%03d', q.number)
           FROM quests q JOIN quest_types t ON t.id = q.quest_type_id
          WHERE q.deleted_at IS NULL",
    )
    .fetch_all(&store.index_pool)
    .await?;
    for (id,) in q {
        set.insert((DocKind::Quest, id.to_uppercase()));
    }

    let c: Vec<(String,)> =
        sqlx::query_as("SELECT campaign_slug FROM campaigns WHERE deleted_at IS NULL")
            .fetch_all(&store.index_pool)
            .await?;
    for (id,) in c {
        set.insert((DocKind::Campaign, id.to_uppercase()));
    }

    let b: Vec<(i64,)> =
        sqlx::query_as("SELECT number FROM library_docs WHERE deleted_at IS NULL")
            .fetch_all(&store.index_pool)
            .await?;
    for (n,) in b {
        set.insert((DocKind::Book, crate::repo::library::book_slug(n).to_uppercase()));
    }

    for r in crate::repo::rules::list_rules(&store.paths).map_err(crate::error::AppError::Internal)? {
        set.insert((DocKind::Rule, r.slug.to_uppercase()));
    }
    Ok(set)
}

/// 한 문서가 **내보내는** 링크를 다시 계산해 doc_links 를 갱신한다.
/// 본문뿐 아니라 그 문서에 달린 댓글까지 포함한다(참조는 댓글에서 더 자주 걸린다).
pub async fn refresh_for(store: &Store, kind: DocKind, id: &str) -> AppResult<()> {
    // (1) 이 문서의 본문 + 댓글 본문 수집.
    let mut bodies: Vec<String> = Vec::new();
    match kind {
        DocKind::Quest => {
            let rows: Vec<(Option<String>,)> = sqlx::query_as(
                "SELECT q.description FROM quests q JOIN quest_types t ON t.id = q.quest_type_id
                  WHERE t.prefix || '-' || printf('%03d', q.number) = ?",
            )
            .bind(id)
            .fetch_all(&store.index_pool)
            .await?;
            bodies.extend(rows.into_iter().filter_map(|(b,)| b));

            let cs: Vec<(String,)> = sqlx::query_as(
                "SELECT c.body FROM quest_comments c
                   JOIN quests q ON q.id = c.quest_id
                   JOIN quest_types t ON t.id = q.quest_type_id
                  WHERE t.prefix || '-' || printf('%03d', q.number) = ?",
            )
            .bind(id)
            .fetch_all(&store.index_pool)
            .await?;
            bodies.extend(cs.into_iter().map(|(b,)| b));
        }
        DocKind::Campaign => {
            let rows: Vec<(Option<String>,)> =
                sqlx::query_as("SELECT description FROM campaigns WHERE campaign_slug = ?")
                    .bind(id)
                    .fetch_all(&store.index_pool)
                    .await?;
            bodies.extend(rows.into_iter().filter_map(|(b,)| b));

            let cs: Vec<(String,)> = sqlx::query_as(
                "SELECT cm.body FROM campaign_comments cm
                   JOIN campaigns c ON c.id = cm.campaign_id
                  WHERE c.campaign_slug = ?",
            )
            .bind(id)
            .fetch_all(&store.index_pool)
            .await?;
            bodies.extend(cs.into_iter().map(|(b,)| b));
        }
        DocKind::Book => {
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT body FROM library_docs WHERE 'BOOK-' || printf('%03d', number) = ?",
            )
            .bind(id)
            .fetch_all(&store.index_pool)
            .await?;
            bodies.extend(rows.into_iter().map(|(b,)| b));
        }
        DocKind::Rule => {
            // 규칙은 index.db 캐시가 없다(file-truth-db-cache §4) — 파일 직독.
            if let Ok(Some(entry)) = crate::repo::rules::read_rule_entry(&store.paths, id) {
                bodies.push(entry.content);
            }
        }
    }

    // (2) 해석 후 교체. 이 문서가 src 인 행만 지우므로 남의 링크는 건드리지 않는다.
    let exist = existing_docs(store).await?;
    const BARE_ORDER: [DocKind; 4] =
        [DocKind::Quest, DocKind::Campaign, DocKind::Book, DocKind::Rule];

    let mut tx = store.index_pool.begin().await?;
    sqlx::query("DELETE FROM doc_links WHERE src_kind = ? AND src_id = ?")
        .bind(kind.as_str())
        .bind(id)
        .execute(&mut *tx)
        .await?;

    for body in &bodies {
        for link in crosslink::extract(body) {
            let key = link.id.to_uppercase();
            let dst = match link.kind {
                Some(k) => exist.contains(&(k, key.clone())).then_some(k),
                None => BARE_ORDER.iter().copied().find(|k| exist.contains(&(*k, key.clone()))),
            };
            let Some(dst_kind) = dst else { continue };
            if dst_kind == kind && key == id.to_uppercase() {
                continue; // 자기 참조는 backlink 로서 의미 없음.
            }
            sqlx::query(
                "INSERT OR IGNORE INTO doc_links (src_kind, src_id, dst_kind, dst_id)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(kind.as_str())
            .bind(id)
            .bind(dst_kind.as_str())
            .bind(&key)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
