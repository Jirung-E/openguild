//! DEV-377: 이벤트를 **밖으로 내보낸다**. 코어가 소유하는 동작은 둘뿐이다.
//!
//! ```text
//! post   HTTP 로 보낸다
//! run    프로세스를 띄우고 stdin 으로 넘긴다
//! ```
//!
//! "밖으로 내보낸다" 의 최소 완전집합이다. slack/discord/email 같은 통합을
//! 코어에 쌓기 시작하면 끝없이 따라다녀야 한다 — 그건 사용자 프로그램 몫이고,
//! 여기 둘 중 하나로 닿는다.
//!
//! # HTTP 클라이언트는 코어가 갖는다
//!
//! 처음엔 컴포넌트가 주입하는 안도 있었다. 하지만 cli/gui/server 가 각자
//! 구현하면 같은 코드가 셋으로 갈라지고, gui 와 server 는 어차피 클라이언트를
//! 새로 들여야 한다. [`Delivery`](super::runtime::Delivery) 트레이트는 남겨
//! 둔다 — 테스트가 네트워크 없이 돌고, 나중에 다른 전송을 끼울 자리가 된다.
//!
//! # 여기서는 기다려도 된다
//!
//! 전달은 전용 스레드에서 돈다([[DEV-375]]) — mutation 경로는 이미 지나갔다.
//! 그래서 blocking 클라이언트를 쓴다(tokio 밖이라 오히려 이쪽이 맞다). 대신
//! **시한은 반드시 둔다** — 하나가 영원히 붙들면 뒤에 줄 선 것이 다 막힌다.

use super::runtime::Delivery;
use super::{Action, Plugin, expand_env};
use crate::events::Event;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// 죽은 자식을 거두러 오는 간격.
const REAP_TICK: Duration = Duration::from_millis(20);

/// 진짜로 내보내는 구현.
#[derive(Default)]
pub struct Outbound {
    /// 처음 `post` 를 만날 때 만든다 — `run` 만 쓰는 길드가 TLS 초기화 비용을
    /// 치를 이유가 없다.
    http: OnceLock<reqwest::blocking::Client>,
}

impl Outbound {
    pub fn new() -> Self {
        Self::default()
    }

    fn client(&self) -> &reqwest::blocking::Client {
        self.http.get_or_init(|| {
            reqwest::blocking::Client::builder()
                // 전체 시한은 요청마다 따로 건다. 여기서는 연결만 제한한다.
                .connect_timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default()
        })
    }
}

impl Delivery for Outbound {
    fn deliver(&self, plugin: &Plugin, _event: &Event, body: &Value) -> Result<(), String> {
        match &plugin.def.action {
            Action::Post { url, headers, .. } => {
                self.post(url, headers, plugin.def.action.timeout(), body)
            }
            Action::Run { command, args, .. } => {
                run(plugin, command, args, plugin.def.action.timeout(), body)
            }
        }
    }
}

impl Outbound {
    fn post(
        &self,
        url: &str,
        headers: &std::collections::BTreeMap<String, String>,
        timeout: Duration,
        body: &Value,
    ) -> Result<(), String> {
        // URL 과 헤더의 `${VAR}` 는 **보내기 직전에** 푼다. 정의에 리터럴을
        // 못 적게 해 둔 것([[DEV-375]])과 짝이다 — 값은 이 기계의 환경에만 있다.
        let url = expand_env(url).map_err(|e| e.to_string())?;
        let mut req = self.client().post(&url).timeout(timeout);
        for (k, v) in headers {
            let v = expand_env(v).map_err(|e| format!("헤더 {k}: {e}"))?;
            req = req.header(k, v);
        }
        let res = req.json(body).send().map_err(|e| {
            // 받는 쪽이 죽어 있어도 길드는 멀쩡해야 한다 — 여기서 끝난다.
            format!("{url} 로 보내지 못했습니다: {e}")
        })?;
        let status = res.status();
        if status.is_success() {
            return Ok(());
        }
        // 본문을 조금만 싣는다 — 오류 페이지 전체를 로그에 붓지 않는다.
        let mut detail = res.text().unwrap_or_default();
        detail.truncate(200);
        Err(format!("{url} 이 {status} 로 답했습니다: {detail}"))
    }
}

