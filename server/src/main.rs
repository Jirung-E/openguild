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
//!
//! DEV-229: frontend 위치 지정 우선순위 — `--frontend-dist`/`FRONTEND_DIST`
//! (CLI/env) > 설정 파일(`openguild-server.toml`, `--config` 로 명시 또는
//! exe 옆/cwd/~/.openguild 자동 탐색 — DEV-247) > 기존 자동 탐색(cwd/exe
//! 조상의 `gui/frontend/build`).
//! 설정 파일 형식은 [`ServerConfig`] 참조.

mod error;
mod routes;

#[cfg(test)]
mod tests;

use openguild_core::guild_file;
use openguild_core::locale::Locale;

use anyhow::{Context, Result};
use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
};
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tower_http::{
    catch_panic::CatchPanicLayer,
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

// DEV-254: 응답 언어 결정. 우선순위: `?lang=ko|en`(명시적 요청 오버라이드)
// > 서버 설정 locale.
// openguild-server 는 host 전용(단일 운영자/길드)이라, 운영자가
// 설정한 locale(`openguild-server locale` / `OPENGUILD_LOCALE` /
// `~/.openguild/locale.json`)이 응답 언어의 기준이 되어야 한다. 이전엔
// `Accept-Language` 를 서버 설정보다 먼저 봐서, 브라우저(GUI)가 자동으로
// 붙이는 Accept-Language 때문에 서버 locale 을 바꿔도 반영이 안 됐다
// (사용자 보고: "서버 언어 변경 안됨"). Accept-Language 자동 감지를 제거하고
// 서버 설정을 따르되, 디버깅/명시 제어용 `?lang=` 만 요청별 오버라이드로 남김.
// core::locale::scoped 로 요청 전체를 감싸 core::tf!() 를 쓰는 에러 메시지
// (AppError::NotFound/BadRequest)가 이 언어를 따른다.
fn detect_request_locale(req: &Request) -> Locale {
    if let Some(q) = req.uri().query() {
        for pair in q.split('&') {
            if let Some(v) = pair.strip_prefix("lang=")
                && let Some(l) = Locale::parse(v)
            {
                return l;
            }
        }
    }
    // 서버 설정 locale(env > locale.json > ko). 요청마다 재조회 — 운영자가
    // `openguild-server locale` 로 바꾸면 재시작 없이 반영(host 전용이라
    // 트래픽이 적어 파일 조회 비용 무시 가능).
    openguild_core::locale::current()
}

/// DEV-357: 정적 자산 캐시 정책.
///
/// SvelteKit 의 `_app/immutable/**` 는 **내용 해시가 파일명에 들어간** 파일이라
/// 같은 URL 이 다른 내용을 가리키는 일이 없다. 그런데 지금까지 `cache-control`
/// 이 없어 브라우저가 매번 조건부 요청을 보냈다 — 파일마다 왕복 한 번씩,
/// 원격(폰)에서 특히 낭비다.
///
/// 반대로 `index.html` 은 **절대 오래 캐시하면 안 된다**. 그 안에 새 해시
/// 파일명들이 들어 있어, 오래된 index 가 캐시되면 업데이트 후에도 옛 번들을
/// 계속 물고 간다. `no-cache`(= 매번 검증)로 둔다.
async fn static_cache_headers(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = req.uri().path().to_string();
    let mut res = next.run(req).await;
    // API 응답은 건드리지 않는다 — 내용이 바뀌는 데이터다.
    if path.starts_with("/api/") {
        return res;
    }
    // 없는 자산은 SPA fallback 이 index.html 로 200 을 준다. 경로만 보고
    // immutable 을 붙이면 브라우저가 **JS URL 아래에 HTML 을** 1년간 캐시해,
    // 배포 후 해시가 바뀌어도 영영 깨진 페이지를 본다. 실제 자산(=HTML 이
    // 아닌 응답)에만 장기 캐시를 건다.
    let is_html = res
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    let value = if path.starts_with("/_app/immutable/") && !is_html {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    res.headers_mut().entry(axum::http::header::CACHE_CONTROL).or_insert(
        axum::http::HeaderValue::from_static(value),
    );
    res
}

async fn locale_middleware(req: Request, next: Next) -> Response {
    let locale = detect_request_locale(&req);
    openguild_core::locale::scoped(locale, next.run(req)).await
}

// DEV-254: `--help`/서브커맨드 help 도 locale 반응 — clap 의 `///` doc 주석은
// 컴파일타임 고정(한글 박제)이라, CLI(cli/src/main.rs)와 동일하게 `about =`/
// `help =` 런타임 표현식으로 바꿔 `openguild_core::tf!()`(effective locale 로
// 분기)로 렌더. `Cli::parse()`(=Command 빌드) 시점에 요청 스코프가 없으므로
// effective() 는 current()(env > locale.json > ko)로 떨어져 서버 설정을 따른다.
// BUG-016: help 문자열에 quest_id 넣지 말 것(leak 가드 테스트가 스캔).
#[derive(Parser, Debug)]
#[command(
    name = "openguild-server",
    version,
    about = openguild_core::tf!(
        "openguild HTTP API server (host 전용 — 정비/데이터 조작은 openguild CLI)",
        "openguild HTTP API server (host only — maintenance/data ops via openguild CLI)"
    )
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = openguild_core::tf!("HTTP API 서버 시작", "Start the HTTP API server"))]
    Host {
        #[arg(long, help = openguild_core::tf!(
            "바인드 포트 (env: PORT, 기본 3000)",
            "Bind port (env: PORT, default 3000)"
        ))]
        port: Option<u16>,
        // DEV-195 후속(admin 피드백): `host --host` 처럼 서브커맨드와 이름이
        // 겹치면 어색하다는 지적으로 `--bind` 로 명명.
        #[arg(long, help = openguild_core::tf!(
            "바인드 주소. `local`(=127.0.0.1, 기본 — 의도치 않은 네트워크 노출 방지) / `public`(=0.0.0.0, 다른 기기 접근 허용) 별칭 또는 IP 리터럴(예: `192.168.1.10`). env: BIND.",
            "Bind address. `local`(=127.0.0.1, default — avoids unintended network exposure) / `public`(=0.0.0.0, allows access from other machines) aliases, or an IP literal (e.g. `192.168.1.10`). env: BIND."
        ))]
        bind: Option<String>,
        #[arg(long, help = openguild_core::tf!(
            "frontend 정적 자산 폴더 직접 지정 (env: FRONTEND_DIST). 설정 파일/자동 탐색보다 우선.",
            "Directly specify the frontend static-asset folder (env: FRONTEND_DIST). Takes precedence over config file / auto-discovery."
        ))]
        frontend_dist: Option<String>,
        #[arg(long, help = openguild_core::tf!(
            "설정 파일(`openguild-server.toml`) 경로 명시. 미지정 시 exe 옆 → cwd → ~/.openguild 순으로 자동 탐색(있으면 사용, 없으면 조용히 skip).",
            "Explicit path to the config file (`openguild-server.toml`). If omitted, auto-discovers next to the exe → cwd → ~/.openguild (uses it if found, silently skips otherwise)."
        ))]
        config: Option<String>,
    },
    #[command(about = openguild_core::tf!(
        "출력/응답 언어 — CLI/GUI 와 같은 위치(`~/.openguild/locale.json`)에 저장, 이후 서버 host 의 기본 응답 언어가 따름. 인자 없으면 현재 값 출력.",
        "Output/response language — saved to the same place as CLI/GUI (`~/.openguild/locale.json`); the server host's default response language follows it. Prints the current value if no arg."
    ))]
    Locale {
        #[arg(help = openguild_core::tf!(
            "ko | en — 미지정 시 현재 값 출력.",
            "ko | en — prints the current value if omitted."
        ))]
        lang: Option<String>,
    },
}

