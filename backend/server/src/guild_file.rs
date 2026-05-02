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
