//! DEV-215: 도서관(Library) 문서 mutation orchestration.
//!
//! 파일(`.guild/library/{BOOK-NNN}.md`)이 진리원, `library_docs` 테이블은
//! 캐시. 각 mutation 마다 journal append + atomic file IO + 캐시 sync —
//! quests 의 ops 패턴과 동일하되 관계(parent/prereq)/status 가 없어 단순.

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::events::{names as ev, payload};
use crate::repo::history as hist;
use crate::ops::doc_history::{self, DocKind};
use crate::repo::library as repo;
use crate::repo::library::{book_slug, BookFile, BookFrontmatter, FolderEntry};
use crate::store::{journal, Store};

/// library_docs 캐시 행 (조회 API 가 반환하는 형태).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct LibraryDocRow {
    pub id: i64,
    pub number: i64,
    pub title: String,
    pub body: String,
    /// DEV-239: 소속 폴더 경로 ("" = 최상위).
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    /// DEV-243: 자유 태그 — many-to-many(`library_tags`)라 SQL 컬럼이 아니라
    /// 조회 후 별도 쿼리로 채움(`attach_tags`). quest_tags 와 동일 패턴.
    #[sqlx(skip)]
    pub tags: Vec<String>,
}

/// 여러 book 행에 `library_tags` 를 일괄 조회해 채워넣는다 (N+1 방지 — id IN (..)).
async fn attach_tags(store: &Store, rows: &mut [LibraryDocRow]) -> AppResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!(
        "SELECT book_id, tag FROM library_tags WHERE book_id IN ({placeholders}) ORDER BY tag"
    );
    let mut q = sqlx::query_as::<_, (i64, String)>(&sql);
    for id in &ids {
        q = q.bind(id);
    }
    let tag_rows = q.fetch_all(&store.index_pool).await?;
    let mut by_id: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    for (book_id, tag) in tag_rows {
        by_id.entry(book_id).or_default().push(tag);
    }
    for row in rows.iter_mut() {
        if let Some(tags) = by_id.remove(&row.id) {
            row.tags = tags;
        }
    }
    Ok(())
}

/// library_folders 캐시 행.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct LibraryFolderRow {
    pub id: i64,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
}

impl LibraryDocRow {
    pub fn book_id(&self) -> String {
        book_slug(self.number)
    }
}

/// 살아있는 문서 목록 (번호 순). body 포함 — rules 와 같은 판단(문서 수가
/// 크지 않고 GUI 목록이 미리보기를 쓸 수 있음). 커지면 후속에서 분리.
pub async fn list_books(store: &Store) -> AppResult<Vec<LibraryDocRow>> {
    list_books_in(store, None).await
}

/// BUG-281: 폴더로 거른 목록. `folder` 가 `None` 이면 전부.
///
/// **거르기는 여기서 한다.** 처음엔 CLI 가 전부 받아 `retain` 으로 거르게
/// 만들었는데, 원격 모드에서는 그게 **길드의 도서관 전체를 HTTP 로 받아서
/// 대부분 버리는** 짓이다. 문서가 늘수록 그대로 비용이 된다.
///
/// 빈 문자열은 "최상위만" 이다(`--folder ""`) — `path` 가 빈 문자열인 행.
/// 그 밖에는 그 폴더와 **하위 폴더까지** 포함한다. `아키텍처` 를 물었는데
/// `아키텍처/결정` 이 안 나오면 트리를 손으로 훑어야 한다.
pub async fn list_books_in(store: &Store, folder: Option<&str>) -> AppResult<Vec<LibraryDocRow>> {
    const BASE: &str = "SELECT id, number, title, body, path, created_at, updated_at, deleted_at
           FROM library_docs WHERE deleted_at IS NULL";
    let mut rows = match folder.map(|f| f.trim_end_matches('/')) {
        None => {
            sqlx::query_as::<_, LibraryDocRow>(&format!("{BASE} ORDER BY number"))
                .fetch_all(&store.index_pool)
                .await?
        }
        Some("") => {
            sqlx::query_as::<_, LibraryDocRow>(&format!("{BASE} AND path = '' ORDER BY number"))
                .fetch_all(&store.index_pool)
                .await?
        }
        Some(f) => {
            // 자기 자신 또는 그 아래. LIKE 의 와일드카드(`%` `_`)가 폴더 이름에
            // 들어 있으면 엉뚱한 것이 걸리므로 ESCAPE 를 건다.
            sqlx::query_as::<_, LibraryDocRow>(&format!(
                "{BASE} AND (path = ?1 OR path LIKE ?2 ESCAPE '\\') ORDER BY number"
            ))
            .bind(f)
            .bind(format!("{}/%", like_escape(f)))
            .fetch_all(&store.index_pool)
            .await?
        }
    };
    attach_tags(store, &mut rows).await?;
    Ok(rows)
}

/// REQ-027 후속(admin "cli는?"): 폴더에 더해 **태그로도** 거른다.
///
/// 여러 개면 **모두 가진 것만**(AND) — 화면의 태그 줄과 같은 규칙이라야 CLI 와 앱이 같은
/// 목록을 낸다. 태그는 이미 위에서 붙여 오므로(`attach_tags`) 여기서 거르면 되고, 도서관은
/// 크지 않아 SQL 을 더 복잡하게 만들 이유가 없다.
pub async fn list_books_filtered(
    store: &Store,
    folder: Option<&str>,
    tags: &[String],
) -> AppResult<Vec<LibraryDocRow>> {
    let rows = list_books_in(store, folder).await?;
    if tags.is_empty() {
        return Ok(rows);
    }
    Ok(rows
        .into_iter()
        .filter(|r| tags.iter().all(|t| r.tags.iter().any(|x| x == t)))
        .collect())
}

