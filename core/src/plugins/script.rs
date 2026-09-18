//! DEV-376: 플러그인의 **스크립트 계층**. rhai 함수가 "이 이벤트에서 무엇을 할지" 를 정한다.
//!
//! # 왜 스크립트가 필요한가
//!
//! 정의 파일만으로는 "미해결 토론 댓글만 AI 에 보낸다" 를 못 적는다. 여기서 조건
//! 문법을 발명하기 시작하면 결국 DSL 을 만들게 된다.
//!
//! # 핵심 제약 — 스크립트는 직접 밖으로 못 나간다
//!
//! ```text
//! rhai   판단하고, 할 일을 "적는다". 파일도 네트워크도 없다.
//! 코어   적힌 일을 함수가 끝난 뒤에 실행한다(post/run).  ← [[DEV-377]]
//! ```
//!
//! 스크립트가 부르는 `send("이름", 본문)` / `run("이름", 입력)` 은 **그 자리에서 아무것도
//! 하지 않는다** — 할 일 목록에 적을 뿐이다([[DEV-403]]). 어디로 보내고 무엇을 띄우는지는
//! 정의 파일의 `[actions]` 에만 있고, 스크립트는 이름만 안다. 그래서 스크립트가 폭주해도 밖으로
//! 못 나가고, 허용 화면은 정의 파일만 보고 "어디로 나가나" 를 다 보여 줄 수 있다.
//!
//! 샌드박스는 두 겹이다. `no_module` 로 `import` 자체를 컴파일에서 없애고(기본
//! 모듈 해석기는 **디스크를 읽는다**), 엔진에는 우리가 등록한 함수 외에는
//! 아무것도 없다 — 파일도 프로세스도 소켓도 이름이 없다.
//!
//! # 계약
//!
//! ```rhai
//! fn 댓글_알림(e) {
//!     if e.comment.discussion {
//!         send("ai", #{ text: `[${e.quest.id}] ${e.comment.body}` });
//!     }
//! }
//! ```
//!
//! 함수 이름은 자유이고, 정의 파일의 `call` 이 가리킨다. 인자는 이벤트 하나. 반환값은 쓰지
//! 않는다(바뀌기 전 단계의 막기·값 바꾸기는 [[DEV-407]]).

use crate::error::{AppError, AppResult};
use crate::events::Event;
use rhai::{AST, Dynamic, Engine, Scope};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 연산 상한. **안 걸면 스크립트 하나가 CLI 를 멈춘다.** 넉넉하되 무한 루프는
/// 초 단위 안에 잡히는 값.
const MAX_OPERATIONS: u64 = 500_000;
/// 벽시계 상한. 연산 상한만으로 충분해야 정상이지만(등록한 함수가 없으므로
/// 연산 수가 곧 시간이다), 상한을 하나만 두면 그 하나가 틀렸을 때 막을 게
/// 없다.
const MAX_WALL_TIME: Duration = Duration::from_secs(2);
/// 한 번 부를 때 적을 수 있는 일의 수 — 반복문 하나가 보내기를 만 번 쌓지 못하게.
pub const MAX_COMMANDS: usize = 32;

/// 스크립트가 적은 할 일 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub kind: CommandKind,
    /// `[actions]` 의 이름.
    pub action: String,
    /// `post` 면 본문, `run` 이면 stdin 으로 넘길 값.
    pub body: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    /// `send(이름, 본문)` — `post` 동작.
    Send,
    /// `run(이름, 입력)` — `run` 동작.
    Run,
    /// DEV-406: `notify(글)` — 이 기계에 알린다(앱은 알림, CLI 는 stderr, 서버는 로그).
    Notify,
    /// DEV-406: `backup()` — 길드 백업을 만든다.
    Backup,
}

impl CommandKind {
    pub fn verb(self) -> &'static str {
        match self {
            CommandKind::Send => "send",
            CommandKind::Run => "run",
            CommandKind::Notify => "notify",
            CommandKind::Backup => "backup",
        }
    }

    /// DEV-406: 길드에 일을 시키는 명령인가 — 정의가 권한으로 밝혀야 하는 것들.
    /// `send`/`run` 은 어디로 나가는지가 `[actions]` 에 다 보이므로 따로 안 밝힌다.
    pub fn needs_permission(self) -> bool {
        matches!(self, CommandKind::Notify | CommandKind::Backup)
    }
}

