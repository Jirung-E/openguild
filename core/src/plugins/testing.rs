//! DEV-412: `openguild plugin test` — 플러그인의 **시험 스크립트**를 돌린다.
//!
//! ```rhai
//! // main.test.rhai
//! fn test_done_goes_out() {
//!     let r = fire("quest.status_changed",
//!                  #{ quest: #{ id: "DEV-1" }, change: #{ from: "open", to: "done" } });
//!     assert_eq(r.commands.len(), 1);
//!     assert_eq(r.commands[0].action, "telegram");
//!     assert(r.lines.contains("보내기"), "그 줄이 안 불렸다");
//! }
//! ```
//!
//! # 실제와 같은 길로 돈다
//!
//! `fire` 는 줄 고르기·`when`·`with`·순서를 [`super::runtime::run_lines`] 그대로 쓴다. 시험용
//! 실행기를 따로 만들면 **시험은 통과하는데 실제로는 안 도는** 일이 생긴다.
//!
//! 다른 것은 둘뿐이다.
//!
//! - 나가는 것이 없다. 명령은 실행하지 않고 [`Recorder`] 가 적기만 한다.
//! - `with` 는 길드 파일 대신 시험이 넘긴 값을 쓴다 — 길드 없이 돌 수 있어야 한다.
//!
//! # 시험 파일
//!
//! 플러그인 폴더의 `*.test.rhai`. `test_` 로 시작하는 인자 없는 함수가 시험 하나다. 시험 파일은
//! `scripts` 에 적지 않는다 — 적으면 진짜로 도는 코드가 된다.

use super::runtime::{Delivery, Probe, run_lines};
use super::script::{self, CommandKind, Hooks};
use super::{Action, Plugin};
use crate::events::{Event, Phase};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// 시험 파일 이름의 끝.
pub const TEST_SUFFIX: &str = ".test.rhai";
/// 시험 함수의 이름 앞.
pub const TEST_PREFIX: &str = "test_";

/// 나가는 것을 **적기만** 하는 전달. 시험이 진짜로 HTTP 를 쏘면 시험이 아니다.
#[derive(Default)]
pub struct Recorder {
    seen: Mutex<Vec<Value>>,
}

impl Recorder {
    fn take(&self) -> Vec<Value> {
        self.seen.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
    }

    fn push(&self, v: Value) {
        if let Ok(mut s) = self.seen.lock() {
            s.push(v);
        }
    }
}

impl Delivery for Recorder {
    fn deliver(
        &self,
        plugin: &Plugin,
        action: &Action,
        _event: &Event,
        body: &Value,
        _values: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        // 동작의 **이름**을 돌려줘야 시험이 읽기 좋다. 정의에서 같은 것을 찾아 이름을 얻는다.
        let name = plugin
            .def
            .all_actions()
            .into_iter()
            .find(|(_, a)| std::ptr::eq(*a, action))
            .map(|(n, _)| n)
            .unwrap_or_default();
        self.push(json!({
            "kind": match action {
                Action::Post { .. } => "send",
                Action::Run { .. } => "run",
            },
            "action": name,
            "target": target(action),
            "body": body,
        }));
        Ok(())
    }

    fn guild_command(
        &self,
        _plugin: &Plugin,
        kind: CommandKind,
        body: &Value,
        _event: &Event,
    ) -> Result<(), String> {
        self.push(json!({ "kind": kind.verb(), "action": "", "target": "", "body": body }));
        Ok(())
    }
}

fn target(a: &Action) -> String {
    match a {
        Action::Post { url, .. } => url.clone(),
        Action::Run { .. } => a
            .run_command()
            .map(|(c, args)| format!("{c} {}", args.join(" ")).trim_end().to_string())
            .unwrap_or_default(),
    }
}

/// 시험 하나가 `fire` 를 부를 때 돌아가는 것. 엔진에 등록되는 값이라 빌려 쓸 수 없어(엔진은
/// `'static` 을 요구한다) 적재된 플러그인을 통째로 들고 있는다 — 시험 한 번의 일이라 싸다.
struct Fire {
    plugin: Plugin,
    /// 이 시험 하나 동안만 먹는 설정값. 시험마다 비운다 — 앞 시험이 켠 것이 다음 시험에
    /// 남아 있으면 "따로 돌리면 실패하는 시험" 이 된다.
    overrides: Mutex<BTreeMap<String, Value>>,
}

