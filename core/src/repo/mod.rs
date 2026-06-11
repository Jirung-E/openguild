//! 파일 진리원 저장소 — `.guild/quests/{slug}.md`, `.guild/types/{prefix}.toml`,
//! `.guild/statuses/{order}-{slug}.toml` 의 read / write / serialize / parse.
//!
//! 이 모듈은 sqlx / DB 와 무관. 순수 파일 IO + (de)serialization.
//! index.db 캐시 갱신은 호출자 (services) 의 몫.
//!
//! 설계 근거: `docs/storage-design.md`.

pub mod auto;
pub mod campaign;
pub mod comments;
pub mod fs;
pub mod quest;
pub mod rules;
pub mod seed;
pub mod status_def;
pub mod tag_def;
pub mod template;
pub mod type_def;

pub use auto::{QuestRef, QuestRelations};
pub use campaign::{extract_checklist_items, CampaignFile, CampaignFrontmatter, ChecklistLine};
pub use quest::{QuestFile, QuestFrontmatter, AUTO_BEGIN, AUTO_END};
pub use seed::{default_statuses, default_types, seed_guild_dir, SeedReport};
pub use status_def::StatusFile;
pub use tag_def::TagFile;
pub use template::{list_templates, TemplateFile, TemplateFrontmatter};
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

    pub fn campaigns_dir(&self) -> PathBuf {
        self.dot_guild().join("campaigns")
    }

    pub fn campaign_path(&self, slug: &str) -> PathBuf {
        self.campaigns_dir().join(format!("{slug}.md"))
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

    /// DEV-016: 길드 규칙 — **legacy 단일 파일** (`.guild/rules.md`).
    /// DEV-016 후속 (multi-file) 부터는 `.guild/rules/{slug}.md` 가 권장. 본
    /// 단일 파일은 backward compat — 첫 list_rules 호출 시 `.guild/rules/general.md`
    /// 로 자동 마이그레이션됨.
    pub fn rules_path(&self) -> PathBuf {
        self.dot_guild().join("rules.md")
    }

    /// DEV-016 multi-file: 규칙 디렉토리 (`.guild/rules/`).
    pub fn rules_dir(&self) -> PathBuf {
        self.dot_guild().join("rules")
    }

    /// DEV-016 multi-file: 한 규칙 파일 (`.guild/rules/{slug}.md`).
    pub fn rule_path(&self, slug: &str) -> PathBuf {
        self.rules_dir().join(format!("{slug}.md"))
    }

    /// DEV-068: tag 정의 디렉토리 (`.guild/tags/`). git tracked.
    /// quest_tags 의 사용 tag 와 별개 — 사용자가 color / description 정의.
    pub fn tags_dir(&self) -> PathBuf {
        self.dot_guild().join("tags")
    }

    /// DEV-060: quest 템플릿 디렉토리 (`.guild/templates/`). git tracked.
    /// `{name}.md` — quest 파일과 동일한 `+++` TOML frontmatter (필드 모두
    /// 선택) + 기본 본문.
    pub fn templates_dir(&self) -> PathBuf {
        self.dot_guild().join("templates")
    }

    /// DEV-060: 한 템플릿 파일 (`.guild/templates/{name}.md`).
    pub fn template_path(&self, name: &str) -> PathBuf {
        self.templates_dir().join(format!("{name}.md"))
    }

    /// DEV-068: 한 tag 정의 파일 (`.guild/tags/{slug}.toml`).
    pub fn tag_path(&self, slug: &str) -> PathBuf {
        self.tags_dir().join(format!("{slug}.toml"))
    }

    /// DEV-012: Quest 별 공개 댓글 (`.guild/quests/{slug}.comments.md`).
    /// frontmatter 없는 plain markdown. git tracked.
    pub fn comments_path(&self, slug: &str) -> PathBuf {
        self.quests_dir().join(format!("{slug}.comments.md"))
    }

    /// DEV-100: Campaign 별 공개 댓글 (`.guild/campaigns/{slug}.comments.md`).
    pub fn campaign_comments_path(&self, slug: &str) -> PathBuf {
        self.campaigns_dir().join(format!("{slug}.comments.md"))
    }

    /// DEV-100: Campaign 별 비공개 메모 (`.guild/campaigns/{slug}.memo.md`).
    /// **gitignored**.
    pub fn campaign_memo_path(&self, slug: &str) -> PathBuf {
        self.campaigns_dir().join(format!("{slug}.memo.md"))
    }

    /// DEV-012: Quest 별 비공개 메모 (`.guild/quests/{slug}.memo.md`).
    /// frontmatter 없는 plain markdown. **gitignored** (개인 노트).
    pub fn memo_path(&self, slug: &str) -> PathBuf {
        self.quests_dir().join(format!("{slug}.memo.md"))
    }

    /// `.guild/.gitignore` 의 표준 내용.
    /// DEV-012: `quests/*.memo.md` 추가 — 비공개 메모 (개인 노트, 팀 공유 X).
    pub fn gitignore_content() -> &'static str {
        "# openguild — 내부 캐시 / UI 상태 / 백업 (git 추적 X)\n\
         index.db\n\
         positions.json\n\
         backups/\n\
         .lock\n\
         # DEV-012: 비공개 메모 (개인 노트)\n\
         quests/*.memo.md\n\
         # DEV-100: 캠페인 비공개 메모\n\
         campaigns/*.memo.md\n"
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