/// 컴파일된 스크립트(한 플러그인의 파일 전부). 적재 때 한 번 컴파일하고 이벤트마다 재사용한다 —
/// 문법 오류는 **적재 때** 드러나야지 첫 이벤트 때 드러나면 안 된다.
pub struct Script {
    engine: Engine,
    ast: AST,
    /// 지금 돌고 있는 호출의 마감. `on_progress` 가 읽는다.
    deadline: std::sync::Arc<Mutex<Instant>>,
    /// REQ-021: 이번 호출이 볼 설정값. `config(key)` 가 읽는다.
    ///
    /// 엔진은 컴파일 때 한 번 만들고 호출마다 값이 달라지므로(길드마다 다른
    /// 값을 쓴다) `deadline` 과 같은 방식으로 셀 하나를 공유한다.
    config: std::sync::Arc<Mutex<rhai::Map>>,
    /// 이번 호출에서 적힌 할 일.
    commands: std::sync::Arc<Mutex<Vec<Command>>>,
}

impl std::fmt::Debug for Script {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Script")
    }
}

impl Script {
    pub fn compile(path: &Path) -> AppResult<Self> {
        let src = std::fs::read_to_string(path).map_err(|e| {
            AppError::BadRequest(format!(
                "스크립트 {} 를 읽지 못했습니다: {e}",
                path.display()
            ))
        })?;
        Self::compile_source(&src)
    }

    pub fn compile_source(src: &str) -> AppResult<Self> {
        Self::compile_sources(&[("script".to_string(), src.to_string())])
    }

    /// 여러 파일을 **한 공간으로** 컴파일한다 — 서로 이름만으로 부를 수 있다. 같은 이름·같은
    /// 인자 수의 함수가 두 파일에 있으면 거절한다(나중 것이 조용히 덮으면 어느 쪽이 도는지
    /// 모른다).
    pub fn compile_sources(sources: &[(String, String)]) -> AppResult<Self> {
        let deadline = std::sync::Arc::new(Mutex::new(Instant::now()));
        let config = std::sync::Arc::new(Mutex::new(rhai::Map::new()));
        let commands = std::sync::Arc::new(Mutex::new(Vec::new()));
        let engine = sandboxed_engine(deadline.clone(), config.clone(), commands.clone());
        let mut merged: Option<AST> = None;
        let mut seen: std::collections::HashMap<(String, usize), String> = Default::default();
        for (name, src) in sources {
            let ast = engine.compile(src).map_err(|e| {
                AppError::BadRequest(format!("스크립트 {name} 를 컴파일하지 못했습니다: {e}"))
            })?;
            for f in ast.iter_functions() {
                let key = (f.name.to_string(), f.params.len());
                if let Some(prev) = seen.insert(key, name.clone()) {
                    return Err(AppError::BadRequest(format!(
                        "함수 `{}` 가 {prev} 와 {name} 에 둘 다 있습니다 — 이름을 나누세요",
                        f.name
                    )));
                }
            }
            merged = Some(match merged {
                None => ast,
                Some(m) => m.merge(&ast),
            });
        }
        Ok(Self {
            engine,
            ast: merged.unwrap_or_default(),
            deadline,
            config,
            commands,
        })
    }

    /// 핸들러로 부를 수 있는 함수(인자 하나)가 있나.
    pub fn has_handler(&self, name: &str) -> bool {
        self.has_function(name, 1)
    }

    /// 그 이름·인자 수의 함수가 있나.
    pub fn has_function(&self, name: &str, arity: usize) -> bool {
        self.ast
            .iter_functions()
            .any(|f| f.name == name && f.params.len() == arity)
    }

