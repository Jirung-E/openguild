pub mod meta;
pub mod quests;

use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};
use sqlx::SqlitePool;

pub fn create_router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/health", get(health))
        // meta
        .route("/api/quest-types", get(meta::list_quest_types))
        .route("/api/quest-statuses", get(meta::list_quest_statuses))
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
        .route("/api/quests/{id}/restore", patch(quests::restore_quest))
        .route("/api/quests/{id}/candidates", get(quests::list_candidates))
        .route("/api/quests/{id}/prerequisites", post(quests::add_prerequisite))
        .route(
            "/api/quests/{id}/prerequisites/{prereq_id}",
            delete(quests::remove_prerequisite),
        )
        .route("/api/quests/{id}/position", put(quests::update_position))
        .route("/api/quests/by/{slug}", get(quests::get_quest_by_slug))
        .route("/api/quest-positions", get(quests::list_positions))
        .route("/api/quest-dependencies", get(quests::list_dependencies))
        .route("/api/deleted-quests", get(quests::list_deleted_quests))
        .with_state(pool)
}

async fn health() -> &'static str {
    "ok"
}
