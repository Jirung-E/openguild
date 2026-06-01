//! DEV-012: Quest 별 댓글 / 메모.
//!
//! - **댓글** (`.guild/quests/{slug}.comments.md`) — 팀 공유, git tracked.
//!   여러 사용자의 토론. 자유 markdown — 작성자 이름 / 시간은 사용자 본인이
//!   markdown 안에 적음 (단일 사용자 데스크탑 단계라 자동화 X).
//! - **메모** (`.guild/quests/{slug}.memo.md`) — 비공개, gitignored.
//!   본인만 보는 작업 메모.
//!
//! 형식: frontmatter 없는 plain Markdown. 부재 = "아직 없음".
//! 파일이 진리원 — DB 캐시 없음.

use anyhow::{Context, Result};

use super::fs::write_atomic;
use super::GuildPaths;

/// 공개 댓글 파일 읽기. 부재 시 `Ok(None)`.
pub fn read_comments(paths: &GuildPaths, slug: &str) -> Result<Option<String>> {
    let p = paths.comments_path(slug);
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("failed to read comments: {}", p.display()))?;
    Ok(Some(s))
}

/// 공개 댓글 파일 쓰기 (atomic).
pub fn write_comments(paths: &GuildPaths, slug: &str, content: &str) -> Result<()> {
    write_atomic(&paths.comments_path(slug), content)
}

/// 비공개 메모 파일 읽기. 부재 시 `Ok(None)`.
pub fn read_memo(paths: &GuildPaths, slug: &str) -> Result<Option<String>> {
    let p = paths.memo_path(slug);
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("failed to read memo: {}", p.display()))?;
    Ok(Some(s))
}

/// 비공개 메모 파일 쓰기 (atomic).
pub fn write_memo(paths: &GuildPaths, slug: &str, content: &str) -> Result<()> {
    write_atomic(&paths.memo_path(slug), content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_paths(label: &str) -> (std::path::PathBuf, GuildPaths) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("og-comments-{label}-{ns}"));
        std::fs::create_dir_all(root.join(".guild/quests")).unwrap();
        (root.clone(), GuildPaths::new(root))
    }

    #[test]
    fn comments_roundtrip() {
        let (root, p) = fresh_paths("c-rt");
        assert!(read_comments(&p, "DEV-001").unwrap().is_none());
        write_comments(&p, "DEV-001", "# Discussion\n- LGTM").unwrap();
        assert_eq!(
            read_comments(&p, "DEV-001").unwrap().as_deref(),
            Some("# Discussion\n- LGTM")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn memo_roundtrip() {
        let (root, p) = fresh_paths("m-rt");
        assert!(read_memo(&p, "DEV-001").unwrap().is_none());
        write_memo(&p, "DEV-001", "TODO: 본문 정리").unwrap();
        assert_eq!(
            read_memo(&p, "DEV-001").unwrap().as_deref(),
            Some("TODO: 본문 정리")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn comments_and_memo_independent_paths() {
        let (root, p) = fresh_paths("indep");
        write_comments(&p, "BUG-007", "public").unwrap();
        write_memo(&p, "BUG-007", "private").unwrap();
        assert_eq!(read_comments(&p, "BUG-007").unwrap().as_deref(), Some("public"));
        assert_eq!(read_memo(&p, "BUG-007").unwrap().as_deref(), Some("private"));
        // 두 파일이 서로 다른 경로.
        assert_ne!(p.comments_path("BUG-007"), p.memo_path("BUG-007"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