    /// 핸들러 함수를 부르고, 그 함수가 적은 할 일을 돌려준다. 스크립트가 던지면 **그 줄만**
    /// 실패한다 — 길드 동작에도 다른 플러그인에도 영향이 없어야 한다.
    pub fn call_handler(
        &self,
        func: &str,
        event: &Event,
        config: &std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<Vec<Command>, String> {
        self.call_handler_with(func, event, &[], config)
    }

    /// DEV-405: 이벤트 뒤에 연결 데이터(`with`)를 차례로 넘긴다.
    pub fn call_handler_with(
        &self,
        func: &str,
        event: &Event,
        extra: &[Value],
        config: &std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<Vec<Command>, String> {
        // REQ-021: 이번 호출이 볼 설정값. **I/O 가 아니다** — 코어가 미리 읽어
        // 넘겨주는 값이라 샌드박스(파일·네트워크 없음)는 그대로다.
        if let Ok(mut c) = self.config.lock() {
            *c = config
                .iter()
                .map(|(k, v)| (k.clone().into(), to_dynamic(v)))
                .collect();
        }
        if let Ok(mut c) = self.commands.lock() {
            c.clear();
        }
        let args: Vec<Dynamic> = std::iter::once(to_dynamic(&event.to_json()))
            .chain(extra.iter().map(to_dynamic))
            .collect();
        self.arm();
        let mut scope = Scope::new();
        let result = self
            .engine
            .call_fn::<Dynamic>(&mut scope, &self.ast, func, args)
            .map_err(|e| format!("{func}: {e}"));
        // 던졌으면 적어 둔 일도 버린다 — 반쯤 돈 함수의 일을 반만 실행하지 않는다.
        let cmds = self
            .commands
            .lock()
            .map(|mut c| std::mem::take(&mut *c))
            .unwrap_or_default();
        result.map(|_| cmds)
    }

    fn arm(&self) {
        if let Ok(mut d) = self.deadline.lock() {
            *d = Instant::now() + MAX_WALL_TIME;
        }
    }
}

/// 할 일 하나를 목록에 적는다 — 상한을 넘으면 그 자리에서 오류.
fn push(
    cmds: &Mutex<Vec<Command>>,
    kind: CommandKind,
    action: String,
    body: Value,
) -> Result<(), Box<rhai::EvalAltResult>> {
    let mut list = cmds
        .lock()
        .map_err(|_| -> Box<rhai::EvalAltResult> { "할 일 목록을 잠그지 못했습니다".into() })?;
    if list.len() >= MAX_COMMANDS {
        return Err(format!("한 번에 적을 수 있는 일은 {MAX_COMMANDS}개까지입니다").into());
    }
    list.push(Command { kind, action, body });
    Ok(())
}

/// 아무것도 등록하지 않은 엔진 + 상한. 등록하지 않은 것이 이 함수의 내용이다.
fn sandboxed_engine(
    deadline: std::sync::Arc<Mutex<Instant>>,
    config: std::sync::Arc<Mutex<rhai::Map>>,
    commands: std::sync::Arc<Mutex<Vec<Command>>>,
) -> Engine {
    let mut e = Engine::new();
    // DEV-406: 길드에 시키는 일 — 이름만 받거나(알림) 인자가 없다(백업).
    {
        let cmds = commands.clone();
        e.register_fn(
            "notify",
            move |text: &str| -> Result<(), Box<rhai::EvalAltResult>> {
                push(&cmds, CommandKind::Notify, String::new(), Value::String(text.to_string()))
            },
        );
        let cmds = commands.clone();
        e.register_fn("backup", move || -> Result<(), Box<rhai::EvalAltResult>> {
            push(&cmds, CommandKind::Backup, String::new(), Value::Null)
        });
    }
    // DEV-403: 할 일을 **적기만** 한다. 실행은 함수가 끝난 뒤 코어가 한다.
    for kind in [CommandKind::Send, CommandKind::Run] {
        let cmds = commands.clone();
        e.register_fn(
            kind.verb(),
            move |action: &str, body: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
                let body = from_dynamic(&body).map_err(|m| -> Box<rhai::EvalAltResult> {
                    format!("{}(\"{action}\"): {m}", kind.verb()).into()
                })?;
                push(&cmds, kind, action.to_string(), body)
            },
        );
    }
    // REQ-021: 사용자가 설정 화면에서 넣은 값. 이것 하나가 체크박스·선택상자가
    // **동작을 바꾸게** 하는 경로다 — 값을 못 읽으면 위젯은 url 에 박히는 것
    // 말고 할 일이 없다.
    //
    // 등록하는 것이 값을 **읽는 것뿐**이라는 점이 중요하다. 파일도 네트워크도
    // 아니고 코어가 미리 읽어 넘긴 맵이라, 등록하지 않은 것이 이 엔진의 내용
    // 이라는 성질은 그대로다.
    e.register_fn("config", move |key: &str| -> Dynamic {
        config
            .lock()
            .ok()
            .and_then(|c| c.get(key).cloned())
            .unwrap_or(Dynamic::UNIT)
    });
    e.set_max_operations(MAX_OPERATIONS);
    e.set_max_call_levels(64);
    e.set_max_expr_depths(64, 32);
    e.set_max_string_size(256 * 1024);
    e.set_max_array_size(8192);
    e.set_max_map_size(8192);
    // 스크립트가 stdout 을 오염시키면 CLI 출력이 깨진다(파이프로 쓰는 사람이
    // 있다). 삼키지는 않고 갈 곳만 막는다 — 진단은 반환값과 오류로 한다.
    e.on_print(|_| {});
    e.on_debug(|_, _, _| {});
    e.on_progress(move |ops| {
        // 매 연산마다 시계를 보면 그 자체가 비용이다.
        if ops % 4096 != 0 {
            return None;
        }
        match deadline.lock() {
            Ok(d) if Instant::now() > *d => Some(Dynamic::UNIT),
            _ => None,
        }
    });
    e
}

// ── JSON ↔ rhai ─────────────────────────────────────────
//
// rhai 의 `serde` 기능을 켜는 대신 손으로 옮긴다. 다루는 모양이 이벤트 JSON
// 하나뿐이라 40줄이면 끝나고, **무엇이 넘어가고 무엇이 안 넘어가는지가
// 코드에 그대로 보인다.**

fn to_dynamic(v: &Value) -> Dynamic {
    match v {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64().unwrap_or(0.0).into(),
        },
        Value::String(s) => s.clone().into(),
        Value::Array(a) => a.iter().map(to_dynamic).collect::<rhai::Array>().into(),
        Value::Object(m) => {
            let mut map = rhai::Map::new();
            for (k, v) in m {
                map.insert(k.as_str().into(), to_dynamic(v));
            }
            map.into()
        }
    }
}

