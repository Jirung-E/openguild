//! openguild desktop (Tauri v2) — library entry point.
//!
//! `run()` 은 Tauri 앱을 빌드/실행한다. `main.rs` 가 이를 호출.
//! lib.rs 로 분리해두는 이유:
//! - 모바일 / iOS / Android 빌드 시 동일 entry 를 공유할 수 있음 (Tauri v2 권장 구조)
//! - 단위 테스트에서 핸들러를 호출하기 쉬움

mod commands;

use openguild_core::Store;
use std::path::{Path, PathBuf};

/// CLI argv[1] 을 guild 디렉토리로 해석.
///
/// 사용자가 `openguild-gui path/to/foo.guild` (파일 더블클릭) 또는
/// `openguild-gui path/to/dir` 로 실행할 때:
/// - `.guild` 확장자 파일이면 그 파일의 부모 디렉토리를 guild 로.
/// - 디렉토리면 그대로 (안의 `.guild` 파일은 Store::open 이 검증).
///
/// 잘못된 경로 (존재 안 함 / 권한 없음) 면 `None` 반환해 폴백.
pub(crate) fn arg_to_guild_path(arg: &Path) -> Option<PathBuf> {
    if !arg.exists() {
        return None;
    }
    if arg.is_file() {
        // .guild 파일이면 부모 디렉토리. 다른 파일은 무시.
        if arg.extension().and_then(|e| e.to_str()) == Some("guild") {
            return arg.parent().map(|p| p.to_path_buf());
        }
        return None;
    }
    if arg.is_dir() {
        return Some(arg.to_path_buf());
    }
    None
}

/// DEV-052 후속: 디렉토리에 길드 마커 (`.guild` 파일 또는 `.guild/` 디렉토리)
/// 가 있는지. 없으면 "이 위치 초기화?" 흐름 (`LaunchMode::Uninit`) 으로 진입.
pub(crate) fn has_guild_marker(dir: &Path) -> bool {
    // (a) `.guild` 파일 (TOML 메타) — `<name>.guild` 같은 형태 등.
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("guild") {
                return true;
            }
        }
    }
    // (b) `.guild/` 디렉토리 (quests/types/statuses 보관). init 후엔 항상 존재.
    dir.join(".guild").is_dir()
}

/// DEV-052: `openguild-gui` 시작 모드.
///
/// - `Guild(path)` — 특정 guild 의 보드 / 디테일 페이지로 진입.
/// - `Welcome` — guild 인자 없이 시작. recents 표시 화면 (`/welcome`).
/// - `Uninit(path)` — argv 로 디렉토리는 받았지만 `.guild` 파일이 없음.
///   "이 위치를 길드로 초기화할지" 확인 UI 가 떠야 함 (DEV-052 후속).
#[derive(Debug, Clone)]
pub(crate) enum LaunchMode {
    Guild(PathBuf),
    Welcome,
    Uninit(PathBuf),
}

