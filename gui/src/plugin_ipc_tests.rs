//! DEV-400 / DEV-394: 플러그인 명령을 **실제 IPC 로** 부른다.
//!
//! 프런트 시험은 `invoke` 를 흉내 내고, 코어 시험은 함수를 직접 부른다. 그 사이 —
//! 명령 이름·인자 이름(`source`/`folder`/`path`)·반환 모양·"길드를 안 열었으면 거절" — 은
//! 둘 다 안 본다. 창 없이 tauri 의 mock 런타임으로 앱을 세워 그 구간을 밟는다.

use openguild_core::Store;
use serde_json::{json, Value};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::{ipc::CallbackFn, ipc::InvokeBody, webview::InvokeRequest, WebviewWindow};

fn app_with(store: Store) -> (tauri::App<tauri::test::MockRuntime>, WebviewWindow<tauri::test::MockRuntime>) {
    let app = mock_builder()
        .manage(store)
        .invoke_handler(tauri::generate_handler![
            crate::commands::plugin_status,
            crate::commands::plugin_allow,
            crate::commands::plugin_sources,
            crate::commands::plugin_add_folder,
            crate::commands::plugin_use,
            crate::commands::plugin_stop_using,
            crate::commands::plugin_source_remove,
            crate::commands::plugin_reload,
        ])
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let w = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, w)
}

fn call(w: &WebviewWindow<tauri::test::MockRuntime>, cmd: &str, body: Value) -> Result<Value, Value> {
    get_ipc_response(
        w,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|b| b.deserialize::<Value>().unwrap())
}

fn tmp(label: &str) -> std::path::PathBuf {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!("og-gui-ipc-{label}-{}-{ns}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_plugin(dir: &std::path::Path, name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("plugin.toml"),
        format!(
            "name = \"{name}\"\nscope = [\"gui\"]\n\n[[handlers]]\npost = [\"quest.created\"]\n[handlers.action.post]\nurl = \"https://example.test/hook\"\n"
        ),
    )
    .unwrap();
}

fn names(st: &Value) -> Vec<String> {
    let mut v: Vec<String> = st["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    v.sort();
    v
}

#[test]
fn plugin_source_commands_work_over_ipc() {
    let home = tmp("home");
    let guild = tmp("guild");
    let src = tmp("src");
    // OPENGUILD_HOME 은 프로세스 전역 — 이 크레이트의 다른 시험과 같은 잠금을 쓴다.
    let _guard = crate::tests::env_lock();
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    openguild_core::repo::seed_guild_dir(&guild).unwrap();
    write_plugin(&src.join("one"), "solo");
    write_plugin(&src.join("many/a"), "alpha");
    write_plugin(&src.join("many/b"), "beta");
    let store = tauri::async_runtime::block_on(Store::open(&guild)).unwrap();
    let events = store.clone();
    let (_app, w) = app_with(store);

    assert_eq!(call(&w, "plugin_sources", json!({})).unwrap(), json!([]));

    // 하나짜리 — 바로 쓴다.
    let one = src.join("one").display().to_string();
    let out = call(&w, "plugin_add_folder", json!({ "path": one })).unwrap();
    assert_eq!(out["kind"], "used", "{out}");
    assert_eq!(out["name"], "solo");
    let st = call(&w, "plugin_status", json!({})).unwrap();
    assert_eq!(names(&st), vec!["solo"]);
    let solo = &st["plugins"][0];
    assert_eq!(solo["granted"], false, "더했다고 허용된 셈이 됐다");
    assert!(solo["source"].is_string(), "출처가 안 실렸다: {solo}");
    assert!(!events.events.has_sink(), "허용 전인데 꽂혔다");

    // 허용하면 곧바로 꽂힌다.
    call(&w, "plugin_allow", json!({ "name": "solo" })).unwrap();
    assert!(events.events.has_sink(), "허용했는데 안 꽂혔다");

    // 여럿 — 등록만.
    let many = src.join("many").display().to_string();
    let out = call(&w, "plugin_add_folder", json!({ "path": many })).unwrap();
    assert_eq!(out["kind"], "registered", "{out}");
    assert_eq!(out["plugins"], 2);
    let many_src = out["source"].as_str().unwrap().to_string();
    assert_eq!(names(&call(&w, "plugin_status", json!({})).unwrap()), vec!["solo"]);

    // 소스 목록 — 인자 이름이 프런트와 같은지(`source`, `folder`).
    let sources = call(&w, "plugin_sources", json!({})).unwrap();
    assert_eq!(sources.as_array().unwrap().len(), 2, "{sources}");
    call(&w, "plugin_use", json!({ "source": many_src, "folder": "b" })).unwrap();
    assert_eq!(names(&call(&w, "plugin_status", json!({})).unwrap()), vec!["beta", "solo"]);
    call(&w, "plugin_stop_using", json!({ "source": many_src, "folder": "b" })).unwrap();
    assert_eq!(names(&call(&w, "plugin_status", json!({})).unwrap()), vec!["solo"]);
    let err = call(&w, "plugin_stop_using", json!({ "source": many_src, "folder": "b" })).unwrap_err();
    assert!(err.as_str().is_some_and(|m| !m.is_empty()), "{err}");

    // 다시 읽기 — 디스크에서 지운 정의는 다시 읽은 뒤 빠진다.
    std::fs::remove_file(src.join("one/plugin.toml")).unwrap();
    assert!(events.events.has_sink(), "다시 읽기 전인데 벌써 빠졌다(자동 감지 없음)");
    let st = call(&w, "plugin_reload", json!({})).unwrap();
    assert!(names(&st).is_empty(), "{st}");
    assert!(!events.events.has_sink(), "다시 읽었는데 옛 것이 남았다");

    // 소스 해제.
    call(&w, "plugin_source_remove", json!({ "name": many_src })).unwrap();
    assert_eq!(call(&w, "plugin_sources", json!({})).unwrap().as_array().unwrap().len(), 1);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    for d in [&home, &guild, &src] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// 길드를 안 연 상태(Welcome)에서는 아무것도 안 바꾼다 — 임시 디렉터리에 기록이 남으면 안 된다.
#[test]
fn without_an_open_guild_nothing_is_recorded() {
    let _guard = crate::tests::env_lock();
    let src = tmp("nog-src");
    write_plugin(&src, "solo");
    let store = tauri::async_runtime::block_on(Store::open_in_memory(crate::welcome_placeholder_path())).unwrap();
    let (_app, w) = app_with(store);
    assert_eq!(call(&w, "plugin_sources", json!({})).unwrap(), json!([]));
    let path = src.display().to_string();
    assert!(call(&w, "plugin_add_folder", json!({ "path": path })).is_err());
    assert!(call(&w, "plugin_use", json!({ "source": "x", "folder": "y" })).is_err());
    assert!(call(&w, "plugin_stop_using", json!({ "source": "x", "folder": "y" })).is_err());
    assert!(call(&w, "plugin_source_remove", json!({ "name": "x" })).is_err());
    assert_eq!(call(&w, "plugin_reload", json!({})).unwrap()["no_guild"], true);
    let _ = std::fs::remove_dir_all(&src);
}
