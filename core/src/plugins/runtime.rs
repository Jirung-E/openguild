//! DEV-375: 적재된 플러그인을 이벤트에 연결한다.
//!
//! [`PluginRuntime`] 이 [`EventSink`](crate::events::EventSink) 을 구현하고,
//! `Store` 에 꽂히면 그때부터 `ops` 의 emit 이 여기로 들어온다.
//!
//! # 인라인 + 비동기 (admin 결정)
//!
//! mutation 직후 **그 프로세스가** 띄우되 **기다리지 않는다**. AI 호출이 몇
//! 초 걸린다고 `openguild comment add` 가 그만큼 멈추면 안 된다. 그래서
//! [`dispatch`](PluginRuntime::dispatch) 는 일감을 전용 스레드로 넘기고 바로
//! 돌아온다 — mutation 응답 시간은 전달 시간과 무관하다.
//!
//! 스레드 하나로 충분하다. 전달은 대부분 대기(HTTP 응답)이고, 순서가 보존되면
//! 플러그인 쪽에서 이벤트 순서를 다시 맞출 필요가 없다. 여기서 스레드 풀까지
//! 만들면 얻는 것 없이 실패 모드만 늘어난다.
//!
//! 대가는 CLI 에서 드러난다 — 명령이 곧 끝나므로 전송이 도중에 끊길 수 있다.
//! [`drain`](PluginRuntime::drain) 으로 짧은 유예를 두고 못 끝내면 포기한다.
//! 여기서 재시도 큐까지 만들면 범위가 터진다. 필요해지면 journal 커서 방식을
//! 얹을 수 있다(구조상 가능하다 — 이벤트가 이미 이름과 페이로드를 갖고 있다).
//!
//! # 실패는 격리한다
//!
//! 플러그인 하나가 죽어도 다른 플러그인과 길드 동작은 멀쩡해야 한다. 전달은
//! 애초에 다른 스레드라 길드 동작을 막을 수 없고, 그 스레드 안에서도 플러그인
//! 단위로 [`catch_unwind`](std::panic::catch_unwind) 로 끊는다.

use crate::events::{Event, EventSink, Phase};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::Plugin;
use super::script::Decision;

/// 이벤트를 실제로 밖으로 내보내는 쪽. [[DEV-377]] 이 HTTP/프로세스 구현을
/// 넣는다. 여기서 분리해 두는 이유는 **적재와 전달을 따로 검증**하기
/// 위해서다 — 이 파일의 테스트는 네트워크 없이 돈다.
pub trait Delivery: Send + Sync {
    /// 한 건을 내보낸다. 전용 스레드에서 불리므로 **여기서는 기다려도 된다** —
    /// mutation 경로는 이미 지나갔다. 대신 무한정 붙들면
    /// [`drain`](PluginRuntime::drain) 이 포기하게 되므로 시한은 둬야 한다.
    ///
    /// `body` 는 스크립트([[DEV-376]])가 만든 모양이거나, 스크립트가 없으면
    /// 이벤트 JSON 그대로다. `event` 는 이름·phase 같은 메타를 볼 때 쓴다.
    ///
    /// 실패는 **돌려준다**. 여기서 삼키면 왜 안 갔는지 알 길이 없어진다 —
    /// [`PluginRuntime`] 이 받아서 문제 목록에 남긴다.
    fn deliver(
        &self,
        plugin: &Plugin,
        event: &Event,
        body: &serde_json::Value,
    ) -> Result<(), String>;
}