fn from_dynamic(d: &Dynamic) -> Result<Value, String> {
    if d.is_unit() {
        return Ok(Value::Null);
    }
    if let Ok(b) = d.as_bool() {
        return Ok(Value::Bool(b));
    }
    if let Ok(i) = d.as_int() {
        return Ok(Value::from(i));
    }
    if let Ok(f) = d.as_float() {
        return Ok(serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number));
    }
    if d.is_string() {
        return Ok(Value::String(d.clone().into_string().unwrap_or_default()));
    }
    if d.is_array() {
        let a = d.clone().into_array().map_err(|t| t.to_string())?;
        return Ok(Value::Array(
            a.iter().map(from_dynamic).collect::<Result<_, _>>()?,
        ));
    }
    if d.is_map() {
        let m = d
            .clone()
            .try_cast::<rhai::Map>()
            .ok_or_else(|| "map 변환 실패".to_string())?;
        let mut out = serde_json::Map::new();
        for (k, v) in m {
            out.insert(k.to_string(), from_dynamic(&v)?);
        }
        return Ok(Value::Object(out));
    }
    Err(format!("보낼 수 없는 값입니다: {}", d.type_name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Phase;
    use serde_json::json;

    fn ev() -> Event {
        let mut data = serde_json::Map::new();
        data.insert(
            "quest".into(),
            json!({ "id": "DEV-1", "title": "훅", "status": "open" }),
        );
        data.insert(
            "comment".into(),
            json!({ "id": 7, "author": "kim", "body": "확인 바람", "discussion": true }),
        );
        Event {
            name: "comment.added",
            phase: Phase::Post,
            ts: "2026-09-07T00:00:00+09:00".into(),
            guild: "g".into(),
            ok: Some(true),
            error: None,
            data,
            origin: Default::default(),
        }
    }

    fn call(src: &str) -> Result<Vec<Command>, String> {
        Script::compile_source(src)
            .unwrap()
            .call_handler("h", &ev(), &Default::default())
    }

    /// 아무것도 적지 않으면 할 일이 없다.
    #[test]
    fn a_handler_that_does_nothing_returns_no_commands() {
        assert!(call("fn h(e) { }").unwrap().is_empty());
    }

    /// **적은 일이 적은 순서대로, 적은 모양 그대로** 돌아온다 — 실행은 하지 않는다.
    #[test]
    fn commands_come_back_in_order_with_their_bodies() {
        let got = call(
            r#"
            fn h(e) {
                send("ai", #{ text: `[${e.quest.id}] ${e.comment.author}: ${e.comment.body}`,
                              nested: #{ n: 3, flag: true, list: [1, "둘"] } });
                run("archive", e.quest.id);
            }
        "#,
        )
        .unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].kind, got[0].action.as_str()), (CommandKind::Send, "ai"));
        assert_eq!(got[0].body["text"], "[DEV-1] kim: 확인 바람");
        assert_eq!(got[0].body["nested"]["list"][1], "둘");
        assert_eq!((got[1].kind, got[1].action.as_str()), (CommandKind::Run, "archive"));
        assert_eq!(got[1].body, json!("DEV-1"));
    }

    /// 이벤트에 실린 것으로 판단한다 — `ok`/`phase`/토론 여부가 전부 스크립트 몫이다([[DEV-374]]).
    #[test]
    fn judgement_uses_what_the_event_carries() {
        let src = r#"
            fn h(e) {
                if e.event == "comment.added" && e.ok && e.comment.discussion { send("ai", e) }
                if e.phase == "pre" { send("never", e) }
            }
        "#;
        let got = call(src).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body["comment"]["id"], 7);
    }

    /// DEV-401: 스크립트가 "누가 일으켰나" 를 본다 — 사람이 한 것만 고를 수 있다.
    #[test]
    fn a_script_can_tell_who_caused_the_event() {
        let sc = Script::compile_source(
            r#"fn h(e) { if e.origin.by == "user" { send("x", 1) } }"#,
        )
        .unwrap();
        assert_eq!(sc.call_handler("h", &ev(), &Default::default()).unwrap().len(), 1);
        let mut by_hook = ev();
        by_hook.origin = crate::events::origin::Origin::from_chain(["other"]);
        assert!(sc.call_handler("h", &by_hook, &Default::default()).unwrap().is_empty());
    }

    /// 호출마다 목록이 새로 시작한다 — 앞 호출의 일이 뒤로 새지 않는다.
    #[test]
    fn each_call_starts_with_an_empty_list() {
        let sc = Script::compile_source(r#"fn h(e) { send("x", 1) }"#).unwrap();
        for _ in 0..3 {
            assert_eq!(sc.call_handler("h", &ev(), &Default::default()).unwrap().len(), 1);
        }
    }

    /// 던지면 **적어 둔 일도 버린다** — 반쯤 돈 함수의 일을 반만 실행하지 않는다.
    #[test]
    fn a_throwing_handler_drops_what_it_wrote() {
        let sc = Script::compile_source(r#"fn h(e) { send("x", 1); throw "안 돼" }"#).unwrap();
        let e = sc.call_handler("h", &ev(), &Default::default()).unwrap_err();
        assert!(e.contains("안 돼"), "{e}");
        let ok = Script::compile_source(r#"fn h(e) { }"#).unwrap();
        assert!(ok.call_handler("h", &ev(), &Default::default()).unwrap().is_empty());
    }

    /// 반복문 하나가 보내기를 끝없이 쌓지 못한다.
    #[test]
    fn a_handler_cannot_queue_unbounded_work() {
        let e = call(r#"fn h(e) { for i in 0..1000 { send("x", i) } }"#).unwrap_err();
        assert!(e.contains(&MAX_COMMANDS.to_string()), "{e}");
    }

    /// **무한 루프가 상한에 걸려 멈춘다.** 이게 안 되면 넣으면 안 된다.
    #[test]
    fn a_runaway_loop_is_stopped() {
        let t = Instant::now();
        let e = call("fn h(e) { let i = 0; loop { i += 1; } }").unwrap_err();
        assert!(
            t.elapsed() < Duration::from_secs(10),
            "상한에 안 걸리고 계속 돌았다"
        );
        assert!(
            e.contains("operation") || e.contains("Operation") || e.contains("Terminated"),
            "무엇에 걸렸는지 안 보인다: {e}"
        );
    }

    /// 여러 파일은 한 공간이다 — 서로 이름만으로 부른다. 같은 함수가 두 파일에 있으면 거절.
    #[test]
    fn several_files_share_one_namespace_and_refuse_duplicates() {
        let sc = Script::compile_sources(&[
            ("main.rhai".into(), r#"fn h(e) { send("x", title(e)) }"#.into()),
            ("fmt.rhai".into(), r#"fn title(e) { "제목: " + e.quest.title }"#.into()),
        ])
        .unwrap();
        let got = sc.call_handler("h", &ev(), &Default::default()).unwrap();
        assert_eq!(got[0].body, json!("제목: 훅"));
        assert!(sc.has_handler("h"));
        assert!(!sc.has_handler("title_missing"));

        let e = Script::compile_sources(&[
            ("a.rhai".into(), "fn h(e) { }".into()),
            ("b.rhai".into(), "fn h(e) { }".into()),
        ])
        .unwrap_err();
        assert!(e.to_string().contains("a.rhai") && e.to_string().contains("b.rhai"), "{e}");
    }

    // ── 샌드박스 ────────────────────────────────────────
    //
    // 이 설계의 전부가 "밖으로 못 나간다" 이므로, 못 나간다는 것을 시험이
    // 붙들고 있어야 한다.

    /// `import` 는 **컴파일부터** 안 된다. 기본 모듈 해석기는 디스크를 읽는다 —
    /// `no_module` 이 그 경로 자체를 없앤다.
    #[test]
    fn import_does_not_even_compile() {
        let e = Script::compile_source(r#"import "std" as s; fn h(e) { }"#).unwrap_err();
        assert!(e.to_string().contains("컴파일"), "{e}");
    }

    /// 파일·프로세스·네트워크는 **이름조차 없다.** 등록한 것은 설정 읽기와 할 일 적기뿐이다.
    #[test]
    fn there_is_no_way_to_touch_the_outside() {
        for src in [
            r#"fn h(e) { open_file("/etc/passwd") }"#,
            r#"fn h(e) { read_file("/etc/passwd") }"#,
            r#"fn h(e) { File("/etc/passwd") }"#,
            r#"fn h(e) { system("ls") }"#,
            r#"fn h(e) { exec("sh") }"#,
            r#"fn h(e) { http_get("https://example.test") }"#,
            r#"fn h(e) { fetch("https://example.test") }"#,
        ] {
            // 컴파일에서 막히든 실행에서 막히든 **못 나가면** 된다.
            let blocked = match Script::compile_source(src) {
                Err(_) => true,
                Ok(sc) => sc.call_handler("h", &ev(), &Default::default()).is_err(),
            };
            assert!(blocked, "밖으로 나가는 길이 열려 있다: {src}");
        }
    }

    /// 등록한 게 없다는 것을 반대편에서도 확인한다 — 순수 계산은 된다.
    #[test]
    fn pure_computation_still_works() {
        let got = call(r#"fn h(e) { let n = 0; for i in 0..10 { n += i } send("x", #{ sum: n }) }"#)
            .unwrap();
        assert_eq!(got[0].body["sum"], 45);
    }
}
