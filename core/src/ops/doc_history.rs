//! BUG-189: 규칙 / 도서관 문서의 변경 기록 — 사이드카 write + 캐시 투영.
//!
//! DEV-288 은 `.guild/history/{slug}.jsonl` 사이드카에 기록하고, 그것을
//! `doc_history` 캐시로 투영하는 일은 **reindex 에만** 맡겼다. 그런데 평소
//! reindex 를 돌 일이 없으니(파일이 진리원이라 그럴 필요가 없다) 작업기록에는
//! 아무것도 안 뜬다 — 사용자 눈엔 "기록이 안 남는" 것과 같았다(admin 보고:
//! "DEV-288 아직 안 고쳐졌음").
//!
//! 그래서 mutation 시점에 캐시에도 같이 넣는다. 방향은 여전히 **파일 → 캐시**
//! 일방향이고(BOOK-001), reindex 는 `doc_history` 를 통째로 비우고 사이드카에서
//! 재구축하므로 여기서 넣은 행이 어긋나도 다음 reindex 가 바로잡는다.

use crate::repo::history as hist;
use crate::store::Store;

/// 문서 종류 — `doc_history.kind` 값. reindex 의 투영과 같은 문자열이어야 한다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocKind {
    Rule,
    Book,
}

impl DocKind {
    fn as_str(self) -> &'static str {
        match self {
            DocKind::Rule => "rule",
            DocKind::Book => "book",
        }
    }
}

/// 사이드카에 1건 기록하고 `doc_history` 캐시에도 같은 내용을 넣는다.
///
/// best-effort — 활동 기록은 부가 정보이지 진리원이 아니므로, 실패해도 mutation
/// 자체는 되돌리지 않는다(사이드카 실패는 `hist::record` 가 warn 으로 삼킨다).
pub async fn record(
    store: &Store,
    kind: DocKind,
    slug: &str,
    op: &str,
    old: Option<String>,
    new: Option<String>,
) {
    hist::record(&store.paths, slug, op, old.clone(), new.clone());
    let ts = crate::time::now_local_iso8601();
    if let Err(e) = sqlx::query(
        "INSERT INTO doc_history (kind, slug, ts, op, old_value, new_value)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(kind.as_str())
    .bind(slug)
    .bind(&ts)
    .bind(op)
    .bind(&old)
    .bind(&new)
    .execute(&store.index_pool)
    .await
    {
        tracing::warn!("doc_history 투영 실패 ({slug}, {op}): {e}");
    }
}

/// 이름이 바뀌면 캐시의 옛 slug 행도 새 slug 로 옮긴다 — 안 그러면 작업기록의
/// 옛 항목이 지금은 없는 문서를 가리켜 클릭해도 열리지 않는다.
/// (사이드카 파일 이동은 호출부가 `hist::rename` 으로 처리한다.)
pub async fn rename(store: &Store, old_slug: &str, new_slug: &str) {
    if let Err(e) = sqlx::query("UPDATE doc_history SET slug = ? WHERE slug = ?")
        .bind(new_slug)
        .bind(old_slug)
        .execute(&store.index_pool)
        .await
    {
        tracing::warn!("doc_history slug 이동 실패 ({old_slug} → {new_slug}): {e}");
    }
}

/// 문서가 지워지면 캐시 행도 정리한다. 사이드카는 남겨두므로(감사 기록) 다음
/// reindex 는 dangling 사이드카로 보고 skip 한다 — 두 경로의 결과가 같다.
pub async fn purge(store: &Store, slug: &str) {
    if let Err(e) = sqlx::query("DELETE FROM doc_history WHERE slug = ?")
        .bind(slug)
        .execute(&store.index_pool)
        .await
    {
        tracing::warn!("doc_history 정리 실패 ({slug}): {e}");
    }
}
