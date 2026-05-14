mod audit;
mod error;
mod routes;

#[cfg(test)]
mod tests;

use openguild_core::{backup, db, guild_file};

use anyhow::Result;
use std::net::SocketAddr;
use std::path::Path;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=debug,tower_http=debug".into()),
        )
        .init();

    // guild 파일 로드 — 없으면 init 안 된 디렉토리이므로 친절한 안내 후 종료.
    let guild_path = std::env::var("GUILD_PATH").unwrap_or_else(|_| ".".to_string());
    let guild = match guild_file::load(&guild_path) {
        Ok(g) => g,
        Err(err) => {
            // `.guild` 파일 부재 vs 그 외 에러를 메시지로 구분.
            let msg = format!("{err:#}");
            let is_missing = msg.contains("no .guild file found");
            if is_missing {
                let abs = std::fs::canonicalize(&guild_path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| guild_path.clone());
                eprintln!();
                eprintln!("✗ 이 디렉토리는 OpenGuild 길드로 초기화되지 않았습니다.");
                eprintln!("  경로: {abs}");
                eprintln!();
                eprintln!("먼저 길드를 초기화하세요:");
                eprintln!("  openguild init                # 현재 디렉토리에 길드 생성");
                eprintln!("  openguild init --name <이름>  # 이름 지정");
                eprintln!();
                eprintln!("또는 GUILD_PATH 환경변수로 기존 길드 경로를 지정:");
                eprintln!("  GUILD_PATH=./my-guild cargo run --bin openguild-server");
                eprintln!();
                std::process::exit(2);
            }
            return Err(err);
        }
    };
    tracing::info!("opening guild: {}", guild.name);

    // DB 연결 및 마이그레이션
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| format!("sqlite:{guild_path}/guild.db"));
    let pool = db::create_pool(&db_url).await?;
    db::run_migrations(&pool).await?;
    tracing::info!("database ready: {db_url}");

    // 자동 백업 백그라운드 task — startup 시 1회 + 1시간마다 + 7일 보관
    backup::spawn_backup_task(pool.clone(), guild_path.clone());

    // 라우터 + audit middleware (mutation HTTP 요청 기록)
    let audit_state = audit::AuditState::new(&guild_path);
    let mut app = routes::create_router(pool).layer(axum::middleware::from_fn_with_state(
        audit_state,
        audit::audit_layer,
    ));

    // frontend 정적 서빙 (선택) — env FRONTEND_DIST 또는 기본 gui/frontend/dist
    //   API 라우트와 충돌 X — fallback 으로 등록. dist 없으면 경고만, API 정상 동작.
    //   호스팅 배포에선 보통 정적 호스팅 (Vercel 등) 으로 분리. 이 옵션은 단일 binary 시연용.
    let dist_path = std::env::var("FRONTEND_DIST")
        .unwrap_or_else(|_| "gui/frontend/dist".to_string());
    if Path::new(&dist_path).is_dir() {
        tracing::info!("serving frontend static files from: {dist_path}");
        app = app.fallback_service(ServeDir::new(&dist_path));
    } else {
        tracing::warn!(
            "frontend dist not found at {dist_path} — API only mode (set FRONTEND_DIST or run `npm run build` in gui/frontend/)"
        );
    }

    let app = app
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
