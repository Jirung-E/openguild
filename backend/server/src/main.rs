mod db;
mod error;
mod guild_file;
mod models;
mod routes;

#[cfg(test)]
mod tests;

use anyhow::Result;
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=debug,tower_http=debug".into()),
        )
        .init();

    // guild 파일 로드
    let guild_path = std::env::var("GUILD_PATH").unwrap_or_else(|_| ".".to_string());
    let guild = guild_file::load(&guild_path)?;
    tracing::info!("opening guild: {}", guild.name);

    // DB 연결 및 마이그레이션
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| format!("sqlite:{guild_path}/guild.db"));
    let pool = db::create_pool(&db_url).await?;
    db::run_migrations(&pool).await?;
    tracing::info!("database ready: {db_url}");

    // 라우터
    let app = routes::create_router(pool)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    tracing::info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