/// 아무 데도 안 보내는 구현 — 테스트와, 전달을 끄고 싶을 때.
pub struct DropDelivery;
impl Delivery for DropDelivery {
    fn deliver(
        &self,
        _plugin: &Plugin,
        _event: &Event,
        _body: &serde_json::Value,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// 아직 안 나간 건수. 0 이 되면 기다리던 쪽을 깨운다.
#[derive(Default)]
struct Pending {
    n: Mutex<usize>,
    idle: Condvar,
}

impl Pending {
    fn enqueued(&self) {
        if let Ok(mut n) = self.n.lock() {
            *n += 1;
        }
    }

    fn finished(&self) {
        if let Ok(mut n) = self.n.lock() {
            *n = n.saturating_sub(1);
            if *n == 0 {
                self.idle.notify_all();
            }
        }
    }

    /// 다 나갈 때까지, 늦어도 `budget` 까지 기다린다.
    fn wait_idle(&self, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        let Ok(mut n) = self.n.lock() else {
            return true; // 잠금이 오염됐으면 기다릴 근거가 없다
        };
        while *n > 0 {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            let Ok((next, t)) = self.idle.wait_timeout(n, left) else {
                return false;
            };
            n = next;
            if t.timed_out() && *n > 0 {
                return false;
            }
        }
        true
    }
}

/// 전용 스레드로 넘기는 일감. 플러그인은 인덱스로 넘긴다 — `Plugin` 을 매번
/// 복제하지 않기 위해서다.
struct Job {
    plugin: usize,
    event: Event,
}

/// 플러그인이 하나라도 있을 때만 만든다 — 안 쓰는 길드가 스레드를 갖지 않게.
struct Worker {
    tx: Sender<Job>,
    pending: Arc<Pending>,
}

/// 적재된 플러그인 + 전달 방법.
pub struct PluginRuntime {
    plugins: Arc<Vec<Plugin>>,
    /// 전달 중 발생한 문제 — 조용히 삼키지 않는다. 어디에 보여줄지는
    /// 컴포넌트가 정한다(GUI 토스트는 CLI 에 없다).
    problems: Arc<Mutex<Vec<String>>>,
    worker: Option<Worker>,
}

/// DEV-381: 문제 목록의 상한. 서버는 몇 달씩 도는 프로세스라 상한이 없으면
/// 끝없이 자란다. 오래된 것부터 버린다 — 최근 것이 고치는 데 쓸모 있다.
const MAX_PROBLEMS: usize = 200;

fn note(problems: &Mutex<Vec<String>>, msg: String) {
    if let Ok(mut p) = problems.lock() {
        if p.len() >= MAX_PROBLEMS {
            p.remove(0);
        }
        p.push(msg);
    }
}

impl PluginRuntime {
    pub fn new(plugins: Vec<Plugin>, delivery: Arc<dyn Delivery>) -> Self {
        let plugins = Arc::new(plugins);
        let problems = Arc::new(Mutex::new(Vec::new()));
        let worker = if plugins.is_empty() {
            None
        } else {
            Some(Self::spawn(&plugins, delivery, &problems))
        };
        Self {
            plugins,
            problems,
            worker,
        }
    }

    fn spawn(
        plugins: &Arc<Vec<Plugin>>,
        delivery: Arc<dyn Delivery>,
        problems: &Arc<Mutex<Vec<String>>>,
    ) -> Worker {
        let (tx, rx) = channel::<Job>();
        let pending = Arc::new(Pending::default());
        let (ps, log, done) = (plugins.clone(), problems.clone(), pending.clone());
        let spawned = std::thread::Builder::new()
            .name("openguild-plugins".into())
            .spawn(move || {
                // 송신부가 떨어지면 recv 가 끝나고 스레드도 끝난다.
                while let Ok(job) = rx.recv() {
                    let p = &ps[job.plugin];
                    // 하나가 패닉해도 다음 일감은 계속 간다. 플러그인 정의는
                    // 사용자 입력이고 전달 구현은 외부와 말한다 — 둘 다 믿을 수
                    // 없다. 스크립트도 사용자 코드라 같은 울타리 안에 둔다.
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // 판단·가공이 먼저다 — 보낼지조차 스크립트가 정한다.
                        let body = match p.compiled.as_deref() {
                            None => job.event.to_json(),
                            Some(sc) => match sc.decide(&job.event) {
                                Ok(Decision::Send(v)) => v,
                                Ok(Decision::Skip) => return,
                                Err(e) => {
                                    note(&log, format!("플러그인 '{}' 스크립트 — {e}", p.def.name));
                                    return;
                                }
                            },
                        };
                        if let Err(e) = delivery.deliver(p, &job.event, &body) {
                            note(&log, format!("플러그인 '{}' — {e}", p.def.name));
                        }
                    }));
                    if r.is_err() {
                        note(
                            &log,
                            format!(
                                "플러그인 '{}' 전달 중 패닉 — 이 이벤트는 건너뜁니다",
                                p.def.name
                            ),
                        );
                    }
                    done.finished();
                }
            });
        if let Err(e) = &spawned {
            note(
                problems,
                format!("플러그인 전달 스레드를 못 띄웠습니다: {e}"),
            );
        }
        Worker { tx, pending }
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    pub fn problems(&self) -> Vec<String> {
        self.problems.lock().map(|p| p.clone()).unwrap_or_default()
    }

    /// 아직 안 나간 것들을 기다린다. 시간 안에 다 나갔으면 `true`.
    pub fn drain(&self, budget: Duration) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|w| w.pending.wait_idle(budget))
    }
}