/// guild 디렉토리 해결 — 내부 구현, 환경 의존성 명시.
///
/// 우선순위:
/// 1. **CLI argv** — `openguild-gui foo.guild` 또는 `openguild-gui /path/to/guild-dir`.
///    .guild 파일 더블클릭 (OS file association, DEV-005) 도 이 경로로 들어옴.
///    **argv 가 명시되었는데 해석 실패 시 `Err` 반환** — 잘못된 경로 시 cwd 로
///    조용히 폴백하지 않고 사용자가 인지하도록 종료.
/// 2. `OPENGUILD_GUILD` env — 테스트 / 명시 지정. 잘못된 값이면 Welcome 으로 폴백.
/// 3. 그 외 → DEV-052: **`Welcome`** (이전엔 cwd 자동 탐색 + 부트스트랩이었지만
///    사용자가 명시 안 한 경우엔 welcome 화면이 더 자연스러움).
pub(crate) fn resolve_launch_mode<I, S>(
    args: I,
    env_guild: Option<String>,
) -> Result<LaunchMode, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    // 사용자 argv — 프로그램명 (skip 1) 제외, `--` 로 시작하는 옵션 플래그 제외.
    let user_args: Vec<_> = args
        .into_iter()
        .skip(1)
        .filter(|s| !s.as_ref().to_string_lossy().starts_with("--"))
        .collect();

    // 1. argv 있으면 해석 결과가 곧 답.
    //    DEV-052 후속: 디렉토리는 있는데 .guild 가 없으면 → Uninit (확인 UI).
    if let Some(first) = user_args.first() {
        let p = Path::new(first.as_ref());
        let resolved = arg_to_guild_path(p).ok_or_else(|| {
            format!(
                "'{}' 는 올바른 guild 경로가 아닙니다. \
                 디렉토리 또는 .guild 파일을 지정하세요.",
                p.display()
            )
        })?;
        // DEV-052 후속 (4회차): `.` / 상대경로 → 절대경로 정규화.
        // Uninit prompt 에서 `.` 그대로 표시되면 어딘지 알 수 없음.
        let resolved = absolutize(&resolved);
        return Ok(if has_guild_marker(&resolved) {
            LaunchMode::Guild(resolved)
        } else {
            LaunchMode::Uninit(resolved)
        });
    }

    // 2. env
    if let Some(path) = env_guild {
        let p = PathBuf::from(path);
        if p.exists() {
            let p = absolutize(&p);
            return Ok(if has_guild_marker(&p) {
                LaunchMode::Guild(p)
            } else {
                LaunchMode::Uninit(p)
            });
        }
        // env 가 잘못된 경우는 자동 폴백.
    }

    // 3. argv / env 모두 없으면 Welcome 으로 진입.
    Ok(LaunchMode::Welcome)
}

