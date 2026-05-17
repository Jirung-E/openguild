//! OpenGuild desktop (Tauri v2) — library entry point.
//!
//! `run()` 은 Tauri 앱을 빌드/실행한다. `main.rs` 가 이를 호출.
//! lib.rs 로 분리해두는 이유:
//! - 모바일 / iOS / Android 빌드 시 동일 entry 를 공유할 수 있음 (Tauri v2 권장 구조)
//! - 단위 테스트에서 핸들러를 호출하기 쉬움

mod commands;

use openguild_core::Store;
use std::path::PathBuf;

/// guild 디렉토리 해결.
///
/// 우선순위:
/// 1. `OPENGUILD_GUILD` env (테스트 / 명시 지정)
/// 2. cwd 부터 부모 방향 탐색 (`.guild` 파일 찾기)
/// 3. cwd 자체 — `.guild` 없어도 Store::open 이 알아서 만든다 (빈 길드 부트스트랩).
///
/// DEV-005 (`.guild` 파일 연결) / DEV-006 (Recent guild) 진입 시 OS args 처리 추가 예정.
fn resolve_guild_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("OPENGUILD_GUILD") {
        return PathBuf::from(env_path);
    }
    if let Some(found) = openguild_core::guild_file::find_from_cwd() {
        return found;
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let guild_path = resolve_guild_path();
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
