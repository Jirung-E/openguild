//! DEV-401: 이 변경을 **누가 일으켰나** — 플러그인 무한 반복 막기.
//!
//! 플러그인이 길드를 바꾸면(지금은 `run` 훅이 `openguild` CLI 를 부르는 것) 그 변경이 다시
//! 이벤트가 되고, 같은 플러그인이 또 돈다. 끝이 없다.
//!
//! 횟수를 세지 않고 **거쳐 온 플러그인 목록**(체인)만 들고 다닌다. 목록에 이미 있는
//! 플러그인에는 그 이벤트를 보내지 않는다. 그러면 A → B 는 되고(스크립트가 만든 백업을
//! `backup-archive` 가 복사) A → A, A → B → A 는 막힌다. 목록은 서로 다른 이름으로만
//! 자라므로 반복은 반드시 끝난다.
//!
//! # 체인이 어디서 오나 — 한 곳에서 읽는다
//!
//! [`current`] 하나가 답한다. 순서는:
//!
//! 1. 이 작업(tokio task)에 [`scope`] 로 붙인 값 — 서버가 요청마다 헤더에서 읽어 붙인다.
//!    나중에 스크립트가 길드 명령을 직접 부르게 되면 그 실행도 여기에 붙이면 된다.
//! 2. 프로세스 기본값 — CLI 가 시작할 때 [`ENV`] 에서 읽어 [`set_process_default`] 로 둔다.
//! 3. 둘 다 없으면 사람이 일으킨 것(빈 목록).
//!
//! 넘기는 쪽도 한 곳이다: `run` 자식에게는 [`ENV`], HTTP 로는 [`HEADER`].

use serde_json::{Value, json};
use std::sync::RwLock;

/// `run` 훅 자식에게 넘기는 환경변수. 값은 JSON 배열(`["a","b"]`).
pub const ENV: &str = "OPENGUILD_PLUGIN_CHAIN";
/// HTTP 요청 헤더. 값은 JSON 배열을 퍼센트 인코딩한 것 — 헤더에는 ASCII 만 들어가는데
/// 플러그인 이름에는 한글이 올 수 있다.
pub const HEADER: &str = "x-openguild-plugin-chain";

/// 거쳐 온 플러그인들. 비어 있으면 사람이 일으킨 변경.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    chain: Vec<String>,
}

impl Origin {
    pub fn user() -> Self {
        Self::default()
    }

    pub fn from_chain<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut chain: Vec<String> = Vec::new();
        for n in names {
            let n = n.into();
            if !n.is_empty() && !chain.contains(&n) {
                chain.push(n);
            }
        }
        Self { chain }
    }

    pub fn chain(&self) -> &[String] {
        &self.chain
    }

    pub fn is_user(&self) -> bool {
        self.chain.is_empty()
    }

    /// 이 플러그인이 이미 거친 이벤트인가 — 그렇다면 그 플러그인에는 보내지 않는다.
    pub fn has(&self, plugin: &str) -> bool {
        self.chain.iter().any(|n| n == plugin)
    }

    /// `plugin` 이 이 이벤트를 받아 무언가를 일으킬 때 넘길 체인.
    pub fn then(&self, plugin: &str) -> Origin {
        Origin::from_chain(self.chain.iter().cloned().chain([plugin.to_string()]))
    }

    /// 플러그인이 받는 모양.
    pub fn to_json(&self) -> Value {
        json!({
            "by": if self.is_user() { "user" } else { "plugin" },
            "chain": self.chain,
        })
    }

    /// [`ENV`] 에 넣을 값.
    pub fn to_env(&self) -> String {
        serde_json::to_string(&self.chain).unwrap_or_else(|_| "[]".into())
    }

    /// [`HEADER`] 에 넣을 값.
    pub fn to_header(&self) -> String {
        percent_encode(&self.to_env())
    }

    /// 환경변수나 헤더 값을 읽는다. 헤더는 퍼센트 인코딩, 환경변수는 JSON 그대로 — 둘 다 받는다.
    ///
    /// 못 읽으면 **사람이 일으킨 것**으로 본다. 손으로 잘못 넣은 값 때문에 명령이 실패하면
    /// 안 되고, 코어가 넘기는 값은 언제나 읽힌다.
    pub fn parse(raw: &str) -> Origin {
        let raw = raw.trim();
        if raw.is_empty() {
            return Origin::user();
        }
        let text = if raw.starts_with('[') {
            raw.to_string()
        } else {
            match percent_decode(raw) {
                Some(t) => t,
                None => return Origin::user(),
            }
        };
        serde_json::from_str::<Vec<String>>(&text)
            .map(Origin::from_chain)
            .unwrap_or_default()
    }

    /// 이 프로세스 환경에서 읽는다([`ENV`]).
    pub fn from_env() -> Origin {
        std::env::var(ENV)
            .map(|v| Origin::parse(&v))
            .unwrap_or_default()
    }
}

tokio::task_local! {
    static TASK: Origin;
}

static PROCESS: RwLock<Option<Origin>> = RwLock::new(None);

/// 이 프로세스 전체의 기본 체인. CLI 가 시작할 때 [`ENV`] 에서 읽어 둔다.
pub fn set_process_default(origin: Origin) {
    if let Ok(mut w) = PROCESS.write() {
        *w = Some(origin);
    }
}

/// 지금 일어나는 변경의 체인. 이벤트를 만들 때 이것을 싣는다.
pub fn current() -> Origin {
    TASK.try_with(Clone::clone).unwrap_or_else(|_| {
        PROCESS
            .read()
            .ok()
            .and_then(|r| r.clone())
            .unwrap_or_default()
    })
}

/// `fut` 안에서 일어나는 변경에 `origin` 을 붙인다(서버의 요청 하나, 백그라운드 작업 하나).
pub async fn scope<F: std::future::Future>(origin: Origin, fut: F) -> F::Output {
    TASK.scope(origin, fut).await
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_grow_only_with_new_names() {
        let a = Origin::user().then("a");
        let ab = a.then("b");
        assert_eq!(ab.chain(), ["a", "b"]);
        // 이미 있는 이름은 다시 붙지 않는다 — 목록이 끝없이 자라지 않는다.
        assert_eq!(ab.then("a").chain(), ["a", "b"]);
        assert!(ab.has("a") && ab.has("b") && !ab.has("c"));
        assert!(Origin::user().is_user());
    }

    #[test]
    fn the_env_and_header_forms_round_trip_including_korean_names() {
        let o = Origin::from_chain(["백업 쌓기", "a,b", "\"q\""]);
        assert_eq!(Origin::parse(&o.to_env()), o);
        let h = o.to_header();
        assert!(h.is_ascii(), "{h}");
        assert_eq!(Origin::parse(&h), o);
    }

    #[test]
    fn garbage_reads_as_a_person() {
        for bad in ["", "   ", "nope", "%ZZ", "[1,2]", "{\"a\":1}", "%5B"] {
            assert!(Origin::parse(bad).is_user(), "{bad:?}");
        }
    }

    #[test]
    fn json_shape_names_who() {
        assert_eq!(Origin::user().to_json(), json!({ "by": "user", "chain": [] }));
        assert_eq!(
            Origin::from_chain(["a"]).to_json(),
            json!({ "by": "plugin", "chain": ["a"] })
        );
    }

    #[tokio::test]
    async fn a_task_scope_wins_over_the_process_default() {
        // 프로세스 기본값은 전역이라 여기서 건드리지 않는다 — 없을 때는 사람이다.
        let inner = scope(Origin::from_chain(["srv"]), async { current() }).await;
        assert_eq!(inner.chain(), ["srv"]);
    }
}
