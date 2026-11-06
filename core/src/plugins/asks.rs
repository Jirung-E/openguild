//! DEV-479: 서버의 플러그인이 `ask()` 하면 **그 일을 일으킨 창에** 묻는다.
//!
//! 서버에는 화면이 없다. 원격으로 붙은 앱 · 웹 · CLI 가 요청마다 자기 이름([`CLIENT_HEADER`])을 붙여 보내고,
//! 그 요청이 일으킨 일에서 플러그인이 물으면 질문이 여기 쌓인다. 그 창은 질문을 가져가고([`wait_for`] —
//! 오래 기다리는 GET), 답을 보낸다([`answer`]). 플러그인은 답이 올 때까지 기다린다.
//!
//! - **물을 창이 없으면 아니요.** 시간 실행처럼 사람이 일으키지 않은 일, 이름을 안 붙인 옛 클라이언트,
//!   그 창이 이미 닫혔을 때(한동안 질문을 가지러 오지 않았을 때)다. 못 본 질문에 "예" 로 넘어가지 않는다.
//! - 답을 너무 오래 안 하면([`MAX_WAIT`]) 아니요.
//!
//! [`CLIENT_HEADER`]: crate::events::origin::CLIENT_HEADER

use super::guild::{AskOptions, Notifier, Stderr};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// 답을 기다리는 가장 긴 시간.
pub const MAX_WAIT: Duration = Duration::from_secs(10 * 60);
/// 질문을 가지러 오는 GET 이 한 번에 기다리는 시간. 창은 이보다 자주 다시 온다.
pub const POLL: Duration = Duration::from_secs(25);
/// 이만큼 질문을 가지러 오지 않은 창은 닫힌 것으로 본다 — [`POLL`] 보다 넉넉히.
const GONE_AFTER: Duration = Duration::from_secs(40);

/// 창에 보이는 질문 하나.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Question {
    pub id: u64,
    pub plugin: String,
    pub text: String,
    pub options: AskOptions,
}

struct Pending {
    client: String,
    question: Question,
    answer: Option<bool>,
}

#[derive(Default)]
struct Broker {
    next: u64,
    pending: HashMap<u64, Pending>,
    /// 창마다 마지막으로 질문을 가지러 온 때.
    seen: HashMap<String, Instant>,
}

static BROKER: Mutex<Option<Broker>> = Mutex::new(None);
static CHANGED: Condvar = Condvar::new();

fn with<T>(f: impl FnOnce(&mut Broker) -> T) -> T {
    let mut g = BROKER.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Broker::default))
}

fn alive(b: &Broker, client: &str, now: Instant) -> bool {
    b.seen
        .get(client)
        .is_some_and(|t| now.duration_since(*t) < GONE_AFTER)
}

/// `client` 창에 묻고 답을 기다린다. 물을 수 없으면 바로 아니요.
pub fn ask(client: &str, plugin: &str, text: &str, options: &AskOptions) -> bool {
    ask_within(client, plugin, text, options, MAX_WAIT)
}

fn ask_within(client: &str, plugin: &str, text: &str, options: &AskOptions, max: Duration) -> bool {
    let start = Instant::now();
    let id = with(|b| {
        if !alive(b, client, start) {
            return None;
        }
        b.next += 1;
        let id = b.next;
        b.pending.insert(
            id,
            Pending {
                client: client.to_string(),
                question: Question {
                    id,
                    plugin: plugin.to_string(),
                    text: text.to_string(),
                    options: options.clone(),
                },
                answer: None,
            },
        );
        Some(id)
    });
    let Some(id) = id else {
        return false;
    };
    CHANGED.notify_all();
    let mut g = BROKER.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let b = g.get_or_insert_with(Broker::default);
        let now = Instant::now();
        let answered = b.pending.get(&id).and_then(|p| p.answer);
        if let Some(yes) = answered {
            b.pending.remove(&id);
            return yes;
        }
        // 창이 닫혔거나 너무 오래 — 아니요. 질문은 거둔다(창이 다시 와도 안 보이게).
        if now.duration_since(start) >= max || !alive(b, client, now) {
            b.pending.remove(&id);
            drop(g);
            CHANGED.notify_all();
            return false;
        }
        let (next, _) = CHANGED
            .wait_timeout(g, Duration::from_secs(1))
            .unwrap_or_else(|e| e.into_inner());
        g = next;
    }
}

/// 이 창이 방금 요청을 보냈다 — 살아 있다. 질문을 가지러 오기 전에 일으킨 일에서도 물을 수 있게
/// 서버가 요청마다 부른다.
pub fn touch(client: &str) {
    with(|b| {
        let now = Instant::now();
        // 창은 열고 닫힌다 — 이름이 끝없이 쌓이지 않게, 많아지면 닫힌 것을 버린다.
        if b.seen.len() > 256 {
            b.seen.retain(|_, t| now.duration_since(*t) < GONE_AFTER);
        }
        b.seen.insert(client.to_string(), now);
    });
}

