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
    std::fs::write(
        dir.join("plugin.json"),
        serde_json::to_string_pretty(&body).unwrap(),
    )
    .unwrap();
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

    consent::grant(&g, &before.needs_consent[0].def).unwrap();
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
    consent::grant(&g, &l.needs_consent[0].def).unwrap();
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

#[test]
fn trusting_a_guild_skips_the_question() {
    let _guard = env_lock();
    let home = fresh_tmp("trust-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("trust");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));
    consent::trust_guild(&g).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);
    unsafe { std::env::remove_var("OPENGUILD_HOME") };
    let _ = std::fs::remove_dir_all(&g);
    let _ = std::fs::remove_dir_all(&home);
}

/// 깨진 정의 하나가 나머지를 막지 않는다.
#[test]
fn one_broken_definition_does_not_block_the_rest() {
    let _guard = env_lock();
    let home = fresh_tmp("broken-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("broken");
    std::fs::create_dir_all(plugins_dir(&g).join("broken")).unwrap();
    std::fs::write(plugins_dir(&g).join("broken/plugin.json"), "{ not json").unwrap();
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));
    consent::trust_guild(&g).unwrap();

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
        on: vec!["quest.created".into()],
        scope: vec![Scope::Cli],
        action,
        script: None,
    }
}

#[test]
fn scope_must_be_explicit() {
    let mut d = def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
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
    });
    d.on.clear();
    assert!(validate(&d).is_err());
}

/// 오타 난 이벤트 이름은 **조용히 안 도는** 대신 적재 때 걸린다.
#[test]
fn unknown_event_name_is_caught_at_load() {
    let mut d = def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
    });
    d.on = vec!["quest.creted".into()];
    let e = validate(&d).unwrap_err().to_string();
    assert!(e.contains("quest.creted"), "{e}");
    // 와일드카드는 통과해야 한다.
    d.on = vec!["quest.*".into(), "pre:*".into()];
    assert!(validate(&d).is_ok());
}

// ── 비밀값 (admin 확정: 거부) ─────────────────────────────

fn post_with_header(k: &str, v: &str) -> PluginDef {
    let mut h = BTreeMap::new();
    h.insert(k.to_string(), v.to_string());
    def(Action::Post {
        url: "https://x.test".into(),
        headers: h,
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
    });
    assert!(validate(&d).is_err());
    let d2 = def(Action::Run {
        command: "curl".into(),
        args: vec!["-H".into(), "Authorization: ghp_xxxxxxxxxxxx".into()],
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

// ── 적재 → 실행 연결 ────────────────────────────────────

#[derive(Default)]
struct Rec(std::sync::Mutex<Vec<String>>);
impl runtime::Delivery for Rec {
    fn deliver(&self, p: &Plugin, e: &crate::events::Event) {
        self.0
            .lock()
            .unwrap()
            .push(format!("{}:{}", p.def.name, e.name));
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
        consent::trust_guild(&g).unwrap();
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
        consent::trust_guild(&g).unwrap();
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