/// LIKE 패턴에서 특별한 뜻을 갖는 문자를 막는다.
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// book_id(`BOOK-NNN`)로 단건 조회 (soft-deleted 제외).
///
/// BUG-134(admin 재보고 — 프론트 재조회만으론 여전히 안 고쳐짐): 이전엔 DB
/// 캐시 행을 그대로 반환해, 파일을 외부(에디터/CLI/git pull)에서 편집해도
/// reindex 전까지 옛 본문이 계속 나왔다. quest 상세의 lazy refresh
/// (DEV-137/BUG-089, incremental.rs::refresh_quest_if_stale)와 동일하게
/// 상세 진입 시 그 파일 하나를 **항상** re-read + 캐시 UPDATE 한다 —
/// 파일 1개라 저렴하고, mtime 게이트로 건너뛰면 다른 프로세스의 편집을
/// 놓친다(BUG-089 와 같은 이유). rules/templates 처럼 "파일 직독" 인
/// 엔티티와 달리 library 는 목록/검색용 DB 캐시를 유지하므로, 진리원
/// 반영은 이 sync 지점이 담당(file-truth-db-cache 규칙 §4).
pub async fn get_book(store: &Store, book_id: &str) -> AppResult<Option<LibraryDocRow>> {
    let Some(number) = repo::parse_book_slug(book_id) else {
        return Err(AppError::BadRequest(format!("invalid book id: {book_id:?}")));
    };
    let path = store.paths.book_path(book_id);
    if let Ok(bf) = BookFile::read(&path)
        && !bf.frontmatter.deleted
    {
        // 파일이 진리 — 캐시 행을 파일 내용으로 갱신(내용이 같으면
        // no-op UPDATE, 저렴). 신규 파일(캐시에 행 없음)은 시동
        // sync / reindex 영역이라 여기서 INSERT 하지 않는다
        // (refresh_quest_if_stale 과 동일한 의도적 한계).
        sqlx::query(
            "UPDATE library_docs
                SET title = ?, body = ?, path = ?, updated_at = ?
              WHERE number = ? AND deleted_at IS NULL",
        )
        .bind(&bf.frontmatter.title)
        .bind(&bf.body)
        .bind(&bf.frontmatter.path)
        .bind(&bf.frontmatter.updated_at)
        .bind(number)
        .execute(&store.index_pool)
        .await?;
        // DEV-243: tags 도 같은 sync 지점에서 파일 기준으로 캐시 갱신.
        if let Some(id) = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM library_docs WHERE number = ? AND deleted_at IS NULL",
        )
        .bind(number)
        .fetch_optional(&store.index_pool)
        .await?
        {
            sync_book_tags_cache(store, id, &bf.frontmatter.tags).await?;
        }
    }
    let mut row = sqlx::query_as::<_, LibraryDocRow>(
        "SELECT id, number, title, body, path, created_at, updated_at, deleted_at
           FROM library_docs WHERE number = ? AND deleted_at IS NULL",
    )
    .bind(number)
    .fetch_optional(&store.index_pool)
    .await?;
    if let Some(r) = &mut row {
        let mut rs = [r.clone()];
        attach_tags(store, &mut rs).await?;
        *r = rs[0].clone();
    }
    Ok(row)
}

/// `library_tags` 를 주어진 목록으로 통째 교체 (wipe + INSERT, quest_tags 와 동일 패턴).
async fn sync_book_tags_cache(store: &Store, book_id: i64, tags: &[String]) -> AppResult<()> {
    let mut tx = store
        .index_pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("begin tx: {e}")))?;
    sqlx::query("DELETE FROM library_tags WHERE book_id = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("clear library_tags: {e}")))?;
    for tag in tags {
        sqlx::query("INSERT INTO library_tags (book_id, tag) VALUES (?, ?)")
            .bind(book_id)
            .bind(tag)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("insert library_tags: {e}")))?;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("commit tx: {e}")))?;
    Ok(())
}

/// DEV-243: 한 문서의 tags 전체 교체. frontmatter + DB 캐시 모두 갱신
/// (quest 의 `set_quest_tags` 와 동일 패턴 — trim/빈 제거/중복 제거, 순서 보존).
pub async fn set_book_tags(
    store: &Store,
    book_id: &str,
    tags: Vec<String>,
) -> AppResult<LibraryDocRow> {
    let _g = store.mutation_guard().await?;
    set_book_tags_locked(store, book_id, tags).await
}

/// BUG-287: 태그를 붙이고 뗀다 — 잠금을 쥔 채 **지금** frontmatter 의 목록에 적용한다.
pub async fn edit_book_tags(
    store: &Store,
    book_id: &str,
    edit: super::TagEdit,
) -> AppResult<LibraryDocRow> {
    let _g = store.mutation_guard().await?;
    let current = BookFile::read(store.paths.book_path(book_id))
        .map_err(|_| AppError::NotFound(format!("book not found: {book_id}")))?
        .frontmatter
        .tags;
    set_book_tags_locked(store, book_id, edit.apply(current)).await
}

async fn set_book_tags_locked(
    store: &Store,
    book_id: &str,
    tags: Vec<String>,
) -> AppResult<LibraryDocRow> {
    use std::collections::HashSet;

    let mut seen: HashSet<String> = HashSet::new();
    let normalized: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .filter(|t| seen.insert(t.clone()))
        .collect();

    let existing = get_book(store, book_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("book not found: {book_id}")))?;

    let _ = journal::append(
        &store.journal_pool,
        "set_book_tags",
        &json!({ "book_id": book_id, "tags": &normalized }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let path = store.paths.book_path(book_id);
    let mut bf = BookFile::read(&path).map_err(AppError::Internal)?;
    bf.frontmatter.tags = normalized.clone();
    bf.write(&path).map_err(AppError::Internal)?;

    sync_book_tags_cache(store, existing.id, &normalized).await?;

    let updated = get_book(store, book_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("updated book not found: {book_id}")))?;
    // DEV-388: 이전 태그는 `existing` — 쓰기 **전에** 읽힌 행이다. 무엇이
    // 붙고 떨어졌는지 없이 `tags_changed` 만 나가면 구독자는 전체를 다시
    // 훑어야 한다(DEV-386 에서 실제로 걸린 결함). 여기서는 `existing` 을
    // 이미 `sync_book_tags_cache` 때문에 읽으므로 게이트할 비용이 없다.
    store.emit_post(ev::BOOK_TAGS_CHANGED, || {
        json!({
            "book": payload::book(&updated),
            "change": payload::change(existing.tags.clone(), updated.tags.clone()),
        })
    });
    Ok(updated)
}

/// 새 문서 생성 — 카운터에서 번호 할당, 파일 작성, 캐시 INSERT.
/// `path` — 소속 폴더 ("" = 최상위).
/// DEV-290: BOOK 의 변경 이력 (최신 → 과거). 도서관도 DB history 테이블이 없어
/// `.guild/history/{book_id}.jsonl` 사이드카에서 직접 읽는다(append-only 라 역순).
pub fn history(store: &Store, book_id: &str) -> AppResult<Vec<hist::HistoryEntry>> {
    // BUG-227: 존재 확인 먼저 — rules::history 와 같은 이유(없는 문서와 이력이
    // 없는 문서를 구분해야 한다). 사이드카는 파일이라 대상 존재를 증명하지 못한다.
    if !store.paths.book_path(book_id).is_file() {
        return Err(AppError::NotFound(crate::tf!(
            "문서 없음: {book_id}",
            "document not found: {book_id}"
        )));
    }
    let path = hist::history_path(&store.paths, book_id);
    let mut v = hist::read_all(&path).map_err(AppError::Internal)?;
    v.reverse();
    Ok(v)
}