impl Fire {
    fn reset(&self) {
        if let Ok(mut o) = self.overrides.lock() {
            o.clear();
        }
    }
}

impl Hooks for Fire {
    fn fire(&self, name: &str, payload: Value, with: Value) -> Result<Value, String> {
        let over = self.overrides.lock().map(|o| o.clone()).unwrap_or_default();
        fire_with(&self.plugin, name, payload, with, &over)
    }

    fn set_config(&self, key: &str, value: Value) {
        if let Ok(mut o) = self.overrides.lock() {
            o.insert(key.to_string(), value);
        }
    }
}

/// 이벤트 하나를 흘려 보고 무슨 일이 났는지 돌려준다.
pub fn fire(p: &Plugin, name: &str, payload: Value, with: Value) -> Result<Value, String> {
    fire_with(p, name, payload, with, &BTreeMap::new())
}

/// 위와 같되 설정값을 덮어쓴다(`set_config`).
pub fn fire_with(
    p: &Plugin,
    name: &str,
    payload: Value,
    with: Value,
    config: &BTreeMap<String, Value>,
) -> Result<Value, String> {
    let (phase, bare) = match name.strip_prefix("pre:") {
        Some(rest) => (Phase::Pre, rest),
        None => (Phase::Post, name),
    };
    let known: &[&'static str] = match phase {
        Phase::Pre => crate::events::names::PRE_CAPABLE,
        Phase::Post => crate::events::names::ALL,
    };
    let Some(event_name) = known.iter().find(|n| **n == bare) else {
        return Err(format!(
            "`{name}` 은 {} 단계의 이벤트가 아닙니다 — `openguild plugin events` 로 확인하세요",
            if phase == Phase::Pre { "바뀌기 전" } else { "바뀐 뒤" }
        ));
    };
    let data = match payload {
        Value::Object(m) => m,
        Value::Null => serde_json::Map::new(),
        other => return Err(format!("이벤트 값은 표여야 합니다 (받은 것: {other})")),
    };
    let given: BTreeMap<String, Value> = match with {
        Value::Object(m) => m.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    let event = Event {
        name: event_name,
        phase,
        ts: crate::time::now_local_iso8601(),
        guild: p.def.name.clone(),
        ok: Some(true),
        error: None,
        data,
        origin: Default::default(),
    };

    let recorder = Recorder::default();
    let log = Mutex::new(Vec::new());
    let mut probe = Probe { given: Some(&given), ..Default::default() };
    let mut answer = crate::events::PreOutcome::default();
    let mut cfg: BTreeMap<String, Value> = super::values::resolve(&p.guild_root, &p.def)
        .into_iter()
        .filter_map(|(k, r)| r.value.map(|v| (k, v)))
        .collect();
    cfg.extend(config.iter().map(|(k, v)| (k.clone(), v.clone())));
    let subst: BTreeMap<String, String> = BTreeMap::new();
    let want = phase;
    run_lines(
        p,
        &event,
        &cfg,
        &subst,
        &recorder,
        &log,
        (phase == Phase::Pre).then_some(&mut answer),
        Some(&mut probe),
        |h| h.phase() == want,
    );
    let problems = log.lock().map(|l| l.clone()).unwrap_or_default();
    Ok(json!({
        "lines": probe.lines,
        "commands": recorder.take(),
        "blocked": answer.blocked,
        "changes": answer.changes,
        // 조용히 넘어간 실패(권한 없음, 없는 동작 이름, 스크립트가 던짐)도 보여야 한다 —
        // 안 보이면 "왜 명령이 없지" 에서 멈춘다.
        "problems": problems,
    }))
}

/// 시험 하나의 결과.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TestResult {
    pub file: String,
    pub name: String,
    pub failure: Option<String>,
}

