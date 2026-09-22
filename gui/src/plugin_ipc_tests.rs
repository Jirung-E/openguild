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
            crate::commands::plugin_set_value,
            crate::commands::add_comment,
            crate::commands::toggle_comment_discussion,
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

/// BUG-335 진단: **앱에서 댓글을 달고 토론으로 바꾸면 훅이 실제로 도는가.**
///
/// CLI 로는 되는데 앱에서는 안 된다는 보고를 받았다. 두 길은 같은 `ops` 를 부르지만
/// 그 위(적재 scope, 알림 구현, 드레인 없음)가 다르다. 예제 플러그인을 **그대로**
/// 꽂고 IPC 로만 조작해서, 앱 쪽 배선에 구멍이 있는지 본다.
#[test]
fn a_comment_turned_into_a_discussion_in_the_app_reaches_the_hook() {
    if cfg!(windows) {
        return; // deliver.sh 는 sh 가 필요하다 — 여기서 보는 것은 배선이다.
    }
    let home = tmp("dta-home");
    let guild = tmp("dta-guild");
    let src = tmp("dta-src");
    let _guard = crate::tests::env_lock();
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    openguild_core::repo::seed_guild_dir(&guild).unwrap();
    // 훅이 부르는 `openguild --guild` 가 길드로 인정하려면 마커가 있어야 한다.
    std::fs::write(
        guild.join("t.guild"),
        openguild_core::guild_file::marker_content("t", "2026-01-01"),
    )
    .unwrap();

    // 예제를 그대로 복사한다 — 고쳐서 시험하면 사용자가 쓰는 것과 달라진다.
    let ex = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/plugins/discussion-to-ai");
    let dst = src.join("discussion-to-ai");
    std::fs::create_dir_all(&dst).unwrap();
    for f in ["plugin.toml", "main.rhai", "deliver.sh", "deliver.ps1"] {
        std::fs::copy(ex.join(f), dst.join(f)).unwrap();
    }

    let store = tauri::async_runtime::block_on(Store::open(&guild)).unwrap();
    let handle = store.clone();
    let (_app, w) = app_with(store);

    let path = dst.display().to_string();
    let out = call(&w, "plugin_add_folder", json!({ "path": path })).unwrap();
    assert_eq!(out["kind"], "used", "{out}");
    call(&w, "plugin_allow", json!({ "name": "discussion-to-ai" })).unwrap();
    assert!(handle.events.has_sink(), "허용했는데 안 꽂혔다");

    // 앱의 설정 화면이 하는 것과 같은 호출 — '모든 댓글' · '명령 실행'.
    let saw = guild.join("saw.txt");
    let og = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/debug/openguild");
    assert!(og.exists(), "cargo build -p openguild-cli 먼저: {}", og.display());
    for (k, v) in [
        ("WHAT", json!("all")),
        ("HOW", json!("command")),
        // README 의 처방 그대로 — AI 자리만 `sed` 로 바꾼다(모델을 부를 수는 없다).
        // 핵심은 **훅이 앱이 살아 있는 채로 길드에 답글을 쓸 수 있는가** 다.
        (
            "AI_COMMAND",
            json!(format!(
                "tee -a {saw} | sed 's/^/답: /' | {og} --guild \"$OPENGUILD_GUILD_DIR\" \
                 quest comment add \"$OG_TARGET_ID\" --author ai --parent-id \"$OG_COMMENT_ID\"",
                saw = saw.display(),
                og = og.display(),
            )),
        ),
    ] {
        call(&w, "plugin_set_value", json!({ "name": "discussion-to-ai", "key": k, "value": v })).unwrap();
    }

    // 댓글을 달 자리.
    let q = tauri::async_runtime::block_on(openguild_core::ops::quests::create_quest(
        &handle,
        openguild_core::models::CreateQuestRequest {
            quest_type_id: 1,
            title: "훅 시험".into(),
            description: None,
            status_slug: "open".into(),
            urgency: None,
            parent_quest_id: None,
        },
    ))
    .unwrap();

    // 앱의 흐름: 먼저 평댓글, 그 다음 토론 토글(BUG-333).
    let c = call(&w, "add_comment", json!({ "slug": q.quest_id, "author": "admin", "body": "이거 왜 안 돼?" })).unwrap();
    let id = c["id"].as_u64().unwrap();
    call(&w, "toggle_comment_discussion", json!({ "slug": q.quest_id, "id": id })).unwrap();

    // 전달은 다른 스레드다 — 앱은 드레인을 안 하므로 여기서 기다려 준다.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline && !saw.exists() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // 영수증(delivered.log)을 찾아 그대로 보여 준다 — 실패 이유가 거기 있다.
    fn dump(dir: &std::path::Path) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                dump(&p);
            } else if p.file_name().is_some_and(|n| n == "delivered.log") {
                eprintln!("=== {} ===\n{}", p.display(), std::fs::read_to_string(&p).unwrap_or_default());
            }
        }
    }
    dump(&home);
    let problems = handle.plugin_problems();
    let got = std::fs::read_to_string(&saw).unwrap_or_default();
    eprintln!("=== 훅이 받은 것 ===\n{got}\n=== 문제 ===\n{problems:#?}");
    assert!(got.contains("이거 왜 안 돼?"), "훅에 안 갔다. 문제: {problems:#?}");
    assert_eq!(got.matches("[openguild]").count(), 1, "두 번 갔다:\n{got}");

    // BUG-335 의 본론: 앱이 살아 있는 채로 훅의 자식이 길드에 **쓸 수 있는가**.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut replies = Vec::new();
    while std::time::Instant::now() < deadline {
        replies = openguild_core::services::comments::list_entries(&handle, &q.quest_id).unwrap();
        if replies.len() > 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    eprintln!("=== 댓글 {} 개 ===\n{replies:#?}", replies.len());
    assert!(
        replies.iter().any(|r| r.author == "ai" && r.parent_id == Some(id)),
        "답글이 안 달렸다({} 개). 문제: {:#?}",
        replies.len(),
        handle.plugin_problems()
    );
    // BUG-335: '모든 댓글' 이면 앱의 두 단계(달기 → 토론으로 바꾸기)에 **한 번만** 간다.
    assert_eq!(
        replies.iter().filter(|r| r.author == "ai").count(),
        1,
        "같은 댓글에 답이 여러 번 달렸다:\n{replies:#?}"
    );

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    for d in [&home, &guild, &src] {
        let _ = std::fs::remove_dir_all(d);
    }
}
