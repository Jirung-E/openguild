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

/// guild 디렉토리 해결 — 내부 구현, 환경 의존성 명시.
///
/// 우선순위:
/// 1. **CLI argv** — `openguild-gui foo.guild` 또는 `openguild-gui /path/to/guild-dir`.
///    .guild 파일 더블클릭 (OS file association, DEV-005) 도 이 경로로 들어옴.
///    **argv 가 명시되었는데 해석 실패 시 `Err` 반환** — 잘못된 경로 시 cwd 로
///    조용히 폴백하지 않고 사용자가 인지하도록 종료.
/// 2. `OPENGUILD_GUILD` env — 테스트 / 명시 지정. 잘못된 값이면 다음 단계로.
/// 3. cwd 부터 부모 방향 탐색 (`.guild` 파일 찾기) — git 방식.
/// 4. cwd 자체 — 최종 fallback. `.guild` 없어도 Store::open 이 빈 길드 부트스트랩.
///
/// argv 가 없는 경우만 2-4 폴백 체인 진입. argv 가 명시되면 해석 결과만 신뢰.
pub(crate) fn resolve_guild_path_inner<I, S>(
    args: I,
    env_guild: Option<String>,
    cwd_search: impl FnOnce() -> Option<PathBuf>,
    cwd: impl FnOnce() -> PathBuf,
) -> Result<PathBuf, String>
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
        return arg_to_guild_path(p).ok_or_else(|| {
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
            return Ok(p);
        }
        // env 가 잘못된 경우는 자동 폴백 (env 는 보통 ambient — 사용자가 직접
        // 지정한 argv 만큼 strict 하게 다룰 필요 없음).
    }

    // 3. cwd 부터 부모 방향
    if let Some(found) = cwd_search() {
        return Ok(found);
    }

    // 4. cwd fallback
    Ok(cwd())
}

/// 외부에서 호출하는 wrapper — 실제 env / cwd 사용.
/// DEV-006 (Recent guild) 진입 시 이 함수보다 위에 "사용자 선택 UI" 가 들어올 예정.
fn resolve_guild_path() -> Result<PathBuf, String> {
    resolve_guild_path_inner(
        std::env::args_os(),
        std::env::var("OPENGUILD_GUILD").ok(),
        openguild_core::guild_file::find_from_cwd,
        || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let guild_path = match resolve_guild_path() {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("[openguild-gui] error: {msg}");
            std::process::exit(2);
        }
    };
    eprintln!("[openguild-gui] guild path: {}", guild_path.display());

    // Store::open 은 async — Tauri 의 async runtime 으로 동기 실행.
    let store = tauri::async_runtime::block_on(Store::open(&guild_path))
        .expect("Store::open 실패 — guild 디렉토리 손상 또는 권한 없음");

    tauri::Builder::default()
        .manage(store)
        .invoke_handler(tauri::generate_handler![
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

    fn fallback_cwd_for_test() -> PathBuf {
        PathBuf::from("/unreachable-cwd-fallback-for-test")
    }
    fn none_search() -> Option<PathBuf> {
        None
    }

    #[test]
    fn resolve_argv_dir_wins_over_env_and_cwd() {
        let dir = fresh_tmp("argv-priority");
        let argv: Vec<std::ffi::OsString> = vec!["program".into(), dir.as_os_str().into()];
        let got = resolve_guild_path_inner(
            argv,
            Some("/nonexistent/path-for-test".into()),
            none_search,
            fallback_cwd_for_test,
        )
        .unwrap();
        assert_eq!(
            std::fs::canonicalize(got).unwrap(),
            std::fs::canonicalize(&dir).unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_argv_guild_file_wins() {
        // .guild 파일 더블클릭 시나리오 — argv[1] 이 파일 경로, 부모가 길드.
        let dir = fresh_tmp("argv-guild-file");
        let guild_file = dir.join("test.guild");
        std::fs::write(&guild_file, "name = \"x\"\nversion = \"1.0\"\ncreated_at = \"\"\n")
            .unwrap();
        let argv: Vec<std::ffi::OsString> =
            vec!["program".into(), guild_file.as_os_str().into()];
        let got =
            resolve_guild_path_inner(argv, None, none_search, fallback_cwd_for_test).unwrap();
        assert_eq!(
            std::fs::canonicalize(got).unwrap(),
            std::fs::canonicalize(&dir).unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_falls_through_to_env_when_argv_empty() {
        let dir = fresh_tmp("env-fallback");
        let argv: Vec<std::ffi::OsString> = vec!["program".into()];
        let got = resolve_guild_path_inner(
            argv,
            Some(dir.to_string_lossy().into_owned()),
            none_search,
            fallback_cwd_for_test,
        )
        .unwrap();
        assert_eq!(got, dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_invalid_argv_errors_no_silent_fallback() {
        // 사용자가 잘못된 path 명시 시 cwd 로 조용히 떨어지지 말고 에러.
        let dir = fresh_tmp("must-not-fall-back");
        let argv: Vec<std::ffi::OsString> =
            vec!["program".into(), "/does/not/exist/argv-test".into()];
        let err = resolve_guild_path_inner(
            argv,
            // env / cwd_search 가 있어도 argv 에러가 이기도록.
            Some(dir.to_string_lossy().into_owned()),
            none_search,
            fallback_cwd_for_test,
        )
        .unwrap_err();
        assert!(err.contains("올바른 guild 경로가 아닙니다"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_argv_non_guild_file_errors() {
        // .txt 등 비-guild 파일도 에러.
        let dir = fresh_tmp("argv-non-guild");
        let txt = dir.join("readme.txt");
        std::fs::write(&txt, "x").unwrap();
        let argv: Vec<std::ffi::OsString> = vec!["program".into(), txt.as_os_str().into()];
        let err =
            resolve_guild_path_inner(argv, None, none_search, fallback_cwd_for_test).unwrap_err();
        assert!(err.contains("올바른 guild 경로가 아닙니다"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_argv_skips_dashdash_flags() {
        // --foo 같은 옵션 플래그는 무시. argv 가 모두 플래그면 폴백.
        let dir = fresh_tmp("flag-skip");
        let dir_for_closure = dir.clone();
        let argv: Vec<std::ffi::OsString> =
            vec!["program".into(), "--no-default-features".into()];
        let got = resolve_guild_path_inner(
            argv,
            None,
            move || Some(dir_for_closure.clone()),
            fallback_cwd_for_test,
        )
        .unwrap();
        assert_eq!(got, dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_skips_invalid_env_falls_to_cwd_search() {
        let dir = fresh_tmp("cwd-search");
        let dir_for_closure = dir.clone();
        let argv: Vec<std::ffi::OsString> = vec!["program".into()];
        let got = resolve_guild_path_inner(
            argv,
            Some("/nonexistent/env".into()),
            move || Some(dir_for_closure.clone()),
            fallback_cwd_for_test,
        )
        .unwrap();
        assert_eq!(got, dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_final_fallback_is_cwd() {
        let argv: Vec<std::ffi::OsString> = vec!["program".into()];
        let got = resolve_guild_path_inner(argv, None, none_search, fallback_cwd_for_test)
            .unwrap();
        assert_eq!(got, PathBuf::from("/unreachable-cwd-fallback-for-test"));
    }
}
