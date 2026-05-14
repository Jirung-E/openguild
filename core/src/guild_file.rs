use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

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
}
