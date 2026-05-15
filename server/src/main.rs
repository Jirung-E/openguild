//! OpenGuild HTTP API 서버 + 관리 CLI.
//!
//! 서브커맨드:
//!   host        HTTP 서버 시작 (기본 명령 — 인자 없이 호출 시 도움말 안내)
//!   backup      수동 스냅샷 1회 (VACUUM INTO)
//!   info        길드 메타 / DB 경로 / 백업 현황
//!
//! 환경변수:
//!   GUILD_PATH  대상 길드 디렉토리 (기본: `.`)
//!   PORT        host 바인드 포트 (기본: 3000)
//!   DATABASE_URL  override (기본: sqlite:{guild_path}/guild.db)
//!   FRONTEND_DIST  정적 자산 폴더 (기본: gui/frontend/dist)

mod audit;
mod error;
mod routes;

#[cfg(test)]
mod tests;

use openguild_core::{backup, db, guild_file};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

#[derive(Parser, Debug)]
#[command(
    name = "openguild-server",
    version,
    about = "OpenGuild HTTP API server + 관리 CLI"
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
    /// 수동 백업 1회 (VACUUM INTO → `<guild>/backups/guild.db.<ts>`)
    Backup,
    /// 길드 메타 / DB 경로 / 백업 현황
    Info,
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
        Command::Backup => rt.block_on(run_backup()),
        Command::Info => rt.block_on(run_info()),
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
                eprintln!("✗ not an OpenGuild project: no `.guild` file in {abs}");
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

async fn open_pool(ctx: &GuildCtx) -> Result<sqlx::SqlitePool> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| format!("sqlite:{}/guild.db", ctx.guild_path));
    let pool = db::create_pool(&db_url).await?;
    db::run_migrations(&pool).await?;
    Ok(pool)
}

// ─────────────────────── host ───────────────────────

async fn run_host(port_arg: Option<u16>) -> Result<()> {
    let ctx = load_guild()?;
    let pool = open_pool(&ctx).await?;

    // 자동 백업 백그라운드 task
    backup::spawn_backup_task(pool.clone(), ctx.guild_path.clone());

    // 라우터 + audit middleware
    let audit_state = audit::AuditState::new(&ctx.guild_path);
    let mut app = routes::create_router(pool).layer(axum::middleware::from_fn_with_state(
        audit_state,
        audit::audit_layer,
    ));

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
    println!("✓ OpenGuild server listening");
    println!("  guild  : {}  (v{})", ctx.guild.name, ctx.guild.version);
    println!("  path   : {}", ctx.abs_path.display());
    println!("  bind   : http://{addr}");
    println!("  static : {}", if serves_static { dist_path.as_str() } else { "(none — API only)" });
    println!("  backup : every 1h → {}/backups/", ctx.abs_path.display());
    println!();
    println!("Press Ctrl+C to stop.");
    println!();

    axum::serve(listener, app).await?;
    Ok(())
}

// ─────────────────────── backup ───────────────────────

async fn run_backup() -> Result<()> {
    let ctx = load_guild()?;
    let pool = open_pool(&ctx).await?;
    let target = backup::backup_once(&pool, &ctx.guild_path).await?;
    println!("✓ backup created: {}", target.display());
    Ok(())
}

// ─────────────────────── info ───────────────────────
//
// `host` 가 시작 시 출력하는 정보 + DB / quest / 백업 통계를 모두 포함한
// **superset**. host 후 따로 확인할 필요 없도록.
//
// TODO: 출력 양 조절 옵션 — `--brief` (1줄 요약), `--detailed` (마이그레이션 이력 /
//       audit log 통계 / DB integrity_check 등 추가) 차후 검토.

