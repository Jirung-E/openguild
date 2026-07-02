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
//!   BIND          host 바인드 주소 (기본: local). `local`/`public` 별칭 또는
//!                 IP 리터럴 — `--bind` 참조.
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
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

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
        // DEV-195 후속(admin 피드백): `host --host` 처럼 서브커맨드와 이름이
        // 겹치면 어색하다는 지적으로 `--bind` 로 명명.
        // BUG-016: doc 에 quest_id leak 금지 — 아래 /// 는 기능 설명만.
        /// 바인드 주소. `local`(=127.0.0.1, 기본 — 의도치 않은 네트워크 노출
        /// 방지) / `public`(=0.0.0.0, 다른 기기 접근 허용) 별칭 또는 IP
        /// 리터럴(예: `192.168.1.10`). env: BIND.
        #[arg(long)]
        bind: Option<String>,
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
        Command::Host { port, bind } => rt.block_on(run_host(port, bind)),
    }
}

/// `--bind`/`BIND` 값 → 실제 바인드 IP. `local`/`public` 별칭 + IP 리터럴.
/// 인식 못 하는 값은 에러(오타로 의도치 않은 노출/바인드 실패 방지).
fn resolve_bind_ip(raw: &str) -> Result<std::net::IpAddr> {
    match raw.trim() {
        "local" => Ok(std::net::IpAddr::from([127, 0, 0, 1])),
        "public" => Ok(std::net::IpAddr::from([0, 0, 0, 0])),
        other => other
            .parse()
            .with_context(|| format!("--bind 값을 IP 로 해석할 수 없음: '{other}' (local / public / IP 리터럴)")),
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

async fn run_host(port_arg: Option<u16>, bind_arg: Option<String>) -> Result<()> {
    let ctx = load_guild()?;
    // Store 는 .guild/index.db + journal.db 둘 다 자동 마이그레이션.
    let store = openguild_core::Store::open(&ctx.guild_path).await?;

    // 라우터. (audit middleware 폐기 — journal.db 의 ops 가 그 역할.
    //          auto-backup task 폐기 — HTTP admin / CLI 로 명시적 실행.)
    let mut app = routes::create_router(store);

    // DEV-195: frontend 정적 서빙 (선택) — 단일 origin 으로 SPA+API 같이 서빙.
    // 기본값은 SvelteKit(adapter-static) 의 실제 빌드 출력 `gui/frontend/build`
    // (이전 기본값 `dist` 는 실제로 생성되지 않아 항상 API-only 로 떨어지던 버그).
    //
    // DEV-195 후속: 기본값이 cwd 기준 상대경로라 repo root 에서 실행할 때만
    // 우연히 맞았다. 사용자가 자연스럽게 실행할 위치인 `target/release/`
    // 에서 실행하면 cwd 가 달라 항상 API-only 로 떨어져 `/` 접속이 404 —
    // exe 의 조상 디렉토리들(`target/release` → `target` → repo root)도
    // 차례로 시도해 cargo build 산출물 레이아웃을 그대로 따라간다.
    let dist_path = std::env::var("FRONTEND_DIST").unwrap_or_else(|_| {
        const REL: &str = "gui/frontend/build";
        if Path::new(REL).is_dir() {
            return REL.to_string();
        }
        if let Ok(exe) = std::env::current_exe() {
            for dir in exe.ancestors() {
                let candidate = dir.join(REL);
                if candidate.is_dir() {
                    return candidate.to_string_lossy().into_owned();
                }
            }
        }
        REL.to_string()
    });
    let serves_static = Path::new(&dist_path).is_dir();
    if serves_static {
        // SPA fallback — 클라이언트 라우트 딥링크(예 `/quests/DEV-001` 직접 접근/
        // 새로고침)는 실제 파일이 없어 404 가 나므로, 매칭 안 되는 경로는 모두
        // index.html 로 떨어뜨려 SPA 라우터가 처리하게 한다. `/api/*`, `/health`
        // 는 이미 위에서 라우트가 매칭되므로 이 fallback 까지 오지 않는다.
        //
        // `.not_found_service()` 대신 `.fallback()` 사용 — not_found_service 는
        // 상태코드를 항상 404 로 고정해버려(tower_http 문서/소스 SetStatus) 본문은
        // index.html 인데 응답이 404 로 보임. `.fallback()` 은 상태코드를 건드리지
        // 않아 파일이 존재하는 index.html 이 정상 200 으로 응답된다.
        let index_html = Path::new(&dist_path).join("index.html");
        let serve_dir = ServeDir::new(&dist_path).fallback(ServeFile::new(index_html));
        app = app.fallback_service(serve_dir);
    }

    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let port: u16 = port_arg
        .or_else(|| std::env::var("PORT").ok().and_then(|p| p.parse().ok()))
        .unwrap_or(3000);
    let bind_raw = bind_arg
        .or_else(|| std::env::var("BIND").ok())
        .unwrap_or_else(|| "local".to_string());
    let bind_ip = resolve_bind_ip(&bind_raw)?;
    let addr = SocketAddr::from((bind_ip, port));

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
    // DEV-195: 인증 계층이 없으므로 loopback 밖으로 노출하면 신뢰된 네트워크/
    // 리버스 프록시 뒤에서만 사용 권장 — 시작 시 한 줄로 경고.
    if !bind_ip.is_loopback() {
        println!(
            "  ⚠ warning: bound to {bind_ip} (네트워크 노출) — 인증 없음, 신뢰된 네트워크에서만 사용하세요."
        );
    }
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
            Command::Host { port, bind } => {
                assert_eq!(port, None);
                assert_eq!(bind, None);
            }
        }
    }

    #[test]
    fn parse_host_with_port() {
        let cli = Cli::try_parse_from(["openguild-server", "host", "--port", "3300"]).unwrap();
        match cli.command {
            Command::Host { port, .. } => assert_eq!(port, Some(3300)),
        }
    }

    #[test]
    fn parse_host_with_bind() {
        let cli =
            Cli::try_parse_from(["openguild-server", "host", "--bind", "public"]).unwrap();
        match cli.command {
            Command::Host { bind, .. } => assert_eq!(bind, Some("public".to_string())),
        }
    }

    #[test]
    fn resolve_bind_ip_aliases() {
        assert_eq!(resolve_bind_ip("local").unwrap(), std::net::IpAddr::from([127, 0, 0, 1]));
        assert_eq!(resolve_bind_ip("public").unwrap(), std::net::IpAddr::from([0, 0, 0, 0]));
        assert_eq!(
            resolve_bind_ip("192.168.1.10").unwrap(),
            std::net::IpAddr::from([192, 168, 1, 10])
        );
        assert!(resolve_bind_ip("not-an-ip").is_err());
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

    /// 사용자 보고(2026-06-29): `openguild-server host --help` 가
    /// `--bind` 옵션 설명에 적힌 "DEV-195"를 그대로 노출(`bind` 필드의
    /// `///` doc comment 에 quest id 가 들어가 있었음). `cli/src/main.rs`
    /// 의 `help_output_has_no_quest_id_leaks`(BUG-016) 와 동일한 회귀
    /// 가드가 **server 의 Cli 에는 없었다** — CLI 쪽 테스트는 CLI 의
    /// `Cli::command()` 만 스캔하므로 server 의 누출을 못 잡았다(사용자
    /// 보고: "테스트는 통과했다"가 이 이유). 별도 binary 라 별도 가드 필요.
    #[test]
    fn help_output_has_no_quest_id_leaks() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let mut violations: Vec<String> = Vec::new();
        check_help_recursive(&cmd, cmd.get_name(), &mut violations);
        assert!(
            violations.is_empty(),
            "quest id 가 help 출력에 leak:\n{}",
            violations.join("\n")
        );
    }

    fn check_help_recursive(cmd: &clap::Command, path: &str, violations: &mut Vec<String>) {
        let mut owned = cmd.clone();
        let help = owned.render_long_help().to_string();
        if let Some(found) = find_quest_id(&help) {
            violations.push(format!("[{path}] '{found}' in help"));
        }
        for sub in cmd.get_subcommands() {
            let sub_path = format!("{path} {}", sub.get_name());
            check_help_recursive(sub, &sub_path, violations);
        }
    }

    /// 처음 발견된 `<PREFIX>-<숫자>` substring (PREFIX 는 ASCII 대문자 2~5).
    /// `cli/src/main.rs` 의 동명 헬퍼와 동일 — 별도 crate 의 테스트라 공유 X.
    fn find_quest_id(s: &str) -> Option<&str> {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let Some(dash_rel) = s[i..].find('-') else { break };
            let dash = i + dash_rel;
            let prefix_end = dash;
            let mut prefix_start = prefix_end;
            while prefix_start > 0
                && bytes[prefix_start - 1].is_ascii_uppercase()
                && prefix_end - prefix_start < 5
            {
                prefix_start -= 1;
            }
            let prefix_len = prefix_end - prefix_start;
            let after = dash + 1;
            let mut digits = after;
            while digits < bytes.len() && bytes[digits].is_ascii_digit() {
                digits += 1;
            }
            if prefix_len >= 2 && digits > after {
                return Some(&s[prefix_start..digits]);
            }
            i = dash + 1;
        }
        None
    }

    #[test]
    fn find_quest_id_detects_patterns() {
        assert!(find_quest_id("foo (DEV-001) bar").is_some());
        assert!(find_quest_id("BUG-44 trailing").is_some());
        assert!(find_quest_id("REQ-7").is_some());
        assert!(find_quest_id("no quest id here").is_none());
        assert!(find_quest_id("D-1").is_none());
        assert!(find_quest_id("DEV-").is_none());
        assert!(find_quest_id("dev-001").is_none());
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
