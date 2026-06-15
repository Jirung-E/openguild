//! DEV-069: 본문 첨부파일 — 저장 + blob 백업 + self-heal.
//!
//! 원칙 (사용자 댓글 #3): **git 은 선택사항** — 첨부도 snapshot 만으로 복원
//! 가능해야 함. 파일 (`.guild/attachments/**`) 이 진리원, `attachment_blobs`
//! 가 백업 캐시 (index.db → snapshot 에 자동 포함).

use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::repo::fs as repo_fs;
use crate::store::{journal, Store};

/// 확장자를 파일명에 안전하게 쓸 수 있도록 정규화 — ascii 영숫자만, 소문자,
/// 최대 16자. 결과가 비면 `bin` (확장자 없는 파일 / 이상한 값 대비).
///
/// DEV-069 후속(admin #8): 이미지/동영상 외 임의 파일도 첨부 가능해야 하므로
/// 확장자 화이트리스트를 제거. 첨부는 사용자 본인 머신의 로컬 저장이고 파일명은
/// 시각+난수라 traversal 위험이 없다. 표시(embed vs 링크)는 frontend 가 결정.
fn sanitize_ext(ext: &str) -> String {
    let cleaned: String = ext
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    if cleaned.is_empty() {
        "bin".into()
    } else {
        cleaned
    }
}

/// 첨부 저장 — bytes 를 `.guild/attachments/{nanos}-{rand}.{ext}` 로 write
/// + blob UPSERT. 반환: `.guild/` 상대 경로 (본문 참조용 `attachments/...`).
pub async fn save_attachment(store: &Store, bytes: &[u8], ext: &str) -> AppResult<String> {
    let ext = sanitize_ext(ext);
    if bytes.is_empty() {
        return Err(AppError::BadRequest("빈 첨부".into()));
    }
    let _ = journal::append(
        &store.journal_pool,
        "save_attachment",
        &json!({ "ext": ext, "len": bytes.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    // 파일명 — 시각 + 난수 (충돌 회피, 사용자 입력 없음 → traversal 불가).
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let rand: u32 = (nanos as u32) ^ 0x5bd1_e995;
    let name = format!("{nanos:x}-{rand:08x}.{ext}");
    let rel = format!("attachments/{name}");

    std::fs::create_dir_all(store.paths.attachments_dir())
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let path = store.paths.dot_guild().join(&rel);
    std::fs::write(&path, bytes)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("첨부 write 실패: {e}")))?;

    upsert_blob(store, &rel, bytes, repo_fs::mtime_unix_nanos(&path)).await?;
    Ok(rel)
}

async fn upsert_blob(store: &Store, rel: &str, bytes: &[u8], mtime: i64) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO attachment_blobs (rel_path, bytes, mtime) VALUES (?, ?, ?)
         ON CONFLICT(rel_path) DO UPDATE SET bytes = excluded.bytes, mtime = excluded.mtime",
    )
    .bind(rel)
    .bind(bytes)
    .bind(mtime)
    .execute(&store.index_pool)
    .await?;
    Ok(())
}

/// 양방향 self-heal — reindex 가 호출.
///
/// 1. 디렉토리의 새 / 변경 (mtime) 파일 → blob UPSERT.
/// 2. blob 만 있고 파일이 사라진 경우 → 파일 복원 (snapshot restore 후
///    git 없이도 첨부가 돌아오는 경로).
///
/// 반환: (blob 갱신 수, 파일 복원 수).
pub async fn sync_attachment_blobs(store: &Store) -> AppResult<(usize, usize)> {
    let dir = store.paths.attachments_dir();
    let mut upserted = 0usize;
    let mut restored = 0usize;

    // DB 의 rel → mtime.
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT rel_path, mtime FROM attachment_blobs")
            .fetch_all(&store.index_pool)
            .await?;
    let mut db: std::collections::HashMap<String, i64> = rows.into_iter().collect();

    // 1. 파일 → blob.
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?
            .flatten()
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let rel = format!("attachments/{name}");
            let mtime = repo_fs::mtime_unix_nanos(&path);
            let stale = match db.remove(&rel) {
                Some(db_mtime) => mtime > db_mtime,
                None => true,
            };
            if stale {
                let bytes = std::fs::read(&path)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
                upsert_blob(store, &rel, &bytes, mtime).await?;
                upserted += 1;
            }
        }
    }

    // 2. blob 만 남은 것 (파일 소실) → 복원.
    for (rel, _) in db {
        let bytes: Vec<u8> =
            sqlx::query_scalar("SELECT bytes FROM attachment_blobs WHERE rel_path = ?")
                .bind(&rel)
                .fetch_one(&store.index_pool)
                .await?;
        let path = store.paths.dot_guild().join(&rel);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&path, &bytes).is_ok() {
            // 복원된 파일의 새 mtime 으로 갱신 — 다음 sync 가 재-upsert 안 하게.
            upsert_blob(store, &rel, &bytes, repo_fs::mtime_unix_nanos(&path)).await?;
            restored += 1;
        }
    }

    Ok((upserted, restored))
}

