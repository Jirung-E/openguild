//! BUG-068: sibling 파일(.comments.md / .memo.md) per-file mtime 캐시.
//!
//! `file_mtime_cache` 테이블(migration 0019)에 "각 sibling 파일이 캐시에
//! 반영된 시점의 mtime" 을 보관. ops 가 sibling 을 write 한 직후 `touch`,
//! reindex 가 `sync_all`, detect_drift 가 `load_all` 로 비교. quest 본문의
//! per-row cached_mtime(BUG-067)과 같은 역할을 sibling 에 부여.

use std::collections::HashMap;
use std::path::Path;

use crate::repo::fs as repo_fs;
use crate::repo::GuildPaths;
use crate::store::Store;

/// abs 경로 → `.guild/` 상대 rel_path (캐시 키). 실패 시 file_name fallback.
/// 예: `.../guild/.guild/quests/DEV-001.comments.md` → `quests/DEV-001.comments.md`.
pub fn rel_key(paths: &GuildPaths, abs: &Path) -> String {
    abs.strip_prefix(paths.dot_guild())
        .ok()
        .and_then(|p| p.to_str())
        .map(|s| s.replace('\\', "/"))
        .unwrap_or_else(|| {
            abs.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string()
        })
}

/// 파일의 현재 mtime 을 캐시에 UPSERT. ops 가 sibling write 직후 호출.
/// 파일이 없으면 (예: 메모/댓글 clear) 캐시에서 제거.
pub async fn touch(store: &Store, abs: &Path) -> Result<(), sqlx::Error> {
    let rel = rel_key(&store.paths, abs);
    if !abs.exists() {
        sqlx::query("DELETE FROM file_mtime_cache WHERE rel_path = ?")
            .bind(&rel)
            .execute(&store.index_pool)
            .await?;
        return Ok(());
    }
    let mtime = repo_fs::mtime_unix_nanos(abs);
    sqlx::query(
        "INSERT INTO file_mtime_cache (rel_path, mtime) VALUES (?, ?)
         ON CONFLICT(rel_path) DO UPDATE SET mtime = excluded.mtime",
    )
    .bind(&rel)
    .bind(mtime)
    .execute(&store.index_pool)
    .await?;
    Ok(())
}

/// DEV-178: per-file mtime 캐시로 외부편집을 감지할 "primary" 파일들 —
/// 캠페인 본문(`campaigns/{slug}.md`) + types/statuses/tags 정의(`*.toml`).
///
/// quest 본문은 per-row `cached_mtime`(quests 테이블)으로, sibling 댓글/메모는
/// 아래 sync_all 이 별도로 다룬다. 그 외 DB 캐시로 읽히는 파일들(캠페인 본문 +
/// 메타 정의)은 per-row mtime 컬럼이 없어 여기서 file_mtime_cache 로 커버한다.
/// detect_drift 와 sync_all 이 같은 목록을 쓰도록 한 곳에 모은다.
pub fn list_primary_cached_files(paths: &GuildPaths) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    // 캠페인 본문 (sibling `.comments.md` / `.memo.md` 제외 — stem 에 '.' 없는 .md).
    if let Ok(c) = repo_fs::list_quest_body_files(&paths.campaigns_dir()) {
        files.extend(c);
    }
    // types / statuses / tags 정의.
    for dir in [paths.types_dir(), paths.statuses_dir(), paths.tags_dir()] {
        if let Ok(t) = repo_fs::list_with_extension(&dir, "toml") {
            files.extend(t);
        }
    }
    files
}

/// 전체 (rel_path → mtime) 맵. detect_drift 가 사용.
pub async fn load_all(store: &Store) -> HashMap<String, i64> {
    sqlx::query_as::<_, (String, i64)>("SELECT rel_path, mtime FROM file_mtime_cache")
        .fetch_all(&store.index_pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// reindex 후 호출: 현존 sibling 파일들의 mtime 으로 캐시를 재구성
/// (사라진 파일 row 는 제거). quest + campaign 의 comments/memo 모두.
pub async fn sync_all(store: &Store) -> Result<(), sqlx::Error> {
    let paths = &store.paths;
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in [paths.quests_dir(), paths.campaigns_dir()] {
        if let Ok(c) = repo_fs::list_quest_comment_files(&dir) {
            files.extend(c);
        }
        if let Ok(m) = repo_fs::list_quest_memo_files(&dir) {
            files.extend(m);
        }
    }
    // DEV-178: 캠페인 본문 + types/statuses/tags 정의도 같은 캐시로 커버.
    files.extend(list_primary_cached_files(paths));
    // 현존 파일 → upsert + 본 set 으로 살아있는 rel 수집.
    let mut alive: std::collections::HashSet<String> = std::collections::HashSet::new();
    for f in &files {
        let rel = rel_key(paths, f);
        let mtime = repo_fs::mtime_unix_nanos(f);
        sqlx::query(
            "INSERT INTO file_mtime_cache (rel_path, mtime) VALUES (?, ?)
             ON CONFLICT(rel_path) DO UPDATE SET mtime = excluded.mtime",
        )
        .bind(&rel)
        .bind(mtime)
        .execute(&store.index_pool)
        .await?;
        alive.insert(rel);
    }
    // 사라진 파일의 row 제거.
    let existing: Vec<String> =
        sqlx::query_scalar("SELECT rel_path FROM file_mtime_cache")
            .fetch_all(&store.index_pool)
            .await?;
    for rel in existing {
        if !alive.contains(&rel) {
            sqlx::query("DELETE FROM file_mtime_cache WHERE rel_path = ?")
                .bind(&rel)
                .execute(&store.index_pool)
                .await?;
        }
    }
    Ok(())
}
