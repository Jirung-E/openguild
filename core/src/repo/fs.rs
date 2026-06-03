//! 파일 시스템 헬퍼 — atomic write, mtime 비교 등.

use anyhow::{Context, Result};
use std::path::Path;
use std::time::SystemTime;

/// 파일 atomic 쓰기: temp 파일에 쓴 뒤 final 위치로 rename.
/// crash / 부분 쓰기 시 final 파일은 옛 내용 그대로 유지됨 (또는 부재).
///
/// Windows / POSIX 양쪽에서 `fs::rename` 은 같은 파일시스템 내 atomic.
pub fn write_atomic<P: AsRef<Path>>(path: P, contents: &str) -> Result<()> {
    let path = path.as_ref();
    let dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(dir)
        .with_context(|| format!("failed to create dir: {}", dir.display()))?;

    // temp 파일명: <원파일>.<pid>.<ns>.tmp — 같은 폴더 안이라 rename 이 atomic.
    let pid = std::process::id();
    let ns = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = path.with_extension(format!("{pid}.{ns}.tmp"));

    std::fs::write(&tmp, contents)
        .with_context(|| format!("failed to write temp file: {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename {} → {}", tmp.display(), path.display()))?;
    Ok(())
}

/// 파일 mtime (수정 시각). 없거나 읽기 실패 시 None.
pub fn mtime<P: AsRef<Path>>(path: P) -> Option<SystemTime> {
    std::fs::metadata(path.as_ref()).ok()?.modified().ok()
}

/// BUG-047: quest 본문 파일인지 — `.guild/quests/{slug}.md` 만 true.
/// sibling 파일 `.comments.md` / `.memo.md` (DEV-012 / DEV-094) 는 false.
///
/// 기준: file stem 이 `.` 을 포함하지 않음 (slug 자체엔 `.` 없음;
/// `DEV-094.comments` 같은 stem 은 sibling).
pub fn is_quest_body_file(path: &Path) -> bool {
    if path.extension().and_then(|s| s.to_str()) != Some("md") {
        return false;
    }
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    !stem.contains('.')
}

/// 디렉토리 안의 quest 본문 파일만 (sibling `.comments.md` / `.memo.md` 제외).
/// BUG-047: drift / reindex 의 false positive 제거 + skip 경고 제거.
pub fn list_quest_body_files<P: AsRef<Path>>(dir: P) -> Result<Vec<std::path::PathBuf>> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read dir: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_quest_body_file(p))
        .collect();
    paths.sort();
    Ok(paths)
}

/// 디렉토리 안의 특정 확장자 파일 경로 나열 (정렬).
pub fn list_with_extension<P: AsRef<Path>>(dir: P, ext: &str) -> Result<Vec<std::path::PathBuf>> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read dir: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some(ext))
        .collect();
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-fs-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn write_atomic_creates_file_and_parents() {
        let dir = fresh_tmp("write");
        let target = dir.join("nested/sub/file.txt");
        write_atomic(&target, "hello world").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello world");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_overwrites() {
        let dir = fresh_tmp("overwrite");
        let target = dir.join("file.txt");
        write_atomic(&target, "v1").unwrap();
        write_atomic(&target, "v2").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "v2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_no_temp_files_left() {
        let dir = fresh_tmp("clean");
        let target = dir.join("file.txt");
        write_atomic(&target, "x").unwrap();
        // dir 안에 .tmp 파일 없어야 함
        let leftover: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftover.is_empty(), "temp file leaked: {leftover:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mtime_returns_some_for_existing_file() {
        let dir = fresh_tmp("mtime");
        let target = dir.join("file.txt");
        write_atomic(&target, "x").unwrap();
        assert!(mtime(&target).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mtime_returns_none_for_missing_file() {
        assert!(mtime("/path/does/not/exist/abcdef").is_none());
    }

    #[test]
    fn list_with_extension_filters_and_sorts() {
        let dir = fresh_tmp("list");
        write_atomic(dir.join("c.md"), "").unwrap();
        write_atomic(dir.join("a.md"), "").unwrap();
        write_atomic(dir.join("b.md"), "").unwrap();
        write_atomic(dir.join("ignored.txt"), "").unwrap();

        let paths = list_with_extension(&dir, "md").unwrap();
        let names: Vec<_> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.md", "b.md", "c.md"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_with_extension_missing_dir_returns_empty() {
        let paths = list_with_extension("/nonexistent/xyz/abc", "md").unwrap();
        assert!(paths.is_empty());
    }

    /// BUG-047: list_quest_body_files 가 sibling 파일을 제외하는지.
    #[test]
    fn list_quest_body_files_excludes_siblings() {
        let dir = fresh_tmp("quest-body-filter");
        write_atomic(dir.join("DEV-001.md"), "").unwrap();
        write_atomic(dir.join("DEV-002.md"), "").unwrap();
        write_atomic(dir.join("DEV-001.comments.md"), "").unwrap();
        write_atomic(dir.join("DEV-001.memo.md"), "").unwrap();
        write_atomic(dir.join("DEV-002.comments.md"), "").unwrap();
        write_atomic(dir.join("README.md"), "").unwrap(); // hidden quest_id 없음? — stem README, dot 없음
        write_atomic(dir.join("ignored.txt"), "").unwrap();

        let names: Vec<String> = list_quest_body_files(&dir)
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        // sibling (`.comments.md` / `.memo.md`) 는 제외 — stem 에 `.` 포함.
        // README.md / DEV-001.md / DEV-002.md 만 (sorted).
        assert_eq!(names, vec!["DEV-001.md", "DEV-002.md", "README.md"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_quest_body_file_recognizes_patterns() {
        use std::path::PathBuf;
        assert!(is_quest_body_file(&PathBuf::from("DEV-001.md")));
        assert!(is_quest_body_file(&PathBuf::from("BUG-099.md")));
        assert!(!is_quest_body_file(&PathBuf::from("DEV-001.comments.md")));
        assert!(!is_quest_body_file(&PathBuf::from("DEV-001.memo.md")));
        assert!(!is_quest_body_file(&PathBuf::from("DEV-001.txt"))); // 잘못된 확장
        assert!(!is_quest_body_file(&PathBuf::from("DEV-001"))); // 확장 없음
    }
}
