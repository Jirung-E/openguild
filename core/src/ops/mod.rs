//! Orchestration 레이어 — SQL mutation + 파일 IO + journal AOF 를 묶음.
//!
//! 호출자 (server routes, cli Backend::Local) 는 본 모듈의 함수를 호출.
//! 이 모듈은 검증 / SQL / 파일 / journal 을 일관된 순서로 실행:
//!
//! ```text
//! 1. journal INSERT (의도 기록 — durable)
//! 2. SQL mutation (services::quests::*) — index.db 반영
//! 3. .guild/quests/{slug}.md atomic write
//! 4. 영향받는 다른 quest 파일들의 auto 블록 재생성
//! ```
//!
//! crash 시: journal 에 기록된 op 는 다음 시작에서 replay 또는 reindex 시 정합 복구.

pub mod campaign_comments;
pub mod campaigns;
pub mod comments;
pub mod counter;
pub mod meta;
pub mod quests;
pub mod rules;

pub use counter::check_and_fix_counters;
pub use meta::{
    count_quests_by_status, count_quests_by_type, create_status, create_type, delete_status,
    delete_tag_def, delete_type, rename_status_slug, rename_type, update_status, update_type,
    upsert_tag_def,
};
pub use quests::{
    add_prerequisite, change_parent, change_quest_type, change_status, create_quest,
    delete_quest, remove_prerequisite, restore_quest, set_due_dates, set_quest_tags,
    update_quest,
};
