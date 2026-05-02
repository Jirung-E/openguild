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
        .route("/api/quests/{id}/prerequisites", post(quests::add_prerequisite))
        .route(
            "/api/quests/{id}/prerequisites/{prereq_id}",
            delete(quests::remove_prerequisite),
        )
        .route("/api/quests/{id}/position", put(quests::update_position))
        .with_state(pool)
}

async fn health() -> &'static str {
    "ok"
}
