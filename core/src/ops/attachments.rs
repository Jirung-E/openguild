//! DEV-069 / BUG-188: 본문 첨부파일 — 파일 저장만 한다.
//!
//! 파일(`.guild/attachments/**`)이 진리원인 건 그대로다. 달라진 건 **백업**이다.
//!
//! 예전엔 같은 바이트를 `attachment_blobs` 테이블에도 넣어 스냅샷이 첨부까지
//! 담게 했다("git 은 선택사항 — 스냅샷만으로 복원 가능해야 한다"). 그런데
//! 첨부는 크기 상한이 없는 유일한 데이터라, 이 설계는 두 군데서 깨진다:
//!
//! - SQLite blob 상한(약 1GB) — 1.5GB 파일 첨부가 `code 18: string or blob too
//!   big` 으로 실패했다(admin 보고). 게다가 파일 write 는 이미 끝난 뒤라
//!   참조 없는 대용량 파일이 남아 **이후 모든 reindex/백업을 계속 깨뜨렸다.**
//! - 용량 — 첨부 하나가 index.db 와 매 스냅샷을 그 크기만큼 부풀린다.
//!
//! admin 결정: **첨부파일은 백업 대상에서 제외한다.** 임계값을 두는 대신 아예
//! 뺀다. 그래서 여기선 blob 을 만들지 않고, 스냅샷도 `attachments/` 를 담지
//! 않는다(snapshot.rs 의 SOURCE_SUBDIRS). 사용자에게는 어드민 > 백업 화면에서
//! "백업에 첨부는 포함되지 않는다"고 밝힌다.

use serde_json::json;

use crate::error::{AppError, AppResult};
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

/// 저장 파일명 — 시각 + 난수 (충돌 회피, 사용자 입력 없음 → traversal 불가).
/// 반환은 `.guild/` 상대 경로(`attachments/…`).
fn new_attachment_rel(ext: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let rand: u32 = (nanos as u32) ^ 0x5bd1_e995;
    format!("attachments/{nanos:x}-{rand:08x}.{ext}")
}

