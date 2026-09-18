//! REQ-025: 핸들러 줄의 조건 `when` — 이 줄이 이 이벤트에서 불릴지 거른다.
//!
//! ```toml
//! [[handlers]]
//!     post = ["quest.status_changed"]
//!     with = ["subject"]
//!     call = "on_done"
//!     [handlers.when]
//!         ok               = true
//!         "change.to"      = ["done", "closed"]   # 목록 중 하나
//!         "subject.tags"   = "notify"             # 필드가 목록이면 포함
//! ```
//!
//! # 조건은 거름망, 함수는 할 일
//!
//! `when` 을 통과해야 줄이 실행된다. 스크립트가 있어도 순서가 하나뿐이라(조건 → 함수) 어느 쪽이
//! 먼저인지 헷갈릴 일이 없다. 더 복잡한 조건(또는, 크기 비교, 설정값에 따라)은 함수 안의 `if` 로
//! 쓴다 — 여기에 조건 문법을 계속 늘리면 결국 DSL 을 만들게 된다.
//!
//! 액션 줄(스크립트 없이 바로 보내는 줄)에는 이것이 **조건을 걸 유일한 수단**이다.
//!
//! # 경로
//!
//! 점으로 이어 쓴다. 맨 앞 조각이 `with` 에 적은 이름이면 **읽어 온 데이터**를, 아니면 이벤트
//! JSON 을 본다(`ok`, `change.to`, `comment.body` …). 없는 경로는 "안 맞음" 이고, 그 이벤트에
//! 있을 수 없는 경로는 적재 때 거절한다([`validate`]).

use crate::error::{AppError, AppResult};
use serde_json::Value;
use std::collections::BTreeMap;

/// 줄 하나의 조건 — 전부 만족해야 한다(AND).
pub type When = BTreeMap<String, Value>;

/// 조건을 만족하나. `extra` 는 `with` 로 읽어 온 값(이름 → 값).
pub fn matches(cond: &When, event: &Value, extra: &BTreeMap<String, Value>) -> bool {
    cond.iter().all(|(path, want)| {
        let got = lookup(path, event, extra);
        one_matches(got, want)
    })
}

/// 경로 하나를 찾는다. `with` 이름이 먼저다 — 같은 이름이면 읽어 온 데이터를 본다.
fn lookup<'a>(path: &str, event: &'a Value, extra: &'a BTreeMap<String, Value>) -> Option<&'a Value> {
    let mut parts = path.split('.');
    let head = parts.next()?;
    let mut cur = match extra.get(head) {
        Some(v) => v,
        None => event.get(head)?,
    };
    for p in parts {
        cur = cur.get(p)?;
    }
    Some(cur)
}

/// 값 하나의 판정.
///
/// - 적은 값이 목록이면 **그중 하나**면 된다.
/// - 실제 값이 목록이면 **포함**하면 된다(태그처럼).
/// - 둘 다 목록이면 하나라도 겹치면 된다.
fn one_matches(got: Option<&Value>, want: &Value) -> bool {
    let Some(got) = got else {
        return false; // 없는 경로는 안 맞는다 — null 과 구분하지 않는다.
    };
    let wants: Vec<&Value> = match want {
        Value::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    match got {
        Value::Array(list) => wants.iter().any(|w| list.iter().any(|g| g == *w)),
        other => wants.contains(&other),
    }
}

/// 적재 때 검사 — 값의 모양과 경로의 맨 앞 조각.
///
/// 경로가 그 이벤트에 있을 수 없으면 거절한다. 오타 난 조건이 조용히 "안 맞음" 이 되면 왜 안
/// 도는지 알 수 없다.
pub fn validate(
    cond: &When,
    with: &[String],
    patterns: &[String],
    phase: crate::events::Phase,
    who: &str,
) -> AppResult<()> {
    let bad = |m: String| Err(AppError::BadRequest(format!("{who}: {m}")));
    let roots = allowed_roots(patterns, phase);
    for (path, want) in cond {
        if path.trim().is_empty() {
            return bad("`when` 의 경로가 비어 있습니다".into());
        }
        if !scalar_or_list(want) {
            return bad(format!(
                "`when` 의 `{path}` 값은 글자·숫자·참거짓이거나 그 목록이어야 합니다"
            ));
        }
        let head = path.split('.').next().unwrap_or(path);
        if with.iter().any(|w| w == head) {
            continue;
        }
        if crate::events::COMMON_FIELDS.contains(&head) || roots.contains(&head) {
            continue;
        }
        let mut can: Vec<&str> = with.iter().map(String::as_str).collect();
        can.extend(roots.iter().copied());
        can.extend(crate::events::COMMON_FIELDS.iter().copied());
        return bad(format!(
            "`when` 의 `{path}` — 이 줄의 이벤트에는 `{head}` 가 없습니다. 쓸 수 있는 것: {}",
            can.join(", ")
        ));
    }
    Ok(())
}

/// 이 줄이 걸리는 이벤트들이 실을 수 있는 칸.
fn allowed_roots(patterns: &[String], phase: crate::events::Phase) -> Vec<&'static str> {
    use crate::events::{Phase, names};
    let candidates: &[&str] = match phase {
        Phase::Pre => names::PRE_CAPABLE,
        Phase::Post => names::ALL,
    };
    let mut out: Vec<&'static str> = Vec::new();
    for n in candidates {
        let hit = patterns.iter().any(|p| {
            let full = match phase {
                Phase::Pre => format!("pre:{p}"),
                Phase::Post => p.clone(),
            };
            names::matches(&full, n, phase)
        });
        if hit {
            for r in crate::events::payload_roots_of(n) {
                if !out.contains(r) {
                    out.push(r);
                }
            }
        }
    }
    out
}

fn scalar_or_list(v: &Value) -> bool {
    match v {
        Value::String(_) | Value::Number(_) | Value::Bool(_) => true,
        Value::Array(a) => !a.is_empty() && a.iter().all(scalar_or_list),
        _ => false,
    }
}
