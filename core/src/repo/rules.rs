//! DEV-016: 길드 규칙 (`.guild/rules.md`).
//!
//! 팀 컨벤션 / 그라운드 룰 / 코드 스타일 / 배포 체크리스트 등 길드별 자유 문서.
//! 형식: frontmatter 없는 plain Markdown. 파일 부재 = "아직 규칙 없음" 상태.
//!
//! 파일이 진리원 — DB 캐시 없음. server / GUI 가 직접 파일 IO.

use anyhow::{Context, Result};

use super::fs::write_atomic;
use super::GuildPaths;

/// 규칙 파일을 읽어 반환. 파일 없으면 `Ok(None)`.
pub fn read(paths: &GuildPaths) -> Result<Option<String>> {
    let p = paths.rules_path();
    if !p.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&p)
        .with_context(|| format!("failed to read rules file: {}", p.display()))?;
    Ok(Some(s))
}

/// 규칙 파일을 atomic 쓰기. 빈 문자열도 그대로 저장 (사용자 의도).
pub fn write(paths: &GuildPaths, content: &str) -> Result<()> {
    // `.guild/` 가 이미 존재한다고 가정 (init 시 생성됨).
    // rules.md 자체는 부모 디렉토리 안에 직접.
    write_atomic(&paths.rules_path(), content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_paths(label: &str) -> (std::path::PathBuf, GuildPaths) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("og-rules-{label}-{ns}"));
        let dot = root.join(".guild");
        std::fs::create_dir_all(&dot).unwrap();
        (root.clone(), GuildPaths::new(root))
    }

    #[test]
    fn read_missing_returns_none() {
        let (root, p) = fresh_paths("none");
        assert!(read(&p).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_then_read_roundtrip() {
        let (root, p) = fresh_paths("rt");
        write(&p, "# Rules\n- branch = quest_id\n").unwrap();
        let got = read(&p).unwrap();
        assert_eq!(got.as_deref(), Some("# Rules\n- branch = quest_id\n"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_empty_is_allowed() {
        let (root, p) = fresh_paths("empty");
        write(&p, "").unwrap();
        let got = read(&p).unwrap();
        assert_eq!(got.as_deref(), Some(""));
        let _ = std::fs::remove_dir_all(&root);
    }
}