/// 첨부 저장 — bytes 를 `.guild/attachments/{nanos}-{rand}.{ext}` 로 write.
/// 반환: `.guild/` 상대 경로 (본문 참조용 `attachments/...`).
///
/// 경로를 이미 아는 데스크탑은 `save_attachment_from_file` 을 쓴다 — 큰 파일을
/// 메모리에 올리지 않는다.
pub async fn save_attachment(store: &Store, bytes: &[u8], ext: &str) -> AppResult<String> {
    let ext = sanitize_ext(ext);
    if bytes.is_empty() {
        return Err(AppError::BadRequest(crate::tf!("빈 첨부", "empty attachment")));
    }
    let _ = journal::append(
        &store.journal_pool,
        "save_attachment",
        &json!({ "ext": ext, "len": bytes.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let rel = new_attachment_rel(&ext);

    std::fs::create_dir_all(store.paths.attachments_dir())
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let path = store.paths.dot_guild().join(&rel);
    std::fs::write(&path, bytes)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(crate::tf!("첨부 write 실패: {e}", "attachment write failed: {e}"))))?;

    Ok(rel)
}

/// BUG-188: 원본 **경로**에서 첨부 저장 — 바이트를 메모리에 올리지 않는다.
///
/// 데스크탑은 파일 선택 다이얼로그가 경로를 주므로 이 경로를 쓴다. 예전엔
/// 경로를 받고도 `std::fs::read` 로 통째로 읽어 `save_attachment` 에 넘겨,
/// 1.5GB 파일이면 그만큼 메모리를 잡았다(그리고 blob INSERT 에서 터졌다).
/// `std::fs::copy` 는 OS 의 복사 경로를 타서 파일 크기와 무관하게 일정하다.
///
/// 실패 시 부분 복사본을 남기지 않는다 — 디스크가 꽉 차면 조각 파일이
/// `.guild/attachments/` 에 남고, 그건 아무도 참조하지 않는 쓰레기가 된다.
pub async fn save_attachment_from_file(
    store: &Store,
    src: &std::path::Path,
    ext: &str,
) -> AppResult<String> {
    save_attachment_from_file_with_progress(store, src, ext, |_copied, _total| {}).await
}

/// DEV-321: 위와 같되 진행 상황을 알린다 — `on_progress(복사된 바이트, 전체)`.
///
/// 대용량 첨부는 저장이 수 초 걸리는데 예전엔 "돌고 있음"밖에 알릴 수 없었다
/// (DEV-298 의 불확정 바). `std::fs::copy` 는 한 번에 끝나 중간을 관측할 수
/// 없으므로, 여기서 버퍼 단위로 직접 옮기며 진행을 흘린다.
///
/// 호출 빈도는 청크 수만큼이다 — 이벤트로 내보낼 쪽(GUI)에서 throttle 한다.
/// core 는 Tauri 를 모르므로 콜백까지만 책임진다.
pub async fn save_attachment_from_file_with_progress(
    store: &Store,
    src: &std::path::Path,
    ext: &str,
    mut on_progress: impl FnMut(u64, u64),
) -> AppResult<String> {
    let ext = sanitize_ext(ext);
    let meta = std::fs::metadata(src)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(crate::tf!("첨부 원본 확인 실패: {e}", "attachment source stat failed: {e}"))))?;
    if meta.len() == 0 {
        return Err(AppError::BadRequest(crate::tf!("빈 첨부", "empty attachment")));
    }
    let _ = journal::append(
        &store.journal_pool,
        "save_attachment",
        &json!({ "ext": ext, "len": meta.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let rel = new_attachment_rel(&ext);
    std::fs::create_dir_all(store.paths.attachments_dir())
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let dst = store.paths.dot_guild().join(&rel);
    if let Err(e) = copy_with_progress(src, &dst, meta.len(), &mut on_progress) {
        let _ = std::fs::remove_file(&dst);
        return Err(AppError::Internal(anyhow::anyhow!(crate::tf!(
            "첨부 복사 실패: {e}",
            "attachment copy failed: {e}"
        ))));
    }
    Ok(rel)
}

/// 진행 보고가 가능한 파일 복사. 버퍼는 4 MiB — 너무 작으면 콜백/syscall 이
/// 잦아지고, 너무 크면 진행이 뚝뚝 끊겨 보인다.
///
/// `total` 은 시작 시점의 크기다. 복사 중에 원본이 커지면 실제 복사량이 이를
/// 넘을 수 있으므로, 보고 값은 total 로 clamp 해서 100%를 넘지 않게 한다.
fn copy_with_progress(
    src: &std::path::Path,
    dst: &std::path::Path,
    total: u64,
    on_progress: &mut impl FnMut(u64, u64),
) -> std::io::Result<()> {
    use std::io::{Read, Write};

    const BUF: usize = 4 * 1024 * 1024;
    let mut reader = std::io::BufReader::with_capacity(64 * 1024, std::fs::File::open(src)?);
    let mut writer = std::io::BufWriter::with_capacity(64 * 1024, std::fs::File::create(dst)?);
    let mut buf = vec![0u8; BUF];
    let mut copied: u64 = 0;

    on_progress(0, total);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        copied = copied.saturating_add(n as u64).min(total.max(1));
        on_progress(copied, total);
    }
    writer.flush()?;
    Ok(())
}


// ───────────────────────── DEV-156: 첨부 목록 (Jira 식) ─────────────────────────
// 본문과 별개로 quest/campaign 에 "첨부된" 파일 목록. 진리원은 sidecar
// `.guild/{quests|campaigns}/{slug}.attachments.json`. 파일 바이트는 DEV-069 의
// save_attachment 가 `.guild/attachments/` 에 저장하므로, 여기서는
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

/// DEV-237: 도서관 문서 첨부 목록 조회.
pub fn list_book_attachments(store: &Store, book_id: &str) -> Vec<QuestAttachment> {
    read_attachment_list(&store.paths.book_attachments_meta_path(book_id))
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

/// DEV-237: 도서관 문서에 첨부 추가 — 이미지/동영상 외 임의 파일 지원.
/// quest/campaign 과 같은 sidecar 패턴 재사용.
pub async fn add_book_attachment(
    store: &Store,
    book_id: &str,
    path: &str,
    name: &str,
) -> AppResult<Vec<QuestAttachment>> {
    add_attachment(store, &store.paths.book_attachments_meta_path(book_id), book_id, path, name)
        .await
}

async fn add_attachment(
    store: &Store,
    meta_path: &std::path::Path,
    slug: &str,
    path: &str,
    name: &str,
) -> AppResult<Vec<QuestAttachment>> {
    if path.trim().is_empty() {
        return Err(AppError::BadRequest(crate::tf!("빈 첨부 경로", "empty attachment path")));
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

/// DEV-237: 도서관 문서 첨부 제거. 갱신 목록 반환.
pub async fn remove_book_attachment(
    store: &Store,
    book_id: &str,
    path: &str,
) -> AppResult<Vec<QuestAttachment>> {
    remove_attachment(store, &store.paths.book_attachments_meta_path(book_id), book_id, path).await
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
        // BUG-084: 다른 첨부 sidecar / 본문에서 더는 참조 안 하면 실제 파일 + blob 삭제
        // (blob 까지 지워야 reindex self-heal 이 복원하지 않음). 참조 중이면 유지.
        gc_attachment_file(store, path).await;
    }
    Ok(list)
}

/// BUG-084: path 가 어떤 첨부 sidecar / 본문에서도 참조 안 되면 실제 파일 삭제.
/// (BUG-188 이후 blob 사본이 없으므로 지울 것은 파일 하나뿐이다.)
async fn gc_attachment_file(store: &Store, path: &str) {
    if path.trim().is_empty() || attachment_referenced(store, path).await {
        return;
    }
    let abs = store.paths.dot_guild().join(path);
    let _ = std::fs::remove_file(&abs);
}

/// path 가 어떤 첨부 sidecar(quest/campaign) 또는 본문에서도 참조되지 않는지 —
/// GC 판정용. (BUG-188 로 blob self-heal 이 사라져 이 per-path 판정만 남았다.)
async fn attachment_referenced(store: &Store, path: &str) -> bool {
    // DEV-237: 도서관 문서(sidecar + body)도 스캔 대상 — 그래야 도서관에서만
    // 쓰이는 첨부를 다른 sidecar/본문에서 안 쓴다고 오판해 GC 하지 않는다.
    for dir in [
        store.paths.quests_dir(),
        store.paths.campaigns_dir(),
        store.paths.library_dir(),
    ] {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let is_sidecar = p
                .file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.ends_with(".attachments.json"))
                .unwrap_or(false);
            if is_sidecar && read_attachment_list(&p).iter().any(|a| a.path == path) {
                return true;
            }
        }
    }
    let like = format!("%{path}%");
    // (테이블, 본문 컬럼) — quests/campaigns 는 description, library_docs 는 body.
    for (table, col) in [
        ("quests", "description"),
        ("campaigns", "description"),
        ("library_docs", "body"),
    ] {
        let q = format!("SELECT 1 FROM {table} WHERE {col} LIKE ? LIMIT 1");
        let hit: Option<i64> = sqlx::query_scalar(&q)
            .bind(&like)
            .fetch_optional(&store.index_pool)
            .await
            .ok()
            .flatten();
        if hit.is_some() {
            return true;
        }
    }
    false
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
    async fn save_writes_file() {
        let (dir, store) = setup("save").await;
        let rel = save_attachment(&store, b"PNGDATA", "png").await.unwrap();
        assert!(rel.starts_with("attachments/") && rel.ends_with(".png"));
        assert!(store.paths.dot_guild().join(&rel).exists());
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

    /// BUG-084: 어디서도 참조 안 되면 remove 가 실제 파일까지 삭제.
    #[tokio::test]
    async fn remove_deletes_orphan_file() {
        let (dir, store) = setup("gc").await;
        let rel = save_attachment(&store, b"DATA", "zip").await.unwrap();
        add_quest_attachment(&store, "DEV-001", &rel, "f.zip").await.unwrap();
        let abs = store.paths.dot_guild().join(&rel);
        assert!(abs.exists());

        remove_quest_attachment(&store, "DEV-001", &rel).await.unwrap();
        assert!(!abs.exists(), "orphan 파일이 삭제되어야");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-321: 진행 콜백 — 0 에서 시작해 단조 증가하고 전체 크기로 끝난다.
    /// (여기가 깨지면 UI 진행률이 뒤로 가거나 100% 에 못 닿는다.)
    #[tokio::test]
    async fn save_from_file_reports_progress() {
        let (dir, store) = setup("progress").await;
        // 버퍼(4 MiB)를 여러 번 도는 크기로 — 청크가 1개면 계약을 못 본다.
        let size = 10 * 1024 * 1024 + 12_345;
        let src = dir.join("big.bin");
        std::fs::write(&src, vec![7u8; size]).unwrap();

        let mut seen: Vec<(u64, u64)> = Vec::new();
        let rel = save_attachment_from_file_with_progress(&store, &src, "bin", |c, t| {
            seen.push((c, t))
        })
        .await
        .unwrap();

        assert!(seen.len() > 2, "청크마다 보고되어야: {}", seen.len());
        assert_eq!(seen.first().unwrap().0, 0, "0 에서 시작");
        assert_eq!(seen.last().unwrap().0, size as u64, "전체 크기로 끝나야");
        assert!(
            seen.iter().all(|&(_, t)| t == size as u64),
            "total 은 내내 같아야"
        );
        assert!(
            seen.windows(2).all(|w| w[0].0 <= w[1].0),
            "진행률이 뒤로 가면 안 됨"
        );
        // 내용도 그대로.
        let dst = store.paths.dot_guild().join(&rel);
        assert_eq!(std::fs::metadata(&dst).unwrap().len(), size as u64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-188: 경로 기반 저장 — 원본을 복사하고 내용이 보존되는지.
    /// (데스크탑 첨부가 타는 경로. 큰 파일을 메모리에 올리지 않는 게 목적이라
    ///  여기선 동작 계약만 고정한다.)
    #[tokio::test]
    async fn save_from_file_copies_source() {
        let (dir, store) = setup("from-file").await;
        let src = dir.join("원본.ZIP");
        std::fs::write(&src, b"ZIPBYTES").unwrap();

        let rel = save_attachment_from_file(&store, &src, "ZIP").await.unwrap();
        assert!(rel.ends_with(".zip"), "확장자는 소문자로 정규화: {rel}");
        let dst = store.paths.dot_guild().join(&rel);
        assert_eq!(std::fs::read(&dst).unwrap(), b"ZIPBYTES");
        assert!(src.exists(), "원본은 그대로 있어야(복사이므로)");

        // 빈 파일 / 없는 파일은 거부.
        let empty = dir.join("empty.bin");
        std::fs::write(&empty, b"").unwrap();
        assert!(save_attachment_from_file(&store, &empty, "bin").await.is_err());
        assert!(
            save_attachment_from_file(&store, &dir.join("없는파일.zip"), "zip")
                .await
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-084: 다른 첨부에서 참조 중이면 파일 유지.
    #[tokio::test]
    async fn remove_keeps_referenced_file() {
        let (dir, store) = setup("gckeep").await;
        let rel = save_attachment(&store, b"DATA", "zip").await.unwrap();
        add_quest_attachment(&store, "DEV-001", &rel, "f.zip").await.unwrap();
        add_quest_attachment(&store, "DEV-002", &rel, "f.zip").await.unwrap();
        let abs = store.paths.dot_guild().join(&rel);

        remove_quest_attachment(&store, "DEV-001", &rel).await.unwrap();
        assert!(abs.exists(), "다른 첨부가 참조 중이면 파일 유지");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-188: 첨부는 백업 대상이 아니다 — 저장이 index.db 를 건드리지 않고,
    /// 파일이 사라져도 되살아나지 않는다(예전엔 blob self-heal 이 복원했다).
    ///
    /// blob 테이블 자체가 사라졌으므로(마이그레이션 0029) "저장 후에도 첨부
    /// 테이블이 없다" 로 정책을 고정한다.
    #[tokio::test]
    async fn attachments_are_not_backed_up_into_index_db() {
        let (dir, store) = setup("no-blob").await;
        let rel = save_attachment(&store, b"IMAGE", "png").await.unwrap();
        add_quest_attachment(&store, "DEV-001", &rel, "img.png").await.unwrap();

        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'attachment_blobs'",
        )
        .fetch_all(&store.index_pool)
        .await
        .unwrap();
        assert!(tables.is_empty(), "attachment_blobs 가 남아 있으면 안 됨: {tables:?}");

        // 파일을 지우면 그걸로 끝 — 재색인이 되살리지 않는다.
        let path = store.paths.dot_guild().join(&rel);
        std::fs::remove_file(&path).unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        assert!(!path.exists(), "첨부는 복원 대상이 아니다");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-237: 도서관 문서 첨부 — quest/campaign 과 동일한 CRUD + GC 시맨틱.
    #[tokio::test]
    async fn book_attachment_list_roundtrip_and_gc() {
        let (dir, store) = setup("book-attach").await;
        assert!(list_book_attachments(&store, "BOOK-001").is_empty());

        let rel = save_attachment(&store, b"DOC", "pdf").await.unwrap();
        let list = add_book_attachment(&store, "BOOK-001", &rel, "spec.pdf")
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "spec.pdf");
        assert!(store.paths.book_attachments_meta_path("BOOK-001").exists());

        let abs = store.paths.dot_guild().join(&rel);
        remove_book_attachment(&store, "BOOK-001", &rel).await.unwrap();
        assert!(!abs.exists(), "미참조 첨부는 제거 시 파일도 삭제");
        assert!(!store.paths.book_attachments_meta_path("BOOK-001").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-237: quest 첨부가 도서관 문서에서도 참조 중이면(교차 참조) 유지.
    #[tokio::test]
    async fn book_attachment_referenced_keeps_quest_attachment() {
        let (dir, store) = setup("book-xref").await;
        let rel = save_attachment(&store, b"DATA", "zip").await.unwrap();
        add_quest_attachment(&store, "DEV-001", &rel, "f.zip").await.unwrap();
        add_book_attachment(&store, "BOOK-001", &rel, "f.zip").await.unwrap();
        let abs = store.paths.dot_guild().join(&rel);

        remove_quest_attachment(&store, "DEV-001", &rel).await.unwrap();
        assert!(abs.exists(), "도서관 문서가 여전히 참조 중이면 파일 유지");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
