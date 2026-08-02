//! DEV-215: 도서관(Library) 문서 mutation orchestration.
//!
//! 파일(`.guild/library/{BOOK-NNN}.md`)이 진리원, `library_docs` 테이블은
//! 캐시. 각 mutation 마다 journal append + atomic file IO + 캐시 sync —
//! quests 의 ops 패턴과 동일하되 관계(parent/prereq)/status 가 없어 단순.

use serde_json::json;

use crate::error::{AppError, AppResult};
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
    let mut rows = sqlx::query_as::<_, LibraryDocRow>(
        "SELECT id, number, title, body, path, created_at, updated_at, deleted_at
           FROM library_docs WHERE deleted_at IS NULL ORDER BY number",
    )
    .fetch_all(&store.index_pool)
    .await?;
    attach_tags(store, &mut rows).await?;
    Ok(rows)
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

    get_book(store, book_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("updated book not found: {book_id}")))
}

/// 새 문서 생성 — 카운터에서 번호 할당, 파일 작성, 캐시 INSERT.
/// `path` — 소속 폴더 ("" = 최상위).
/// DEV-290: BOOK 의 변경 이력 (최신 → 과거). 도서관도 DB history 테이블이 없어
/// `.guild/history/{book_id}.jsonl` 사이드카에서 직접 읽는다(append-only 라 역순).
pub fn history(store: &Store, book_id: &str) -> AppResult<Vec<hist::HistoryEntry>> {
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

    get_book(store, &book_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("created book not found: {book_id}")))
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

    get_book(store, book_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("updated book not found: {book_id}")))
}

/// soft delete — frontmatter `deleted = true` + 캐시 deleted_at. 파일은 남긴다
/// (quests 와 동일 — 번호 재사용 금지는 카운터가 보장).
pub async fn delete_book(store: &Store, book_id: &str) -> AppResult<()> {
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

    sqlx::query("INSERT INTO library_folders (path, created_at, updated_at) VALUES (?, ?, ?)")
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
    Ok(row)
}

/// 폴더 삭제 — 하위(자신 포함)에 살아있는 문서나 다른 살아있는 폴더가 하나도
/// 없어야 함 (안전을 위해 빈 폴더만 삭제 허용 — v1).
pub async fn delete_folder(store: &Store, path: &str) -> AppResult<()> {
    let path = repo::normalize_folder_path(path).map_err(|e| AppError::BadRequest(e.to_string()))?;
    if path.is_empty() {
        return Err(AppError::BadRequest(crate::tf!("루트는 삭제할 수 없습니다", "the root cannot be deleted")));
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
    if !folders.iter().any(|f| f.path == path) {
        return Err(AppError::NotFound(format!("folder not found: {path}")));
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
    Ok(())
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
        assert!(delete_folder(&store, "아키텍처").await.is_err(), "재삭제는 NotFound");

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
}
