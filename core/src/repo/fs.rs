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

/// DEV-121: 파일 mtime 을 Unix nanoseconds (i64) 로. timezone-independent
/// absolute time — SQLite INTEGER 와 직접 비교. 없거나 epoch 이전이면 0.
pub fn mtime_unix_nanos<P: AsRef<Path>>(path: P) -> i64 {
    let Some(t) = mtime(path) else { return 0 };
    t.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_nanos()).ok())
        .unwrap_or(0)
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

/// DEV-102: sibling 댓글 파일 (`{slug}.comments.md`) 인지.
pub fn is_quest_comment_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".comments.md"))
        .unwrap_or(false)
}

/// DEV-102: sibling 메모 파일 (`{slug}.memo.md`) 인지.
pub fn is_quest_memo_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".memo.md"))
        .unwrap_or(false)
}

/// DEV-102: `DEV-001.comments.md` / `DEV-001.memo.md` → `DEV-001` 추출.
/// 두 가지 suffix 모두 시도. 매칭 안 되면 None.
pub fn quest_slug_from_sibling_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    for suffix in [".comments.md", ".memo.md"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return Some(stem.to_string());
        }
    }
    None
}

/// DEV-102: 디렉토리 안의 quest 댓글 파일 (`*.comments.md`) 만 (정렬).
pub fn list_quest_comment_files<P: AsRef<Path>>(dir: P) -> Result<Vec<std::path::PathBuf>> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read dir: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_quest_comment_file(p))
        .collect();
    paths.sort();
    Ok(paths)
}

/// DEV-102: 디렉토리 안의 quest 메모 파일 (`*.memo.md`) 만 (정렬).
pub fn list_quest_memo_files<P: AsRef<Path>>(dir: P) -> Result<Vec<std::path::PathBuf>> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read dir: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_quest_memo_file(p))
        .collect();
    paths.sort();
    Ok(paths)
}

/// DEV-102: file mtime → ISO 8601 (Local TZ). 파일 부재 / 메타 실패 시 None.
pub fn mtime_iso8601<P: AsRef<Path>>(path: P) -> Option<String> {
    let mtime = std::fs::metadata(path.as_ref()).ok()?.modified().ok()?;
    let dt: chrono::DateTime<chrono::Local> = mtime.into();
    Some(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
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

    /// DEV-102: sibling 파일 인식 + slug 추출.
    #[test]
    fn sibling_helpers_recognize_and_extract_slug() {
        use std::path::PathBuf;
        let c = PathBuf::from("DEV-001.comments.md");
        let m = PathBuf::from("BUG-099.memo.md");
        let body = PathBuf::from("DEV-001.md");
        let other = PathBuf::from("README.md");

        assert!(is_quest_comment_file(&c));
        assert!(!is_quest_comment_file(&m));
        assert!(!is_quest_comment_file(&body));

        assert!(is_quest_memo_file(&m));
        assert!(!is_quest_memo_file(&c));
        assert!(!is_quest_memo_file(&body));

        assert_eq!(quest_slug_from_sibling_path(&c).as_deref(), Some("DEV-001"));
        assert_eq!(quest_slug_from_sibling_path(&m).as_deref(), Some("BUG-099"));
        assert!(quest_slug_from_sibling_path(&body).is_none());
        assert!(quest_slug_from_sibling_path(&other).is_none());
    }

    /// DEV-102: 디렉토리에서 sibling 파일들만 추리는지.
    #[test]
    fn list_sibling_files_filters_correctly() {
        let dir = fresh_tmp("sibling-list");
        write_atomic(dir.join("DEV-001.md"), "").unwrap();
        write_atomic(dir.join("DEV-001.comments.md"), "").unwrap();
        write_atomic(dir.join("DEV-001.memo.md"), "").unwrap();
        write_atomic(dir.join("DEV-002.comments.md"), "").unwrap();
        write_atomic(dir.join("README.md"), "").unwrap();

        let cnames: Vec<String> = list_quest_comment_files(&dir)
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(cnames, vec!["DEV-001.comments.md", "DEV-002.comments.md"]);

        let mnames: Vec<String> = list_quest_memo_files(&dir)
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(mnames, vec!["DEV-001.memo.md"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-102: mtime_iso8601 — 존재 파일은 Some, 부재는 None.
    #[test]
    fn mtime_iso8601_returns_some_for_existing() {
        let dir = fresh_tmp("mtime-iso");
        let target = dir.join("x.md");
        write_atomic(&target, "").unwrap();
        let iso = mtime_iso8601(&target).unwrap();
        // RFC 3339 형식: "YYYY-MM-DDTHH:MM:SS±HH:MM" → 최소 25자 (Z 포함은 20자).
        assert!(iso.len() >= 19, "iso too short: {iso}");
        assert!(iso.contains('T'), "iso missing T: {iso}");
        let none = mtime_iso8601(dir.join("nonexistent")).is_none();
        assert!(none);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