async fn run_info() -> Result<()> {
    let ctx = load_guild()?;

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let bind = format!("http://127.0.0.1:{port}");

    let dist_path = std::env::var("FRONTEND_DIST")
        .unwrap_or_else(|_| "gui/frontend/dist".to_string());
    let static_state = if Path::new(&dist_path).is_dir() {
        format!("{dist_path} (ok)")
    } else {
        format!("{dist_path} (missing — host 시 API only)")
    };

    let db_file = ctx.abs_path.join("guild.db");
    let db_size = std::fs::metadata(&db_file).map(|m| m.len()).unwrap_or(0);

    // DB 통계: quest 수 + 마지막 마이그레이션 버전
    let (quests_alive, quests_deleted, schema_version): (i64, i64, Option<String>) =
        if db_file.exists() {
            let pool = open_pool(&ctx).await?;
            let alive: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM quests WHERE deleted_at IS NULL")
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(0);
            let dead: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM quests WHERE deleted_at IS NOT NULL")
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(0);
            // sqlx migration history — `_sqlx_migrations` 의 가장 큰 version 행의 description.
            let last_mig: Option<(i64, String)> = sqlx::query_as(
                "SELECT version, description FROM _sqlx_migrations \
                 WHERE success = 1 ORDER BY version DESC LIMIT 1",
            )
            .fetch_optional(&pool)
            .await
            .unwrap_or(None);
            pool.close().await;
            let mig_str = last_mig.map(|(v, d)| format!("{v:04} {d}"));
            (alive, dead, mig_str)
        } else {
            (0, 0, None)
        };

    // 백업 디렉토리
    let backups_dir = ctx.abs_path.join("backups");
    let (backup_count, latest_backup, backup_total_size): (usize, Option<String>, u64) =
        match std::fs::read_dir(&backups_dir) {
            Ok(rd) => {
                let mut entries: Vec<_> = rd
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with("guild.db."))
                    .collect();
                entries.sort_by_key(|e| e.file_name());
                let total: u64 = entries
                    .iter()
                    .filter_map(|e| e.metadata().ok().map(|m| m.len()))
                    .sum();
                let latest = entries
                    .last()
                    .map(|e| e.file_name().to_string_lossy().to_string());
                (entries.len(), latest, total)
            }
            Err(_) => (0, None, 0),
        };

    // audit log
    let audit_file = ctx.abs_path.join("audit.log");
    let (audit_lines, audit_size) = if audit_file.exists() {
        let size = std::fs::metadata(&audit_file).map(|m| m.len()).unwrap_or(0);
        let lines = std::fs::read_to_string(&audit_file)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        (lines, size)
    } else {
        (0, 0)
    };

    // ── 출력 ──
    println!("guild   : {}  (v{}, created {})", ctx.guild.name, ctx.guild.version, ctx.guild.created_at);
    println!("path    : {}", ctx.abs_path.display());
    println!();
    println!("server  : {bind}");
    println!("static  : {static_state}");
    println!("backup  : every 1h, keep 7d");
    println!();
    println!("db      : {} ({} bytes)", db_file.display(), db_size);
    if let Some(mig) = schema_version {
        println!("schema  : {mig}");
    } else {
        println!("schema  : (db not initialized)");
    }
    println!("quests  : {quests_alive} alive, {quests_deleted} deleted");
    println!();
    println!(
        "backups : {} file(s), {} bytes total (latest: {})",
        backup_count,
        backup_total_size,
        latest_backup.as_deref().unwrap_or("(none)")
    );
    println!("audit   : {audit_lines} entries, {audit_size} bytes");

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
            _ => panic!("expected Host"),
        }
    }

    #[test]
    fn parse_host_with_port() {
        let cli = Cli::try_parse_from(["openguild-server", "host", "--port", "3300"]).unwrap();
        match cli.command {
            Command::Host { port } => assert_eq!(port, Some(3300)),
            _ => panic!("expected Host"),
        }
    }

    #[test]
    fn parse_backup() {
        let cli = Cli::try_parse_from(["openguild-server", "backup"]).unwrap();
        assert!(matches!(cli.command, Command::Backup));
    }

    #[test]
    fn parse_info() {
        let cli = Cli::try_parse_from(["openguild-server", "info"]).unwrap();
        assert!(matches!(cli.command, Command::Info));
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
        let err = Cli::try_parse_from(["openguild-server", "nope"]).unwrap_err();
        assert!(matches!(err.kind(), clap::error::ErrorKind::InvalidSubcommand));
    }
}

// ─────────────────────── 통합: run_info / run_backup ───────────────────────
//
// guild + DB 가 있는 tempdir 에서 helper 들을 실제로 호출. stdout 출력은
// 검증하지 않고, 에러 없이 끝나는지만 본다. (출력 포맷은 회귀 시 사람이 확인)

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
    /// 다른 테스트와 env 충돌 가능 — 같은 모듈 내에서만 직렬화 보장.
    fn with_guild_env<F: FnOnce() -> R, R>(path: &std::path::Path, f: F) -> R {
        use std::sync::Mutex;
        static GUARD: Mutex<()> = Mutex::new(());
        let _lock = GUARD.lock().unwrap();
        let prev = std::env::var("GUILD_PATH").ok();
        // SAFETY: 같은 모듈의 GUARD mutex 로 직렬화. process 전체 env 를 만지지만
        // 다른 테스트가 GUILD_PATH 를 동시에 만지지 않는 한 안전.
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
    fn run_backup_creates_file() {
        let dir = make_guild_dir("backup");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        with_guild_env(&dir, || {
            rt.block_on(run_backup()).expect("backup ok");
        });
        // backups/guild.db.* 가 1개 이상 생겼는지
        let backup_dir = dir.join("backups");
        let count = std::fs::read_dir(&backup_dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .map(|e| e.file_name().to_string_lossy().starts_with("guild.db."))
                    .unwrap_or(false)
            })
            .count();
        assert!(count >= 1, "backup file should be created");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_info_succeeds_on_fresh_guild() {
        let dir = make_guild_dir("info");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        with_guild_env(&dir, || {
            rt.block_on(run_info()).expect("info ok");
        });
        let _ = std::fs::remove_dir_all(&dir);
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
