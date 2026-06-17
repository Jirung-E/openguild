//! openguild HTTP API 서버 (host 전용).
//!
//! 서브커맨드:
//!   host   HTTP API 서버 시작 (유일)
//!
//! DEV-163 결정: **server = host 전용**. 오프라인 정비(reindex / snapshot /
//! restore / check-drift / vacuum / journal-tail / check-counters /
//! migrate-to-files / info)와 데이터 조작은 전부 `openguild`(CLI)가 담당한다.
//! 실행 중 host 의 런타임 정비는 HTTP admin endpoint(`/api/admin/*`)로 한다
//! (별도 프로세스로 정비 명령을 돌리면 index.db 동시 writer + 인메모리 stale 위험).
//!
//! 환경변수:
//!   GUILD_PATH    대상 길드 디렉토리 (기본: `.`)
//!   PORT          host 바인드 포트 (기본: 3000)
//!   FRONTEND_DIST 정적 자산 폴더 (기본: gui/frontend/dist)

mod error;
mod routes;

#[cfg(test)]
mod tests;

use openguild_core::guild_file;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

#[derive(Parser, Debug)]
#[command(
    name = "openguild-server",
    version,
    about = "openguild HTTP API server (host 전용 — 정비/데이터 조작은 openguild CLI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// HTTP API 서버 시작
    Host {
        /// 바인드 포트 (env: PORT, 기본 3000)
        #[arg(long)]
        port: Option<u16>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openguild_server=info,tower_http=info".into()),
        )
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    match cli.command {
        Command::Host { port } => rt.block_on(run_host(port)),
    }
}

// ─────────────────────── 공통: guild 로드 ───────────────────────

struct GuildCtx {
    /// `.guild` 메타
    guild: guild_file::GuildFile,
    /// 길드 디렉토리 (GUILD_PATH 또는 ".")
    guild_path: String,
    /// 절대경로 (출력용)
    abs_path: PathBuf,
}

fn load_guild() -> Result<GuildCtx> {
    let guild_path = std::env::var("GUILD_PATH").unwrap_or_else(|_| ".".to_string());
    let guild = match guild_file::load(&guild_path) {
        Ok(g) => g,
        Err(err) => {
            let msg = format!("{err:#}");
            let is_missing = msg.contains("no .guild file found");
            if is_missing {
                let abs = std::fs::canonicalize(&guild_path)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| guild_path.clone());
                eprintln!();
                eprintln!("✗ not an openguild project: no `.guild` file in {abs}");
                eprintln!("  hint: set GUILD_PATH env var to point at a directory with a `.guild`.");
                eprintln!();
                std::process::exit(2);
            }
            return Err(err);
        }
    };
    let abs_path = std::fs::canonicalize(&guild_path)
        .unwrap_or_else(|_| PathBuf::from(&guild_path));
    Ok(GuildCtx {
        guild,
        guild_path,
        abs_path,
    })
}

// ─────────────────────── host ───────────────────────

async fn run_host(port_arg: Option<u16>) -> Result<()> {
    let ctx = load_guild()?;
    // Store 는 .guild/index.db + journal.db 둘 다 자동 마이그레이션.
    let store = openguild_core::Store::open(&ctx.guild_path).await?;

    // 라우터. (audit middleware 폐기 — journal.db 의 ops 가 그 역할.
    //          auto-backup task 폐기 — HTTP admin / CLI 로 명시적 실행.)
    let mut app = routes::create_router(store);

    // frontend 정적 서빙 (선택)
    let dist_path = std::env::var("FRONTEND_DIST")
        .unwrap_or_else(|_| "gui/frontend/dist".to_string());
    let serves_static = Path::new(&dist_path).is_dir();
    if serves_static {
        app = app.fallback_service(ServeDir::new(&dist_path));
    }

    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let port: u16 = port_arg
        .or_else(|| std::env::var("PORT").ok().and_then(|p| p.parse().ok()))
        .unwrap_or(3000);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    // 떴다는 알림 — tracing 이 아닌 stdout 으로 명시적 안내 (사용자가 바로 확인할 수 있게)
    println!();
    println!("✓ openguild server listening");
    println!("  guild  : {}  (v{})", ctx.guild.name, ctx.guild.version);
    println!("  path   : {}", ctx.abs_path.display());
    println!("  bind   : http://{addr}");
    println!("  static : {}", if serves_static { dist_path.as_str() } else { "(none — API only)" });
    // DEV-163: 정비는 CLI(`openguild ...`) 또는 HTTP admin(`/api/admin/*`).
    println!("  admin  : HTTP /api/admin/* (snapshot/reindex/drift/vacuum/journal)");
    println!("  maint  : offline 은 `openguild` CLI (backup/restore/reindex/…)");
    println!();
    println!("Press Ctrl+C to stop.");
    println!();

    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_host_default_port() {
        let cli = Cli::try_parse_from(["openguild-server", "host"]).unwrap();
        match cli.command {
            Command::Host { port } => assert_eq!(port, None),
        }
    }

    #[test]
    fn parse_host_with_port() {
        let cli = Cli::try_parse_from(["openguild-server", "host", "--port", "3300"]).unwrap();
        match cli.command {
            Command::Host { port } => assert_eq!(port, Some(3300)),
        }
    }

    #[test]
    fn no_args_shows_error() {
        // 인자 없이 호출 시 clap 이 도움말 안내 + 에러. main() 호출 전 차단됨.
        let err = Cli::try_parse_from(["openguild-server"]).unwrap_err();
        assert!(
            err.kind() == clap::error::ErrorKind::MissingSubcommand
                || err.kind() == clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand,
            "got kind: {:?}",
            err.kind()
        );
    }

    #[test]
    fn unknown_subcommand_errors() {
        // DEV-163: 옛 정비 서브커맨드(reindex 등)는 이제 server 에 없음 → 알 수 없는 명령.
        let err = Cli::try_parse_from(["openguild-server", "reindex"]).unwrap_err();
        assert!(matches!(err.kind(), clap::error::ErrorKind::InvalidSubcommand));
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    fn make_guild_dir(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-server-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("test.guild"),
            "name = \"T\"\nversion = \"1.0\"\ncreated_at = \"2026-05-15\"\n",
        )
        .unwrap();
        dir
    }

    /// `GUILD_PATH` env 를 잠시 바꿔 helper 실행. 동시 실행 안전성을 위해 mutex.
    fn with_guild_env<F: FnOnce() -> R, R>(path: &std::path::Path, f: F) -> R {
        use std::sync::Mutex;
        static GUARD: Mutex<()> = Mutex::new(());
        let _lock = GUARD.lock().unwrap();
        let prev = std::env::var("GUILD_PATH").ok();
        // SAFETY: 같은 모듈의 GUARD mutex 로 직렬화.
        unsafe {
            std::env::set_var("GUILD_PATH", path);
        }
        let result = f();
        unsafe {
            match prev {
                Some(v) => std::env::set_var("GUILD_PATH", v),
                None => std::env::remove_var("GUILD_PATH"),
            }
        }
        result
    }

    #[test]
    fn load_guild_reads_meta() {
        let dir = make_guild_dir("load");
        let ctx = with_guild_env(&dir, load_guild).unwrap();
        assert_eq!(ctx.guild.name, "T");
        assert_eq!(ctx.guild.version, "1.0");
        assert_eq!(ctx.guild.created_at, "2026-05-15");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