/// `client` 창의 질문들 — 창이 이미 아는 것(`known`)과 같으면 [`POLL`] 까지 기다린다. 새 질문이 오거나
/// 아는 질문이 끝나면(다른 창이 답했거나 시간이 지났으면) 바로 돌려준다. 부를 때마다 "이 창은 살아 있다" 로 적는다.
pub fn wait_for(client: &str, known: &[u64]) -> Vec<Question> {
    wait_for_within(client, known, POLL)
}

fn wait_for_within(client: &str, known: &[u64], max: Duration) -> Vec<Question> {
    let mut known: Vec<u64> = known.to_vec();
    known.sort_unstable();
    let start = Instant::now();
    let mut g = BROKER.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let b = g.get_or_insert_with(Broker::default);
        b.seen.insert(client.to_string(), Instant::now());
        let mut out: Vec<Question> = b
            .pending
            .values()
            .filter(|p| p.client == client && p.answer.is_none())
            .map(|p| p.question.clone())
            .collect();
        out.sort_by_key(|q| q.id);
        let same = out.iter().map(|q| q.id).eq(known.iter().copied());
        if !same || start.elapsed() >= max {
            return out;
        }
        let left = max.saturating_sub(start.elapsed());
        let (next, _) = CHANGED
            .wait_timeout(g, left.min(Duration::from_secs(1)))
            .unwrap_or_else(|e| e.into_inner());
        g = next;
    }
}

/// `client` 창이 답했다. 그 창의 질문이 아니거나 이미 끝났으면 `false`.
pub fn answer(client: &str, id: u64, yes: bool) -> bool {
    let done = with(|b| match b.pending.get_mut(&id) {
        Some(p) if p.client == client && p.answer.is_none() => {
            p.answer = Some(yes);
            true
        }
        _ => false,
    });
    CHANGED.notify_all();
    done
}

/// 서버의 알림 — 글은 로그로, `ask()` 는 일을 일으킨 창으로.
pub struct Remote;

impl Notifier for Remote {
    fn notify(&self, plugin: &str, text: &str) -> Result<(), String> {
        Stderr.notify(plugin, text)
    }

    fn ask(
        &self,
        plugin: &str,
        text: &str,
        options: &AskOptions,
        client: Option<&str>,
    ) -> Result<bool, String> {
        let yes = client.is_some_and(|c| ask(c, plugin, text, options));
        if client.is_none() {
            eprintln!(
                "{}",
                crate::tf!(
                    "[plugin:{plugin}] {text} — 물을 창이 없어(시간 실행 · 이름 없는 클라이언트) 아니요로 답했습니다",
                    "[plugin:{plugin}] {text} — no window to ask (scheduled run or unnamed client), so answered no"
                )
            );
        }
        Ok(yes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(tag: &str) -> String {
        format!("t-{tag}-{}", crate::test_seq())
    }

    #[test]
    fn the_window_that_caused_it_gets_the_question_and_its_answer_comes_back() {
        let c = client("yes");
        wait_for_within(&c, &[], Duration::ZERO); // 창이 살아 있다.
        let asker = {
            let c = c.clone();
            std::thread::spawn(move || {
                ask_within(&c, "p", "지울까요?", &AskOptions::default(), MAX_WAIT)
            })
        };
        let qs = wait_for_within(&c, &[], Duration::from_secs(5));
        assert_eq!(qs.len(), 1);
        assert_eq!(
            (qs[0].plugin.as_str(), qs[0].text.as_str()),
            ("p", "지울까요?")
        );
        assert!(!answer("다른-창", qs[0].id, true), "다른 창은 답할 수 없다");
        // 같은 질문을 보여 주던 다른 창 — 답이 나면 바로 돌아와 창을 닫는다(25초를 다 기다리지 않는다).
        let id = qs[0].id;
        let other = {
            let c = c.clone();
            std::thread::spawn(move || {
                let t = Instant::now();
                (
                    wait_for_within(&c, &[id], Duration::from_secs(20)),
                    t.elapsed(),
                )
            })
        };
        std::thread::sleep(Duration::from_millis(100));
        assert!(answer(&c, qs[0].id, true));
        assert!(asker.join().unwrap());
        let (left, took) = other.join().unwrap();
        assert!(left.is_empty() && took < Duration::from_secs(5), "{took:?}");
        assert!(
            wait_for_within(&c, &[], Duration::ZERO).is_empty(),
            "끝난 질문이 남았다"
        );
        assert!(!answer(&c, qs[0].id, false), "끝난 질문에 또 답했다");
    }

    #[test]
    fn nobody_to_ask_or_no_answer_in_time_is_no() {
        let gone = client("gone");
        assert!(
            !ask_within(&gone, "p", "?", &AskOptions::default(), MAX_WAIT),
            "한 번도 안 온 창"
        );
        let slow = client("slow");
        wait_for_within(&slow, &[], Duration::ZERO);
        assert!(!ask_within(
            &slow,
            "p",
            "?",
            &AskOptions::default(),
            Duration::from_millis(200)
        ));
        assert!(
            wait_for_within(&slow, &[], Duration::ZERO).is_empty(),
            "시간이 지난 질문은 거둔다"
        );
        assert!(
            !Remote.ask("p", "?", &AskOptions::default(), None).unwrap(),
            "일으킨 창이 없다"
        );
    }
}