/// DEV-254: 출력/응답 언어 조회·변경 — `~/.openguild/locale.json`(CLI/GUI 와
/// 공유). 인자 없으면 현재값(저장값 + env override 표시), 있으면 저장.
/// CLI 의 `openguild locale` 과 동일 동작 — 서버 바이너리만 있어도 설정 가능.
fn handle_locale(lang: Option<String>) -> Result<()> {
    use openguild_core::locale::{self, Locale};
    match lang {
        None => {
            let saved = locale::load_saved()?;
            let effective = locale::current();
            if saved == effective {
                println!("current language: {}", saved.as_str());
            } else {
                println!(
                    "current language: {} (saved: {}, overridden by OPENGUILD_LOCALE)",
                    effective.as_str(),
                    saved.as_str()
                );
            }
        }
        Some(l) => {
            let Some(parsed) = Locale::parse(&l) else {
                anyhow::bail!("unknown language '{l}' — use ko or en");
            };
            locale::save(parsed)?;
            // 확인 메시지는 방금 저장한 **새 언어**로 — 이전 언어로 나오면
            // 혼란(사용자 보고). parsed 기준으로 직접 분기.
            match parsed {
                Locale::Ko => println!("✓ 언어를 '{}' 로 저장했습니다.", parsed.as_str()),
                Locale::En => println!("✓ language saved as '{}'.", parsed.as_str()),
            }
        }
    }
    Ok(())
}

