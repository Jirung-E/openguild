//! DEV-374: 플러그인이 구독하는 **이벤트**.
//!
//! `ops` 계층이 성공/실패 지점에서 직접 낸다(emit). journal 을 훔쳐보는 값싼
//! 방법을 먼저 검토했다가 접었다 — 자세한 배경은 [[DEV-373]] 이지만 요점은
//! 셋이다:
//!
//! 1. journal 은 **의도**를 하기 전에 쓴다. 뒤에서 실패해도 남으므로 구독하면
//!    실패한 시도까지 나간다.
//! 2. `journal::append` 54곳 중 53곳이 `result` 를 `None` 으로 넘긴다 —
//!    **결과가 없다**. `create_quest` 의 args 에는 새로 부여된 `DEV-007` 이 없다.
//! 3. journal 은 AOF(복구 replay)라 스냅샷 때 truncate 되고 replay 억제가
//!    붙는다. 겸용하면 **복원 중 과거 이벤트가 쏟아진다**.
//!
//! 무엇보다 journal 로는 `pre`/`post` 를 구분할 자리가 없다.
//!
//! # 공개 계약
//!
//! **내부 함수명을 그대로 노출하지 않는다.** 이름과 페이로드 모두 여기서
//! 정의한 것이 계약이고, 내부 구조는 자유롭게 바꿀 수 있다. 내부 op 이름은
//! 동사 위치가 뒤섞이고(`add_attachment` vs `campaign_checklist_add`) 같은
//! 개념을 다르게 부르며(`add_comment_entry` vs `add_campaign_comment`),
//! `toggle_*` 6종은 해결/해제를 구분조차 못 한다.
//!
//! # 구독자가 없으면 아무 일도 안 한다
//!
//! [`Events::wants`] 가 false 면 페이로드를 **만들지도 않는다**. 그래서 플러그인을
//! 쓰지 않는 사용자와 기존 테스트가 비용을 치르지 않는다.

pub mod catalog;
pub mod names;
pub mod payload;
#[cfg(test)]
mod tests;

use serde_json::{Map, Value, json};
use std::sync::{Arc, RwLock};

/// 이벤트가 mutation 의 어느 쪽에서 났는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// mutation 직전. **관찰만 한다** — 거부(veto)는 지원하지 않는다.
    /// 훅이 죽으면 길드가 멈추기 때문이다(로컬 우선 도구에서 치명적).
    Pre,
    /// mutation 이 끝난 뒤. 성공/실패가 `ok` 로 실린다.
    Post,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Pre => "pre",
            Phase::Post => "post",
        }
    }
}

/// 플러그인에게 전달되는 한 건.
#[derive(Debug, Clone)]
pub struct Event {
    /// 공개 이름 — `quest.created` 처럼 `리소스.동사`.
    pub name: &'static str,
    pub phase: Phase,
    /// ISO 8601 + TZ offset.
    pub ts: String,
    /// 길드 이름(길드 루트 디렉터리명).
    pub guild: String,
    /// `Phase::Post` 에서만 실린다. **`pre` 에는 아예 없다** — 아직 결과가
    /// 없는데 `false` 를 실으면 실패로 오해된다.
    pub ok: Option<bool>,
    /// 실패했을 때의 메시지. 성공이면 없다.
    pub error: Option<String>,
    /// 리소스별 본문 — `{"quest": {...}, "comment": {...}}`.
    pub data: Map<String, Value>,
}

impl Event {
    /// 플러그인이 받는 JSON. **이 모양이 계약이다** — 내부 구조체를 그대로
    /// 흘리지 않는다(필드를 바꾸면 플러그인이 깨진다).
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("event".into(), json!(self.name));
        m.insert("phase".into(), json!(self.phase.as_str()));
        m.insert("ts".into(), json!(self.ts));
        m.insert("guild".into(), json!(self.guild));
        if let Some(ok) = self.ok {
            m.insert("ok".into(), json!(ok));
        }
        if let Some(e) = &self.error {
            m.insert("error".into(), json!(e));
        }
        for (k, v) in &self.data {
            m.insert(k.clone(), v.clone());
        }
        Value::Object(m)
    }
}

/// 이벤트를 실제로 받아 처리하는 쪽. 플러그인 적재기가 구현한다([[DEV-375]]).
pub trait EventSink: Send + Sync {
    /// 이 이벤트를 구독하는 곳이 있나. **페이로드를 만들기 전에** 물어본다.
    fn wants(&self, name: &str, phase: Phase) -> bool;
    /// 전달. **여기서 오래 붙들면 안 된다** — 호출자는 mutation 경로다.
    fn dispatch(&self, event: Event);
    /// 아직 나가지 못한 것들에 짧은 유예를 준다. 곧 끝나는 프로세스(CLI)가
    /// 종료 직전에 부른다. 시간 안에 다 나갔으면 `true`.
    ///
    /// 기본값은 "붙들 게 없다" — 동기 구현은 이걸 그대로 쓰면 된다.
    fn drain(&self, _budget: std::time::Duration) -> bool {
        true
    }
}

/// `Store` 가 들고 다니는 이벤트 출구. sink 가 없으면 전부 no-op.
#[derive(Clone, Default)]
pub struct Events(Arc<RwLock<Option<Arc<dyn EventSink>>>>);

impl std::fmt::Debug for Events {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.has_sink() {
            "Events(sink)"
        } else {
            "Events(none)"
        })
    }
}

impl Events {
    /// 플러그인 적재기가 자기를 꽂는다. 없으면 이벤트는 만들어지지도 않는다.
    pub fn set_sink(&self, sink: Arc<dyn EventSink>) {
        if let Ok(mut w) = self.0.write() {
            *w = Some(sink);
        }
    }

    pub fn clear_sink(&self) {
        if let Ok(mut w) = self.0.write() {
            *w = None;
        }
    }

    pub fn has_sink(&self) -> bool {
        self.0.read().map(|r| r.is_some()).unwrap_or(false)
    }

    pub fn wants(&self, name: &str, phase: Phase) -> bool {
        match self.0.read() {
            Ok(r) => r.as_ref().is_some_and(|s| s.wants(name, phase)),
            // 잠금이 오염됐으면 이벤트를 포기한다 — 길드 동작을 막지 않는다.
            Err(_) => false,
        }
    }

    pub fn dispatch(&self, event: Event) {
        if let Ok(r) = self.0.read()
            && let Some(s) = r.as_ref()
        {
            s.dispatch(event);
        }
    }

    /// 종료 직전에 부른다. sink 가 없으면 기다릴 것도 없다.
    pub fn drain(&self, budget: std::time::Duration) -> bool {
        match self.0.read() {
            Ok(r) => r.as_ref().is_none_or(|s| s.drain(budget)),
            Err(_) => true,
        }
    }
}