/// 프로세스를 띄우고 **stdin** 으로 이벤트를 넘긴다.
///
/// 인자로 넘기지 않는 이유는 길이 제한과 이스케이프다. 작업 디렉터리는
/// 플러그인 폴더 — 스크립트가 옆 파일을 상대 경로로 부를 수 있어야 한다.
/// 환경변수는 물려준다. 자식은 사용자가 **동의한** 코드이고([[DEV-375]]),
/// API 키를 환경에서 받는 것이 이 설계의 전제다.
fn run(
    plugin: &Plugin,
    command: &str,
    args: &[String],
    timeout: Duration,
    body: &Value,
) -> Result<(), String> {
    let mut child = Command::new(command)
        .args(args)
        .current_dir(&plugin.dir)
        .stdin(Stdio::piped())
        // 자식의 출력이 CLI 표준출력에 섞이면 파이프로 쓰는 사람이 깨진다.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{command} 를 띄우지 못했습니다: {e}"))?;

    if let Some(mut si) = child.stdin.take() {
        let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
        // 자식이 stdin 을 안 읽고 죽으면 EPIPE 가 난다 — 그건 자식의 사정이라
        // 여기서 실패로 삼지 않고 종료 코드로 판단한다.
        let _ = si.write_all(&payload);
        // drop 으로 EOF — 이걸 안 하면 `cat` 류가 영원히 기다린다.
    }

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => return Ok(()),
            Ok(Some(st)) => return Err(format!("{command} 가 {st} 로 끝났습니다")),
            Ok(None) => {}
            Err(e) => return Err(format!("{command} 상태를 못 읽었습니다: {e}")),
        }
        if Instant::now() >= deadline {
            // 죽이고 **반드시 거둔다** — wait 를 안 하면 좀비가 남는다.
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{command} 가 {}ms 안에 안 끝나 중단했습니다",
                timeout.as_millis()
            ));
        }
        std::thread::sleep(REAP_TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Phase;
    use crate::plugins::{PluginDef, Scope};
    use std::collections::BTreeMap;
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// 받은 요청을 그대로 적어 두는 최소 HTTP 대역 서버. 테스트 의존성을
    /// 하나도 안 들이려고 손으로 쓴다 — 우리가 보내는 것만 받으면 된다.
    struct Probe {
        url: String,
        got: Arc<Mutex<Vec<String>>>,
    }

    fn probe(answer: bool) -> Probe {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/hook", l.local_addr().unwrap());
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = got.clone();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let mut s = s;
                s.set_read_timeout(Some(Duration::from_secs(5))).ok();
                let mut raw = Vec::new();
                let mut buf = [0u8; 2048];
                // 헤더 끝까지 읽고, content-length 만큼 더 읽는다.
                loop {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            raw.extend_from_slice(&buf[..n]);
                            let text = String::from_utf8_lossy(&raw).to_string();
                            if let Some(h) = text.find("\r\n\r\n") {
                                let len: usize = text
                                    .to_lowercase()
                                    .split("content-length:")
                                    .nth(1)
                                    .and_then(|t| t.split("\r\n").next())
                                    .and_then(|t| t.trim().parse().ok())
                                    .unwrap_or(0);
                                if raw.len() >= h + 4 + len {
                                    break;
                                }
                            }
                        }
                    }
                }
                sink.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&raw).to_string());
                if answer {
                    let _ = s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
                } else {
                    // 답하지 않는다 — 시한이 걸리는지 보려고.
                    std::thread::sleep(Duration::from_secs(30));
                }
            }
        });
        Probe { url, got }
    }

    fn plugin(action: Action, dir: std::path::PathBuf) -> Plugin {
        Plugin {
            def: PluginDef {
                name: "p".into(),
                on: vec!["quest.created".into()],
                scope: vec![Scope::Cli],
                action,
                script: None,
            },
            dir,
            compiled: None,
            script_src: None,
        }
    }

    fn event() -> Event {
        Event {
            name: "quest.created",
            phase: Phase::Post,
            ts: "2026-09-07T00:00:00+09:00".into(),
            guild: "g".into(),
            ok: Some(true),
            error: None,
            data: Default::default(),
        }
    }

    fn body() -> Value {
        serde_json::json!({ "text": "왔다" })
    }

    fn tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-deliver-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // ── post ────────────────────────────────────────────

    /// **구독한 이벤트에서 실제로 요청이 나간다.**
    #[test]
    fn post_actually_sends() {
        let pr = probe(true);
        let d = tmp("post");
        let mut h = BTreeMap::new();
        h.insert("X-Source".to_string(), "openguild".to_string());
        let p = plugin(
            Action::Post {
                url: pr.url.clone(),
                headers: h,
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        Outbound::new().deliver(&p, &event(), &body()).unwrap();

        let got = pr.got.lock().unwrap().clone();
        assert_eq!(got.len(), 1, "요청이 안 왔다");
        assert!(got[0].starts_with("POST /hook "), "{}", got[0]);
        assert!(got[0].contains("x-source: openguild") || got[0].contains("X-Source: openguild"));
        assert!(
            got[0].contains(r#"{"text":"왔다"}"#),
            "본문이 안 왔다: {}",
            got[0]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 비밀값은 **보내기 직전에** 환경에서 푼다 — 정의에는 참조만 있다([[DEV-375]]).
    #[test]
    fn env_refs_are_resolved_at_send_time() {
        let _guard = crate::test_env::env_lock();
        unsafe { std::env::set_var("OG_TEST_DELIVERY_KEY", "s3cret") };
        let pr = probe(true);
        let d = tmp("env");
        let mut h = BTreeMap::new();
        h.insert(
            "Authorization".to_string(),
            "Bearer ${OG_TEST_DELIVERY_KEY}".to_string(),
        );
        let p = plugin(
            Action::Post {
                url: pr.url.clone(),
                headers: h,
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        Outbound::new().deliver(&p, &event(), &body()).unwrap();
        assert!(
            pr.got.lock().unwrap()[0].contains("Bearer s3cret"),
            "환경변수가 안 풀렸다"
        );
        unsafe { std::env::remove_var("OG_TEST_DELIVERY_KEY") };
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 참조한 환경변수가 없으면 **조용히 빈 값으로 보내지 않는다.**
    #[test]
    fn a_missing_env_var_is_an_error_not_an_empty_header() {
        let _guard = crate::test_env::env_lock();
        unsafe { std::env::remove_var("OG_TEST_DELIVERY_ABSENT") };
        let pr = probe(true);
        let d = tmp("noenv");
        let mut h = BTreeMap::new();
        h.insert(
            "Authorization".to_string(),
            "Bearer ${OG_TEST_DELIVERY_ABSENT}".to_string(),
        );
        let p = plugin(
            Action::Post {
                url: pr.url.clone(),
                headers: h,
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(e.contains("OG_TEST_DELIVERY_ABSENT"), "{e}");
        assert!(pr.got.lock().unwrap().is_empty(), "빈 키로 보내 버렸다");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **받는 쪽이 죽어 있어도 길드는 멀쩡하다** — 실패가 값으로 돌아온다.
    #[test]
    fn a_dead_receiver_is_an_error_not_a_panic() {
        // 포트를 잡았다 놓아 아무도 안 듣는 주소를 만든다.
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/hook", l.local_addr().unwrap());
        drop(l);
        let d = tmp("dead");
        let p = plugin(
            Action::Post {
                url,
                headers: Default::default(),
                timeout_ms: Some(2_000),
            },
            d.clone(),
        );
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(e.contains("보내지 못했습니다"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **시한이 걸린다.** 안 걸면 전달 스레드가 그대로 잠긴다.
    #[test]
    fn post_gives_up_at_the_timeout() {
        let pr = probe(false); // 받기만 하고 답하지 않는다
        let d = tmp("slow");
        let p = plugin(
            Action::Post {
                url: pr.url.clone(),
                headers: Default::default(),
                timeout_ms: Some(300),
            },
            d.clone(),
        );
        let t = Instant::now();
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "시한이 안 걸렸다 ({:?})",
            t.elapsed()
        );
        assert!(e.contains("보내지 못했습니다"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 시한 상한을 넘겨 적어도 상한까지만 — 하나가 스레드를 영원히 잡으면
    /// 뒤에 줄 선 것이 다 막힌다.
    #[test]
    fn the_timeout_is_capped() {
        let a = Action::Post {
            url: "https://x.test".into(),
            headers: Default::default(),
            timeout_ms: Some(9_999_999),
        };
        assert_eq!(
            a.timeout(),
            Duration::from_millis(super::super::MAX_TIMEOUT_MS)
        );
        let b = Action::Post {
            url: "https://x.test".into(),
            headers: Default::default(),
            timeout_ms: None,
        };
        assert_eq!(
            b.timeout(),
            Duration::from_millis(super::super::DEFAULT_TIMEOUT_MS)
        );
    }

    /// 2xx 가 아니면 문제로 남긴다 — 200 인 척 넘기면 왜 안 갔는지 못 찾는다.
    #[test]
    fn a_non_success_status_is_reported() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/hook", l.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Some(mut s) = l.incoming().flatten().next() {
                let mut buf = [0u8; 2048];
                s.set_read_timeout(Some(Duration::from_secs(2))).ok();
                let _ = s.read(&mut buf);
                let _ = s.write_all(
                    "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 6\r\n\r\n터짐"
                        .as_bytes(),
                );
            }
        });
        let d = tmp("500");
        let p = plugin(
            Action::Post {
                url,
                headers: Default::default(),
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(e.contains("500"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    // ── run ─────────────────────────────────────────────

    /// **stdin 으로 이벤트 JSON 이 들어간다.** 인자로 넘기면 길이 제한과
    /// 이스케이프 문제가 생긴다.
    #[test]
    fn run_feeds_the_event_on_stdin() {
        let d = tmp("run");
        let p = plugin(
            Action::Run {
                command: "sh".into(),
                // 작업 디렉터리가 플러그인 폴더이므로 상대 경로로 받는다.
                args: vec!["-c".into(), "cat > got.json".into()],
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        Outbound::new().deliver(&p, &event(), &body()).unwrap();
        let got = std::fs::read_to_string(d.join("got.json")).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&got).unwrap(),
            body(),
            "stdin 으로 들어온 것이 본문과 다르다"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **오래 걸려도 길드가 안 멈춘다** — 시한에 죽이고 거둔다(좀비 없음).
    ///
    /// 시간만 재면 "포기하고 돌아왔다" 까지만 보이고 자식이 계속 도는지는
    /// 안 보인다. 그래서 자식이 살아 있으면 계속 커지는 파일을 두고, 돌아온
    /// 뒤로 **더 안 커지는지**를 본다.
    #[test]
    fn a_hanging_process_is_killed() {
        let d = tmp("hang");
        let p = plugin(
            Action::Run {
                command: "sh".into(),
                args: vec![
                    "-c".into(),
                    "while true; do echo x >> ticks; sleep 0.05; done".into(),
                ],
                timeout_ms: Some(300),
            },
            d.clone(),
        );
        let t = Instant::now();
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "시한이 안 걸렸다 ({:?})",
            t.elapsed()
        );
        assert!(e.contains("중단"), "{e}");

        let ticks = d.join("ticks");
        let a = std::fs::metadata(&ticks).map(|m| m.len()).unwrap_or(0);
        assert!(
            a > 0,
            "자식이 아예 안 돌았다 — 시험이 아무것도 안 보고 있다"
        );
        std::thread::sleep(Duration::from_millis(500));
        let b = std::fs::metadata(&ticks).map(|m| m.len()).unwrap_or(0);
        assert_eq!(a, b, "포기만 하고 자식은 계속 돌고 있다");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 실패한 프로세스는 조용히 넘어가지 않는다.
    #[test]
    fn a_failing_process_is_reported() {
        let d = tmp("fail");
        let p = plugin(
            Action::Run {
                command: "sh".into(),
                args: vec!["-c".into(), "exit 3".into()],
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(e.contains("3"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 없는 명령은 적재가 아니라 전달에서 걸린다 — 정의만으로는 그 기계에
    /// 그 프로그램이 있는지 알 수 없다.
    #[test]
    fn a_missing_command_is_reported() {
        let d = tmp("nocmd");
        let p = plugin(
            Action::Run {
                command: "og-no-such-program-42".into(),
                args: vec![],
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(e.contains("띄우지 못했습니다"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
