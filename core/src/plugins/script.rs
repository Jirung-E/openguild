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
//! 샌드박스는 두 겹이다. 엔진에는 우리가 등록한 함수 외에는 아무것도 없고 — 파일도 프로세스도
//! 소켓도 이름이 없다 — `import` 는 **메모리 안에서만** 풀린다([[DEV-408]]). 코어가 적재 때
//! 파일을 읽어 넘기므로, 엔진 자신은 디스크를 모른다(기본 해석기는 디스크를 읽는다).
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
    /// DEV-409: 이름을 읽어 줄 길드. 적재 때 정한다([`Script::set_guild`]).
    guild: std::sync::Arc<Mutex<super::info::GuildInfo>>,
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
        Self::compile_with_imports(sources, &[])
    }

    /// DEV-412: 시험 스크립트용 — 위와 같지만 `fire`/`assert` 가 더 있다. 시험 파일에만 준다.
    /// 플러그인 스크립트 자신은 이 함수들을 못 본다(시험이 대상의 능력을 늘리면 안 된다).
    pub fn compile_test(
        sources: &[(String, String)],
        hooks: std::sync::Arc<dyn Hooks>,
    ) -> AppResult<Self> {
        Self::compile_inner(sources, &[], Some(hooks))
    }

    /// DEV-408: 불러올 파일(`import "경로" as 이름`)을 **메모리 모듈**로 함께 넣는다.
    /// `imports` 는 스크립트에 적힌 경로 그대로와 그 원문 — 순서는 상관없다(서로 불러도 된다).
    pub fn compile_with_imports(
        sources: &[(String, String)],
        imports: &[(String, String)],
    ) -> AppResult<Self> {
        Self::compile_inner(sources, imports, None)
    }

    fn compile_inner(
        sources: &[(String, String)],
        imports: &[(String, String)],
        hooks: Option<std::sync::Arc<dyn Hooks>>,
    ) -> AppResult<Self> {
        let deadline = std::sync::Arc::new(Mutex::new(Instant::now()));
        let config = std::sync::Arc::new(Mutex::new(rhai::Map::new()));
        let commands = std::sync::Arc::new(Mutex::new(Vec::new()));
        let guild = std::sync::Arc::new(Mutex::new(super::info::GuildInfo::default()));
        let mut engine = sandboxed_engine(
            deadline.clone(),
            config.clone(),
            commands.clone(),
            guild.clone(),
        );
        if let Some(h) = hooks {
            register_test(&mut engine, h);
        }
        // 불러올 것을 먼저 모듈로 만든다. 서로 부르는 경우가 있으므로 될 때까지 되풀이한다.
        let mut resolver = rhai::module_resolvers::StaticModuleResolver::new();
        let mut left: Vec<&(String, String)> = imports.iter().collect();
        while !left.is_empty() {
            engine.set_module_resolver(resolver.clone());
            let before = left.len();
            let mut failed: Vec<&(String, String)> = Vec::new();
            let mut last_err = String::new();
            for item in left {
                let (path, src) = item;
                let built = engine
                    .compile(src)
                    .map_err(|e| e.to_string())
                    .and_then(|ast| {
                        rhai::Module::eval_ast_as_new(rhai::Scope::new(), &ast, &engine)
                            .map_err(|e| e.to_string())
                    });
                match built {
                    Ok(m) => {
                        resolver.insert(path.clone(), m);
                    }
                    Err(e) => {
                        last_err = format!("{path}: {e}");
                        failed.push(item);
                    }
                }
            }
            if failed.len() == before {
                return Err(AppError::BadRequest(format!(
                    "불러올 스크립트를 준비하지 못했습니다 — {last_err}"
                )));
            }
            left = failed;
        }
        engine.set_module_resolver(resolver);
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
            guild,
        })
    }

    /// DEV-409: 이름·링크를 어느 길드에서 읽을지. 적재 때 한 번 정한다 — 이걸 안 부르면
    /// `status_name()` 은 슬러그를 그대로 돌려준다(빈칸이 되지는 않는다).
    pub fn set_guild(&self, root: &Path) {
        if let Ok(mut g) = self.guild.lock() {
            g.set_root(root);
        }
    }

    /// DEV-412: 인자 없는 함수 하나를 부른다 — 시험 하나. 던지면 그 시험이 실패다.
    pub fn call_test(&self, func: &str) -> Result<(), String> {
        self.arm();
        let mut scope = Scope::new();
        self.engine
            .call_fn::<Dynamic>(&mut scope, &self.ast, func, ())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// 핸들러로 부를 수 있는 함수(인자 하나)가 있나.
    pub fn has_handler(&self, name: &str) -> bool {
        self.has_function(name, 1)
    }

    /// DEV-411: 이 스크립트가 가진 함수들(이름, 인자 수) — `plugin check` 가 "아무 줄도 안
    /// 부르는 함수" 를 짚을 때 쓴다.
    pub fn function_names(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = self
            .ast
            .iter_functions()
            .map(|f| (f.name.to_string(), f.params.len()))
            .collect();
        out.sort();
        out
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
        self.call_handler_with(func, event, &[], config).map(|(_, c)| c)
    }

    /// DEV-405: 이벤트 뒤에 연결 데이터(`with`)를 차례로 넘긴다.
    /// 돌려주는 것은 **함수의 반환값과 적어 둔 할 일**이다. 반환값은 바뀌기 전 단계에서 쓴다
    /// (막을 이유 또는 바꿀 칸 — [[DEV-407]]).
    pub fn call_handler_with(
        &self,
        func: &str,
        event: &Event,
        extra: &[Value],
        config: &std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<(Value, Vec<Command>), String> {
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
        result.and_then(|v| from_dynamic(&v).map(|v| (v, cmds)))
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
    guild: std::sync::Arc<Mutex<super::info::GuildInfo>>,
) -> Engine {
    let mut e = Engine::new();
    register_info(&mut e, guild);
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

/// DEV-412: 시험 스크립트가 부르는 것 — 이벤트를 흘려 보고, 결과를 확인한다.
pub trait Hooks: Send + Sync {
    /// 이벤트 하나를 이 플러그인에 흘린다. 돌려주는 것은 불린 줄·나온 명령·막힘·바뀐 값.
    /// 명령은 **실행하지 않고 적기만** 한다.
    fn fire(&self, event: &str, payload: Value, with: Value) -> Result<Value, String>;

    /// 설정값을 이 시험 동안만 바꾼다. 설정이 동작을 바꾸는 플러그인은(REQ-021) 이게 없으면
    /// 기본값 하나만 시험할 수 있다.
    fn set_config(&self, key: &str, value: Value);
}

/// 시험 파일에만 있는 함수들. `fire` 는 위 훅으로 가고, `assert*` 는 **던진다** — 던지면 그
/// 시험이 실패로 잡힌다.
fn register_test(e: &mut Engine, hooks: std::sync::Arc<dyn Hooks>) {
    let h = hooks.clone();
    e.register_fn(
        "fire",
        move |name: &str, payload: Dynamic| -> Result<Dynamic, Box<rhai::EvalAltResult>> {
            fire(&*h, name, payload, Dynamic::UNIT)
        },
    );
    let h = hooks.clone();
    e.register_fn(
        "fire",
        move |name: &str, payload: Dynamic, with: Dynamic| -> Result<Dynamic, Box<rhai::EvalAltResult>> {
            fire(&*h, name, payload, with)
        },
    );
    let h = hooks.clone();
    e.register_fn(
        "set_config",
        move |key: &str, value: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
            let v = from_dynamic(&value).map_err(|e| -> Box<rhai::EvalAltResult> {
                format!("set_config(\"{key}\"): {e}").into()
            })?;
            h.set_config(key, v);
            Ok(())
        },
    );
    e.register_fn("assert", |ok: bool| -> Result<(), Box<rhai::EvalAltResult>> {
        if ok { Ok(()) } else { Err("assert 실패".into()) }
    });
    e.register_fn(
        "assert",
        |ok: bool, msg: &str| -> Result<(), Box<rhai::EvalAltResult>> {
            if ok { Ok(()) } else { Err(format!("assert 실패 — {msg}").into()) }
        },
    );
    e.register_fn(
        "assert_eq",
        |a: Dynamic, b: Dynamic| -> Result<(), Box<rhai::EvalAltResult>> {
            let (x, y) = (from_dynamic(&a), from_dynamic(&b));
            if x == y {
                Ok(())
            } else {
                // 무엇이 달랐는지 눈에 보여야 한다 — "실패" 만 나오면 다시 찍어 보게 된다.
                Err(format!(
                    "assert_eq 실패 — 왼쪽 {} · 오른쪽 {}",
                    show(&x),
                    show(&y)
                )
                .into())
            }
        },
    );
}

fn show(v: &Result<Value, String>) -> String {
    match v {
        Ok(v) => v.to_string(),
        Err(e) => format!("(옮길 수 없음: {e})"),
    }
}

fn fire(
    hooks: &dyn Hooks,
    name: &str,
    payload: Dynamic,
    with: Dynamic,
) -> Result<Dynamic, Box<rhai::EvalAltResult>> {
    let payload = from_dynamic(&payload).map_err(|e| -> Box<rhai::EvalAltResult> {
        format!("fire(\"{name}\"): 이벤트 값을 옮기지 못했습니다 — {e}").into()
    })?;
    let with = from_dynamic(&with).map_err(|e| -> Box<rhai::EvalAltResult> {
        format!("fire(\"{name}\"): with 값을 옮기지 못했습니다 — {e}").into()
    })?;
    let out = hooks
        .fire(name, payload, with)
        .map_err(|e| -> Box<rhai::EvalAltResult> { e.into() })?;
    Ok(to_dynamic(&out))
}

/// DEV-409: 길드 정보와 글 다듬기. **읽기만** 하는 함수들이라 샌드박스의 성질(밖으로 못 나감)은
/// 그대로다 — 슬러그를 사람 말로 바꾸려고 스크립트가 파일을 읽게 하는 것보다 이 편이 안전하다.
fn register_info(e: &mut Engine, guild: std::sync::Arc<Mutex<super::info::GuildInfo>>) {
    use super::info;
    macro_rules! with_guild {
        ($g:expr, $body:expr) => {
            match $g.lock() {
                Ok(mut g) => $body(&mut *g),
                // 잠금이 깨져도 이벤트 전달을 세우지 않는다.
                Err(_) => String::new(),
            }
        };
    }
    let g = guild.clone();
    e.register_fn("guild_name", move || -> String {
        with_guild!(g, |i: &mut info::GuildInfo| i.guild_name())
    });
    let g = guild.clone();
    e.register_fn("status_name", move |slug: &str| -> String {
        with_guild!(g, |i: &mut info::GuildInfo| i.status_name(slug, None))
    });
    let g = guild.clone();
    e.register_fn("status_name", move |slug: &str, lang: &str| -> String {
        with_guild!(g, |i: &mut info::GuildInfo| i.status_name(slug, Some(lang)))
    });
    let g = guild.clone();
    e.register_fn("type_name", move |prefix: &str| -> String {
        with_guild!(g, |i: &mut info::GuildInfo| i.type_name(prefix))
    });
    // 언어를 하나 고르는 대신 표로 받는다 — 두 언어와 색·완료 여부까지.
    let g = guild.clone();
    e.register_fn("status_info", move |slug: &str| -> Dynamic {
        match g.lock() {
            Ok(mut i) => to_dynamic(&i.status_info(slug)),
            Err(_) => Dynamic::UNIT,
        }
    });
    let g = guild.clone();
    e.register_fn("type_info", move |prefix: &str| -> Dynamic {
        match g.lock() {
            Ok(mut i) => to_dynamic(&i.type_info(prefix)),
            Err(_) => Dynamic::UNIT,
        }
    });
    let g = guild.clone();
    e.register_fn("link", move |kind: &str, id: &str| -> String {
        with_guild!(g, |i: &mut info::GuildInfo| i.link(kind, id))
    });
    // 시간과 글 다듬기 — 길드를 안 봐도 되는 것들.
    e.register_fn("now", crate::time::now_local_iso8601);
    e.register_fn("ago", |ts: &str| -> String {
        crate::time::format_relative(ts).unwrap_or_else(|| ts.to_string())
    });
    e.register_fn("truncate", |s: &str, n: i64| info::truncate(s, n));
    e.register_fn("plain_text", |s: &str| info::plain_text(s));
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

    /// DEV-408: `import` 는 **코어가 넘긴 것만** 풀린다. 엔진은 디스크를 모른다 — 넘기지 않은
    /// 이름은 실행할 때 실패한다(컴파일은 통과한다).
    #[test]
    fn import_only_resolves_what_the_core_handed_over() {
        let sc = Script::compile_source(r#"import "std" as s; fn h(e) { }"#).unwrap();
        let e = sc.call_handler("h", &ev(), &Default::default()).unwrap_err();
        assert!(e.contains("std"), "{e}");

        // 넘긴 것은 풀리고, 그 함수를 쓸 수 있다.
        let sc = Script::compile_with_imports(
            &[(
                "main.rhai".into(),
                r#"import "../공통/fmt.rhai" as fmt; fn h(e) { send("x", fmt::hello(e.quest.id)) }"#
                    .into(),
            )],
            &[(
                "../공통/fmt.rhai".into(),
                r#"fn hello(id) { "안녕 " + id }"#.into(),
            )],
        )
        .unwrap();
        let got = sc.call_handler("h", &ev(), &Default::default()).unwrap();
        assert_eq!(got[0].body, json!("안녕 DEV-1"));
    }

    /// 불러온 파일이 또 불러와도 된다 — 순서와 상관없이 준비된다.
    #[test]
    fn imports_can_import_each_other() {
        let sc = Script::compile_with_imports(
            &[(
                "main.rhai".into(),
                r#"import "a.rhai" as a; fn h(e) { send("x", a::top()) }"#.into(),
            )],
            &[
                ("a.rhai".into(), r#"import "b.rhai" as b; fn top() { b::deep() + "!" }"#.into()),
                ("b.rhai".into(), r#"fn deep() { "바닥" }"#.into()),
            ],
        )
        .unwrap();
        assert_eq!(
            sc.call_handler("h", &ev(), &Default::default()).unwrap()[0].body,
            json!("바닥!")
        );
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

    /// DEV-409: 언어를 골라 쓰거나, 표로 다 받거나.
    #[test]
    fn a_script_can_pick_a_language_or_take_the_whole_map() {
        let dir = std::env::temp_dir().join(format!("og-script-lang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".guild/statuses")).unwrap();
        std::fs::write(
            dir.join(".guild/statuses/3-done.toml"),
            "sort_order = 3\nname_en = \"Done\"\nname_ko = \"완료\"\ncolor = \"#0a0\"\ncounts_as_done = true\n",
        )
        .unwrap();
        let sc = Script::compile_source(
            r#"fn h(e) {
                   let s = status_info("done");
                   send("x", #{ ko: status_name("done", "ko"), en: status_name("done", "en"),
                                map_ko: s.ko, map_en: s.en, done: s.done, color: s.color });
               }"#,
        )
        .unwrap();
        sc.set_guild(&dir);
        let got = sc.call_handler("h", &ev(), &Default::default()).unwrap();
        assert_eq!(got[0].body["ko"], "완료");
        assert_eq!(got[0].body["en"], "Done");
        assert_eq!(got[0].body["map_ko"], "완료");
        assert_eq!(got[0].body["map_en"], "Done");
        // 슬러그를 외워 박지 않고 "완료인가" 를 물을 수 있다.
        assert_eq!(got[0].body["done"], true);
        assert_eq!(got[0].body["color"], "#0a0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-409: 길드를 정해 주면 스크립트가 **사람이 보는 이름**을 쓴다.
    #[test]
    fn a_script_can_read_display_names_from_the_guild() {
        let dir = std::env::temp_dir().join(format!("og-script-info-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".guild/statuses")).unwrap();
        std::fs::create_dir_all(dir.join(".guild/types")).unwrap();
        std::fs::write(
            dir.join(".guild/statuses/3-done.toml"),
            "sort_order = 3\nname_en = \"Done\"\nname_ko = \"완료\"\ncolor = \"#fff\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(".guild/types/DEV.toml"),
            "prefix = \"DEV\"\ncolor = \"#fff\"\ndescription = \"일반 개발 작업\"\n",
        )
        .unwrap();
        let src = r#"
            fn h(e) {
                send("x", #{ status: status_name("done", "ko"), en: status_name("done", "en"),
                             kind: type_name("DEV"), guild: guild_name(),
                             url: link("quest", e.quest.id), short: truncate(e.comment.body, 3) });
            }
        "#;
        let sc = Script::compile_source(src).unwrap();
        // 길드를 안 정했으면 슬러그 그대로 — 그래도 실패하지는 않는다.
        let before = sc.call_handler("h", &ev(), &Default::default()).unwrap();
        assert_eq!(before[0].body["status"], "done");
        sc.set_guild(&dir);
        let got = sc.call_handler("h", &ev(), &Default::default()).unwrap();
        assert_eq!(got[0].body["status"], "완료");
        assert_eq!(got[0].body["en"], "Done");
        assert_eq!(got[0].body["kind"], "일반 개발 작업");
        assert_eq!(got[0].body["guild"], dir.file_name().unwrap().to_str().unwrap());
        assert_eq!(got[0].body["url"], "/quests/DEV-1");
        assert_eq!(got[0].body["short"], "확인…");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 편의 함수도 샌드박스 안이다 — 길드를 안 정한 채로도 **던지지 않는다**.
    #[test]
    fn convenience_functions_never_break_delivery() {
        let got = call(
            r##"fn h(e) { send("x", #{ a: guild_name(), b: status_name("open"),
                                      c: plain_text("# 제목\n본문"), d: ago(e.ts) }) }"##,
        )
        .unwrap();
        assert_eq!(got[0].body["a"], "");
        assert_eq!(got[0].body["b"], "open");
        assert_eq!(got[0].body["c"], "제목\n본문");
        assert!(got[0].body["d"].is_string());
    }

    /// 등록한 게 없다는 것을 반대편에서도 확인한다 — 순수 계산은 된다.
    #[test]
    fn pure_computation_still_works() {
        let got = call(r#"fn h(e) { let n = 0; for i in 0..10 { n += i } send("x", #{ sum: n }) }"#)
            .unwrap();
        assert_eq!(got[0].body["sum"], 45);
    }
}