/// DEV-229: `openguild-server.toml` 설정 파일 형식.
///
/// ```toml
/// [server]
/// frontend_dist = "C:/path/to/frontend/build"
/// ```
///
/// 향후 port / bind addr / guild_path 등도 이 파일로 통합 가능(현재는
/// frontend_dist 만) — 알 수 없는 키는 무시(전방 호환, `#[serde(default)]`
/// 로 필드 부재도 허용).
#[derive(Debug, Default, serde::Deserialize)]
struct ServerConfig {
    #[serde(default)]
    server: ServerConfigSection,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ServerConfigSection {
    frontend_dist: Option<String>,
}

/// 설정 파일을 읽어 파싱. `explicit` 지정 시 그 경로만 시도(없으면 에러 —
/// 사용자가 명시한 경로이므로 조용히 무시하면 오타를 놓침). 미지정 시
/// exe 옆 → cwd → `~/.openguild/` 순 자동 탐색(전부 없으면 `Ok(None)`,
/// 에러 아님). ~/.openguild 는 사용자 데이터 홈 — 배치 무관 공통 설정 위치.
///
/// 반환값에 실제 사용된 경로도 함께 — 시동 로그에 어느 파일을 읽었는지 표시.
fn load_server_config(explicit: Option<&str>) -> Result<Option<(ServerConfig, PathBuf)>> {
    const FILENAME: &str = "openguild-server.toml";

    let path = if let Some(p) = explicit {
        let p = PathBuf::from(p);
        if !p.is_file() {
            anyhow::bail!(openguild_core::tf!(
                "--config 로 지정한 파일이 없음: {}",
                "no such file specified via --config: {}",
                p.display()
            ));
        }
        p
    } else {
        let exe_adjacent = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.join(FILENAME)));
        let home = openguild_core::user_dirs::openguild_home()
            .ok()
            .map(|h| h.join(FILENAME));
        let cwd = PathBuf::from(FILENAME);
        match exe_adjacent {
            Some(p) if p.is_file() => p,
            _ if cwd.is_file() => cwd,
            _ => match home {
                Some(p) if p.is_file() => p,
                _ => return Ok(None),
            },
        }
    };

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("설정 파일 읽기 실패: {}", path.display()))?;
    let cfg: ServerConfig = toml::from_str(&raw)
        .with_context(|| format!("설정 파일 파싱 실패: {}", path.display()))?;
    Ok(Some((cfg, path)))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // DEV-254: locale 은 서버 구동/길드/런타임 무관 — 로그 subscriber 나
    // tokio 런타임 없이 조기 처리(CLI 의 locale 과 동일 취지).
    if let Command::Locale { lang } = cli.command {
        return handle_locale(lang);
    }

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
        Command::Host { port, bind, frontend_dist, config } => {
            rt.block_on(run_host(port, bind, frontend_dist, config))
        }
        Command::Locale { .. } => unreachable!("handled above"),
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
                eprintln!(
                    "{}",
                    openguild_core::tf!(
                        "✗ openguild 프로젝트가 아님: {abs} 에 `.guild` 파일이 없음",
                        "✗ not an openguild project: no `.guild` file in {abs}"
                    )
                );
                eprintln!(
                    "{}",
                    openguild_core::tf!(
                        "  힌트: `.guild` 가 있는 디렉토리를 가리키도록 GUILD_PATH 환경변수를 설정하세요.",
                        "  hint: set GUILD_PATH env var to point at a directory with a `.guild`."
                    )
                );
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

