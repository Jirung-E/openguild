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
            Action::Post {
                url,
                headers,
                body_env,
                ..
            } => self.post(url, headers, body_env, plugin.def.action.timeout(), body),
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
        body_env: &std::collections::BTreeMap<String, String>,
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
        // DEV-384: 정의가 지목한 키만 환경에서 채운다. 스크립트에는 I/O 가
        // 없어서 `payload()` 가 환경변수를 못 읽는데, 텔레그램의 `chat_id`
        // 처럼 본문에 개인 식별자를 요구하는 API 가 흔하다.
        let body = if body_env.is_empty() {
            std::borrow::Cow::Borrowed(body)
        } else {
            let mut obj = match body {
                Value::Object(m) => m.clone(),
                // 본문이 객체가 아니면 끼워 넣을 자리가 없다.
                other => {
                    return Err(format!(
                        "body_env 를 쓰려면 payload 가 객체여야 합니다 (받은 것: {})",
                        match other {
                            Value::Array(_) => "배열",
                            Value::Null => "없음",
                            _ => "단일 값",
                        }
                    ));
                }
            };
            for (key, var) in body_env {
                let got = std::env::var(var).map_err(|_| {
                    format!("body_env.{key}: 환경변수 {var} 가 설정되지 않았습니다")
                })?;
                obj.insert(key.clone(), Value::String(got));
            }
            std::borrow::Cow::Owned(Value::Object(obj))
        };
        let res = req.json(body.as_ref()).send().map_err(|e| {
            // 받는 쪽이 죽어 있어도 길드는 멀쩡해야 한다 — 여기서 끝난다.
            format!("{url} 로 보내지 못했습니다: {e}")
        })?;
        let status = res.status();
        if status.is_success() {
            return Ok(());
        }
        // 본문을 조금만 싣는다 — 오류 페이지 전체를 로그에 붓지 않는다.
        // DEV-381: `String::truncate` 는 200바이트째가 문자 경계가 아니면
        // 패닉한다. 한글 오류 페이지에서 실제로 걸린다 — 글자 단위로 자른다.
        let detail: String = res.text().unwrap_or_default().chars().take(200).collect();
        Err(format!("{url} 이 {status} 로 답했습니다: {detail}"))
    }
}

/// DEV-385: 프로세스 그룹째 죽인다 — 손자를 남기지 않기 위해서다.
///
/// BUG-276: 처음엔 `libc` 를 안 들이려고 `kill(1)` 을 셸아웃했다. 그게 틀렸다 —
/// **BSD(macOS)와 procps(Linux)의 인자 해석이 달라** 리눅스에서만 손자가 안
/// 죽고 남았고, `while true` 훅이 CI 러너를 폭주시켜 러너가 통신을 잃었다.
/// macOS 는 통과했으므로 로컬에서는 영영 안 보였다.
///
/// 시그널은 셸 도구가 아니라 시스템 호출로 보낸다. 프로세스도 하나 덜 띄운다.
/// 실패는 무시한다 — 이미 끝났을 수도 있고, 여기서 실패한다고 길드가 멈출
/// 이유는 없다.
#[cfg(unix)]
fn kill_group(pid: u32) {
    // 자식을 `process_group(0)` 으로 띄웠으므로 pgid == 자식 pid 다.
    unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
}