/// 상대 경로 (`.` / `..` / `foo` 등) → 절대 + Windows `\\?\` prefix 제거.
/// `canonicalize` 실패 시 원본 그대로 (방어적).
fn absolutize(p: &Path) -> PathBuf {
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    // Windows: `\\?\C:\…` → `C:\…`.
    let s = abs.to_string_lossy().to_string();
    let cleaned = s
        .trim_start_matches(r"\\?\")
        .trim_start_matches(r"\\\\?\\");
    PathBuf::from(cleaned)
}

/// DEV-052: managed state — frontend 가 invoke 로 조회하여 첫 진입 URL 결정.
pub struct LaunchInfo {
    pub mode: &'static str, // "guild" | "welcome" | "uninit"
    /// Uninit 모드일 때 사용자가 원하는 길드 path. "이 위치 초기화?" 확인 후
    /// `init_and_open_guild(path)` 호출에 사용.
    pub uninit_path: Option<PathBuf>,
}

/// BUG-041 후속: Windows release 의 `windows_subsystem = "windows"` 빌드는
/// stdout/stderr 이 부모 콘솔에서 분리됨. `AttachConsole(ATTACH_PARENT_PROCESS)`
/// 만으로는 부족 — Rust 의 stdout 핸들이 invalid 인 채라 println! 이 어디로도
/// 안 감. `CONOUT$` 파일을 열어 `SetStdHandle` 로 STDOUT/STDERR 를 redirect
/// 까지 해야 println!/eprintln! 이 콘솔에 보임.
///
/// 이미 stdout handle 이 valid (`>`/`|` 로 redirect / pipe 된 경우) 면 noop —
/// 그쪽으로 정상 흐름.
///
/// debug 빌드 / non-Windows / 부모 콘솔 없는 환경 (GUI launcher 더블클릭) 시 noop.
fn attach_parent_console() {
    #[cfg(windows)]
    unsafe {
        use std::ptr;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        use windows_sys::Win32::System::Console::{
            AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS,
            STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
        };

        // stdout 이 이미 valid (redirect 등) 면 그대로 — 건드리면 그쪽으로 안 감.
        let cur = GetStdHandle(STD_OUTPUT_HANDLE);
        if !cur.is_null() && cur != INVALID_HANDLE_VALUE {
            return;
        }
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return; // 부모 콘솔 없음 — GUI launcher 등.
        }
        // `CONOUT$` (콘솔 출력 파일) 을 GENERIC_WRITE 로 open → handle 을
        // STD_OUTPUT_HANDLE / STD_ERROR_HANDLE 로 SetStdHandle.
        // GENERIC_WRITE = 0x4000_0000.
        const GENERIC_WRITE: u32 = 0x4000_0000;
        let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        let h = CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        );
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            return;
        }
        SetStdHandle(STD_OUTPUT_HANDLE, h);
        SetStdHandle(STD_ERROR_HANDLE, h);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // BUG-041 후속: `--version` / `--help` 짧은 flag 처리.
    // Tauri 시동 전에 stdout 으로 답하고 종료 — launcher / 스크립트 친화.
    //
    // Windows release 는 windows_subsystem="windows" 로 빌드되어 cmd 에서 직접
    // 실행 시 stdout 이 부모 콘솔에서 detach (cmd 가 즉시 prompt 복귀).
    // `attach_parent_console()` 로 부모 콘솔 attach 후 stdout/stderr redirect.
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--version" | "-V" => {
                attach_parent_console();
                println!("openguild-gui {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                attach_parent_console();
                println!(
                    "openguild-gui — desktop GUI\n\
                     \n\
                     Usage:\n  \
                       openguild-gui [GUILD_PATH]\n\
                     \n\
                     GUILD_PATH 가 디렉토리 / `.guild` 파일이면 해당 길드로 시동.\n\
                     미지정 시 welcome 화면.\n\
                     \n\
                     Env:\n  \
                       OPENGUILD_GUILD=PATH  argv 대신 환경변수로 길드 지정.\n\
                     \n\
                     Flags:\n  \
                       -V, --version   버전 출력 후 종료\n  \
                       -h, --help      이 도움말 출력 후 종료"
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }

    let launch_mode = match resolve_launch_mode(
        std::env::args_os(),
        std::env::var("OPENGUILD_GUILD").ok(),
    ) {
        Ok(m) => m,
        Err(msg) => {
            eprintln!("[openguild-gui] error: {msg}");
            std::process::exit(2);
        }
    };

    // BUG-041: Welcome / Uninit 모드는 진짜 길드 없이 시동 — 디스크에 placeholder
    // DB 를 만들면 그 DB 가 binary 의 schema 만큼 migrate 된 채 영구 남아, 이후
    // 더 이전 (mig 모르는) binary 가 같은 placeholder 를 열면 brick. → Welcome
    // /Uninit 은 **in-memory DB** 로 시동, 디스크 placeholder 안 만듦. 사용자가
    // 실제 길드를 선택하면 commands 가 file-backed Store 로 swap.
    let (mode_label, store_path, uninit_path) = match &launch_mode {
        LaunchMode::Guild(p) => ("guild", Some(p.clone()), None),
        LaunchMode::Welcome => ("welcome", None, None),
        LaunchMode::Uninit(p) => ("uninit", None, Some(p.clone())),
    };
    let launch_info = LaunchInfo { mode: mode_label, uninit_path };
    eprintln!(
        "[openguild-gui] launch mode: {} (path: {})",
        launch_info.mode,
        store_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<in-memory>".into())
    );

    // Store::open 은 async — Tauri 의 async runtime 으로 동기 실행.
    let store = tauri::async_runtime::block_on(async {
        match &store_path {
            Some(p) => Store::open(p).await,
            // BUG-041: Welcome / Uninit 은 in-memory — paths 인자는 임시
            // placeholder (디렉토리 만들기 위함이지만 in-memory pool 은 디스크
            // 에 안 씀).
            None => {
                let tmp = std::env::temp_dir().join("openguild-welcome-placeholder");
                Store::open_in_memory(&tmp).await
            }
        }
    })
    .expect("Store::open 실패 — guild 디렉토리 손상 또는 권한 없음");

    // Recent guild 자동 등록 (DEV-006). Welcome / Uninit 은 placeholder 라 등록 안 함.
    if let Some(p) = &store_path
        && let Err(e) = openguild_core::recents::add(p)
    {
        eprintln!("[openguild-gui] warn: recents 갱신 실패 — {e:#}");
    }

    // BUG-049 / DEV-121: 시동 시 외부 편집 sync. 사용자가 CLI / 외부 편집으로
    // 파일을 바꿨다면 index.db 가 stale.
    //
    // DEV-121: `incremental::sync_on_open` — modified file 들 cheap UPDATE +
    // 필요 시 풀 reindex fallback. 대부분 case 가 빠름 (stat() 만으로 감지).
    // Welcome / Uninit 은 in-memory 라 sync 없음.
    if store_path.is_some() {
        match tauri::async_runtime::block_on(openguild_core::incremental::sync_on_open(&store)) {
            Ok((inc, Some(rep))) => eprintln!(
                "[openguild-gui] incremental {} + full reindex: {} quests / {} deps / {} campaigns",
                inc.updated, rep.quests_loaded, rep.dependencies_loaded, rep.campaigns_loaded
            ),
            Ok((inc, None)) if inc.updated > 0 => {
                eprintln!("[openguild-gui] incremental sync: {} quests updated", inc.updated)
            }
            Ok(_) => {}
            Err(e) => eprintln!("[openguild-gui] warn: sync_on_open 실패 — {e:#}"),
        }
    }

    // DEV-087: setup closure 로 넘길 asset scope 대상 — 길드 root.
    let asset_scope_path = store_path.clone();

    tauri::Builder::default()
        // DEV-053: 디렉토리 선택 dialog — Welcome 의 "폴더 열기".
        .plugin(tauri_plugin_dialog::init())
        // DEV-063: auto-update — updater (체크/다운로드/설치) + process (relaunch).
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // BUG-040: 외부 링크 시스템 브라우저로.
        .plugin(tauri_plugin_opener::init())
        .manage(store)
        .manage(launch_info)
        .invoke_handler(tauri::generate_handler![
            commands::launch_mode,
            commands::current_guild_path,
            // BUG-041: DB schema 가 binary 보다 새로운지 — banner 표시용.
            commands::get_db_schema_status,
            commands::inspect_guild_path,
            commands::open_guild_in_current_window,
            commands::init_and_open_guild,
            // meta
            commands::list_quest_types,
            commands::list_quest_statuses,
            // quests (read)
            commands::list_quests,
            commands::list_deleted_quests,
            commands::get_quest,
            commands::get_quest_by_slug,
            commands::list_quest_candidates,
            commands::list_quest_positions,
            commands::list_quest_dependencies,
            commands::list_quest_history,
            // quests (mutation)
            commands::create_quest,
            commands::update_quest,
            commands::change_quest_status,
            commands::change_quest_parent,
            // DEV-076 / BUG-031: 희망 / 필수 기한 — Tauri transport invoke.
            commands::set_quest_due_dates,
            // DEV-068: tag 전체 교체.
            commands::set_quest_tags,
            commands::change_quest_type,
            commands::delete_quest,
            commands::restore_quest,
            commands::add_prerequisite,
            commands::remove_prerequisite,
            commands::update_quest_position,
            // admin
            commands::admin_create_snapshot,
            commands::admin_list_snapshots,
            commands::admin_restore,
            commands::admin_check_drift,
            commands::admin_reindex,
            // admin meta (DEV-014)
            commands::admin_list_types,
            commands::admin_create_type,
            commands::admin_update_type,
            commands::admin_delete_type,
            commands::admin_list_statuses,
            commands::admin_create_status,
            commands::admin_update_status,
            commands::admin_delete_status,
            // DEV-068: tag defs
            commands::admin_list_tag_defs,
            commands::admin_upsert_tag_def,
            commands::admin_delete_tag_def,
            // recents (DEV-006)
            commands::list_recents,
            commands::clear_recents,
            commands::remove_recent,
            // campaigns (DEV-011)
            commands::list_campaigns,
            commands::create_campaign,
            commands::get_campaign,
            commands::update_campaign,
            commands::delete_campaign,
            commands::campaign_link_quest,
            commands::campaign_unlink_quest,
            commands::campaign_checklist_add,
            commands::campaign_checklist_set,
            commands::campaign_checklist_rm,
            commands::list_campaign_active_summaries,
            commands::list_campaign_upcoming_summaries,
            commands::list_campaigns_for_quest,
            // DEV-016: 길드 규칙.
            commands::get_rules,
            commands::set_rules,
            // DEV-016 (multi-file): 다중 규칙 CRUD.
            commands::list_rules,
            commands::get_rule,
            commands::set_rule,
            commands::create_rule,
            commands::delete_rule,
            commands::rename_rule,
            // DEV-012 / DEV-094: 메모 (단일 텍스트) + 댓글 (entry 단위).
            commands::get_memo,
            commands::set_memo,
            commands::list_comments,
            commands::add_comment,
            commands::update_comment,
            commands::delete_comment,
            commands::toggle_comment_reaction,
            // DEV-100: 캠페인 댓글 / 메모.
            commands::list_campaign_comments,
            commands::add_campaign_comment,
            commands::update_campaign_comment,
            commands::delete_campaign_comment,
            commands::toggle_campaign_comment_reaction,
            commands::get_campaign_memo,
            commands::set_campaign_memo,
            // DEV-087: 캠페인 배너 이미지 (파일 선택은 frontend dialog plugin).
            commands::set_campaign_banner,
            commands::clear_campaign_banner,
            // DEV-060: 퀘스트 템플릿 (NewQuestModal 의 선택 dropdown).
            commands::list_templates,
        ])
        // DEV-087: asset protocol scope — 길드 경로가 동적이라 (사용자가 임의
        // 폴더 open) config scope 대신 런타임 allow. `.guild/assets/` 의 배너
        // 이미지와 (DEV-069) 본문 로컬 이미지를 convertFileSrc 로 표시 가능.
        .setup(move |app| {
            if let Some(p) = &asset_scope_path {
                use tauri::Manager;
                let scope = app.asset_protocol_scope();
                if let Err(e) = scope.allow_directory(p, true) {
                    eprintln!("[openguild-gui] warn: asset scope allow 실패 — {e:#}");
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tmp(label: &str) -> PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-gui-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn arg_to_guild_path_dir_passes_through() {
        let dir = fresh_tmp("dir");
        let got = arg_to_guild_path(&dir).unwrap();
        assert_eq!(std::fs::canonicalize(got).unwrap(), std::fs::canonicalize(&dir).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arg_to_guild_path_dot_guild_file_returns_parent() {
        let dir = fresh_tmp("file");
        let guild_file = dir.join("test.guild");
        std::fs::write(&guild_file, "name = \"x\"\nversion = \"1.0\"\ncreated_at = \"\"\n")
            .unwrap();
        let got = arg_to_guild_path(&guild_file).unwrap();
        assert_eq!(
            std::fs::canonicalize(got).unwrap(),
            std::fs::canonicalize(&dir).unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arg_to_guild_path_non_guild_file_rejected() {
        let dir = fresh_tmp("non-guild");
        let other = dir.join("readme.txt");
        std::fs::write(&other, "x").unwrap();
        assert!(arg_to_guild_path(&other).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arg_to_guild_path_missing_returns_none() {
        let phantom = std::env::temp_dir().join("og-gui-does-not-exist-xyz");
        assert!(arg_to_guild_path(&phantom).is_none());
    }

    // DEV-052: launch mode resolution.
    fn is_guild(mode: &LaunchMode) -> bool {
        matches!(mode, LaunchMode::Guild(_))
    }

    /// 디렉토리에 길드 마커 (`.guild` 디렉토리) 시드 — 테스트용 헬퍼.
    fn init_marker(dir: &Path) {
        std::fs::create_dir_all(dir.join(".guild")).unwrap();
    }

    #[test]
    fn resolve_argv_dir_with_marker_is_guild() {
        let dir = fresh_tmp("argv-priority");
        init_marker(&dir);
        let argv: Vec<std::ffi::OsString> = vec!["program".into(), dir.as_os_str().into()];
        let got = resolve_launch_mode(argv, Some("/nonexistent/path-for-test".into())).unwrap();
        match got {
            LaunchMode::Guild(p) => assert_eq!(
                std::fs::canonicalize(p).unwrap(),
                std::fs::canonicalize(&dir).unwrap()
            ),
            _ => panic!("expected Guild"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_argv_dir_without_marker_is_uninit() {
        // DEV-052 후속: .guild 마커가 없는 디렉토리는 Uninit — 사용자에게
        // "초기화?" 확인 UI.
        let dir = fresh_tmp("argv-uninit");
        let argv: Vec<std::ffi::OsString> = vec!["program".into(), dir.as_os_str().into()];
        let got = resolve_launch_mode(argv, None).unwrap();
        match got {
            LaunchMode::Uninit(p) => assert_eq!(
                std::fs::canonicalize(p).unwrap(),
                std::fs::canonicalize(&dir).unwrap()
            ),
            other => panic!("expected Uninit, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_argv_guild_file_wins() {
        let dir = fresh_tmp("argv-guild-file");
        let guild_file = dir.join("test.guild");
        std::fs::write(&guild_file, "name = \"x\"\nversion = \"1.0\"\ncreated_at = \"\"\n")
            .unwrap();
        let argv: Vec<std::ffi::OsString> =
            vec!["program".into(), guild_file.as_os_str().into()];
        let got = resolve_launch_mode(argv, None).unwrap();
        match got {
            LaunchMode::Guild(p) => assert_eq!(
                std::fs::canonicalize(p).unwrap(),
                std::fs::canonicalize(&dir).unwrap()
            ),
            _ => panic!("expected Guild"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_env_used_when_argv_empty() {
        let dir = fresh_tmp("env-fallback");
        init_marker(&dir);
        let argv: Vec<std::ffi::OsString> = vec!["program".into()];
        let got = resolve_launch_mode(argv, Some(dir.to_string_lossy().into_owned())).unwrap();
        match got {
            LaunchMode::Guild(p) => assert_eq!(p, dir),
            _ => panic!("expected Guild"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_invalid_argv_errors_no_silent_fallback() {
        let dir = fresh_tmp("must-not-fall-back");
        let argv: Vec<std::ffi::OsString> =
            vec!["program".into(), "/does/not/exist/argv-test".into()];
        let err = resolve_launch_mode(argv, Some(dir.to_string_lossy().into_owned())).unwrap_err();
        assert!(err.contains("올바른 guild 경로가 아닙니다"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_argv_non_guild_file_errors() {
        let dir = fresh_tmp("argv-non-guild");
        let txt = dir.join("readme.txt");
        std::fs::write(&txt, "x").unwrap();
        let argv: Vec<std::ffi::OsString> = vec!["program".into(), txt.as_os_str().into()];
        let err = resolve_launch_mode(argv, None).unwrap_err();
        assert!(err.contains("올바른 guild 경로가 아닙니다"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_argv_skips_dashdash_flags_then_welcome() {
        // 모든 argv 가 플래그면 빈 argv 와 동일 → Welcome.
        let argv: Vec<std::ffi::OsString> =
            vec!["program".into(), "--no-default-features".into()];
        let got = resolve_launch_mode(argv, None).unwrap();
        assert!(matches!(got, LaunchMode::Welcome));
    }

    #[test]
    fn resolve_invalid_env_falls_to_welcome() {
        let argv: Vec<std::ffi::OsString> = vec!["program".into()];
        let got = resolve_launch_mode(argv, Some("/nonexistent/env".into())).unwrap();
        assert!(matches!(got, LaunchMode::Welcome));
    }

    #[test]
    fn resolve_empty_argv_no_env_returns_welcome() {
        // DEV-052: 이전엔 cwd fallback 으로 Guild 였음 → 이제 Welcome.
        let argv: Vec<std::ffi::OsString> = vec!["program".into()];
        let got = resolve_launch_mode(argv, None).unwrap();
        assert!(matches!(got, LaunchMode::Welcome));
    }

    #[test]
    fn resolve_argv_helper_categorizes_guild_vs_welcome() {
        let dir = fresh_tmp("helper");
        init_marker(&dir);
        let argv: Vec<std::ffi::OsString> = vec!["program".into(), dir.as_os_str().into()];
        assert!(is_guild(&resolve_launch_mode(argv, None).unwrap()));
        let argv_empty: Vec<std::ffi::OsString> = vec!["program".into()];
        assert!(!is_guild(&resolve_launch_mode(argv_empty, None).unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn has_guild_marker_detects_dot_guild_dir() {
        let dir = fresh_tmp("marker-dir");
        assert!(!has_guild_marker(&dir));
        std::fs::create_dir_all(dir.join(".guild")).unwrap();
        assert!(has_guild_marker(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn has_guild_marker_detects_dot_guild_file() {
        let dir = fresh_tmp("marker-file");
        assert!(!has_guild_marker(&dir));
        std::fs::write(dir.join("my.guild"), "name=\"x\"").unwrap();
        assert!(has_guild_marker(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
