//! OpenGuild desktop (Tauri v2) — library entry point.
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

/// DEV-052: `openguild-gui` 시작 모드.
///
/// - `Guild(path)` — 특정 guild 의 보드 / 디테일 페이지로 진입.
/// - `Welcome` — guild 인자 없이 시작. recents 표시 화면 (`/welcome`).
#[derive(Debug, Clone)]
pub(crate) enum LaunchMode {
    Guild(PathBuf),
    Welcome,
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

    // 1. argv 있으면 해석 결과가 곧 답. 실패 시 에러.
    if let Some(first) = user_args.first() {
        let p = Path::new(first.as_ref());
        return arg_to_guild_path(p).map(LaunchMode::Guild).ok_or_else(|| {
            format!(
                "'{}' 는 올바른 guild 경로가 아닙니다. \
                 .guild 파일이 있는 디렉토리 또는 .guild 파일 자체를 지정하세요.",
                p.display()
            )
        });
    }

    // 2. env
    if let Some(path) = env_guild {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(LaunchMode::Guild(p));
        }
        // env 가 잘못된 경우는 자동 폴백.
    }

    // 3. argv / env 모두 없으면 Welcome 으로 진입.
    Ok(LaunchMode::Welcome)
}

/// DEV-052: managed state — frontend 가 invoke 로 조회하여 첫 진입 URL 결정.
pub struct LaunchInfo {
    pub mode: &'static str, // "guild" | "welcome"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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

    // Welcome 모드는 OS temp 디렉토리에 빈 길드 부트스트랩 — Store / Tauri
    // commands 가 항상 valid 한 store 가정해서. recents 자동 등록은 건너뜀.
    let (guild_path, launch_info) = match &launch_mode {
        LaunchMode::Guild(p) => (p.clone(), LaunchInfo { mode: "guild" }),
        LaunchMode::Welcome => {
            let tmp = std::env::temp_dir().join("openguild-welcome-placeholder");
            std::fs::create_dir_all(&tmp).ok();
            (tmp, LaunchInfo { mode: "welcome" })
        }
    };
    eprintln!(
        "[openguild-gui] launch mode: {} (path: {})",
        launch_info.mode,
        guild_path.display()
    );

    // Store::open 은 async — Tauri 의 async runtime 으로 동기 실행.
    let store = tauri::async_runtime::block_on(Store::open(&guild_path))
        .expect("Store::open 실패 — guild 디렉토리 손상 또는 권한 없음");

    // Recent guild 자동 등록 (DEV-006). Welcome 모드는 placeholder 라 등록 안 함.
    if matches!(launch_mode, LaunchMode::Guild(_))
        && let Err(e) = openguild_core::recents::add(&guild_path)
    {
        eprintln!("[openguild-gui] warn: recents 갱신 실패 — {e:#}");
    }

    tauri::Builder::default()
        .manage(store)
        .manage(launch_info)
        .invoke_handler(tauri::generate_handler![
            commands::launch_mode,
            commands::open_guild_in_new_window,
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
            // recents (DEV-006)
            commands::list_recents,
            commands::clear_recents,
        ])
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

    #[test]
    fn resolve_argv_dir_wins_over_env() {
        let dir = fresh_tmp("argv-priority");
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
        let argv: Vec<std::ffi::OsString> = vec!["program".into(), dir.as_os_str().into()];
        assert!(is_guild(&resolve_launch_mode(argv, None).unwrap()));
        let argv_empty: Vec<std::ffi::OsString> = vec!["program".into()];
        assert!(!is_guild(&resolve_launch_mode(argv_empty, None).unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