/// Windows 에는 프로세스 그룹이 없다 — job object 가 필요한데 그건 별개
/// 작업이다. 직계 자식만 죽이는 기존 동작을 그대로 둔다.
#[cfg(not(unix))]
fn kill_group(_pid: u32) {}

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
    // DEV-380: `${VAR}` 는 여기서도 푼다. 비밀값 검사가 `run` 의 command/args 도
    // 훑으므로 사용자는 **반드시** 참조로 적어야 하는데, 안 풀면 자식이 리터럴
    // `${MY_API_KEY}` 를 받는다. 오류 메시지에는 원문(`command`)을 쓴다 — 푼
    // 값에는 비밀이 들어 있다.
    // BUG-279: 작업 디렉터리가 데이터 폴더로 바뀌었으므로 코드 폴더를 가리킬
    // 수단이 필요하다. 자식 환경에 넣는 것만으로는 **여기서** 푸는 `${...}` 를
    // 못 채우므로 확장 표에도 같이 심는다.
    let workdir = plugin
        .data_dir()
        .map_err(|e| format!("{command}: 데이터 폴더를 준비하지 못했습니다: {e}"))?;
    let mut vars = std::collections::BTreeMap::new();
    vars.insert(
        "OPENGUILD_PLUGIN_DIR".to_string(),
        plugin.dir.display().to_string(),
    );
    vars.insert(
        "OPENGUILD_PLUGIN_DATA_DIR".to_string(),
        workdir.display().to_string(),
    );
    let exe =
        crate::plugins::expand_env_with(command, &vars).map_err(|e| format!("{command}: {e}"))?;
    let argv = args
        .iter()
        .map(|a| crate::plugins::expand_env_with(a, &vars))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{command}: {e}"))?;
    let mut cmd = Command::new(&exe);
    // DEV-385: 자식을 **자기 프로세스 그룹의 리더로** 띄운다. 이게 없으면
    // `child.kill()` 이 직계 자식 하나만 죽여서, 훅이 띄운 손자(백그라운드로
    // 던진 `curl &`, 셸이 부른 프로그램)는 시한이 지나도 계속 돈다.
    // 그룹 리더로 만들어 두면 `-pgid` 로 한 번에 거둘 수 있다.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    // 작업 디렉터리는 **데이터 폴더**다. 예전엔 플러그인 폴더였는데, 동의
    // 지문이 그 폴더 전체를 보므로([[DEV-381]]) 훅이 출력을 옆에 쓰는 순간
    // 자기 동의를 깼다 — 한 번 돌고 조용히 멈춘다.
    let mut child = cmd
        .args(&argv)
        .current_dir(&workdir)
        // 옆 파일(`notify.sh`)을 부르려면 코드 폴더를 알아야 한다 — 작업
        // 디렉터리가 더는 그곳이 아니기 때문이다.
        .env("OPENGUILD_PLUGIN_DIR", &plugin.dir)
        // 훅이 자기 출력 자리를 알아야 절대경로로 쓸 수도 있다.
        .env("OPENGUILD_PLUGIN_DATA_DIR", &workdir)
        .stdin(Stdio::piped())
        // 자식의 출력이 CLI 표준출력에 섞이면 파이프로 쓰는 사람이 깨진다.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{command} 를 띄우지 못했습니다: {e}"))?;

    // DEV-381: **stdin 쓰기를 시한 밖에 두면 안 된다.** 자식이 stdin 을 안 읽고
    // 페이로드가 파이프 버퍼(~64KB)를 넘으면 `write_all` 이 그 자리에서 영원히
    // 막힌다. 전달 스레드는 하나라 그 순간 모든 플러그인이 멈추고 `drain` 도
    // 못 구한다. 쓰기를 떼어 내고, 시한이 지나면 자식을 죽여 그 쓰기가 EPIPE
    // 로 풀리게 한다.
    //
    // 자식이 stdin 을 안 읽고 죽어도 EPIPE 는 실패로 안 삼는다 — 그건 자식의
    // 사정이고 판단은 종료 코드로 한다.
    if let Some(mut si) = child.stdin.take() {
        let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
        let spawned = std::thread::Builder::new()
            .name("openguild-plugin-stdin".into())
            .spawn(move || {
                let _ = si.write_all(&payload);
                // drop 으로 EOF — 이걸 안 하면 `cat` 류가 영원히 기다린다.
            });
        if spawned.is_err() {
            // 스레드를 못 띄우면 자식은 EOF 를 영영 못 본다 — 여기서 끝낸다.
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{command}: stdin 전달 스레드를 못 띄웠습니다"));
        }
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
            kill_group(child.id());
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
        plugin_in(action, dir.clone(), dir)
    }

    fn plugin_in(
        action: Action,
        dir: std::path::PathBuf,
        guild_root: std::path::PathBuf,
    ) -> Plugin {
        Plugin {
            def: PluginDef {
                description: None,
                name: "p".into(),
                on: vec!["quest.created".into()],
                scope: vec![Scope::Cli],
                action,
                script: None,
            },
            dir,
            guild_root,
            compiled: None,
            script_src: None,
            folder: Default::default(),
        }
    }

    /// BUG-279: `run` 은 이제 **데이터 폴더**에서 돈다. 그래서 시험은 두 가지를
    /// 지켜야 한다 — 훅이 만든 파일을 그 폴더에서 찾을 것, 그리고 **사용자의
    /// 진짜 `~/.openguild` 를 절대 건드리지 않을 것.**
    ///
    /// `OPENGUILD_HOME` 은 프로세스 전역이라 다른 시험과 겹치면 서로의 홈을
    /// 지운다. crate 공통 `env_lock` 으로 직렬화한다.
    struct RunLab {
        _guard: std::sync::MutexGuard<'static, ()>,
        home: std::path::PathBuf,
        guild: std::path::PathBuf,
        /// 플러그인 폴더 — **코드**가 있는 곳. 훅이 여기 쓰면 안 된다.
        code: std::path::PathBuf,
    }

    impl RunLab {
        fn new(label: &str) -> Self {
            let guard = crate::test_env::env_lock();
            let root = tmp(label);
            let home = root.join("home");
            let guild = root.join("guild");
            let code = guild.join(".guild/plugins/p");
            std::fs::create_dir_all(&home).unwrap();
            std::fs::create_dir_all(&code).unwrap();
            unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
            Self {
                _guard: guard,
                home,
                guild,
                code,
            }
        }

        fn plugin(&self, action: Action) -> Plugin {
            plugin_in(action, self.code.clone(), self.guild.clone())
        }

        /// 훅이 파일을 쓰는 자리.
        fn data(&self) -> std::path::PathBuf {
            crate::plugins::data_dir(&self.guild, "p").unwrap()
        }
    }

    impl Drop for RunLab {
        fn drop(&mut self) {
            unsafe { std::env::remove_var("OPENGUILD_HOME") };
            let _ = std::fs::remove_dir_all(&self.home);
            let _ = std::fs::remove_dir_all(&self.guild);
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
                body_env: Default::default(),
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
                body_env: Default::default(),
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
                body_env: Default::default(),
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
                body_env: Default::default(),
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
                body_env: Default::default(),
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
            body_env: Default::default(),
            timeout_ms: Some(9_999_999),
        };
        assert_eq!(
            a.timeout(),
            Duration::from_millis(super::super::MAX_TIMEOUT_MS)
        );
        let b = Action::Post {
            url: "https://x.test".into(),
            headers: Default::default(),
            body_env: Default::default(),
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
                body_env: Default::default(),
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
        let lab = RunLab::new("run");
        let p = lab.plugin(Action::Run {
            command: "sh".into(),
            // 작업 디렉터리가 데이터 폴더이므로 상대 경로는 그쪽에 떨어진다.
            args: vec!["-c".into(), "cat > got.json".into()],
            timeout_ms: Some(5_000),
        });
        Outbound::new().deliver(&p, &event(), &body()).unwrap();
        let got = std::fs::read_to_string(lab.data().join("got.json")).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&got).unwrap(),
            body(),
            "stdin 으로 들어온 것이 본문과 다르다"
        );
        // BUG-279 의 핵심 — **코드 폴더에는 아무것도 안 생긴다.** 생기면 그
        // 순간 동의 지문이 바뀌어 플러그인이 스스로 꺼진다.
        assert!(
            !lab.code.join("got.json").exists(),
            "훅의 출력이 플러그인 폴더(코드)에 생겼다 — 자기 동의를 깬다"
        );
    }

    /// **오래 걸려도 길드가 안 멈춘다** — 시한에 죽이고 거둔다(좀비 없음).
    ///
    /// 시간만 재면 "포기하고 돌아왔다" 까지만 보이고 자식이 계속 도는지는
    /// 안 보인다. 그래서 자식이 살아 있으면 계속 커지는 파일을 두고, 돌아온
    /// 뒤로 **더 안 커지는지**를 본다.
    #[test]
    fn a_hanging_process_is_killed() {
        let lab = RunLab::new("hang");
        let p = lab.plugin(Action::Run {
            command: "sh".into(),
            args: vec![
                "-c".into(),
                // BUG-276: **유한 루프**로 둔다. `while true` 였을 때, 죽이기가 리눅스에서
                // 깨지자 훅이 CI 러너에 영원히 남아 러너를 죽였다. 시험이
                // 무는 것과 별개로, 시험이 남기는 것도 시험의 책임이다.
                "i=0; while [ $i -lt 400 ]; do echo x >> ticks; sleep 0.05; i=$((i+1)); done"
                    .into(),
            ],
            timeout_ms: Some(300),
        });
        let t = Instant::now();
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "시한이 안 걸렸다 ({:?})",
            t.elapsed()
        );
        assert!(e.contains("중단"), "{e}");

        let ticks = lab.data().join("ticks");
        let a = std::fs::metadata(&ticks).map(|m| m.len()).unwrap_or(0);
        assert!(
            a > 0,
            "자식이 아예 안 돌았다 — 시험이 아무것도 안 보고 있다"
        );
        std::thread::sleep(Duration::from_millis(500));
        let b = std::fs::metadata(&ticks).map(|m| m.len()).unwrap_or(0);
        assert_eq!(a, b, "포기만 하고 자식은 계속 돌고 있다");
    }

    /// **`run` 도 `${VAR}` 를 푼다.** 비밀값 검사가 command/args 도 훑으므로
    /// 사용자는 반드시 참조로 적어야 하는데, 안 풀면 자식이 리터럴을 받는다.
    #[test]
    fn run_expands_env_refs_in_command_and_args() {
        // RunLab 이 env_lock 을 들고 있다 — 여기서 또 잡으면 자기 자신과
        // 교착한다.
        let lab = RunLab::new("runenv");
        unsafe { std::env::set_var("OG_TEST_RUN_TOKEN", "s3cret") };
        let p = lab.plugin(Action::Run {
            command: "sh".into(),
            args: vec![
                "-c".into(),
                "printf %s \"$0\" > got.txt".into(),
                "tok=${OG_TEST_RUN_TOKEN}".into(),
            ],
            timeout_ms: Some(5_000),
        });
        Outbound::new().deliver(&p, &event(), &body()).unwrap();
        assert_eq!(
            std::fs::read_to_string(lab.data().join("got.txt")).unwrap(),
            "tok=s3cret",
            "자식이 리터럴 ${{VAR}} 를 받았다"
        );
        unsafe { std::env::remove_var("OG_TEST_RUN_TOKEN") };
    }

    /// DEV-384: **본문에 개인 식별자를 요구하는 API** (텔레그램의 `chat_id`)를
    /// 쓰려면 이게 있어야 한다. 스크립트에는 I/O 가 없어서 `payload()` 가
    /// 환경변수를 못 읽는다 — 그게 샌드박스의 요점이라 바꿀 수 없다.
    #[test]
    fn body_env_fills_named_keys_from_the_environment() {
        let _guard = crate::test_env::env_lock();
        unsafe { std::env::set_var("OG_TEST_CHAT_ID", "987654321") };
        let pr = probe(true);
        let d = tmp("bodyenv");
        let mut be = BTreeMap::new();
        be.insert("chat_id".to_string(), "OG_TEST_CHAT_ID".to_string());
        let p = plugin(
            Action::Post {
                url: pr.url.clone(),
                headers: Default::default(),
                body_env: be,
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        Outbound::new()
            .deliver(&p, &event(), &serde_json::json!({ "text": "왔다" }))
            .unwrap();
        let got = pr.got.lock().unwrap()[0].clone();
        assert!(got.contains(r#""chat_id":"987654321""#), "{got}");
        // 스크립트가 만든 것도 그대로 남는다.
        assert!(got.contains(r#""text":"왔다""#), "{got}");
        unsafe { std::env::remove_var("OG_TEST_CHAT_ID") };
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 지목한 키만 건드린다 — 사용자가 쓴 댓글에 `${HOME}` 이 들어 있어도
    /// 확장되지 않아야 한다. 본문 전체를 훑지 않는 이유가 이것이다.
    #[test]
    fn body_env_never_expands_user_text() {
        let _guard = crate::test_env::env_lock();
        unsafe { std::env::set_var("OG_TEST_CHAT_ID2", "42") };
        let pr = probe(true);
        let d = tmp("bodyenv-safe");
        let mut be = BTreeMap::new();
        be.insert("chat_id".to_string(), "OG_TEST_CHAT_ID2".to_string());
        let p = plugin(
            Action::Post {
                url: pr.url.clone(),
                headers: Default::default(),
                body_env: be,
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        Outbound::new()
            .deliver(
                &p,
                &event(),
                &serde_json::json!({ "text": "경로는 ${HOME} 입니다" }),
            )
            .unwrap();
        let got = pr.got.lock().unwrap()[0].clone();
        assert!(
            got.contains("${HOME}"),
            "사용자가 쓴 글자가 환경변수로 확장됐다: {got}"
        );
        unsafe { std::env::remove_var("OG_TEST_CHAT_ID2") };
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 참조한 변수가 없으면 리터럴로 넘기지 않고 실패로 남긴다.
    #[test]
    fn run_with_a_missing_env_var_fails_loudly() {
        let _guard = crate::test_env::env_lock();
        unsafe { std::env::remove_var("OG_TEST_RUN_ABSENT") };
        let d = tmp("runenv-missing");
        let p = plugin(
            Action::Run {
                command: "sh".into(),
                args: vec!["-c".into(), "true".into(), "${OG_TEST_RUN_ABSENT}".into()],
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(e.contains("OG_TEST_RUN_ABSENT"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// **stdin 을 안 읽는 자식이 전달 스레드를 잡아먹으면 안 된다.**
    ///
    /// 파이프 버퍼(~64KB)를 넘는 페이로드를 쓰는 동안 자식이 읽지 않으면
    /// `write_all` 이 그 자리에서 막힌다. 전달 스레드는 하나뿐이라 그러면 모든
    /// 플러그인이 멈춘다 — 시한 안에 돌아오는지 본다.
    #[test]
    fn a_child_that_never_reads_stdin_does_not_wedge_the_thread() {
        let d = tmp("nostdin");
        let big = serde_json::json!({ "text": "가".repeat(200_000) });
        let p = plugin(
            Action::Run {
                command: "sh".into(),
                // stdin 을 아예 안 읽고 그냥 잔다.
                args: vec!["-c".into(), "sleep 30".into()],
                timeout_ms: Some(400),
            },
            d.clone(),
        );
        let t = Instant::now();
        let e = Outbound::new().deliver(&p, &event(), &big).unwrap_err();
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "stdin 쓰기에 막혀 전달 스레드가 잠겼다 ({:?})",
            t.elapsed()
        );
        assert!(e.contains("중단"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 응답 본문이 멀티바이트면 자를 때 패닉하지 않는다.
    #[test]
    fn a_multibyte_error_body_does_not_panic() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/hook", l.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Some(mut s) = l.incoming().flatten().next() {
                let mut buf = [0u8; 4096];
                s.set_read_timeout(Some(Duration::from_secs(2))).ok();
                let _ = s.read(&mut buf);
                // 200바이트째가 문자 중간에 오도록 3바이트 글자로 채운다.
                let body = "오".repeat(300);
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 500 Internal Server Error\r\ncontent-length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                );
            }
        });
        let d = tmp("mb500");
        let p = plugin(
            Action::Post {
                url,
                headers: Default::default(),
                body_env: Default::default(),
                timeout_ms: Some(5_000),
            },
            d.clone(),
        );
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(e.contains("500"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// DEV-385: **손자도 거둬야 한다.** 훅이 백그라운드로 던진 프로그램은
    /// `child.kill()` 로는 안 죽는다 — 시한이 지났다고 보고해 놓고 실제로는
    /// 계속 도는 것이 제일 나쁘다.
    #[cfg(unix)]
    #[test]
    fn a_timeout_kills_grandchildren_too() {
        let lab = RunLab::new("grandchild");
        // 셸이 손자를 백그라운드로 띄우고 자기는 잔다. 손자는 살아 있는 동안
        // 계속 파일을 키운다.
        let p = lab.plugin(Action::Run {
            command: "sh".into(),
            args: vec![
                "-c".into(),
                // BUG-276: 손자도 유한하게 — 위와 같은 이유다.
                "(i=0; while [ $i -lt 400 ]; do echo x >> grand.log; sleep 0.05; i=$((i+1)); done) & sleep 30".into(),
            ],
            timeout_ms: Some(400),
        });
        let e = Outbound::new().deliver(&p, &event(), &body()).unwrap_err();
        assert!(e.contains("중단"), "{e}");

        let log = lab.data().join("grand.log");
        let a = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
        assert!(
            a > 0,
            "손자가 아예 안 돌았다 — 시험이 아무것도 안 보고 있다"
        );
        std::thread::sleep(Duration::from_millis(600));
        let b = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
        assert_eq!(a, b, "직계만 죽이고 손자는 계속 돌고 있다");
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