// ───────────────────────── DEV-156: 첨부 목록 (Jira 식) ─────────────────────────
// 본문과 별개로 quest/campaign 에 "첨부된" 파일 목록. 진리원은 sidecar
// `.guild/{quests|campaigns}/{slug}.attachments.json`. 파일 바이트는 DEV-069 의
// save_attachment 가 `.guild/attachments/` 에 저장 + blob 백업하므로, 여기서는
// (경로, 원본 파일명) 메타만 관리한다.

use crate::models::QuestAttachment;

fn read_attachment_list(path: &std::path::Path) -> Vec<QuestAttachment> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_attachment_list(path: &std::path::Path, list: &[QuestAttachment]) -> AppResult<()> {
    if list.is_empty() {
        // 비면 sidecar 제거 (불필요 파일 남기지 않음).
        let _ = std::fs::remove_file(path);
        return Ok(());
    }
    let json = serde_json::to_string_pretty(list)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize attachments: {e}")))?;
    std::fs::write(path, json)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write attachments sidecar: {e}")))?;
    Ok(())
}

/// quest 첨부 목록 조회.
pub fn list_quest_attachments(store: &Store, slug: &str) -> Vec<QuestAttachment> {
    read_attachment_list(&store.paths.quest_attachments_meta_path(slug))
}

/// campaign 첨부 목록 조회.
pub fn list_campaign_attachments(store: &Store, slug: &str) -> Vec<QuestAttachment> {
    read_attachment_list(&store.paths.campaign_attachments_meta_path(slug))
}

/// quest 에 첨부 추가 (path 중복이면 무시). 갱신된 목록 반환.
pub async fn add_quest_attachment(
    store: &Store,
    slug: &str,
    path: &str,
    name: &str,
) -> AppResult<Vec<QuestAttachment>> {
    add_attachment(store, &store.paths.quest_attachments_meta_path(slug), slug, path, name).await
}

/// campaign 에 첨부 추가. 갱신된 목록 반환.
pub async fn add_campaign_attachment(
    store: &Store,
    slug: &str,
    path: &str,
    name: &str,
) -> AppResult<Vec<QuestAttachment>> {
    add_attachment(store, &store.paths.campaign_attachments_meta_path(slug), slug, path, name).await
}

