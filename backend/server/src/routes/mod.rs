use axum::{routing::get, Router};
use sqlx::SqlitePool;

pub fn create_router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/health", get(health))
        .with_state(pool)
}

async fn health() -> &'static str {
    "ok"
}