pub async fn create_book(
    store: &Store,
    title: &str,
    body: &str,
    path: &str,
) -> AppResult<LibraryDocRow> {
    let _g = store.mutation_guard().await?;
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title is empty".into()));
    }
    let path = repo::normalize_folder_path(path).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let _ = journal::append(
        &store.journal_pool,
        "create_book",
        &json!({ "title": title, "path": path, "len": body.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    // REQ-003: 번호 할당(카운터 읽기 → 파일 최대값 스캔 → +1 → 쓰기)부터
    // 본체 쓰기까지가 한 덩어리여야 한다. 동시 2건이면 둘 다 같은 N+1 을
    // 계산하고 두 번째 write 가 첫 문서를 덮어써 **문서가 영구 소실**된다.
    // 맨 앞의 잠금이 그 덩어리를 감싼다(BUG-287 — 프로세스 사이까지).
    //
    // 참고: `max(카운터, 실존최대)+1` 공식 자체는 BOOK-001 의 A2 처방(파일-로컬
    // heal)을 구현한 것이라 그대로 둔다 — 빠진 건 동시성 보호뿐이었다.
    let number = repo::allocate_number(&store.paths).map_err(AppError::Internal)?;
    let book_id = book_slug(number);
    let now = crate::time::now_local_iso8601();
    let file = BookFile {
        frontmatter: BookFrontmatter {
            book_id: book_id.clone(),
            title: title.to_string(),
            path: path.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
            deleted: false,
            tags: vec![],
        },
        body: body.trim().to_string(),
    };
    file.write(store.paths.book_path(&book_id))
        .map_err(AppError::Internal)?;

    sqlx::query(
        "INSERT INTO library_docs (number, title, body, path, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(number)
    .bind(title)
    .bind(&file.body)
    .bind(&path)
    .bind(&now)
    .bind(&now)
    .execute(&store.index_pool)
    .await?;

    doc_history::record(store, DocKind::Book, &book_id, "create", None, None).await; // BUG-189

    let created = get_book(store, &book_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("created book not found: {book_id}")))?;
    // DEV-388: 파일·캐시·이력이 모두 끝난 뒤에만 낸다 — journal 은 위에서
    // **의도**를 먼저 적지만 이벤트는 일어난 일만 싣는다.
    store.emit_post(
        ev::BOOK_CREATED,
        || json!({ "book": payload::book(&created) }),
    );
    Ok(created)
}

/// 문서 수정 — title / body / path 중 제공된 필드만. updated_at 갱신.
/// `path: Some("")` 는 "최상위로 이동", `None` 은 "변경 없음".
pub async fn update_book(
    store: &Store,
    book_id: &str,
    title: Option<&str>,
    body: Option<&str>,
    path: Option<&str>,
) -> AppResult<LibraryDocRow> {
    let _g = store.mutation_guard().await?;
    let existing = get_book(store, book_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("book not found: {book_id}")))?;
    if title.is_none() && body.is_none() && path.is_none() {
        return Ok(existing);
    }
    let new_title = match title {
        Some(t) if t.trim().is_empty() => {
            return Err(AppError::BadRequest("title is empty".into()))
        }
        Some(t) => t.trim().to_string(),
        None => existing.title.clone(),
    };
    let new_body = body.map(|b| b.trim().to_string()).unwrap_or_else(|| existing.body.clone());
    let new_path = match path {
        Some(p) => repo::normalize_folder_path(p).map_err(|e| AppError::BadRequest(e.to_string()))?,
        None => existing.path.clone(),
    };

    let _ = journal::append(
        &store.journal_pool,
        "update_book",
        &json!({ "book_id": book_id, "title": new_title, "path": new_path, "len": new_body.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let now = crate::time::now_local_iso8601();
    let file = BookFile {
        frontmatter: BookFrontmatter {
            book_id: book_id.to_string(),
            title: new_title.clone(),
            path: new_path.clone(),
            created_at: existing.created_at.clone(),
            updated_at: now.clone(),
            deleted: false,
            // DEV-243: tags 는 이 함수의 대상이 아니므로 기존 값 보존
            // (quest 의 write_quest_file 이 existing tags 보존하는 것과 동일 의도).
            tags: existing.tags.clone(),
        },
        body: new_body.clone(),
    };
    file.write(store.paths.book_path(book_id))
        .map_err(AppError::Internal)?;

    sqlx::query(
        "UPDATE library_docs SET title = ?, body = ?, path = ?, updated_at = ? WHERE number = ?",
    )
    .bind(&new_title)
    .bind(&new_body)
    .bind(&new_path)
    .bind(&now)
    .bind(existing.number)
    .execute(&store.index_pool)
    .await?;

    doc_history::record(store, DocKind::Book, book_id, "update", None, None).await; // BUG-189
    // REQ-008: 이 문서가 내보내는 cross-link 재계산 — BUG-189 가 doc_history 를
    // 즉시 투영한 것과 같은 이유다(reindex 전까지 반영이 안 되면 기능이 없는 것과
    // 같다). 색인은 파생물이라 실패해도 본 작업은 성공으로 둔다.
    let _ =
        crate::ops::backlinks::refresh_for(store, crate::repo::crosslink::DocKind::Book, book_id)
            .await;

    let updated = get_book(store, book_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("updated book not found: {book_id}")))?;
    // DEV-388: 위의 "바꿀 것이 하나도 없다" 조기 반환은 여기 못 온다 —
    // 아무것도 안 바뀐 mutation 을 변경으로 내보내지 않는다.
    store.emit_post(
        ev::BOOK_UPDATED,
        || json!({ "book": payload::book(&updated) }),
    );
    Ok(updated)
}

/// soft delete — frontmatter `deleted = true` + 캐시 deleted_at. 파일은 남긴다
/// (quests 와 동일 — 번호 재사용 금지는 카운터가 보장).
pub async fn delete_book(store: &Store, book_id: &str) -> AppResult<()> {
    let _g = store.mutation_guard().await?;
    let existing = get_book(store, book_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("book not found: {book_id}")))?;

    let _ = journal::append(
        &store.journal_pool,
        "delete_book",
        &json!({ "book_id": book_id }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let now = crate::time::now_local_iso8601();
    let file = BookFile {
        frontmatter: BookFrontmatter {
            book_id: book_id.to_string(),
            title: existing.title.clone(),
            path: existing.path.clone(),
            created_at: existing.created_at.clone(),
            updated_at: now.clone(),
            deleted: true,
            tags: existing.tags.clone(),
        },
        body: existing.body.clone(),
    };
    file.write(store.paths.book_path(book_id))
        .map_err(AppError::Internal)?;

    sqlx::query("UPDATE library_docs SET deleted_at = ?, updated_at = ? WHERE number = ?")
        .bind(&now)
        .bind(&now)
        .bind(existing.number)
        .execute(&store.index_pool)
        .await?;
    doc_history::record(store, DocKind::Book, book_id, "delete", None, None).await;
    doc_history::purge(store, book_id).await; // BUG-189
    // DEV-388: 지운 뒤에는 무엇이었는지 못 읽는다. `delete_quest` 는 그래서
    // 미리 잡아 두지만, 여기 `existing` 은 파일을 다시 쓰느라 어차피 읽은
    // 행이라 구독자가 없어도 추가 비용이 없다 — 그대로 싣는다.
    store.emit_post(
        ev::BOOK_DELETED,
        || json!({ "book": payload::book(&existing) }),
    );
    Ok(())
}

// ─── 폴더 (.guild/library/folders.toml) ───

/// 살아있는 폴더 목록 (path 순).
pub async fn list_folders(store: &Store) -> AppResult<Vec<LibraryFolderRow>> {
    let rows = sqlx::query_as::<_, LibraryFolderRow>(
        "SELECT id, path, created_at, updated_at
           FROM library_folders WHERE deleted_at IS NULL ORDER BY path",
    )
    .fetch_all(&store.index_pool)
    .await?;
    Ok(rows)
}

/// 새 폴더 생성 — 순수 컨테이너(본문 없음). 이미 존재하면 에러.
pub async fn create_folder(store: &Store, path: &str) -> AppResult<LibraryFolderRow> {
    let _g = store.mutation_guard().await?;
    let path = repo::normalize_folder_path(path).map_err(|e| AppError::BadRequest(e.to_string()))?;
    if path.is_empty() {
        return Err(AppError::BadRequest(crate::tf!(
            "루트는 폴더로 만들 수 없습니다",
            "the root cannot be made into a folder"
        )));
    }
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM library_folders WHERE path = ? AND deleted_at IS NULL",
    )
    .bind(&path)
    .fetch_one(&store.index_pool)
    .await?;
    if exists > 0 {
        return Err(AppError::BadRequest(crate::tf!(
            "이미 존재하는 폴더입니다: {path}",
            "folder already exists: {path}"
        )));
    }

    let _ = journal::append(
        &store.journal_pool,
        "create_folder",
        &json!({ "path": path }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let now = crate::time::now_local_iso8601();
    let mut f = repo::read_folders(&store.paths).map_err(AppError::Internal)?;
    f.folders.retain(|e| e.path != path);
    f.folders.push(FolderEntry {
        path: path.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
        deleted: false,
    });
    repo::write_folders(&store.paths, &f).map_err(AppError::Internal)?;

    // BUG-306: 지운 폴더와 **같은 이름**으로 다시 만들면 DB 오류가 났다
    // (`UNIQUE constraint failed: library_folders.path`).
    //
    // 지우기는 행을 남기고 `deleted_at` 만 찍는데(soft delete), 위의 존재 검사는
    // `deleted_at IS NULL` 로 걸러 "없다" 고 답한다. 그런데 유니크 제약은 `path` **하나**에
    // 걸려 있어 그냥 INSERT 하면 남아 있는 그 행과 부딪힌다. 파일 쪽(`folders.toml`)은 옛
    // 항목을 지우고 새로 넣으므로 멀쩡했다 — 캐시만 깨졌다.
    //
    // 같은 경로가 이미 있으면 **되살린다**. 폴더는 경로가 곧 정체성이라 되살리는 것이 맞다.
    sqlx::query(
        "INSERT INTO library_folders (path, created_at, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET
             created_at = excluded.created_at,
             updated_at = excluded.updated_at,
             deleted_at = NULL",
    )
        .bind(&path)
        .bind(&now)
        .bind(&now)
        .execute(&store.index_pool)
        .await?;

    let row = sqlx::query_as::<_, LibraryFolderRow>(
        "SELECT id, path, created_at, updated_at FROM library_folders WHERE path = ? AND deleted_at IS NULL",
    )
    .bind(&path)
    .fetch_one(&store.index_pool)
    .await?;
    // DEV-388: 폴더는 경로가 곧 정체성이라 실을 것이 그것뿐이다.
    store.emit_post(
        ev::FOLDER_CREATED,
        || json!({ "folder": payload::folder(&row.path) }),
    );
    Ok(row)
}

/// DEV-397: 폴더를 옮기거나 이름을 바꾼다 — `to` 는 **새 전체 경로**다.
///
/// 옮기기와 이름 바꾸기는 같은 일이다(부모가 달라지면 옮기기, 마지막 조각이 달라지면 이름
/// 바꾸기). 폴더는 진짜 디렉터리가 아니라 문서의 `path` 필드라([[DEV-239]] — 문서를 옮겨도
/// 파일명인 BOOK 번호가 안 바뀌게), 폴더 하나를 옮기면 **하위 폴더 전부와 그 아래 문서 전부**의
/// 경로를 같이 고쳐야 한다. 그래서 한 잠금 안에서 한꺼번에 한다.
pub async fn move_folder(store: &Store, from: &str, to: &str) -> AppResult<Vec<LibraryFolderRow>> {
    let _g = store.mutation_guard().await?;
    let from = repo::normalize_folder_path(from).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let to = repo::normalize_folder_path(to).map_err(|e| AppError::BadRequest(e.to_string()))?;
    if from.is_empty() {
        return Err(AppError::BadRequest(crate::tf!(
            "루트는 옮길 수 없습니다",
            "the root cannot be moved"
        )));
    }
    if to.is_empty() {
        return Err(AppError::BadRequest(crate::tf!(
            "옮길 곳이 비었습니다 — 새 경로를 주세요",
            "the destination is empty — give a new path"
        )));
    }
    if to == from {
        return list_folders(store).await;
    }
    // 자기 자신 아래로는 못 간다 — 그러면 그 아래가 영영 닿지 않는 곳이 된다.
    if repo::path_is_self_or_descendant(&to, &from) {
        return Err(AppError::BadRequest(crate::tf!(
            "폴더를 자기 자신의 하위로 옮길 수 없습니다: {from} → {to}",
            "cannot move a folder into itself: {from} → {to}"
        )));
    }

    let folders = list_folders(store).await?;
    let books = list_books(store).await?;
    // BUG-293: 있음/없음은 **트리에 보이는 폴더**로 판단한다 — 등록된 폴더만이 아니라 그
    // 조상(`설계/초안` 만 만들면 `설계` 는 등록이 없다)과 문서 경로(+조상)까지. 등록된
    // 것만 보면 화면에 있는 폴더로 끌어 넣거나 그 이름을 바꿀 때 "없다" 고 한다.
    let visible = visible_folders(
        folders
            .iter()
            .map(|f| f.path.as_str())
            .chain(books.iter().map(|b| b.path.as_str())),
    );
    if !visible.contains(&from) {
        return Err(AppError::NotFound(format!("folder not found: {from}")));
    }
    if visible.contains(&to) {
        return Err(AppError::BadRequest(crate::tf!(
            "이미 존재하는 폴더입니다: {to}",
            "folder already exists: {to}"
        )));
    }
    // 옮겨 갈 곳의 부모가 없으면 만들지 않고 거절한다 — 조용히 만들면 오타 한 번에
    // 엉뚱한 폴더가 생긴다.
    if let Some((parent, _)) = to.rsplit_once('/')
        && !visible.contains(parent)
    {
        return Err(AppError::NotFound(format!("folder not found: {parent}")));
    }

    let _ = journal::append(
        &store.journal_pool,
        "move_folder",
        &json!({ "from": from, "to": to }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    // `from` 과 그 하위 전부 — 경로 앞부분만 갈아 끼운다.
    let rename = |p: &str| -> String { format!("{to}{}", &p[from.len()..]) };
    let moved_folders: Vec<String> = folders
        .iter()
        .map(|f| f.path.clone())
        .filter(|p| repo::path_is_self_or_descendant(p, &from))
        .collect();
    let moved_books: Vec<&LibraryDocRow> = books
        .iter()
        .filter(|b| repo::path_is_self_or_descendant(&b.path, &from))
        .collect();

    let now = crate::time::now_local_iso8601();

    // 1) 폴더 레지스트리(파일 진리원) + 캐시.
    let mut reg = repo::read_folders(&store.paths).map_err(AppError::Internal)?;
    for e in reg.folders.iter_mut() {
        if !e.deleted && repo::path_is_self_or_descendant(&e.path, &from) {
            e.path = rename(&e.path);
            e.updated_at = now.clone();
        }
    }
    repo::write_folders(&store.paths, &reg).map_err(AppError::Internal)?;
    for old in &moved_folders {
        sqlx::query("UPDATE library_folders SET path = ?, updated_at = ? WHERE path = ?")
            .bind(rename(old))
            .bind(&now)
            .bind(old)
            .execute(&store.index_pool)
            .await?;
    }

    // 2) 그 아래 문서 — frontmatter(진리원)와 캐시 둘 다.
    for b in &moved_books {
        let path = store.paths.book_path(&b.book_id());
        let mut file = BookFile::read(&path).map_err(AppError::Internal)?;
        file.frontmatter.path = rename(&file.frontmatter.path);
        file.frontmatter.updated_at = now.clone();
        file.write(&path).map_err(AppError::Internal)?;
        sqlx::query("UPDATE library_docs SET path = ?, updated_at = ? WHERE number = ?")
            .bind(rename(&b.path))
            .bind(&now)
            .bind(b.number)
            .execute(&store.index_pool)
            .await?;
    }

    let book_ids: Vec<String> = moved_books.iter().map(|b| b.book_id()).collect();
    store.emit_post(ev::FOLDER_MOVED, || {
        json!({
            "from": payload::folder(&from),
            "folder": payload::folder(&to),
            "books": book_ids,
        })
    });
    list_folders(store).await
}

/// 트리(`gui/frontend/src/lib/utils/library-tree.ts`)가 폴더로 그리는 경로 전부 — 주어진
/// 경로와 그 조상. 빈 경로(최상위)는 폴더가 아니다.
fn visible_folders<'a>(paths: impl Iterator<Item = &'a str>) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for p in paths.filter(|p| !p.is_empty()) {
        let mut cur = p;
        while out.insert(cur.to_string()) {
            match cur.rsplit_once('/') {
                Some((parent, _)) => cur = parent,
                None => break,
            }
        }
    }
    out
}

/// 폴더 삭제 — 하위(자신 포함)에 살아있는 문서나 다른 살아있는 폴더가 하나도
/// 없어야 함 (안전을 위해 빈 폴더만 삭제 허용 — v1).
/// 돌려주는 값: **실제로 지웠나.** `false` 면 이미 없던 폴더다(오류가 아니다 — 아래 참고).
pub async fn delete_folder(store: &Store, path: &str) -> AppResult<bool> {
    let _g = store.mutation_guard().await?;
    let path =
        repo::normalize_folder_path(path).map_err(|e| AppError::BadRequest(e.to_string()))?;
    if path.is_empty() {
        return Err(AppError::BadRequest(crate::tf!(
            "루트는 삭제할 수 없습니다",
            "the root cannot be deleted"
        )));
    }
    let docs = list_books(store).await?;
    if docs
        .iter()
        .any(|d| repo::path_is_self_or_descendant(&d.path, &path))
    {
        return Err(AppError::BadRequest(crate::tf!(
            "폴더 안에 문서가 있어 삭제할 수 없습니다 — 먼저 비우세요",
            "cannot delete — the folder contains documents. Empty it first."
        )));
    }
    let folders = list_folders(store).await?;
    if folders
        .iter()
        .any(|f| f.path != path && repo::path_is_self_or_descendant(&f.path, &path))
    {
        return Err(AppError::BadRequest(crate::tf!(
            "하위 폴더가 있어 삭제할 수 없습니다 — 먼저 비우세요",
            "cannot delete — the folder has subfolders. Empty it first."
        )));
    }
    // BUG-299: **없는 폴더를 지우라는 것은 오류가 아니다.**
    //
    // 예전에는 "folder not found" 를 냈다. 그런데 지우기에서 그 말은 아무 쓸모가 없다 —
    // 사용자가 원한 상태(그 폴더가 없음)가 **이미 참**이기 때문이다. 실제로는 화면에 남아
    // 있던 폴더(다른 창에서 지웠거나, 목록을 아직 안 다시 받았거나, 레지스트리 없이 문서
    // 경로로만 있던 폴더)를 누를 때마다 이 오류가 떴다(admin: "자꾸 난다").
    //
    // 그래서 지우기는 **두 번 해도 되는 일**로 만든다. 문서가 남아 있는지 보는 검사는
    // 그대로다 — 그건 진짜 막아야 하는 것이다. CLI 는 아무것도 안 지웠으면 그렇다고 말한다.
    let in_registry = folders.iter().any(|f| f.path == path);
    if !in_registry {
        return Ok(false);
    }

    let _ = journal::append(
        &store.journal_pool,
        "delete_folder",
        &json!({ "path": path }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let now = crate::time::now_local_iso8601();
    let mut f = repo::read_folders(&store.paths).map_err(AppError::Internal)?;
    for e in f.folders.iter_mut() {
        if e.path == path {
            e.deleted = true;
            e.updated_at = now.clone();
        }
    }
    repo::write_folders(&store.paths, &f).map_err(AppError::Internal)?;

    sqlx::query("UPDATE library_folders SET deleted_at = ?, updated_at = ? WHERE path = ?")
        .bind(&now)
        .bind(&now)
        .bind(&path)
        .execute(&store.index_pool)
        .await?;
    // DEV-388: 빈 폴더만 지울 수 있으므로 함께 사라지는 문서는 없다 —
    // 경로 하나면 구독자가 알아야 할 것이 전부다.
    store.emit_post(
        ev::FOLDER_DELETED,
        || json!({ "folder": payload::folder(&path) }),
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed_guild_dir;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-libops-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        Store::open(dir).await.unwrap()
    }

    /// BUG-306: 지운 폴더와 **같은 이름**으로 다시 만들 수 있어야 한다.
    ///
    /// 지우기는 행을 남기고 `deleted_at` 만 찍는데 유니크 제약은 `path` 하나에 걸려 있다.
    /// 그래서 존재 검사(`deleted_at IS NULL`)는 "없다" 고 하고 INSERT 는 부딪혔다 —
    /// 사용자에게는 DB 오류 문자열이 그대로 나왔다.
    #[tokio::test]
    async fn a_deleted_folder_name_can_be_used_again() {
        let dir = fresh_tmp("folder-reuse");
        let store = setup(&dir).await;

        create_folder(&store, "재활용").await.unwrap();
        delete_folder(&store, "재활용").await.unwrap();
        // 예전에는 여기서 `UNIQUE constraint failed: library_folders.path`.
        let again = create_folder(&store, "재활용").await.unwrap();
        assert_eq!(again.path, "재활용");

        // 목록에 한 번만 나온다 — 되살린 것이지 새로 넣은 것이 아니다.
        let listed: Vec<String> = list_folders(&store)
            .await
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .filter(|p| p == "재활용")
            .collect();
        assert_eq!(listed, vec!["재활용".to_string()]);

        // 살아 있는 폴더를 또 만들려 하면 여전히 거절한다(그건 진짜 중복이다).
        assert!(create_folder(&store, "재활용").await.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-299: **트리에 보이는 폴더는 지울 수 있어야 한다.**
    ///
    /// 폴더는 레지스트리(`folders.toml`)에 있을 수도 있고, 문서의 `path` 로만 존재할 수도
    /// 있다(레지스트리가 생기기 전의 길드). 예전에는 뒤쪽을 지우려 하면 "folder not found" 가
    /// 났다 — 화면에 보이는 것을 못 지우니 사용자는 이유를 알 수 없다. BUG-293 이 옮기기에
    /// 대해 고친 것과 같은 문제다.
    #[tokio::test]
    async fn deleting_a_folder_that_only_documents_know_about() {
        let dir = fresh_tmp("del-derived");
        let store = setup(&dir).await;
        // 레지스트리를 거치지 않고 만들어진 폴더 — 문서가 그 경로를 적고 있을 뿐이다.
        let b = create_book(&store, "문서", "", "등록안된/하위").await.unwrap().book_id();
        assert!(list_folders(&store).await.unwrap().iter().all(|f| f.path != "등록안된/하위"));

        // 문서가 있는 동안에는 지워지지 않는다 — 그 규칙은 그대로다.
        let err = delete_folder(&store, "등록안된/하위").await.unwrap_err();
        assert!(format!("{err}").contains("문서"), "{err}");

        // 문서를 옮기면 그냥 끝난다 — 지울 기록이 없을 뿐, 오류가 아니다.
        update_book(&store, &b, None, None, Some("")).await.unwrap();
        assert!(!delete_folder(&store, "등록안된/하위").await.unwrap(), "지울 기록이 없으면 false");

        // 아예 없던 폴더도 마찬가지 — 사용자가 원한 상태가 이미 참이다.
        assert!(!delete_folder(&store, "아예없는폴더").await.unwrap(), "없던 폴더도 오류가 아니다");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// REQ-027 후속: 도서관 목록도 태그로 거른다 — 여러 개면 모두 가진 것만(AND).
    #[tokio::test]
    async fn library_list_filters_by_tag() {
        let dir = fresh_tmp("tag-filter");
        let store = setup(&dir).await;
        let a = create_book(&store, "설계 문서", "", "").await.unwrap().book_id();
        let b = create_book(&store, "그냥 문서", "", "").await.unwrap().book_id();
        set_book_tags(&store, &a, vec!["설계".into(), "결정".into()]).await.unwrap();
        set_book_tags(&store, &b, vec!["설계".into()]).await.unwrap();

        let ids = |rows: Vec<LibraryDocRow>| {
            let mut v: Vec<String> = rows.into_iter().map(|r| r.book_id()).collect();
            v.sort();
            v
        };
        let one = list_books_filtered(&store, None, &["설계".into()]).await.unwrap();
        let mut both = vec![a.clone(), b.clone()];
        both.sort();
        assert_eq!(ids(one), both);

        let two = list_books_filtered(&store, None, &["설계".into(), "결정".into()])
            .await
            .unwrap();
        assert_eq!(ids(two), vec![a]);

        let none = list_books_filtered(&store, None, &["없는태그".into()]).await.unwrap();
        assert!(none.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-397: 폴더를 옮기면 **하위 폴더와 그 아래 문서의 경로까지** 함께 간다.
    ///
    /// 폴더는 디스크 디렉터리가 아니라 문서의 `path` 필드라(DEV-239), 한 곳만 고치면 그 아래가
    /// 통째로 길을 잃는다.
    #[tokio::test]
    async fn moving_a_folder_takes_its_subfolders_and_documents() {
        let dir = fresh_tmp("move-folder");
        let store = setup(&dir).await;
        create_folder(&store, "설계").await.unwrap();
        create_folder(&store, "설계/결정").await.unwrap();
        create_folder(&store, "보관").await.unwrap();
        let a = create_book(&store, "루트 문서", "", "설계").await.unwrap();
        let b = create_book(&store, "하위 문서", "", "설계/결정").await.unwrap();

        move_folder(&store, "설계", "보관/설계").await.unwrap();

        let paths: Vec<String> = list_folders(&store)
            .await
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert!(paths.contains(&"보관/설계".to_string()), "{paths:?}");
        assert!(paths.contains(&"보관/설계/결정".to_string()), "{paths:?}");
        assert!(!paths.iter().any(|p| p.starts_with("설계")), "옛 경로가 남았다: {paths:?}");

        let book = |id: &str| -> String {
            crate::repo::library::BookFile::read(store.paths.book_path(id))
                .unwrap()
                .frontmatter
                .path
        };
        // 파일(진리원)과 캐시가 함께 움직여야 한다.
        assert_eq!(book(&a.book_id()), "보관/설계");
        assert_eq!(book(&b.book_id()), "보관/설계/결정");
        let cached: Vec<String> = list_books(&store)
            .await
            .unwrap()
            .into_iter()
            .map(|d| d.path)
            .collect();
        assert!(cached.contains(&"보관/설계/결정".to_string()), "{cached:?}");

        // reindex 뒤에도 같다 — 파일에서 복원된다.
        crate::reindex::reindex(&store).await.unwrap();
        let after: Vec<String> = list_books(&store)
            .await
            .unwrap()
            .into_iter()
            .map(|d| d.path)
            .collect();
        assert!(after.contains(&"보관/설계".to_string()), "{after:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 이름 바꾸기도 같은 연산이다 — 마지막 조각만 다른 경로로 옮기는 것.
    #[tokio::test]
    async fn renaming_is_the_same_operation() {
        let dir = fresh_tmp("rename-folder");
        let store = setup(&dir).await;
        create_folder(&store, "설게").await.unwrap(); // 오타
        let b = create_book(&store, "문서", "", "설게").await.unwrap();

        move_folder(&store, "설게", "설계").await.unwrap();

        assert_eq!(
            crate::repo::library::BookFile::read(store.paths.book_path(&b.book_id()))
                .unwrap()
                .frontmatter
                .path,
            "설계"
        );
        // BOOK 번호는 그대로다 — 폴더는 파일 위치가 아니다.
        assert!(store.paths.book_path(&b.book_id()).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-293: 트리에 보이지만 등록은 안 된 폴더 — 만든 폴더의 조상, 문서만 있는 경로.
    /// 그리로 옮기고, 그것을 옮길 수 있어야 한다. 겹치는 이름은 여전히 거절한다.
    #[tokio::test]
    async fn folders_seen_only_in_the_tree_can_be_moved_and_moved_into() {
        let dir = fresh_tmp("move-folder-implicit");
        let store = setup(&dir).await;
        create_folder(&store, "설계/초안").await.unwrap(); // `설계` 는 등록 없음
        create_folder(&store, "기타").await.unwrap();
        let memo = create_book(&store, "회의록", "", "회의/2026").await.unwrap(); // 폴더 등록 없음

        // 등록 안 된 조상 밑으로.
        move_folder(&store, "기타", "설계/기타").await.unwrap();
        // 문서 경로로만 있는 폴더의 이름 바꾸기 — 조상까지 포함해서.
        move_folder(&store, "회의", "회의록").await.unwrap();
        // 등록 안 된 조상 자체를 옮기기 — 등록된 하위가 따라간다.
        move_folder(&store, "설계", "회의록/설계").await.unwrap();

        let book_path = crate::repo::library::BookFile::read(store.paths.book_path(&memo.book_id()))
            .unwrap()
            .frontmatter
            .path;
        assert_eq!(book_path, "회의록/2026");
        let mut paths: Vec<String> = list_folders(&store)
            .await
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        paths.sort();
        assert_eq!(paths, vec!["회의록/설계/기타", "회의록/설계/초안"]);

        // 보이는 폴더와 겹치는 이름은 거절 — 문서 경로로만 있는 것과도.
        let e = move_folder(&store, "회의록/설계/기타", "회의록/2026").await.unwrap_err();
        assert!(matches!(e, AppError::BadRequest(_)), "{e}");
        let e = move_folder(&store, "회의록/설계/기타", "회의록/설계").await.unwrap_err();
        assert!(matches!(e, AppError::BadRequest(_)), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn moving_refuses_what_would_lose_a_folder() {
        let dir = fresh_tmp("move-folder-bad");
        let store = setup(&dir).await;
        create_folder(&store, "설계").await.unwrap();
        create_folder(&store, "설계/결정").await.unwrap();
        create_folder(&store, "보관").await.unwrap();

        let bad = |r: AppResult<Vec<LibraryFolderRow>>| match r.unwrap_err() {
            AppError::BadRequest(m) => m,
            AppError::NotFound(m) => m,
            e => panic!("{e}"),
        };
        // 자기 자신 아래로 — 그 아래가 영영 닿지 않는 곳이 된다.
        assert!(bad(move_folder(&store, "설계", "설계/결정/설계").await).contains("설계"));
        // 이미 있는 이름으로.
        assert!(bad(move_folder(&store, "설계", "보관").await).contains("보관"));
        // 없는 부모 밑으로 — 조용히 만들지 않는다(오타 한 번에 엉뚱한 폴더가 생긴다).
        assert!(bad(move_folder(&store, "설계", "없는곳/설계").await).contains("없는곳"));
        // 없는 폴더.
        assert!(bad(move_folder(&store, "그런폴더", "보관/x").await).contains("그런폴더"));
        // 루트는 못 옮긴다.
        assert!(!bad(move_folder(&store, "", "보관").await).is_empty());

        // 하나도 안 바뀌었다.
        let paths: Vec<String> = list_folders(&store)
            .await
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(paths, vec!["보관", "설계", "설계/결정"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-227: 없는 문서의 이력 조회는 **성공이 아니라 NotFound** 여야 한다.
    /// 사이드카가 없으면 빈 목록이 되는 구조라, 예전엔 오타를 쳐도 `(no history)`
    /// + exit 0 으로 끝나 "이력 없음" 과 "문서 없음" 이 구분되지 않았다.
    #[tokio::test]
    async fn history_of_missing_book_is_not_found() {
        let dir = fresh_tmp("hist-missing");
        let store = setup(&dir).await;

        let err = history(&store, "BOOK-999").unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "NotFound 여야: {err:?}");

        // 대조군: 실제로 있는 문서는 이력이 없어도 Ok(빈 목록).
        create_book(&store, "있는 문서", "", "").await.unwrap();
        let h = history(&store, "BOOK-001").expect("존재하는 문서는 Ok");
        assert!(h.is_empty() || !h.is_empty(), "존재 확인만 통과하면 된다");
    }

    #[tokio::test]
    async fn create_get_update_delete_roundtrip() {
        let dir = fresh_tmp("crud");
        let store = setup(&dir).await;

        let b = create_book(&store, "설계 결정", "본문입니다", "").await.unwrap();
        assert_eq!(b.book_id(), "BOOK-001");
        assert_eq!(b.title, "설계 결정");
        assert_eq!(b.body, "본문입니다");
        assert_eq!(b.path, "");
        // 파일 진리원 확인.
        let f = BookFile::read(store.paths.book_path("BOOK-001")).unwrap();
        assert_eq!(f.frontmatter.title, "설계 결정");
        assert!(!f.frontmatter.deleted);

        let b2 = create_book(&store, "second", "", "").await.unwrap();
        assert_eq!(b2.book_id(), "BOOK-002", "카운터 단조 증가");

        let up = update_book(&store, "BOOK-001", Some("바뀐 제목"), None, None).await.unwrap();
        assert_eq!(up.title, "바뀐 제목");
        assert_eq!(up.body, "본문입니다", "body 미지정 시 보존");

        delete_book(&store, "BOOK-001").await.unwrap();
        assert!(get_book(&store, "BOOK-001").await.unwrap().is_none(), "soft delete 후 조회 제외");
        let f = BookFile::read(store.paths.book_path("BOOK-001")).unwrap();
        assert!(f.frontmatter.deleted, "파일엔 deleted flag 로 남음");

        // 삭제된 번호 재사용 금지.
        let b3 = create_book(&store, "third", "", "").await.unwrap();
        assert_eq!(b3.book_id(), "BOOK-003");

        let list = list_books(&store).await.unwrap();
        let ids: Vec<String> = list.iter().map(|b| b.book_id()).collect();
        assert_eq!(ids, vec!["BOOK-002", "BOOK-003"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-134: 파일을 외부(에디터/CLI/git pull)에서 편집한 뒤 상세 조회하면
    /// DB 캐시가 아니라 파일의 최신 내용이 반환돼야 한다 — quest 상세의
    /// lazy refresh(DEV-137)와 동일한 sync 지점.
    #[tokio::test]
    async fn get_book_refreshes_from_externally_edited_file() {
        let dir = fresh_tmp("lazyref");
        let store = setup(&dir).await;

        let b = create_book(&store, "설계 결정", "원래 본문", "").await.unwrap();
        let book_id = b.book_id();

        // 외부 편집 시뮬레이션 — ops 를 거치지 않고 파일만 직접 수정
        // (DB 캐시는 여전히 "원래 본문" 인 상태).
        let path = store.paths.book_path(&book_id);
        let mut f = BookFile::read(&path).unwrap();
        f.body = "외부에서 바뀐 본문".into();
        f.frontmatter.title = "외부에서 바뀐 제목".into();
        f.write(&path).unwrap();

        let got = get_book(&store, &book_id).await.unwrap().unwrap();
        assert_eq!(got.body, "외부에서 바뀐 본문", "상세 조회는 파일 최신 내용 반환");
        assert_eq!(got.title, "외부에서 바뀐 제목");

        // 캐시(list 경로)에도 반영됐는지 — get 이 sync 지점.
        let list = list_books(&store).await.unwrap();
        let row = list.iter().find(|r| r.book_id() == book_id).unwrap();
        assert_eq!(row.body, "외부에서 바뀐 본문");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rejects_empty_title_and_bad_id() {
        let dir = fresh_tmp("valid");
        let store = setup(&dir).await;
        assert!(create_book(&store, "  ", "b", "").await.is_err());
        assert!(get_book(&store, "DEV-001").await.is_err(), "book id 형식 아님");
        assert!(
            update_book(&store, "BOOK-999", Some("t"), None, None).await.is_err(),
            "미존재 NotFound"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn book_path_create_move_and_list() {
        let dir = fresh_tmp("path");
        let store = setup(&dir).await;

        let b = create_book(&store, "라우터 설계", "본문", "아키텍처").await.unwrap();
        assert_eq!(b.path, "아키텍처");
        let f = BookFile::read(store.paths.book_path(&b.book_id())).unwrap();
        assert_eq!(f.frontmatter.path, "아키텍처", "파일 진리원에도 path 기록");

        let moved = update_book(&store, &b.book_id(), None, None, Some(""))
            .await
            .unwrap();
        assert_eq!(moved.path, "", "루트로 이동");

        assert!(
            create_book(&store, "x", "", "아키텍처/..").await.is_err(),
            "잘못된 경로 세그먼트 거부"
        );
    }

    #[tokio::test]
    async fn folder_create_delete_and_guard_non_empty() {
        let dir = fresh_tmp("folder");
        let store = setup(&dir).await;

        let f = create_folder(&store, "아키텍처").await.unwrap();
        assert_eq!(f.path, "아키텍처");
        assert!(create_folder(&store, "아키텍처").await.is_err(), "중복 생성 거부");
        assert!(create_folder(&store, "").await.is_err(), "루트는 폴더 불가");

        let folders = list_folders(&store).await.unwrap();
        assert_eq!(folders.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["아키텍처"]);

        // 빈 폴더는 삭제 가능.
        delete_folder(&store, "아키텍처").await.unwrap();
        assert!(list_folders(&store).await.unwrap().is_empty());
        // BUG-299: 두 번 지워도 오류가 아니다 — 원한 상태가 이미 참이다. 다만 "지웠다" 고
        // 하지는 않는다(false).
        assert!(!delete_folder(&store, "아키텍처").await.unwrap(), "두 번째 삭제는 한 일이 없다");

        // 문서가 있으면 삭제 거부.
        create_folder(&store, "운영").await.unwrap();
        create_book(&store, "가이드", "", "운영").await.unwrap();
        assert!(delete_folder(&store, "운영").await.is_err(), "문서 있는 폴더는 삭제 거부");
    }

    #[tokio::test]
    async fn folder_delete_rejects_when_subfolder_exists() {
        let dir = fresh_tmp("folder-sub");
        let store = setup(&dir).await;

        create_folder(&store, "아키텍처").await.unwrap();
        create_folder(&store, "아키텍처/서브").await.unwrap();
        assert!(
            delete_folder(&store, "아키텍처").await.is_err(),
            "하위 폴더가 있으면 삭제 거부 (문서가 하나도 없어도)"
        );
        // 하위 폴더부터 지우면 이제 부모도 삭제 가능.
        delete_folder(&store, "아키텍처/서브").await.unwrap();
        delete_folder(&store, "아키텍처").await.unwrap();
        assert!(list_folders(&store).await.unwrap().is_empty());
    }

    /// BUG-281: **거르기는 SQL 이 한다.**
    ///
    /// 처음엔 CLI 가 전부 받아 `retain` 으로 걸렀는데, 원격 모드에서 그건 도서관
    /// 전체를 HTTP 로 받아 대부분 버리는 짓이다(admin 지적). 여기서 걸러야
    /// 서버가 보내는 양이 실제로 준다.
    #[tokio::test]
    async fn listing_filters_by_folder_in_sql() {
        let dir = fresh_tmp("folder-filter");
        let store = setup(&dir).await;
        create_book(&store, "결정", "", "아키텍처/결정").await.unwrap();
        create_book(&store, "설계", "", "아키텍처").await.unwrap();
        create_book(&store, "최상위", "", "").await.unwrap();
        create_book(&store, "운영", "", "운영").await.unwrap();

        let titles = |rows: Vec<LibraryDocRow>| {
            let mut v: Vec<String> = rows.into_iter().map(|r| r.title).collect();
            v.sort();
            v
        };

        // 하위 폴더까지 — `아키텍처` 를 물었는데 `아키텍처/결정` 이 빠지면
        // 트리를 손으로 훑어야 한다.
        assert_eq!(
            titles(list_books_in(&store, Some("아키텍처")).await.unwrap()),
            vec!["결정".to_string(), "설계".to_string()]
        );
        // 빈 문자열은 "최상위만" — `None`(전부)과 뜻이 다르다.
        assert_eq!(
            titles(list_books_in(&store, Some("")).await.unwrap()),
            vec!["최상위".to_string()]
        );
        assert_eq!(list_books_in(&store, None).await.unwrap().len(), 4);
        // 끝의 `/` 는 있어도 없어도 같다.
        assert_eq!(list_books_in(&store, Some("아키텍처/")).await.unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 폴더 이름에 LIKE 와일드카드가 들어 있어도 엉뚱한 것이 안 걸린다.
    /// `100%` 로 거르면 `100X` 가 따라오면 안 된다 — ESCAPE 를 안 걸면 그렇게 된다.
    #[tokio::test]
    async fn folder_filter_does_not_treat_names_as_like_patterns() {
        let dir = fresh_tmp("folder-like");
        let store = setup(&dir).await;
        // **하위 폴더에 둬야 LIKE 가지가 실제로 쓰인다.** 처음엔 `100%` 바로
        // 아래에만 뒀는데, 그건 `path = ?1` 이 먼저 잡아서 ESCAPE 를 빼도
        // 시험이 통과했다 — 주장하는 것을 안 보고 있었다.
        create_book(&store, "퍼센트하위", "", "100%/안").await.unwrap();
        create_book(&store, "함정하위", "", "100X/안").await.unwrap();
        create_book(&store, "밑줄하위", "", "a_b/안").await.unwrap();
        create_book(&store, "밑줄함정하위", "", "axb/안").await.unwrap();

        let one = |rows: Vec<LibraryDocRow>| {
            assert_eq!(rows.len(), 1, "{:?}", rows.iter().map(|r| &r.title).collect::<Vec<_>>());
            rows[0].title.clone()
        };
        // `%` 를 와일드카드로 두면 `100X/안` 까지 딸려 온다.
        assert_eq!(one(list_books_in(&store, Some("100%")).await.unwrap()), "퍼센트하위");
        // `_` 는 한 글자 와일드카드 — `axb/안` 이 딸려 온다.
        assert_eq!(one(list_books_in(&store, Some("a_b")).await.unwrap()), "밑줄하위");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
