use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct GuildFile {
    pub name: String,
    pub version: String,
    pub created_at: String,
}

/// guild 디렉터리에서 `{name}.guild` 파일을 찾아 파싱한다.
pub fn load(guild_path: &str) -> Result<GuildFile> {
    let dir = Path::new(guild_path);

    let guild_file = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory: {guild_path}"))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|e| e.to_str()) == Some("guild"))
        .with_context(|| format!("no .guild file found in: {guild_path}"))?;

    let content = std::fs::read_to_string(&guild_file)
        .with_context(|| format!("failed to read: {}", guild_file.display()))?;

    toml::from_str(&content)
        .with_context(|| format!("failed to parse: {}", guild_file.display()))
}

/// `start` 에서 시작해 부모 방향으로 거슬러 올라가며 `.guild` 가 있는 첫 디렉토리를 반환.
/// git 의 `.git` 탐색과 동일 패턴. 못 찾으면 None.
pub fn find_from(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    loop {
        if has_guild_file(&current) {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// cwd 부터 부모 방향 탐색. 못 찾으면 None.
pub fn find_from_cwd() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    find_from(&cwd)
}

fn has_guild_file(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("guild"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = base.join(format!("openguild-test-{id}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn load_parses_valid_guild_file() {
        let dir = tmp_dir();
        fs::write(
            dir.join("monitor.guild"),
            "name = \"모니터\"\nversion = \"1.0\"\ncreated_at = \"2026-05-12\"\n",
        )
        .unwrap();

        let g = load(dir.to_str().unwrap()).unwrap();
        assert_eq!(g.name, "모니터");
        assert_eq!(g.version, "1.0");
        assert_eq!(g.created_at, "2026-05-12");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_fails_when_no_guild_file() {
        let dir = tmp_dir();
        // 빈 디렉토리
        let err = load(dir.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("no .guild file"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_fails_when_directory_missing() {
        let err = load("/nonexistent/path/that/should/not/exist").unwrap_err();
        assert!(err.to_string().contains("failed to read directory"));
    }

    #[test]
    fn load_finds_guild_file_regardless_of_name() {
        let dir = tmp_dir();
        // 파일명이 "monitor.guild" 가 아니라 "anything.guild" 여도 OK
        fs::write(
            dir.join("anything.guild"),
            "name = \"X\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();
        let g = load(dir.to_str().unwrap()).unwrap();
        assert_eq!(g.name, "X");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_fails_on_malformed_toml() {
        let dir = tmp_dir();
        fs::write(dir.join("broken.guild"), "this is not toml === ").unwrap();
        let err = load(dir.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("failed to parse"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_from_finds_in_same_dir() {
        let dir = tmp_dir();
        fs::write(
            dir.join("monitor.guild"),
            "name = \"M\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();
        let found = find_from(&dir).expect("should find guild");
        assert_eq!(found, dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_from_walks_up_to_parent() {
        let root = tmp_dir();
        fs::write(
            root.join("monitor.guild"),
            "name = \"M\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();
        let nested = root.join("a/b/c");
        fs::create_dir_all(&nested).unwrap();

        let found = find_from(&nested).expect("should walk up");
        assert_eq!(found, root);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_from_returns_none_when_no_guild_anywhere() {
        // 임시 디렉토리 하위에서만 검사 — 부모(시스템 temp)에 .guild 가 없다고 가정
        let dir = tmp_dir();
        let nested = dir.join("x/y");
        fs::create_dir_all(&nested).unwrap();
        // 결과: 부모로 거슬러 올라가며 시스템 root 까지 갈 수 있음. 매우 드물게 시스템 어딘가에
        // .guild 가 있을 수 있으나 일반 환경에선 None. 발견 시엔 우리 dir 위가 아닌 곳이어야 함.
        let found = find_from(&nested);
        if let Some(p) = &found {
            assert!(
                !p.starts_with(&dir),
                "false positive — our tmp tree had no .guild"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