/// 한 폴더의 시험을 다 돌린다. 시험 파일이 없으면 빈 목록(오류가 아니다).
pub fn run_tests(p: &Plugin) -> Result<Vec<TestResult>, String> {
    let files = test_files(&p.dir);
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut sources: Vec<(String, String)> = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(p.dir.join(f))
            .map_err(|e| format!("{f} 를 읽지 못했습니다: {e}"))?;
        sources.push((f.clone(), src));
    }
    // 시험 파일은 플러그인 스크립트와 **한 공간**이다 — 도우미 함수를 그대로 부를 수 있어야
    // 시험을 쓸 수 있다.
    for s in &p.def.scripts {
        let src = std::fs::read_to_string(p.dir.join(s))
            .map_err(|e| format!("{s} 를 읽지 못했습니다: {e}"))?;
        sources.push((s.clone(), src));
    }
    let hooks = std::sync::Arc::new(Fire {
        plugin: p.clone(),
        overrides: Mutex::new(BTreeMap::new()),
    });
    let script =
        script::Script::compile_test(&sources, hooks.clone()).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (name, arity) in script.function_names() {
        if arity != 0 || !name.starts_with(TEST_PREFIX) {
            continue;
        }
        let file = files
            .iter()
            .find(|f| {
                std::fs::read_to_string(p.dir.join(f))
                    .map(|s| s.contains(&format!("fn {name}(")))
                    .unwrap_or(false)
            })
            .cloned()
            .unwrap_or_default();
        hooks.reset();
        out.push(TestResult {
            file,
            name: name.clone(),
            failure: script.call_test(&name).err(),
        });
    }
    Ok(out)
}

/// 폴더 안의 시험 파일들(이름순).
pub fn test_files(dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|f| f.ends_with(TEST_SUFFIX))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{MANIFEST, load_all, plugins_dir};

    fn guild(label: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let g = std::env::temp_dir().join(format!("og-test-{label}-{ns}"));
        let dir = plugins_dir(&g).join("p");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            std::fs::write(dir.join(name), body).unwrap();
        }
        g
    }

    const DEF: &str = r#"
name = "p"
scope = ["cli"]
scripts = ["main.rhai"]

[actions.out.post]
url = "https://example.test/hook"

[[handlers]]
id = "보내기"
post = ["quest.status_changed"]
call = "on_status"
[handlers.when]
"change.to" = ["done"]

[[handlers]]
id = "막기"
pre = ["quest.deleted"]
call = "guard"
"#;

    const SCRIPT: &str = r#"
