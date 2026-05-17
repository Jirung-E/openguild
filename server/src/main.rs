//! OpenGuild HTTP API 서버 + 관리 CLI.
//!
//! 서브커맨드:
//!   host             HTTP 서버 시작
//!   info             길드 메타 / DB 경로 / 스냅샷 현황
//!   snapshot         `.guild/backups/snapshots/{ts}.db` 신설 + journal 절단
//!   restore [--to]   snapshot 으로 index.db 복원
//!   reindex          파일 → index.db 캐시 재구축
//!   migrate-to-files legacy guild.db → .guild/quests/*.md
//!   check-counters   type counter 무결성 검증
//!   backup           snapshot 의 alias (호환성)
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
    /// 수동 snapshot 1회 (snapshot 의 alias, 호환성 유지)
    Backup,
    /// 길드 메타 / DB 경로 / 백업 현황
    Info,
    /// legacy guild.db → .guild/quests/*.md 파일 진리원 구조로 일회성 이전
    MigrateToFiles,
    /// .guild/quests/*.md 파일들로부터 index.db 캐시 재구축 (외부 편집 / 손상 후 복구)
    Reindex,
    /// `.guild/backups/snapshots/{ts}.db` 신설 + journal 절단 (RDB)
    Snapshot,
    /// type 의 last_number 가 실제 max quest 번호와 일치하는지 확인 + 자동 보정
    CheckCounters {
        /// 발견된 불일치를 type 파일에 직접 기록 (기본: 보고만)
        #[arg(long)]
        fix: bool,
    },
    /// 외부 편집 / 손상으로 index.db 가 파일과 어긋났는지 검사 + 자동 resync
    CheckDrift {
        /// 발견된 drift 를 자동으로 reindex 로 해소 (기본: 보고만)
        #[arg(long)]
        resync: bool,
    },
    /// snapshot 으로 index.db 복원
    Restore {
        /// 특정 snapshot 의 타임스탬프 (`YYYYMMDD-HHMMSS`). 미지정 시 최신 사용.
        #[arg(long)]
        to: Option<String>,
        /// 사용 가능한 snapshot 목록 출력만 (복원 X)
        #[arg(long)]
        list: bool,
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
        Command::Backup => rt.block_on(run_backup()),
        Command::Info => rt.block_on(run_info()),
        Command::MigrateToFiles => rt.block_on(run_migrate_to_files()),
        Command::Reindex => rt.block_on(run_reindex()),
        Command::Snapshot => rt.block_on(run_snapshot()),
        Command::Restore { to, list } => rt.block_on(run_restore(to, list)),
        Command::CheckCounters { fix } => run_check_counters(fix),
        Command::CheckDrift { resync } => rt.block_on(run_check_drift(resync)),
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


// ─────────────────────── host ───────────────────────

async fn run_host(port_arg: Option<u16>) -> Result<()> {
    let ctx = load_guild()?;
    // Store 는 .guild/index.db + journal.db 둘 다 자동 마이그레이션.
    let store = openguild_core::Store::open(&ctx.guild_path).await?;

    // 라우터. (audit middleware 폐기 — journal.db 의 ops 가 그 역할.
    //          auto-backup task 폐기 — snapshot/restore 명령으로 명시적 실행.)
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
    println!("✓ OpenGuild server listening");
    println!("  guild  : {}  (v{})", ctx.guild.name, ctx.guild.version);
    println!("  path   : {}", ctx.abs_path.display());
    println!("  bind   : http://{addr}");
    println!("  static : {}", if serves_static { dist_path.as_str() } else { "(none — API only)" });
    println!("  backup : on-demand (`openguild-server snapshot`)");
    println!();
    println!("Press Ctrl+C to stop.");
    println!();

    axum::serve(listener, app).await?;
    Ok(())
}

// ─────────────────────── backup ───────────────────────
//
// `backup` 은 새 모델의 `snapshot` 의 alias. 호환성 유지용.

async fn run_backup() -> Result<()> {
    run_snapshot().await
}

// ─────────────────────── info ───────────────────────
//
// `host` 가 시작 시 출력하는 정보 + DB / quest / 백업 통계를 모두 포함한
// **superset**. host 후 따로 확인할 필요 없도록.
//
// TODO: 출력 양 조절 옵션 — `--brief` (1줄 요약), `--detailed` (마이그레이션 이력 /
//       audit log 통계 / DB integrity_check 등 추가) 차후 검토.

// ─────────────────────── migrate-to-files ───────────────────────

async fn run_migrate_to_files() -> Result<()> {
    let ctx = load_guild()?;

    // .guild/ 가 시드되어 있는지 확인 — 없으면 안내.
    let dot_guild = ctx.abs_path.join(".guild");
    if !dot_guild.join("types").exists() {
        eprintln!();
        eprintln!("✗ .guild/ 구조가 시드되지 않았습니다.");
        eprintln!("  먼저 `openguild init` 을 한 번 더 실행하면 자동 업그레이드됩니다.");
        eprintln!();
        std::process::exit(2);
    }

    // 기존 .guild/quests/ 에 파일이 있는지 확인 — 있으면 안내 후 중단.
    let quests_dir = dot_guild.join("quests");
    let existing: Vec<_> = std::fs::read_dir(&quests_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    if !existing.is_empty() {
        eprintln!();
        eprintln!("✗ .guild/quests/ 에 이미 {} 개 quest 파일이 있습니다.", existing.len());
        eprintln!("  마이그레이션은 한 번만 실행해야 합니다.");
        eprintln!("  덮어쓰려면 quests/ 를 직접 비운 뒤 재시도하세요.");
        eprintln!();
        std::process::exit(2);
    }

    println!("▸ 마이그레이션 시작: {}", ctx.abs_path.display());
    let report = openguild_core::migrate::migrate_to_files(&ctx.abs_path).await?;

    println!();
    println!("✓ 마이그레이션 완료");
    println!("  legacy DB    : {}", report.legacy_db_path.display());
    println!("  quests 작성  : {}", report.quests_written);
    println!("  - alive      : {}", report.quests_written - report.deleted_quests_included);
    println!("  - soft-deleted: {}", report.deleted_quests_included);
    println!("  types 갱신   : {} (counter)", report.types_updated);
    println!(
        "  index.db     : {}",
        if report.index_db_copied { "복사됨" } else { "이미 존재 — 건드리지 않음" }
    );
    println!();
    println!("다음 단계:");
    println!("  1. .guild/quests/ 를 열어 quest 파일이 정상인지 확인");
    println!("  2. 만족하면 legacy guild.db 와 backups/ 삭제 (gitignored 라 commit 영향 없음)");
    println!();

    Ok(())
}

// ─────────────────────── drift check ───────────────────────

async fn run_check_drift(resync: bool) -> Result<()> {
    let ctx = load_guild()?;
    let store = openguild_core::Store::open(&ctx.guild_path).await?;
    let report = openguild_core::drift::detect_drift(&store).await?;

    if report.is_clean() {
        println!("✓ index.db 가 파일과 일치 (drift 없음)");
        return Ok(());
    }

    println!("⚠ drift 발견:");
    if !report.missing_in_index.is_empty() {
        println!();
        println!("  파일은 있는데 index 에 없음 ({}):", report.missing_in_index.len());
        for s in &report.missing_in_index {
            println!("    - {s}");
        }
    }
    if !report.stale_in_index.is_empty() {
        println!();
        println!("  index 에 있는데 파일이 없음 ({}):", report.stale_in_index.len());
        for s in &report.stale_in_index {
            println!("    - {s}");
        }
    }
    if !report.fresh_files.is_empty() {
        println!();
        println!("  파일 mtime > index.db mtime ({}):", report.fresh_files.len());
        for s in &report.fresh_files {
            println!("    - {s}");
        }
    }

    if resync {
        println!();
        println!("▸ reindex 실행 중...");
        let _ = openguild_core::reindex::reindex(&store).await?;
        println!("✓ resync 완료");
    } else {
        println!();
        println!("(--resync 로 자동 reindex 가능)");
    }
    Ok(())
}

// ─────────────────────── counter check ───────────────────────

fn run_check_counters(fix: bool) -> Result<()> {
    let ctx = load_guild()?;
    let paths = openguild_core::repo::GuildPaths::new(&ctx.abs_path);
    let report = openguild_core::counter::check_counters(&paths, fix)?;

    println!("✓ counter 검증 완료");
    println!("  검사된 type 수 : {}", report.types_checked);
    println!("  발견 이슈     : {}", report.issues.len());
    for issue in &report.issues {
        println!();
        println!("  • type {}:", issue.prefix);
        println!("    저장된 last_number   : {}", issue.stored_last_number);
        println!("    실제 max quest 번호  : {}", issue.actual_max_number);
        if fix {
            println!("    → {} 으로 보정됨", issue.corrected_to);
        } else {
            println!("    (--fix 로 자동 보정 가능)");
        }
    }
    Ok(())
}

// ─────────────────────── snapshot / restore ───────────────────────

async fn run_snapshot() -> Result<()> {
    let ctx = load_guild()?;
    let store = openguild_core::Store::open(&ctx.guild_path).await?;
    let info = openguild_core::snapshot::create_snapshot(&store).await?;
    println!("✓ snapshot 생성");
    println!("  timestamp : {}", info.timestamp);
    println!("  path      : {}", info.path.display());
    println!("  size      : {} bytes", info.size_bytes);
    println!();
    println!("journal.db 의 ops 가 절단되었습니다 (RDB 패턴).");
    Ok(())
}

async fn run_restore(to: Option<String>, list_only: bool) -> Result<()> {
    let ctx = load_guild()?;
    let store = openguild_core::Store::open(&ctx.guild_path).await?;
    let snapshots = openguild_core::snapshot::list_snapshots(&store.paths)?;

    if list_only || snapshots.is_empty() {
        if snapshots.is_empty() {
            println!("(사용 가능한 snapshot 없음)");
            println!();
            println!("`openguild-server snapshot` 으로 생성하세요.");
            return Ok(());
        }
        println!("사용 가능한 snapshots (오래된 순):");
        for s in &snapshots {
            println!("  {} — {} bytes", s.timestamp, s.size_bytes);
        }
        if list_only {
            return Ok(());
        }
    }

    let target = if let Some(ts) = to {
        snapshots
            .iter()
            .find(|s| s.timestamp == ts)
            .with_context(|| format!("snapshot 없음: {ts}"))?
            .clone()
    } else {
        snapshots.last().cloned().context("snapshot 이 없습니다")?
    };

    println!("▸ snapshot 으로 복원: {}", target.timestamp);
    openguild_core::snapshot::restore_snapshot(&store, &target).await?;
    println!("✓ index.db 복원 완료");
    println!();
    println!("주의: 파일 시스템 (`.guild/quests/*.md`) 은 자동 갱신 안 됨.");
    println!("      필요시 `openguild-server reindex` 또는 export (추후 명령) 사용.");
    Ok(())
}

// ─────────────────────── reindex ───────────────────────

async fn run_reindex() -> Result<()> {
    let ctx = load_guild()?;
    let store = openguild_core::Store::open(&ctx.guild_path).await?;
    let report = openguild_core::reindex::reindex(&store).await?;

    println!("✓ index.db 재구축 완료");
    println!("  types        : {}", report.types_loaded);
    println!("  statuses     : {}", report.statuses_loaded);
    println!("  quests       : {}", report.quests_loaded);
    println!("  dependencies : {}", report.dependencies_loaded);
    println!("  positions    : {} 복원 (board UI 상태)", report.positions_restored);
    if !report.skipped.is_empty() {
        println!();
        println!("⚠ {} 개 파일 skip 됨 (파싱 / 무결성 실패):", report.skipped.len());
        for (path, reason) in &report.skipped {
            println!("  - {path}");
            println!("    → {reason}");
        }
    }
    Ok(())
}

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

    let dot_guild_paths = openguild_core::repo::GuildPaths::new(&ctx.abs_path);
    let db_file = dot_guild_paths.index_db();
    let db_size = std::fs::metadata(&db_file).map(|m| m.len()).unwrap_or(0);

    // DB 통계: quest 수 + 마지막 마이그레이션 버전
    let (quests_alive, quests_deleted, schema_version): (i64, i64, Option<String>) =
        if db_file.exists() {
            let store = openguild_core::Store::open(&ctx.guild_path).await?;
            let pool = &store.index_pool;
            let alive: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM quests WHERE deleted_at IS NULL")
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0);
            let dead: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM quests WHERE deleted_at IS NOT NULL")
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0);
            let last_mig: Option<(i64, String)> = sqlx::query_as(
                "SELECT version, description FROM _sqlx_migrations \
                 WHERE success = 1 ORDER BY version DESC LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            let mig_str = last_mig.map(|(v, d)| format!("{v:04} {d}"));
            (alive, dead, mig_str)
        } else {
            (0, 0, None)
        };

    // 새 snapshots 디렉토리
    let dot_guild_paths = openguild_core::repo::GuildPaths::new(&ctx.abs_path);
    let snapshots = openguild_core::snapshot::list_snapshots(&dot_guild_paths)
        .unwrap_or_default();
    let snapshot_count = snapshots.len();
    let snapshot_total_size: u64 = snapshots.iter().map(|s| s.size_bytes).sum();
    let latest_snapshot = snapshots.last().map(|s| s.timestamp.clone());

    // journal.db ops 수
    let journal_count: i64 = if dot_guild_paths.journal_db().exists() {
        let url = format!(
            "sqlite:{}?mode=ro",
            dot_guild_paths
                .journal_db()
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('\\', "/")
        );
        if let Ok(pool) = openguild_core::db::create_pool(&url).await {
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ops")
                .fetch_one(&pool)
                .await
                .unwrap_or(0);
            pool.close().await;
            n
        } else {
            0
        }
    } else {
        0
    };

    // ── 출력 ──
    println!("guild   : {}  (v{}, created {})", ctx.guild.name, ctx.guild.version, ctx.guild.created_at);
    println!("path    : {}", ctx.abs_path.display());
    println!();
    println!("server  : {bind}");
    println!("static  : {static_state}");
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
        "snapshots: {} file(s), {} bytes total (latest: {})",
        snapshot_count,
        snapshot_total_size,
        latest_snapshot.as_deref().unwrap_or("(none)")
    );
    println!("journal : {journal_count} ops since last snapshot");

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
    fn run_backup_creates_snapshot_file() {
        let dir = make_guild_dir("backup");
        // backup 는 snapshot 의 alias — `.guild/backups/snapshots/{ts}.db` 생성.
        // .guild/ 시드 필요.
        openguild_core::repo::seed_guild_dir(&dir).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        with_guild_env(&dir, || {
            rt.block_on(run_backup()).expect("backup ok");
        });
        let snapshot_dir = dir.join(".guild/backups/snapshots");
        let count = std::fs::read_dir(&snapshot_dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .map(|e| e.path().extension().and_then(|s| s.to_str()) == Some("db"))
                    .unwrap_or(false)
            })
            .count();
        assert!(count >= 1, "snapshot file should be created");
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