async fn run_host(
    port_arg: Option<u16>,
    bind_arg: Option<String>,
    frontend_dist_arg: Option<String>,
    config_arg: Option<String>,
) -> Result<()> {
    let ctx = load_guild()?;
    // Store 는 .guild/index.db + journal.db 둘 다 자동 마이그레이션.
    let store = openguild_core::Store::open(&ctx.guild_path).await?;
    // DEV-299: 서버는 프로세스가 계속 살아 있으므로 auto-snapshot 을
    // 백그라운드로 — 임계치에 걸린 요청 하나가 ~2초 멈추던 것을 없앤다.
    store.set_background_snapshots(true);

    // 라우터. (audit middleware 폐기 — journal.db 의 ops 가 그 역할.
    //          auto-backup task 폐기 — HTTP admin / CLI 로 명시적 실행.)
    let mut app = routes::create_router(store);

    // DEV-229: 설정 파일 로드 — `--config` 명시 또는 exe 옆/cwd 자동 탐색.
    let server_config = load_server_config(config_arg.as_deref())?;

    // DEV-195: frontend 정적 서빙 (선택) — 단일 origin 으로 SPA+API 같이 서빙.
    // 기본값은 SvelteKit(adapter-static) 의 실제 빌드 출력 `gui/frontend/build`
    // (이전 기본값 `dist` 는 실제로 생성되지 않아 항상 API-only 로 떨어지던 버그).
    //
    // DEV-195 후속: 기본값이 cwd 기준 상대경로라 repo root 에서 실행할 때만
    // 우연히 맞았다. 사용자가 자연스럽게 실행할 위치인 `target/release/`
    // 에서 실행하면 cwd 가 달라 항상 API-only 로 떨어져 `/` 접속이 404 —
    // exe 의 조상 디렉토리들(`target/release` → `target` → repo root)도
    // 차례로 시도해 cargo build 산출물 레이아웃을 그대로 따라간다.
    //
    // DEV-229: 우선순위 — `--frontend-dist` > `FRONTEND_DIST` env > 설정
    // 파일의 `[server] frontend_dist` > 기존 자동 탐색(위 REL 탐색).
    let dist_path = frontend_dist_arg
        .or_else(|| std::env::var("FRONTEND_DIST").ok())
        .or_else(|| {
            server_config
                .as_ref()
                .and_then(|(cfg, _)| cfg.server.frontend_dist.clone())
        })
        .unwrap_or_else(|| {
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

    // BUG-004: production CSP — eval 금지(unsafe-eval 미포함) + 리소스 출처를
    // same-origin 으로 제한. DEV-195 단일 origin 배포라 API 도 'self' 로 충분.
    // script-src 'unsafe-inline' 은 SvelteKit adapter-static 이 index.html 에
    // hydration 부트스트랩을 inline <script> 로 심기 때문에 필요(빌드마다 내용이
    // 바뀌어 hash 고정 불가). style-src 'unsafe-inline' 은 Svelte/cytoscape 의
    // inline style 때문. img/media 의 data:/blob: 은 첨부 미리보기(클립보드
    // paste)용.
    let csp = concat!(
        "default-src 'self'; ",
        "script-src 'self' 'unsafe-inline'; ",
        "style-src 'self' 'unsafe-inline'; ",
        "img-src 'self' data: blob:; ",
        "media-src 'self' data: blob:; ",
        "font-src 'self' data:; ",
        "connect-src 'self'; ",
        "object-src 'none'; ",
        "base-uri 'self'; ",
        "frame-ancestors 'self'"
    );
    let app = app
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::CONTENT_SECURITY_POLICY,
            axum::http::HeaderValue::from_static(csp),
        ))
        .layer(middleware::from_fn(locale_middleware))
        // DEV-357: 정적 자산 캐시 정책 — _app/immutable 은 영구, 그 외는 no-cache.
        .layer(middleware::from_fn(static_cache_headers))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        // DEV-332: 압축은 가장 바깥 — 정적 자산(fallback_service)까지 덮는다.
        // axum 의 `layer` 는 그 시점까지 등록된 라우트를 감싸므로, 정적 fallback
        // 이 이미 붙은 뒤인 여기서 걸어야 _app/*.js 도 압축된다.
        .layer(routes::compression_layer())
        // DEV-367: catch-panic 은 **압축보다도 바깥**. 같은 이유로 정적
        // fallback 까지 덮고, 압축 계층 자체가 패닉해도 잡는다. 이게 없으면
        // 패닉이 500 이 아니라 연결 끊김으로 나가 클라이언트가 네트워크
        // 오류로 오인한다(BUG-249 가 '검색 결과 없음' 으로 보인 이유).
        //
        // 덮이지 **않는** 것(실측): 첨부 zip 처럼 `tokio::spawn` 된 태스크에서
        // 나는 패닉. 별도 태스크라 이 계층을 아예 거치지 않는다 — 클라이언트는
        // **HTTP 200 에 잘린 본문**을 받고 성공으로 오해한다(curl exit 0).
        // 자세한 실측은 DEV-368.
        .layer(CatchPanicLayer::custom(error::panic_to_500));

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
    // DEV-229(BUG-105 후속): API-only 로 떨어졌을 때 그냥 "없다" 는 표시만으론
    // 왜 화면이 안 뜨는지 알 수 없다는 게 실제 혼란 원인이었음 — 지정 방법을
    // 함께 안내.
    if serves_static {
        println!("  static : {dist_path}");
    } else {
        println!(
            "{}",
            openguild_core::tf!(
                "  static : (none — API-only 모드)",
                "  static : (none — API-only mode)"
            )
        );
        println!(
            "{}",
            openguild_core::tf!(
                "           frontend 자산 없음. --frontend-dist / FRONTEND_DIST env / openguild-server.toml 의 [server] frontend_dist 로 지정 가능.",
                "           no frontend assets. Specify via --frontend-dist / FRONTEND_DIST env / [server] frontend_dist in openguild-server.toml."
            )
        );
    }
    if let Some((_, path)) = &server_config {
        println!("  config : {}", path.display());
    }
    // DEV-163: 정비는 CLI(`openguild ...`) 또는 HTTP admin(`/api/admin/*`).
    println!("  admin  : HTTP /api/admin/* (snapshot/reindex/drift/vacuum/journal)");
    println!(
        "{}",
        openguild_core::tf!(
            "  maint  : offline 은 `openguild` CLI (backup/restore/reindex/…)",
            "  maint  : offline via the `openguild` CLI (backup/restore/reindex/…)"
        )
    );
    // DEV-195: 인증 계층이 없으므로 loopback 밖으로 노출하면 신뢰된 네트워크/
    // 리버스 프록시 뒤에서만 사용 권장 — 시작 시 한 줄로 경고.
    if !bind_ip.is_loopback() {
        println!(
            "{}",
            openguild_core::tf!(
                "  ⚠ warning: bound to {bind_ip} (네트워크 노출) — 인증 없음, 신뢰된 네트워크에서만 사용하세요.",
                "  ⚠ warning: bound to {bind_ip} (network-exposed) — no auth; use only on a trusted network."
            )
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
            Command::Host { port, bind, .. } => {
                assert_eq!(port, None);
                assert_eq!(bind, None);
            }
            Command::Locale { .. } => unreachable!(),
        }
    }

    #[test]
    fn parse_host_with_port() {
        let cli = Cli::try_parse_from(["openguild-server", "host", "--port", "3300"]).unwrap();
        match cli.command {
            Command::Host { port, .. } => assert_eq!(port, Some(3300)),
            Command::Locale { .. } => unreachable!(),
        }
    }

    #[test]
    fn parse_host_with_bind() {
        let cli =
            Cli::try_parse_from(["openguild-server", "host", "--bind", "public"]).unwrap();
        match cli.command {
            Command::Host { bind, .. } => assert_eq!(bind, Some("public".to_string())),
            Command::Locale { .. } => unreachable!(),
        }
    }

    // DEV-229: --frontend-dist / --config CLI 파싱.
    #[test]
    fn parse_host_with_frontend_dist_and_config() {
        let cli = Cli::try_parse_from([
            "openguild-server",
            "host",
            "--frontend-dist",
            "C:/dist",
            "--config",
            "C:/my.toml",
        ])
        .unwrap();
        match cli.command {
            Command::Host { frontend_dist, config, .. } => {
                assert_eq!(frontend_dist, Some("C:/dist".to_string()));
                assert_eq!(config, Some("C:/my.toml".to_string()));
            }
            Command::Locale { .. } => unreachable!(),
        }
    }

    // DEV-229: 설정 파일 로드 — explicit 경로 지정, 정상 파싱.
    #[test]
    fn load_server_config_explicit_path_parses_frontend_dist() {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-srv-cfg-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join("openguild-server.toml");
        std::fs::write(&cfg_path, "[server]\nfrontend_dist = \"C:/from/config\"\n").unwrap();

        let (cfg, used_path) = load_server_config(Some(cfg_path.to_str().unwrap()))
            .unwrap()
            .expect("config should load");
        assert_eq!(cfg.server.frontend_dist.as_deref(), Some("C:/from/config"));
        assert_eq!(used_path, cfg_path);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // DEV-229: --config 로 명시했는데 파일이 없으면 조용히 넘어가지 않고 에러
    // (자동 탐색과 달리 사용자가 명시한 경로라 오타를 놓치면 안 됨).
    #[test]
    fn load_server_config_explicit_missing_path_errors() {
        assert!(load_server_config(Some("Z:/definitely/does/not/exist.toml")).is_err());
    }

    // DEV-229: 미지정 시 자동 탐색 대상(exe 옆/cwd/~/.openguild)에 파일이 없으면
    // 에러 아님 — None.
    // BUG-148: 개발 머신에 실제 `~/.openguild/openguild-server.toml` 이 있으면
    // 탐색이 그걸 찾아 Some 을 반환해 이 테스트가 실패했다(테스트 격리 결함).
    // `OPENGUILD_HOME` 을 빈 temp 로 격리해 실제 홈을 안 보게 한다(exe 옆/cwd 는
    // 이 repo 상 원래 openguild-server.toml 이 없음). env 는 프로세스 전역이라
    // 전용 mutex 로 직렬화.
    #[test]
    fn load_server_config_none_when_not_found_and_not_explicit() {
        use std::sync::Mutex;
        static GUARD: Mutex<()> = Mutex::new(());
        let _lock = GUARD.lock().unwrap();

        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("og-srv-cfg-none-{ns}"));
        std::fs::create_dir_all(&home).unwrap();

        let prev = std::env::var("OPENGUILD_HOME").ok();
        // SAFETY: 위 GUARD mutex 로 직렬화.
        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
        let result = load_server_config(None);
        unsafe {
            match prev {
                Some(v) => std::env::set_var("OPENGUILD_HOME", v),
                None => std::env::remove_var("OPENGUILD_HOME"),
            }
        }
        let _ = std::fs::remove_dir_all(&home);

        assert!(result.unwrap().is_none());
    }

    // DEV-229: 잘못된 TOML 은 explicit 경로에서 에러.
    #[test]
    fn load_server_config_malformed_toml_errors() {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-srv-cfg-bad-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join("bad.toml");
        std::fs::write(&cfg_path, "not = [valid toml").unwrap();

        assert!(load_server_config(Some(cfg_path.to_str().unwrap())).is_err());
        let _ = std::fs::remove_dir_all(&dir);
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