async fn add_attachment(
    store: &Store,
    meta_path: &std::path::Path,
    slug: &str,
    path: &str,
    name: &str,
) -> AppResult<Vec<QuestAttachment>> {
    if path.trim().is_empty() {
        return Err(AppError::BadRequest("빈 첨부 경로".into()));
    }
    let _ = journal::append(
        &store.journal_pool,
        "add_attachment",
        &json!({ "slug": slug, "path": path }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    let mut list = read_attachment_list(meta_path);
    if !list.iter().any(|a| a.path == path) {
        list.push(QuestAttachment {
            path: path.to_string(),
            name: if name.trim().is_empty() { path.to_string() } else { name.to_string() },
        });
        write_attachment_list(meta_path, &list)?;
    }
    Ok(list)
}

/// quest 첨부 제거 (목록에서만 — blob/파일은 self-heal 정책상 유지). 갱신 목록 반환.
pub async fn remove_quest_attachment(
    store: &Store,
    slug: &str,
    path: &str,
) -> AppResult<Vec<QuestAttachment>> {
    remove_attachment(store, &store.paths.quest_attachments_meta_path(slug), slug, path).await
}

/// campaign 첨부 제거. 갱신 목록 반환.
pub async fn remove_campaign_attachment(
    store: &Store,
    slug: &str,
    path: &str,
) -> AppResult<Vec<QuestAttachment>> {
    remove_attachment(store, &store.paths.campaign_attachments_meta_path(slug), slug, path).await
}

async fn remove_attachment(
    store: &Store,
    meta_path: &std::path::Path,
    slug: &str,
    path: &str,
) -> AppResult<Vec<QuestAttachment>> {
    let _ = journal::append(
        &store.journal_pool,
        "remove_attachment",
        &json!({ "slug": slug, "path": path }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;
    let mut list = read_attachment_list(meta_path);
    let before = list.len();
    list.retain(|a| a.path != path);
    if list.len() != before {
        write_attachment_list(meta_path, &list)?;
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::seed_guild_dir;

    async fn setup(label: &str) -> (std::path::PathBuf, Store) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-attach-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        (dir, store)
    }

    #[tokio::test]
    async fn save_writes_file_and_blob() {
        let (dir, store) = setup("save").await;
        let rel = save_attachment(&store, b"PNGDATA", "png").await.unwrap();
        assert!(rel.starts_with("attachments/") && rel.ends_with(".png"));
        assert!(store.paths.dot_guild().join(&rel).exists());
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attachment_blobs")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
        // DEV-069 후속(admin #8): 임의 확장자 허용 — 미디어 외 파일도 첨부 가능.
        let z = save_attachment(&store, b"ZIPDATA", "zip").await.unwrap();
        assert!(z.ends_with(".zip"));
        // 확장자 없으면 .bin 으로 정규화.
        let b = save_attachment(&store, b"RAW", "").await.unwrap();
        assert!(b.ends_with(".bin"));
        // 빈 바이트는 여전히 거부.
        assert!(save_attachment(&store, b"", "png").await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-156: quest 첨부 목록 sidecar add/list/remove 라운드트립 + 중복 무시.
    #[tokio::test]
    async fn attachment_list_roundtrip() {
        let (dir, store) = setup("list").await;
        assert!(list_quest_attachments(&store, "DEV-001").is_empty());

        let l1 = add_quest_attachment(&store, "DEV-001", "attachments/a.zip", "design.zip")
            .await
            .unwrap();
        assert_eq!(l1.len(), 1);
        assert_eq!(l1[0].name, "design.zip");
        // 중복 path 는 무시.
        let l2 = add_quest_attachment(&store, "DEV-001", "attachments/a.zip", "design.zip")
            .await
            .unwrap();
        assert_eq!(l2.len(), 1);
        // 다른 path 추가.
        add_quest_attachment(&store, "DEV-001", "attachments/b.pdf", "ref.pdf")
            .await
            .unwrap();
        assert_eq!(list_quest_attachments(&store, "DEV-001").len(), 2);
        // sidecar 파일 실재.
        assert!(store.paths.quest_attachments_meta_path("DEV-001").exists());

        let l3 = remove_quest_attachment(&store, "DEV-001", "attachments/a.zip")
            .await
            .unwrap();
        assert_eq!(l3.len(), 1);
        assert_eq!(l3[0].path, "attachments/b.pdf");
        // 마지막 제거 시 sidecar 삭제.
        remove_quest_attachment(&store, "DEV-001", "attachments/b.pdf").await.unwrap();
        assert!(!store.paths.quest_attachments_meta_path("DEV-001").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// self-heal — 파일 삭제 후 sync 가 blob 에서 복원.
    #[tokio::test]
    async fn sync_restores_missing_file_from_blob() {
        let (dir, store) = setup("heal").await;
        let rel = save_attachment(&store, b"IMAGE", "png").await.unwrap();
        let path = store.paths.dot_guild().join(&rel);
        std::fs::remove_file(&path).unwrap();

        let (up, restored) = sync_attachment_blobs(&store).await.unwrap();
        assert_eq!((up, restored), (0, 1));
        assert_eq!(std::fs::read(&path).unwrap(), b"IMAGE");

        // 외부 추가 파일 → blob 으로.
        std::fs::write(store.paths.attachments_dir().join("manual.png"), b"M").unwrap();
        let (up2, _) = sync_attachment_blobs(&store).await.unwrap();
        assert_eq!(up2, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
