//! 파일 진리원 저장소 — `.guild/quests/{slug}.md`, `.guild/types/{prefix}.toml`,
//! `.guild/statuses/{order}-{slug}.toml` 의 read / write / serialize / parse.
//!
//! 이 모듈은 sqlx / DB 와 무관. 순수 파일 IO + (de)serialization.
//! index.db 캐시 갱신은 호출자 (services) 의 몫.
//!
//! 설계 근거: `docs/storage-design.md`.

pub mod auto;
pub mod fs;
pub mod quest;
pub mod seed;
pub mod status_def;
pub mod type_def;

pub use auto::{QuestRef, QuestRelations};
pub use quest::{QuestFile, QuestFrontmatter, AUTO_BEGIN, AUTO_END};
pub use seed::{default_statuses, default_types, seed_guild_dir, SeedReport};
pub use status_def::StatusFile;
pub use type_def::{Counter, TypeFile};

use std::path::{Path, PathBuf};

/// `.guild/` 디렉토리 경로 계산.
#[derive(Clone)]
pub struct GuildPaths {
    pub guild_root: PathBuf,
}

impl GuildPaths {
    pub fn new<P: AsRef<Path>>(guild_root: P) -> Self {
        Self {
            guild_root: guild_root.as_ref().to_path_buf(),
        }
    }

    pub fn dot_guild(&self) -> PathBuf {
        self.guild_root.join(".guild")
    }

    pub fn quests_dir(&self) -> PathBuf {
        self.dot_guild().join("quests")
    }

    pub fn types_dir(&self) -> PathBuf {
        self.dot_guild().join("types")
    }

    pub fn statuses_dir(&self) -> PathBuf {
        self.dot_guild().join("statuses")
    }

    pub fn index_db(&self) -> PathBuf {
        self.dot_guild().join("index.db")
    }

    pub fn positions_json(&self) -> PathBuf {
        self.dot_guild().join("positions.json")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.dot_guild().join("backups")
    }

    pub fn journal_db(&self) -> PathBuf {
        self.backups_dir().join("journal.db")
    }

    pub fn snapshots_dir(&self) -> PathBuf {
        self.backups_dir().join("snapshots")
    }

    pub fn lock_file(&self) -> PathBuf {
        self.dot_guild().join(".lock")
    }

    pub fn quest_path(&self, slug: &str) -> PathBuf {
        self.quests_dir().join(format!("{slug}.md"))
    }

    pub fn type_path(&self, prefix: &str) -> PathBuf {
        self.types_dir().join(format!("{prefix}.toml"))
    }

    /// `.guild/.gitignore` 의 표준 내용.
    pub fn gitignore_content() -> &'static str {
        "# openguild — 내부 캐시 / UI 상태 / 백업 (git 추적 X)\n\
         index.db\n\
         positions.json\n\
         backups/\n\
         .lock\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_compose_under_dot_guild() {
        let p = GuildPaths::new("/tmp/monitor");
        assert_eq!(p.dot_guild(), PathBuf::from("/tmp/monitor/.guild"));
        assert_eq!(
            p.quests_dir(),
            PathBuf::from("/tmp/monitor/.guild/quests")
        );
        assert_eq!(
            p.quest_path("DEV-001"),
            PathBuf::from("/tmp/monitor/.guild/quests/DEV-001.md")
        );
        assert_eq!(
            p.type_path("DEV"),
            PathBuf::from("/tmp/monitor/.guild/types/DEV.toml")
        );
        assert_eq!(
            p.journal_db(),
            PathBuf::from("/tmp/monitor/.guild/backups/journal.db")
        );
    }

    #[test]
    fn gitignore_content_lists_internals() {
        let s = GuildPaths::gitignore_content();
        assert!(s.contains("index.db"));
        assert!(s.contains("positions.json"));
        assert!(s.contains("backups/"));
        assert!(s.contains(".lock"));
    }
}
