pub mod admin;
pub mod campaigns;
pub mod comments;
pub mod meta;
pub mod quests;
pub mod rules;

use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};
use openguild_core::Store;

pub fn create_router(store: Store) -> Router {
    Router::new()
        .route("/health", get(health))
        // meta
        .route("/api/quest-types", get(meta::list_quest_types))
        .route("/api/quest-statuses", get(meta::list_quest_statuses))
        // DEV-016: 길드 규칙 (`.guild/rules.md`).
        .route("/api/rules", get(rules::get_rules).put(rules::set_rules))
        // DEV-012: Quest 별 댓글 (공개, tracked) / 메모 (비공개, gitignored).
        .route(
            "/api/quests/by/{slug}/comments",
            get(comments::get_comments).put(comments::set_comments),
        )
        .route(
            "/api/quests/by/{slug}/memo",
            get(comments::get_memo).put(comments::set_memo),
        )
        // quests
        .route("/api/quests", get(quests::list_quests).post(quests::create_quest))
        .route(
            "/api/quests/{id}",
            get(quests::get_quest)
                .patch(quests::update_quest)
                .delete(quests::delete_quest),
        )
        .route("/api/quests/{id}/status", patch(quests::change_status))
        .route("/api/quests/{id}/parent", patch(quests::change_parent))
        // DEV-076: 희망 / 필수 기한 설정 / 해제.
        .route("/api/quests/{id}/due", patch(quests::set_due_dates))
        .route("/api/quests/{id}/restore", patch(quests::restore_quest))
        .route("/api/quests/{id}/candidates", get(quests::list_candidates))
        .route("/api/quests/{id}/prerequisites", post(quests::add_prerequisite))
        .route(
            "/api/quests/{id}/prerequisites/{prereq_id}",
            delete(quests::remove_prerequisite),
        )
        .route("/api/quests/{id}/position", put(quests::update_position))
        .route("/api/quests/{id}/history", get(quests::list_history))
        .route("/api/quests/by/{slug}", get(quests::get_quest_by_slug))
        // DEV-011: quest 가 속한 campaigns 목록 — Quest Detail UI 의 Campaigns 섹션.
        .route("/api/quests/{id}/campaigns", get(campaigns::list_for_quest))
        .route("/api/quest-positions", get(quests::list_positions))
        .route("/api/quest-dependencies", get(quests::list_dependencies))
        .route("/api/deleted-quests", get(quests::list_deleted_quests))
        // campaigns (DEV-011)
        .route(
            "/api/campaigns",
            get(campaigns::list_campaigns).post(campaigns::create_campaign),
        )
        .route("/api/campaigns/summaries/active", get(campaigns::list_active_summaries))
        .route(
            "/api/campaigns/summaries/upcoming",
            get(campaigns::list_upcoming_summaries),
        )
        .route(
            "/api/campaigns/{slug}",
            get(campaigns::get_campaign)
                .patch(campaigns::update_campaign)
                .delete(campaigns::delete_campaign),
        )
        .route("/api/campaigns/{slug}/quests", post(campaigns::link_quest))
        .route(
            "/api/campaigns/{slug}/quests/{quest_slug}",
            delete(campaigns::unlink_quest),
        )
        .route("/api/campaigns/{slug}/checklist", post(campaigns::add_checklist))
        .route(
            "/api/campaigns/{slug}/checklist/{index}",
            patch(campaigns::set_checklist).delete(campaigns::remove_checklist),
        )
        // admin
        .route("/api/admin/snapshot", post(admin::create_snapshot))
        .route("/api/admin/snapshots", get(admin::list_snapshots))
        .route("/api/admin/restore", post(admin::restore))
        .route("/api/admin/drift", get(admin::check_drift))
        .route("/api/admin/reindex", post(admin::run_reindex))
        .with_state(store)
}

async fn health() -> &'static str {
    "ok"
}
