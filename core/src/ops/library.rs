//! DEV-215: 도서관(Library) 문서 mutation orchestration.
//!
//! 파일(`.guild/library/{BOOK-NNN}.md`)이 진리원, `library_docs` 테이블은
//! 캐시. 각 mutation 마다 journal append + atomic file IO + 캐시 sync —
//! quests 의 ops 패턴과 동일하되 관계(parent/prereq)/status 가 없어 단순.

use serde_json::json;

use crate::error::{AppError, AppResult};
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
    let rows = sqlx::query_as::<_, LibraryDocRow>(
        "SELECT id, number, title, body, path, created_at, updated_at, deleted_at
           FROM library_docs WHERE deleted_at IS NULL ORDER BY number",
    )
    .fetch_all(&store.index_pool)
    .await?;
    Ok(rows)
}

/// book_id(`BOOK-NNN`)로 단건 조회 (soft-deleted 제외).
pub async fn get_book(store: &Store, book_id: &str) -> AppResult<Option<LibraryDocRow>> {
    let Some(number) = repo::parse_book_slug(book_id) else {
        return Err(AppError::BadRequest(format!("invalid book id: {book_id:?}")));
    };
    let row = sqlx::query_as::<_, LibraryDocRow>(
        "SELECT id, number, title, body, path, created_at, updated_at, deleted_at
           FROM library_docs WHERE number = ? AND deleted_at IS NULL",
    )
    .bind(number)
    .fetch_optional(&store.index_pool)
    .await?;
    Ok(row)
}

/// 새 문서 생성 — 카운터에서 번호 할당, 파일 작성, 캐시 INSERT.
/// `path` — 소속 폴더 ("" = 최상위).
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
        return Err(AppError::BadRequest("루트는 폴더로 만들 수 없습니다".into()));
    }
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM library_folders WHERE path = ? AND deleted_at IS NULL",
    )
    .bind(&path)
    .fetch_one(&store.index_pool)
    .await?;
    if exists > 0 {
        return Err(AppError::BadRequest(format!("이미 존재하는 폴더입니다: {path}")));
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
        return Err(AppError::BadRequest("루트는 삭제할 수 없습니다".into()));
    }
    let docs = list_books(store).await?;
    if docs
        .iter()
        .any(|d| repo::path_is_self_or_descendant(&d.path, &path))
    {
        return Err(AppError::BadRequest(
            "폴더 안에 문서가 있어 삭제할 수 없습니다 — 먼저 비우세요".into(),
        ));
    }
    let folders = list_folders(store).await?;
    if folders
        .iter()
        .any(|f| f.path != path && repo::path_is_self_or_descendant(&f.path, &path))
    {
        return Err(AppError::BadRequest(
            "하위 폴더가 있어 삭제할 수 없습니다 — 먼저 비우세요".into(),
        ));
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