impl EventSink for PluginRuntime {
    fn wants(&self, name: &str, phase: Phase) -> bool {
        self.plugins.iter().any(|p| p.wants(name, phase))
    }

    fn dispatch(&self, event: Event) {
        let Some(w) = self.worker.as_ref() else {
            return;
        };
        for (i, p) in self.plugins.iter().enumerate() {
            if !p.wants(event.name, event.phase) {
                continue;
            }
            w.pending.enqueued();
            if w.tx
                .send(Job {
                    plugin: i,
                    event: event.clone(),
                })
                .is_err()
            {
                w.pending.finished();
                note(
                    &self.problems,
                    format!(
                        "플러그인 '{}' — 전달 스레드가 없어 이 이벤트를 버립니다",
                        p.def.name
                    ),
                );
            }
        }
    }

    fn drain(&self, budget: Duration) -> bool {
        PluginRuntime::drain(self, budget)
    }

    fn problems(&self) -> Vec<String> {
        PluginRuntime::problems(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::names as ev;
    use crate::plugins::{Action, PluginDef, Scope};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn plugin(name: &str, on: &[&str]) -> Plugin {
        Plugin {
            def: PluginDef {
                name: name.into(),
                on: on.iter().map(|s| s.to_string()).collect(),
                scope: vec![Scope::Cli],
                action: Action::Post {
                    url: "https://example.test".into(),
                    headers: Default::default(),
                    timeout_ms: None,
                },
                script: None,
            },
            dir: std::path::PathBuf::from("/tmp/none"),
            compiled: None,
            script_src: None,
            folder: Default::default(),
        }
    }

    fn scripted(name: &str, on: &[&str], src: &str) -> Plugin {
        let mut p = plugin(name, on);
        p.compiled = Some(Arc::new(
            crate::plugins::script::Script::compile_source(src).unwrap(),
        ));
        p
    }

    fn event(name: &'static str, phase: Phase) -> Event {
        Event {
            name,
            phase,
            ts: "2026-09-07T00:00:00+09:00".into(),
            guild: "g".into(),
            ok: Some(true),
            error: None,
            data: Default::default(),
        }
    }

    #[derive(Default)]
    struct Counter(AtomicUsize);
    impl Delivery for Counter {
        fn deliver(&self, _p: &Plugin, _e: &Event, _b: &serde_json::Value) -> Result<(), String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct Exploding;
    impl Delivery for Exploding {
        fn deliver(&self, _p: &Plugin, _e: &Event, _b: &serde_json::Value) -> Result<(), String> {
            panic!("전달 실패");
        }
    }

    #[test]
    fn only_subscribed_plugins_are_called() {
        let c = Arc::new(Counter::default());
        let rt = PluginRuntime::new(
            vec![
                plugin("a", &["quest.created"]),
                plugin("b", &["comment.added"]),
            ],
            c.clone(),
        );
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.drain(Duration::from_secs(5)), "유예 안에 못 나갔다");
        assert_eq!(
            c.0.load(Ordering::SeqCst),
            1,
            "구독하지 않은 플러그인까지 불렸다"
        );
    }

    #[test]
    fn wants_is_false_when_nobody_subscribes() {
        let rt = PluginRuntime::new(
            vec![plugin("a", &["comment.added"])],
            Arc::new(DropDelivery),
        );
        assert!(!rt.wants(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.wants(ev::COMMENT_ADDED, Phase::Post));
    }

    /// `pre:` 없이 등록하면 pre 이벤트를 받지 않는다.
    #[test]
    fn phase_is_part_of_the_subscription() {
        let rt = PluginRuntime::new(
            vec![plugin("a", &["comment.added"])],
            Arc::new(DropDelivery),
        );
        assert!(!rt.wants(ev::COMMENT_ADDED, Phase::Pre));
        let rt2 = PluginRuntime::new(
            vec![plugin("a", &["pre:comment.added"])],
            Arc::new(DropDelivery),
        );
        assert!(rt2.wants(ev::COMMENT_ADDED, Phase::Pre));
        assert!(!rt2.wants(ev::COMMENT_ADDED, Phase::Post));
    }

    /// **하나가 죽어도 나머지는 돈다.** 플러그인 정의는 사용자 입력이다.
    #[test]
    fn a_panicking_plugin_does_not_stop_the_others() {
        struct Mixed(Arc<AtomicUsize>);
        impl Delivery for Mixed {
            fn deliver(
                &self,
                p: &Plugin,
                _e: &Event,
                _b: &serde_json::Value,
            ) -> Result<(), String> {
                if p.def.name == "bad" {
                    panic!("펑");
                }
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
        let n = Arc::new(AtomicUsize::new(0));
        let rt = PluginRuntime::new(
            vec![plugin("bad", &["quest.*"]), plugin("good", &["quest.*"])],
            Arc::new(Mixed(n.clone())),
        );
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.drain(Duration::from_secs(5)));
        assert_eq!(
            n.load(Ordering::SeqCst),
            1,
            "앞 플러그인이 죽자 뒤가 안 돌았다"
        );
        assert_eq!(rt.problems().len(), 1, "문제를 조용히 삼켰다");
    }

    #[test]
    fn panic_is_recorded_not_swallowed() {
        let rt = PluginRuntime::new(vec![plugin("a", &["*"])], Arc::new(Exploding));
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.drain(Duration::from_secs(5)));
        assert!(rt.problems()[0].contains("a"));
    }

    /// **mutation 이 플러그인을 기다리지 않는다.** 이게 무너지면 AI 호출
    /// 하나가 `openguild comment add` 를 몇 초씩 세운다.
    #[test]
    fn dispatch_does_not_wait_for_delivery() {
        struct Slow(Arc<AtomicUsize>);
        impl Delivery for Slow {
            fn deliver(
                &self,
                _p: &Plugin,
                _e: &Event,
                _b: &serde_json::Value,
            ) -> Result<(), String> {
                std::thread::sleep(Duration::from_millis(400));
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
        let n = Arc::new(AtomicUsize::new(0));
        let rt = PluginRuntime::new(vec![plugin("slow", &["*"])], Arc::new(Slow(n.clone())));

        let t = Instant::now();
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        let handed_off = t.elapsed();
        assert!(
            handed_off < Duration::from_millis(100),
            "dispatch 가 전달을 기다렸다 ({handed_off:?})"
        );
        assert_eq!(n.load(Ordering::SeqCst), 0, "동기로 이미 끝나 있다");

        // 그래도 버려지지는 않는다 — 유예를 주면 나간다.
        assert!(rt.drain(Duration::from_secs(5)));
        assert_eq!(n.load(Ordering::SeqCst), 1);
    }

    // ── 판단·가공 ([[DEV-376]]) ─────────────────────────

    /// 받은 본문을 그대로 적어 두는 전달 구현.
    #[derive(Default)]
    struct Body(Mutex<Vec<serde_json::Value>>);
    impl Delivery for Body {
        fn deliver(&self, _p: &Plugin, _e: &Event, b: &serde_json::Value) -> Result<(), String> {
            self.0.lock().unwrap().push(b.clone());
            Ok(())
        }
    }

    /// **`should_send` 가 false 면 액션이 호출되지 않는다** — 구독은 했지만
    /// 판단에서 걸러진다.
    #[test]
    fn a_script_can_veto_a_subscribed_event() {
        let b = Arc::new(Body::default());
        let rt = PluginRuntime::new(
            vec![scripted("veto", &["*"], "fn should_send(e) { false }")],
            b.clone(),
        );
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.drain(Duration::from_secs(5)));
        assert!(b.0.lock().unwrap().is_empty(), "거른 이벤트가 나갔다");
        assert!(rt.problems().is_empty(), "정상 거름을 문제로 적었다");
    }

    /// **`payload` 가 만든 모양이 그대로 전달된다.**
    #[test]
    fn the_body_is_what_the_script_built() {
        let b = Arc::new(Body::default());
        let rt = PluginRuntime::new(
            vec![scripted(
                "shape",
                &["*"],
                r#"fn payload(e) { #{ kind: e.event, mine: 42 } }"#,
            )],
            b.clone(),
        );
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.drain(Duration::from_secs(5)));
        let got = b.0.lock().unwrap().clone();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["kind"], "quest.created");
        assert_eq!(got[0]["mine"], 42);
    }

    /// 스크립트가 없으면 이벤트 JSON 이 그대로 간다.
    #[test]
    fn without_a_script_the_event_json_is_the_body() {
        let b = Arc::new(Body::default());
        let rt = PluginRuntime::new(vec![plugin("plain", &["*"])], b.clone());
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.drain(Duration::from_secs(5)));
        assert_eq!(b.0.lock().unwrap()[0]["event"], "quest.created");
    }

    /// **스크립트가 던져도 다른 플러그인은 멀쩡하다.** 그리고 조용히 삼키지
    /// 않는다.
    #[test]
    fn a_throwing_script_does_not_take_the_others_down() {
        let b = Arc::new(Body::default());
        let rt = PluginRuntime::new(
            vec![
                scripted("bad", &["*"], r#"fn should_send(e) { throw "터짐" }"#),
                plugin("good", &["*"]),
            ],
            b.clone(),
        );
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.drain(Duration::from_secs(5)));
        assert_eq!(
            b.0.lock().unwrap().len(),
            1,
            "앞 스크립트가 죽자 뒤가 안 갔다"
        );
        assert_eq!(rt.problems().len(), 1, "스크립트 오류를 조용히 삼켰다");
        assert!(rt.problems()[0].contains("터짐"), "{:?}", rt.problems());
    }

    /// 폭주하는 스크립트도 그 이벤트만 잃는다 — 길드도 다른 플러그인도 안 멈춘다.
    #[test]
    fn a_runaway_script_only_loses_its_own_event() {
        let b = Arc::new(Body::default());
        let rt = PluginRuntime::new(
            vec![
                scripted(
                    "loop",
                    &["*"],
                    "fn should_send(e) { let i = 0; loop { i += 1; } }",
                ),
                plugin("good", &["*"]),
            ],
            b.clone(),
        );
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.drain(Duration::from_secs(20)), "상한에 안 걸렸다");
        assert_eq!(b.0.lock().unwrap().len(), 1);
        assert_eq!(rt.problems().len(), 1);
    }

