//! DEV-376: 플러그인의 **판단·가공 계층**. rhai 스크립트가 "이 이벤트를
//! 보낼지" 와 "어떤 모양으로 보낼지" 를 정한다.
//!
//! # 왜 스크립트가 필요한가
//!
//! JSON 만으로는 "미해결 토론 댓글만 AI 에 보낸다" 를 못 적는다. 여기서 조건
//! 문법을 발명하기 시작하면 결국 DSL 을 만들게 된다.
//!
//! # 핵심 제약 — 여기에 I/O 를 주지 않는다
//!
//! ```text
//! rhai   판단·가공만. 파일도 네트워크도 없다. 순수 함수.
//! 코어   내보내기(post/run)를 담당.        ← [[DEV-377]]
//! ```
//!
//! 이 분리가 설계의 전부다. 스크립트가 폭주해도 **밖으로 못 나간다.** 그래서
//! JS 를 접었다 — AI 를 부르려면 스크립트에 네트워크를 줘야 하고, 그러면
//! 샌드박스가 셸과 위험이 같아지면서 런타임 값만 치른다.
//!
//! 샌드박스는 두 겹이다. `no_module` 로 `import` 자체를 컴파일에서 없애고(기본
//! 모듈 해석기는 **디스크를 읽는다**), 엔진에는 우리가 등록한 함수 외에는
//! 아무것도 없다 — 파일도 프로세스도 소켓도 이름이 없다.
//!
//! # 계약
//!
//! ```rhai
//! fn should_send(event) { event.name == "comment.added" && event.ok }
//! fn payload(event)     { #{ text: `[${event.quest.id}] ${event.comment.body}` } }
//! ```
//!
//! 둘 다 선택이다. `should_send` 가 없으면 전부 보내고, `payload` 가 없으면
//! 이벤트 JSON 을 그대로 보낸다.

use crate::error::{AppError, AppResult};
use crate::events::Event;
use rhai::{AST, Dynamic, Engine, Scope};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SHOULD_SEND: &str = "should_send";
const PAYLOAD: &str = "payload";

/// 연산 상한. **안 걸면 스크립트 하나가 CLI 를 멈춘다.** 넉넉하되 무한 루프는
/// 초 단위 안에 잡히는 값.
const MAX_OPERATIONS: u64 = 500_000;
/// 벽시계 상한. 연산 상한만으로 충분해야 정상이지만(등록한 함수가 없으므로
/// 연산 수가 곧 시간이다), 상한을 하나만 두면 그 하나가 틀렸을 때 막을 게
/// 없다.
const MAX_WALL_TIME: Duration = Duration::from_secs(2);

/// 스크립트가 내린 결정.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// `should_send` 가 false — 이 이벤트는 안 보낸다.
    Skip,
    /// 이 본문으로 보낸다.
    Send(Value),
}

/// 컴파일된 스크립트 하나. 적재 때 한 번 컴파일하고 이벤트마다 재사용한다 —
/// 문법 오류는 **적재 때** 드러나야지 첫 이벤트 때 드러나면 안 된다.
pub struct Script {
    engine: Engine,
    ast: AST,
    /// 지금 돌고 있는 호출의 마감. `on_progress` 가 읽는다.
    deadline: std::sync::Arc<Mutex<Instant>>,
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
        let deadline = std::sync::Arc::new(Mutex::new(Instant::now()));
        let engine = sandboxed_engine(deadline.clone());
        let ast = engine
            .compile(src)
            .map_err(|e| AppError::BadRequest(format!("스크립트를 컴파일하지 못했습니다: {e}")))?;
        Ok(Self {
            engine,
            ast,
            deadline,
        })
    }

    fn has(&self, name: &str) -> bool {
        self.ast
            .iter_functions()
            .any(|f| f.name == name && f.params.len() == 1)
    }

    /// 이 이벤트를 어떻게 할지. 스크립트가 던지면 **그 이벤트만** 건너뛴다 —
    /// 길드 동작에도 다른 플러그인에도 영향이 없어야 한다.
    pub fn decide(&self, event: &Event) -> Result<Decision, String> {
        let json = event.to_json();
        let arg = to_dynamic(&json);
        self.arm();

        if self.has(SHOULD_SEND) {
            let v = self.call(SHOULD_SEND, arg.clone())?;
            match v.as_bool() {
                Ok(false) => return Ok(Decision::Skip),
                Ok(true) => {}
                // 모호하게 넘기지 않는다 — "보낼지 말지" 는 bool 이어야 한다.
                Err(t) => {
                    return Err(format!(
                        "{SHOULD_SEND} 가 bool 이 아니라 {t} 를 돌려줬습니다"
                    ));
                }
            }
        }

        if !self.has(PAYLOAD) {
            return Ok(Decision::Send(json));
        }
        self.arm();
        let v = self.call(PAYLOAD, arg)?;
        Ok(Decision::Send(from_dynamic(&v)?))
    }

    fn arm(&self) {
        if let Ok(mut d) = self.deadline.lock() {
            *d = Instant::now() + MAX_WALL_TIME;
        }
    }

    fn call(&self, name: &str, arg: Dynamic) -> Result<Dynamic, String> {
        let mut scope = Scope::new();
        self.engine
            .call_fn::<Dynamic>(&mut scope, &self.ast, name, (arg,))
            .map_err(|e| format!("{name}: {e}"))
    }
}