fn on_status(e) { send("out", #{ id: e.quest.id }); }
fn guard(e) { if e.quest.id == "DEV-9" { return "지우지 마세요" } }
"#;

    fn loaded(g: &std::path::Path) -> Plugin {
        let mut l = load_all(g);
        assert!(l.errors.is_empty(), "{:?}", l.errors);
        l.needs_consent.remove(0)
    }

    /// 조건이 걸린 줄은 맞을 때만 불리고, 나간 것은 **적히기만** 한다.
    #[test]
    fn firing_an_event_runs_the_matching_lines_only() {
        let g = guild("fire", &[(MANIFEST, DEF), ("main.rhai", SCRIPT)]);
        let p = loaded(&g);
        let hit = fire(
            &p,
            "quest.status_changed",
            json!({ "quest": { "id": "DEV-1" }, "change": { "from": "open", "to": "done" } }),
            Value::Null,
        )
        .unwrap();
        assert_eq!(hit["lines"], json!(["보내기"]));
        assert_eq!(hit["commands"][0]["action"], "out");
        assert_eq!(hit["commands"][0]["target"], "https://example.test/hook");
        assert_eq!(hit["commands"][0]["body"]["id"], "DEV-1");

        // 조건에 안 맞으면 줄 자체가 안 불린다.
        let miss = fire(
            &p,
            "quest.status_changed",
            json!({ "quest": { "id": "DEV-1" }, "change": { "to": "open" } }),
            Value::Null,
        )
        .unwrap();
        assert_eq!(miss["lines"], json!([]));
        assert_eq!(miss["commands"], json!([]));
        let _ = std::fs::remove_dir_all(&g);
    }

    /// 바뀌기 전 줄은 막은 이유를 돌려준다.
    #[test]
    fn a_pre_line_reports_what_it_blocked() {
        let g = guild("block", &[(MANIFEST, DEF), ("main.rhai", SCRIPT)]);
        let p = loaded(&g);
        let r = fire(&p, "pre:quest.deleted", json!({ "quest": { "id": "DEV-9" } }), Value::Null)
            .unwrap();
        assert_eq!(r["blocked"], "지우지 마세요");
        let ok = fire(&p, "pre:quest.deleted", json!({ "quest": { "id": "DEV-1" } }), Value::Null)
            .unwrap();
        assert_eq!(ok["blocked"], Value::Null);
        let _ = std::fs::remove_dir_all(&g);
    }

    #[test]
    fn an_unknown_event_name_is_an_error_not_silence() {
        let g = guild("unknown", &[(MANIFEST, DEF), ("main.rhai", SCRIPT)]);
        let p = loaded(&g);
        let e = fire(&p, "quest.crated", json!({}), Value::Null).unwrap_err();
        assert!(e.contains("quest.crated"), "{e}");
        // 바뀌기 전 단계에 없는 이름도 그 자리에서 말해 준다.
        let e2 = fire(&p, "pre:backup.created", json!({}), Value::Null).unwrap_err();
        assert!(e2.contains("바뀌기 전"), "{e2}");
        let _ = std::fs::remove_dir_all(&g);
    }

    /// 시험 파일이 도는지 — 통과와 실패가 둘 다 제대로 잡혀야 한다.
    #[test]
    fn test_files_run_and_failures_say_why() {
        let test_src = r#"
fn test_passes() {
    let r = fire("quest.status_changed",
                 #{ quest: #{ id: "DEV-1" }, change: #{ to: "done" } });
    assert_eq(r.commands.len(), 1);
    assert(r.lines.len() == 1, "줄이 안 불렸다");
}
fn test_fails() {
    let r = fire("quest.status_changed", #{ quest: #{ id: "DEV-1" }, change: #{ to: "open" } });
    assert_eq(r.commands.len(), 1);
}
"#;
        let g = guild(
            "run",
            &[(MANIFEST, DEF), ("main.rhai", SCRIPT), ("main.test.rhai", test_src)],
        );
        let p = loaded(&g);
        let out = run_tests(&p).unwrap();
        assert_eq!(out.len(), 2, "{out:?}");
        let passed = out.iter().find(|t| t.name == "test_passes").unwrap();
        assert!(passed.failure.is_none(), "{passed:?}");
        let failed = out.iter().find(|t| t.name == "test_fails").unwrap();
        let why = failed.failure.clone().unwrap();
        assert!(why.contains("assert_eq"), "{why}");
        // 무엇이 달랐는지 보여야 한다.
        assert!(why.contains('0') && why.contains('1'), "{why}");
        let _ = std::fs::remove_dir_all(&g);
    }

    /// `set_config` 는 **그 시험 하나**에만 먹는다 — 앞 시험이 켠 것이 남으면 따로 돌릴 때만
    /// 실패하는 시험이 된다.
    #[test]
    fn a_config_override_does_not_leak_into_the_next_test() {
        let def = r#"
name = "p"
scope = ["cli"]
scripts = ["main.rhai"]

[actions.out.post]
url = "https://example.test/hook"

[[handlers]]
post = ["quest.created"]
call = "on_created"

[[inputs]]
key = "ON"
type = "checkbox"
default = false
"#;
        let script = r#"fn on_created(e) { if config("ON") { send("out", 1) } }"#;
        // 이름 순서상 `test_a_...` 가 먼저 돈다 — 그게 켠 값이 뒤 시험에 남으면 안 된다.
        let tests = r#"
fn test_a_turns_it_on() {
    set_config("ON", true);
    assert_eq(fire("quest.created", #{ quest: #{ id: "DEV-1" } }).commands.len(), 1);
}
fn test_b_sees_the_default() {
    assert_eq(fire("quest.created", #{ quest: #{ id: "DEV-1" } }).commands.len(), 0);
}
"#;
        let g = guild(
            "leak",
            &[(MANIFEST, def), ("main.rhai", script), ("main.test.rhai", tests)],
        );
        let p = loaded(&g);
        let out = run_tests(&p).unwrap();
        assert_eq!(out.len(), 2);
        for t in &out {
            assert!(t.failure.is_none(), "{t:?}");
        }
        let _ = std::fs::remove_dir_all(&g);
    }

    /// 시험 파일이 없으면 오류가 아니라 "없음" 이다.
    #[test]
    fn no_test_file_is_not_a_failure() {
        let g = guild("none", &[(MANIFEST, DEF), ("main.rhai", SCRIPT)]);
        let p = loaded(&g);
        assert!(run_tests(&p).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&g);
    }
}