    /// **전달 실패를 조용히 삼키지 않는다.** 어디에 보여줄지는 컴포넌트가
    /// 정하지만, 남기지 않으면 왜 안 갔는지 알 길이 없다.
    #[test]
    fn a_delivery_failure_is_recorded() {
        struct Refusing;
        impl Delivery for Refusing {
            fn deliver(
                &self,
                _p: &Plugin,
                _e: &Event,
                _b: &serde_json::Value,
            ) -> Result<(), String> {
                Err("받는 쪽이 죽어 있습니다".into())
            }
        }
        let rt = PluginRuntime::new(vec![plugin("a", &["*"])], Arc::new(Refusing));
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        assert!(rt.drain(Duration::from_secs(5)));
        assert_eq!(rt.problems().len(), 1, "실패를 삼켰다");
        assert!(
            rt.problems()[0].contains("죽어 있습니다"),
            "{:?}",
            rt.problems()
        );
    }

    /// **문제 목록은 누가 읽어야 의미가 있다.** 조용히 삼키지 않으려고 모은
    /// 것인데 아무도 안 읽으면 삼킨 것과 같다 — sink 를 통해 올라오는지 본다.
    /// 그리고 장수 프로세스에서 무한히 자라면 안 된다.
    #[test]
    fn problems_reach_the_sink_and_stay_bounded() {
        use crate::events::EventSink as _;
        struct Refusing;
        impl Delivery for Refusing {
            fn deliver(
                &self,
                _p: &Plugin,
                _e: &Event,
                _b: &serde_json::Value,
            ) -> Result<(), String> {
                Err("받는 쪽이 죽어 있습니다".into())
            }
        }
        let rt = PluginRuntime::new(vec![plugin("a", &["*"])], Arc::new(Refusing));
        for _ in 0..(MAX_PROBLEMS + 50) {
            rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        }
        assert!(rt.drain(Duration::from_secs(10)));
        // 트레이트 경유로 읽힌다 — Store/Events 가 이 길로 가져간다.
        let via_sink = EventSink::problems(&rt);
        assert!(!via_sink.is_empty(), "sink 로 안 올라온다");
        assert_eq!(via_sink.len(), MAX_PROBLEMS, "상한 없이 자란다");
        assert!(via_sink[0].contains("죽어 있습니다"));
    }

    /// 유예가 짧으면 포기한다 — 무한정 붙들려 CLI 가 안 끝나면 안 된다.
    #[test]
    fn drain_gives_up_when_the_budget_runs_out() {
        struct Stuck;
        impl Delivery for Stuck {
            fn deliver(
                &self,
                _p: &Plugin,
                _e: &Event,
                _b: &serde_json::Value,
            ) -> Result<(), String> {
                std::thread::sleep(Duration::from_secs(3));
                Ok(())
            }
        }
        let rt = PluginRuntime::new(vec![plugin("stuck", &["*"])], Arc::new(Stuck));
        rt.dispatch(event(ev::QUEST_CREATED, Phase::Post));
        let t = Instant::now();
        assert!(
            !rt.drain(Duration::from_millis(150)),
            "안 끝났는데 끝났다고 했다"
        );
        assert!(t.elapsed() < Duration::from_secs(1), "유예를 넘겨 기다렸다");
    }
}