/// 아무것도 등록하지 않은 엔진 + 상한. 등록하지 않은 것이 이 함수의 내용이다.
fn sandboxed_engine(deadline: std::sync::Arc<Mutex<Instant>>) -> Engine {
    let mut e = Engine::new();
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
        }
    }

    fn decide(src: &str) -> Result<Decision, String> {
        Script::compile_source(src).unwrap().decide(&ev())
    }

    /// 스크립트가 없으면 이벤트 JSON 이 그대로 나간다.
    #[test]
    fn no_functions_means_send_the_event_as_is() {
        let d = decide("fn unrelated(x) { x }").unwrap();
        let Decision::Send(v) = d else {
            panic!("건너뛰었다")
        };
        assert_eq!(v["event"], "comment.added");
        assert_eq!(v["quest"]["id"], "DEV-1");
    }

    /// **`should_send` 가 false 면 액션이 호출되지 않는다.**
    #[test]
    fn should_send_false_skips() {
        assert_eq!(
            decide("fn should_send(e) { false }").unwrap(),
            Decision::Skip
        );
    }

    /// 이벤트에 실린 것으로 판단할 수 있어야 한다 — `phase`/`ok` 가 오므로
    /// "성공한 것만", "미해결 토론만" 이 전부 스크립트 몫이 된다([[DEV-374]]).
    #[test]
    fn judgement_uses_what_the_event_carries() {
        let src = r#"
            fn should_send(e) {
                e.event == "comment.added" && e.ok && e.comment.discussion
            }
        "#;
        assert!(matches!(decide(src).unwrap(), Decision::Send(_)));

        let src_no = r#"fn should_send(e) { e.phase == "pre" }"#;
        assert_eq!(decide(src_no).unwrap(), Decision::Skip);
    }

    /// **`payload` 가 만든 모양이 그대로 전달된다.**
    #[test]
    fn payload_shape_passes_through() {
        let src = r#"
            fn payload(e) {
                #{ text: `[${e.quest.id}] ${e.comment.author}: ${e.comment.body}`,
                   nested: #{ n: 3, flag: true, list: [1, "둘"] } }
            }
        "#;
        let Decision::Send(v) = decide(src).unwrap() else {
            panic!("건너뛰었다")
        };
        assert_eq!(v["text"], "[DEV-1] kim: 확인 바람");
        assert_eq!(v["nested"]["n"], 3);
        assert_eq!(v["nested"]["flag"], true);
        assert_eq!(v["nested"]["list"][1], "둘");
    }

    /// **무한 루프가 상한에 걸려 멈춘다.** 이게 안 되면 넣으면 안 된다.
    #[test]
    fn a_runaway_loop_is_stopped() {
        let t = Instant::now();
        let e = decide("fn should_send(e) { let i = 0; loop { i += 1; } }").unwrap_err();
        assert!(
            t.elapsed() < Duration::from_secs(10),
            "상한에 안 걸리고 계속 돌았다"
        );
        assert!(
            e.contains("operation") || e.contains("Operation") || e.contains("Terminated"),
            "무엇에 걸렸는지 안 보인다: {e}"
        );
    }

    /// 스크립트가 던지면 **그 이벤트만** 실패한다. 패닉이 아니라 오류다.
    #[test]
    fn a_throwing_script_returns_an_error() {
        let e = decide(r#"fn should_send(e) { throw "안 돼" }"#).unwrap_err();
        assert!(e.contains("안 돼"), "{e}");
    }

    /// 보낼지 말지는 bool 이어야 한다 — 모호하게 넘기지 않는다.
    #[test]
    fn non_bool_judgement_is_an_error() {
        let e = decide(r#"fn should_send(e) { "아마도" }"#).unwrap_err();
        assert!(e.contains("bool"), "{e}");
    }

    // ── 샌드박스 ────────────────────────────────────────
    //
    // 이 설계의 전부가 "밖으로 못 나간다" 이므로, 못 나간다는 것을 시험이
    // 붙들고 있어야 한다.

    /// `import` 는 **컴파일부터** 안 된다. 기본 모듈 해석기는 디스크를 읽는다 —
    /// `no_module` 이 그 경로 자체를 없앤다.
    #[test]
    fn import_does_not_even_compile() {
        let e = Script::compile_source(r#"import "std" as s; fn payload(e) { 1 }"#).unwrap_err();
        assert!(e.to_string().contains("컴파일"), "{e}");
    }

    /// 파일·프로세스·네트워크는 **이름조차 없다.** 아무것도 등록하지 않았다.
    #[test]
    fn there_is_no_way_to_touch_the_outside() {
        for src in [
            r#"fn payload(e) { open_file("/etc/passwd") }"#,
            r#"fn payload(e) { read_file("/etc/passwd") }"#,
            r#"fn payload(e) { File("/etc/passwd") }"#,
            r#"fn payload(e) { system("ls") }"#,
            r#"fn payload(e) { exec("sh") }"#,
            r#"fn payload(e) { http_get("https://example.test") }"#,
            r#"fn payload(e) { fetch("https://example.test") }"#,
        ] {
            // 컴파일에서 막히든 실행에서 막히든 **못 나가면** 된다.
            let blocked = match Script::compile_source(src) {
                Err(_) => true,
                Ok(sc) => sc.decide(&ev()).is_err(),
            };
            assert!(blocked, "밖으로 나가는 길이 열려 있다: {src}");
        }
    }

    /// 등록한 게 없다는 것을 반대편에서도 확인한다 — 순수 계산은 된다.
    #[test]
    fn pure_computation_still_works() {
        let Decision::Send(v) =
            decide(r#"fn payload(e) { let n = 0; for i in 0..10 { n += i } #{ sum: n } }"#)
                .unwrap()
        else {
            panic!()
        };
        assert_eq!(v["sum"], 45);
    }
}
