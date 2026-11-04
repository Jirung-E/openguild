//! DEV-458(REQ-026): 이벤트가 아니라 **시간이 되면** 도는 줄.
//!
//! ```toml
//! [[handlers]]
//!     id    = "마감 알림"
//!     every = "1h"                                  # 분 m · 시간 h(경과 시간) · 날 d(날짜), 최소 1m
//!     query = "status:[open, in_progress] due:overdue" # REQ-039 의 쿼리 문법
//!     of    = "quest"                               # quest(기본) · campaign · library · comment
//!     call  = "마감_알림"                           # fn 마감_알림(e) — e.items 가 맞는 목록
//! ```
//!
//! # 누가 시간을 재나
//!
//! 앱(`gui`)과 서버(`server`)만 — 프로세스가 계속 살아 있다. CLI 는 명령이 끝나면 꺼지므로 안 돈다.
//! [`Ticker`] 가 전용 스레드에서 30초마다 확인하고, [`super::runtime::PluginRuntime`] 이 버려지면(다시 불러오기 ·
//! 길드 바꾸기) 함께 멈춘다.
//!
//! # 두 번 돌지 않게
//!
//! 같은 기계에서 앱과 서버가 같은 길드를 열면 둘 다 시간을 잰다. 그래서 줄마다 **마지막으로 돈 시각**을 이 기계의
//! `~/.openguild/plugin-schedule/{길드}/{플러그인}.json` 에 적고, 고르는 동안은 옆의 `.lock` 을 잠근다 — 먼저 잡은 쪽이
//! 시각을 적고 돌린다. 다른 기계는 각자 돈다(동의가 기계마다인 것과 같다). 훅이 보는 데이터 폴더에 두지 않은 것은,
//! 훅이 지우거나 고치면 몰래 다시 돌기 때문이다.
//!
//! `d` 는 **날짜**로 센다 — `1d` 는 마지막으로 돈 날과 오늘(이 기계의 시간대)의 날짜가 다르면 돈다. 24시간마다는 `24h`.
//!
//! 기록이 없으면(처음 허용) 곧 한 번 돈다. 앱이 꺼져 있던 동안 놓친 것은 켜질 때 **한 번만** 돈다 — 쌓아 두었다가
//! 여러 번 돌지 않는다.
//!
//! # 줄이 받는 것
//!
//! 다른 줄과 같은 `fn 이름(e)`. `e.event` 는 `"every"`, `e.items` 는 맞는 목록(모양은 이벤트와 같다), `e.count` 는 개수, `e.line` ·
//! `e.every` · `e.query` · `e.of` 도 실린다. **맞는 것이 없으면 부르지 않는다.** `action` 줄이면 본문이 이 이벤트 JSON 이다.
use super::runtime::{Delivery, run_lines};
use super::{Handler, Plugin};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// 시간마다 도는 줄이 받는 이벤트 이름. 다른 줄의 패턴(`*` 포함)에는 걸리지 않는다 — [`Handler::wants`].
pub const EVENT: &str = "every";

/// 가장 짧은 간격 — 이보다 자주 돌면 알림이 소음이 되고 목록 조회가 쉬지 않는다.
const MIN_EVERY: Duration = Duration::from_secs(60);
/// 가장 긴 간격.
const MAX_EVERY: Duration = Duration::from_secs(366 * 24 * 3600);
/// 몇 초마다 확인하나.
const TICK: Duration = Duration::from_secs(30);
/// 켜진 뒤 첫 확인까지 — 앱이 길드를 여는 동안 붐비지 않게 조금 미룬다.
const FIRST_TICK: Duration = Duration::from_secs(5);

/// 쿼리를 걸 목록.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Of {
    #[default]
    Quest,
    Campaign,
    Library,
    Comment,
}

impl Of {
    pub fn as_str(self) -> &'static str {
        match self {
            Of::Quest => "quest",
            Of::Campaign => "campaign",
            Of::Library => "library",
            Of::Comment => "comment",
        }
    }

    fn target(self) -> crate::query::Target {
        match self {
            Of::Quest => crate::query::Target::Quest,
            Of::Campaign => crate::query::Target::Campaign,
            Of::Library => crate::query::Target::Library,
            Of::Comment => crate::query::Target::Comment,
        }
    }
}

/// `30m` · `1h` · `1d` · `90s` 같은 간격. `d` 의 실제 판단은 경과 시간이 아니라 날짜다([`is_ready`]).
pub fn parse_every(s: &str) -> Result<Duration, String> {
    let t = s.trim();
    let split = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let n: u64 = num.parse().map_err(|_| {
        crate::tf!(
            "`every = \"{s}\"` 를 읽지 못했습니다 — `30m` · `1h` · `1d` 처럼 숫자 뒤에 단위(s · m · h · d)를 붙이세요",
            "could not read `every = \"{s}\"` — write a number followed by a unit (s · m · h · d), like `30m`, `1h`, `1d`"
        )
    })?;
    let secs = match unit.trim() {
        "s" => n,
        "m" => n.saturating_mul(60),
        "h" => n.saturating_mul(3600),
        "d" => n.saturating_mul(86_400),
        _ => {
            return Err(crate::tf!(
                "`every = \"{s}\"` 의 단위를 모릅니다 — s · m · h · d 중 하나를 쓰세요",
                "unknown unit in `every = \"{s}\"` — use one of s · m · h · d"
            ));
        }
    };
    let d = Duration::from_secs(secs);
    if d < MIN_EVERY {
        return Err(crate::tf!(
            "`every = \"{s}\"` 는 너무 잦습니다 — 1m 보다 짧게는 안 됩니다",
            "`every = \"{s}\"` is too frequent — it cannot be shorter than 1m"
        ));
    }
    if d > MAX_EVERY {
        return Err(crate::tf!(
            "`every = \"{s}\"` 는 너무 깁니다 — 366d 까지입니다",
            "`every = \"{s}\"` is too long — up to 366d"
        ));
    }
    Ok(d)
}

/// `every` 가 날 단위(`1d`)면 그 날수.
fn calendar_days(every: &str) -> Option<i64> {
    every.trim().strip_suffix('d')?.trim().parse().ok()
}

/// 유닉스 초의 이 기계 시간대 날짜.
fn local_date(secs: u64) -> Option<chrono::NaiveDate> {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(secs as i64, 0)
        .earliest()
        .map(|t| t.date_naive())
}

/// 마지막으로 돈 때(`last`)에서 지금(`now`)까지 차례가 됐나 — `d` 는 날짜 차이, 나머지는 경과 시간.
fn is_ready(every: &str, period: Duration, last: u64, now: u64) -> bool {
    if let (Some(days), Some(from), Some(to)) =
        (calendar_days(every), local_date(last), local_date(now))
    {
        return (to - from).num_days() >= days;
    }
    now.saturating_sub(last) >= period.as_secs()
}

/// 시간마다 도는 줄의 검사 — 섞을 수 없는 칸과 `every` · `query`. `call` / `action` 은 [`super::validate`] 가 본다.
pub fn validate_line(h: &Handler) -> Result<(), String> {
    let every = h.every.as_deref().unwrap_or_default();
    parse_every(every)?;
    let clash = [
        ("pre", !h.pre.is_empty()),
        ("post", !h.post.is_empty()),
        ("when", !h.when.is_empty()),
        ("with", !h.with.is_empty()),
        ("wait", h.wait),
        ("group", h.group.is_some()),
        ("run_after", !h.run_after.is_empty()),
        ("run_before", !h.run_before.is_empty()),
        ("on_timeout", h.on_timeout.is_some()),
    ];
    if let Some((k, _)) = clash.iter().find(|(_, on)| *on) {
        return Err(crate::tf!(
            "`every` 줄에는 `{k}` 를 쓸 수 없습니다 — 거르기는 `query` 가 하고, 시간마다 따로 돕니다",
            "`{k}` cannot be used on an `every` line — `query` does the filtering, and the line runs on its own schedule"
        ));
    }
    if h.on_error == Some(super::Policy::Block) {
        return Err(crate::tf!(
            "`every` 줄에는 막을 것이 없습니다 — `on_error = \"block\"` 은 `pre` 줄에만 씁니다",
            "an `every` line has nothing to block — `on_error = \"block\"` is for `pre` lines"
        ));
    }
    let Some(q) = h.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) else {
        return Err(crate::tf!(
            "`every` 줄에는 `query` 가 있어야 합니다 — 무엇의 목록을 받을지 적으세요(예: `status:open due:overdue`)",
            "an `every` line needs a `query` — say which list it receives (e.g. `status:open due:overdue`)"
        ));
    };
    let of = h.of.unwrap_or_default();
    crate::query::parse_for(of.target(), q)
        .map_err(|e| crate::tf!("`query` — {}", "`query` — {}", e.to_string()))?;
    Ok(())
}

/// 기록 · 잠금 파일에서 줄을 부르는 이름 — `id`, 없으면 번호. 언어에 따라 바뀌는 "N번째 줄" 은 쓰지 않는다.
fn key(h: &Handler, i: usize) -> String {
    h.id.clone().unwrap_or_else(|| format!("#{}", i + 1))
}

/// 이 플러그인의 기록 파일(`.json`) — 잠금은 옆의 `.lock`.
fn state_file(p: &Plugin) -> crate::error::AppResult<PathBuf> {
    Ok(crate::user_dirs::openguild_home()?
        .join("plugin-schedule")
        .join(super::guild_data_key(&p.guild_root))
        .join(format!("{}.json", super::safe_segment(&p.def.name))))
}

fn every_lines(p: &Plugin) -> impl Iterator<Item = (usize, &Handler)> {
    p.def
        .handlers
        .iter()
        .enumerate()
        .filter(|(_, h)| h.every.is_some())
}

/// 지금(`now`, 유닉스 초) 돌 차례인 줄을 **잡는다** — 기록에 지금을 적고 그 번호들을 돌려준다. 다른 프로세스가 잠금을
/// 쥐고 있으면 이번에는 아무것도 안 잡는다(그쪽이 고르는 중이다).
fn claim_due(file: &Path, p: &Plugin, now: u64) -> Result<Vec<usize>, String> {
    let dir = file.parent().ok_or("schedule path")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(file.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    if lock.try_lock().is_err() {
        return Ok(Vec::new());
    }
    // DEV-471: 다 고른 뒤 **풀고** 나간다 — 닫기만으로 풀기를 맡기면, 그사이 다른 실이 자식 프로세스를 띄울 때
    // (fork 와 exec 사이) 자식이 같은 잠금을 잠깐 함께 쥐어 다음 확인이 "다른 쪽이 고르는 중" 으로 보고 건너뛴다.
    // flock 의 풀기는 함께 쥔 것까지 푼다. (전체 시험에서 이 시험이 가끔 실패했다.)
    let out = claim_locked(file, p, now);
    let _ = lock.unlock();
    out
}

/// [`claim_due`] 의 잠금 안쪽.
fn claim_locked(file: &Path, p: &Plugin, now: u64) -> Result<Vec<usize>, String> {
    let mut state: BTreeMap<String, u64> = std::fs::read_to_string(file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut due = Vec::new();
    for (i, h) in every_lines(p) {
        let every = h.every.as_deref().unwrap_or_default();
        let Ok(period) = parse_every(every) else {
            continue; // 적재가 막는다.
        };
        let k = key(h, i);
        let ready = state
            .get(&k)
            .is_none_or(|&last| is_ready(every, period, last, now));
        if ready {
            state.insert(k, now);
            due.push(i);
        }
    }
    if !due.is_empty() {
        let text = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, file).map_err(|e| e.to_string())?;
    }
    Ok(due)
}

/// 쿼리에 맞는 목록. **한 건의 모양은 같은 것의 이벤트와 같다**([`crate::events::payload`]) — 같은 댓글이 이벤트로
/// 올 때와 시간으로 올 때 칸 이름이 다르면 스크립트를 두 벌 써야 한다. 이벤트 칸에 목록에서만 쓸모 있는 칸을 더할 뿐이다.
async fn items(store: &crate::Store, of: Of, q: &str) -> crate::error::AppResult<Vec<Value>> {
    use crate::events::payload;
    Ok(match of {
        Of::Quest => {
            let query = crate::models::ListQuery {
                q: Some(q.to_string()),
                ..Default::default()
            };
            crate::services::quests::list(&store.index_pool, &query)
                .await?
                .into_iter()
                .map(|r| {
                    let mut item = payload::quest(&r);
                    item["desired_due"] = json!(r.desired_due);
                    item["required_due"] = json!(r.required_due);
                    item["campaign_due"] = json!(r.earliest_campaign_due);
                    item["comment_count"] = json!(r.comment_count);
                    item
                })
                .collect()
        }
        Of::Campaign => crate::services::campaigns::list_matching(&store.index_pool, None, Some(q))
            .await?
            .iter()
            .map(payload::campaign)
            .collect(),
        Of::Library => crate::ops::library::list_books_query(store, None, &[], Some(q))
            .await?
            .iter()
            .map(payload::book)
            .collect(),
        Of::Comment => crate::services::comments::search_global(
            &store.index_pool,
            crate::services::comments::GlobalCommentsFilter {
                query: Some(q),
                ..Default::default()
            },
        )
        .await?
        .into_iter()
        .map(|c| {
            let entry = crate::repo::comments::CommentEntry {
                id: c.entry_id as u64,
                ts: c.ts,
                author: c.author,
                body: c.body,
                parent_id: c.parent_id.map(|p| p as u64),
                reactions: c
                    .reactions
                    .split(',')
                    .filter(|r| !r.is_empty())
                    .map(str::to_string)
                    .collect(),
                discussion: c.discussion,
                resolved: c.resolved,
                pinned: c.pinned,
                edited_at: c.edited_at,
            };
            // 이벤트에서는 `e.target` 이다 — 목록에서는 한 건마다 달라 그 안에 싣는다.
            let mut item = payload::comment(&entry);
            item["target"] = payload::target(&c.scope, &c.slug);
            item
        })
        .collect(),
    })
}

/// 시간마다 도는 줄 하나를 `items` 로 부른다 — 실행기([`run_lines`])를 그대로 쓰고, 그 줄만 고른다.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_line(
    p: &Plugin,
    i: usize,
    items: Vec<Value>,
    guild: &str,
    cfg: &BTreeMap<String, Value>,
    subst: &BTreeMap<String, String>,
    delivery: &dyn Delivery,
    log: &Mutex<Vec<String>>,
    probe: Option<&mut super::runtime::Probe<'_>>,
) {
    let line = &p.def.handlers[i];
    let mut data = serde_json::Map::new();
    data.insert("line".into(), json!(key(line, i)));
    data.insert("every".into(), json!(line.every));
    data.insert("query".into(), json!(line.query));
    data.insert("of".into(), json!(line.of.unwrap_or_default().as_str()));
    data.insert("count".into(), json!(items.len()));
    data.insert("items".into(), Value::Array(items));
    let event = crate::events::Event {
        name: EVENT,
        phase: crate::events::Phase::Post,
        ts: crate::time::now_local_iso8601(),
        guild: guild.to_string(),
        ok: Some(true),
        error: None,
        data,
        origin: Default::default(),
        version: None,
    };
    run_lines(p, &event, cfg, subst, delivery, log, None, probe, |h| {
        std::ptr::eq(h, line)
    });
}

fn note(log: &Mutex<Vec<String>>, msg: String) {
    if let Ok(mut l) = log.lock() {
        l.push(msg);
    }
}

/// 한 번 확인한다 — 차례가 된 줄마다 목록을 읽어 부른다. `state_root` 가 있으면 기록을 거기 둔다(시험용).
pub(super) async fn tick(
    store: &crate::Store,
    plugins: &[Plugin],
    delivery: &dyn Delivery,
    log: &Mutex<Vec<String>>,
    now: u64,
    state_root: Option<&Path>,
) {
    for p in plugins {
        if every_lines(p).next().is_none() {
            continue;
        }
        let name = &p.def.name;
        let file = match state_root {
            Some(r) => r.join(format!("{}.json", super::safe_segment(name))),
            None => match state_file(p) {
                Ok(f) => f,
                Err(e) => {
                    note(
                        log,
                        crate::tf!(
                            "플러그인 '{name}' 시간 기록 자리를 못 찾았습니다 — {e}",
                            "plugin '{name}' could not find where to keep its schedule — {e}"
                        ),
                    );
                    continue;
                }
            },
        };
        let due = match claim_due(&file, p, now) {
            Ok(d) => d,
            Err(e) => {
                note(
                    log,
                    crate::tf!(
                        "플러그인 '{name}' 시간 기록을 못 적었습니다 — {e}",
                        "plugin '{name}' could not write its schedule — {e}"
                    ),
                );
                continue;
            }
        };
        if due.is_empty() {
            continue;
        }
        // BUG-389: 목록은 index 를 읽는다 — 켜 둔 앱 · 서버는 밖에서 고친 파일을 모르니, 넘기기 전에 맞춘다.
        // 도는 줄이 있을 때만 하니 주기적으로 파일을 훑지 않는다. 못 맞춰도 줄은 있는 것으로 돈다.
        if let Err(e) = crate::incremental::sync_if_changed(store).await {
            note(
                log,
                crate::tf!(
                    "플러그인 '{name}' — 파일을 다시 읽지 못해 있는 목록으로 돕니다: {e}",
                    "plugin '{name}' — could not re-read the files, running on the list as it is: {e}"
                ),
            );
        }
        let (cfg, subst) = super::runtime::values_for(p);
        for i in due {
            let h = &p.def.handlers[i];
            let q = h.query.as_deref().unwrap_or_default();
            let found = match items(store, h.of.unwrap_or_default(), q).await {
                Ok(v) => v,
                Err(e) => {
                    note(
                        log,
                        crate::tf!(
                            "플러그인 '{name}' {} — 목록을 못 읽었습니다: {e}",
                            "plugin '{name}' {} — could not read the list: {e}",
                            h.label(i)
                        ),
                    );
                    continue;
                }
            };
            // 맞는 것이 없으면 부르지 않는다.
            if found.is_empty() {
                continue;
            }
            let guild = store.guild_name.clone();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_line(p, i, found, &guild, &cfg, &subst, delivery, log, None);
            }));
            if r.is_err() {
                note(
                    log,
                    crate::tf!(
                        "플러그인 '{name}' {} 에서 패닉 — 이번 차례는 건너뜁니다",
                        "plugin '{name}' {} panicked — this round is skipped",
                        h.label(i)
                    ),
                );
            }
        }
    }
}

/// 시간을 재는 스레드. 버리면 멈춘다 — 돌던 줄은 끝까지 가고, 다음 확인은 없다.
pub struct Ticker {
    stop: Arc<(Mutex<bool>, Condvar)>,
}

impl Drop for Ticker {
    fn drop(&mut self) {
        let (m, cv) = &*self.stop;
        if let Ok(mut s) = m.lock() {
            *s = true;
        }
        cv.notify_all();
    }
}

/// 시간마다 도는 줄이 있을 때만 스레드를 띄운다.
pub(super) fn start(
    store: crate::Store,
    plugins: Arc<Vec<Plugin>>,
    delivery: Arc<dyn Delivery>,
    log: Arc<Mutex<Vec<String>>>,
) -> Option<Ticker> {
    if !plugins.iter().any(|p| every_lines(p).next().is_some()) {
        return None;
    }
    let stop = Arc::new((Mutex::new(false), Condvar::new()));
    let flag = stop.clone();
    let spawned = std::thread::Builder::new()
        .name("openguild-schedule".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    note(
                        &log,
                        crate::tf!(
                            "주기 실행을 못 띄웠습니다: {e}",
                            "could not start scheduled lines: {e}"
                        ),
                    );
                    return;
                }
            };
            let mut wait = FIRST_TICK;
            loop {
                let (m, cv) = &*flag;
                let Ok(guard) = m.lock() else { return };
                let Ok((guard, _)) = cv.wait_timeout_while(guard, wait, |stopped| !*stopped) else {
                    return;
                };
                if *guard {
                    return;
                }
                drop(guard);
                wait = TICK;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or_default();
                rt.block_on(tick(&store, &plugins, delivery.as_ref(), &log, now, None));
            }
        });
    if let Err(e) = spawned {
        eprintln!(
            "[openguild] {}",
            crate::tf!(
                "주기 실행 스레드를 못 띄웠습니다: {e}",
                "could not start the schedule thread: {e}"
            )
        );
        return None;
    }
    Some(Ticker { stop })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reads_units_and_refuses_extremes() {
        assert_eq!(parse_every("30m").unwrap(), Duration::from_secs(1800));
        assert_eq!(parse_every("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_every(" 2d ").unwrap(), Duration::from_secs(172_800));
        assert_eq!(parse_every("90s").unwrap(), Duration::from_secs(90));
        for bad in ["", "h", "10", "5x", "30s", "999d", "-1h"] {
            assert!(parse_every(bad).is_err(), "{bad}");
        }
    }

    const HEAD: &str =
        "name = \"s\"\nscope = [\"gui\"]\n[actions.out.post]\nurl = \"https://example.com\"\n";

    #[test]
    fn an_every_line_needs_a_valid_query_and_nothing_event_shaped() {
        let ok = format!(
            "{HEAD}[[handlers]]\nevery = \"1h\"\nquery = \"status:open\"\naction = \"out\"\n"
        );
        assert!(super::super::parse_def(&ok).is_ok());
        let cases = [
            ("every = \"1h\"\naction = \"out\"\n", "query"),
            (
                "every = \"1h\"\nquery = \"stauts:open\"\naction = \"out\"\n",
                "stauts",
            ),
            (
                "every = \"1h\"\nquery = \"urgency:1\"\nof = \"campaign\"\naction = \"out\"\n",
                "urgency",
            ),
            (
                "every = \"1h\"\nquery = \"status:open\"\npost = [\"quest.created\"]\naction = \"out\"\n",
                "post",
            ),
            (
                "every = \"1h\"\nquery = \"status:open\"\nwait = true\naction = \"out\"\n",
                "wait",
            ),
            (
                "query = \"status:open\"\npost = [\"quest.created\"]\naction = \"out\"\n",
                "every",
            ),
        ];
        for (line, word) in cases {
            let e = super::super::parse_def(&format!("{HEAD}[[handlers]]\n{line}"))
                .unwrap_err()
                .to_string();
            assert!(e.contains(word), "{line} → {e}");
        }
    }

    fn plugin(def: &str, guild: &Path) -> Plugin {
        Plugin {
            def: super::super::parse_def(def).unwrap(),
            dir: guild.join(".guild/plugins/s"),
            guild_root: guild.to_path_buf(),
            compiled: None,
            script_src: None,
            folder: Default::default(),
            imports: Vec::new(),
            source: None,
        }
    }

    fn tmp(label: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "og-sched-{label}-{}-{}",
            std::process::id(),
            crate::test_seq()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_line_is_claimed_once_per_period_and_a_held_lock_skips_the_round() {
        let root = tmp("claim");
        let p = plugin(
            &format!(
                "{HEAD}[[handlers]]\nid = \"매시\"\nevery = \"1h\"\nquery = \"status:open\"\naction = \"out\"\n\
                 [[handlers]]\nid = \"하루\"\nevery = \"24h\"\nquery = \"status:open\"\naction = \"out\"\n"
            ),
            &root,
        );
        let file = root.join("s.json");
        // 처음엔 둘 다.
        assert_eq!(claim_due(&file, &p, 1_000_000).unwrap(), [0, 1]);
        // 곧바로 다시 — 아무것도.
        assert!(claim_due(&file, &p, 1_000_030).unwrap().is_empty());
        // 한 시간 뒤 — 매시만. 오래 꺼져 있다 켜져도 한 번만이다.
        assert_eq!(claim_due(&file, &p, 1_003_600).unwrap(), [0]);
        assert_eq!(claim_due(&file, &p, 1_003_601 + 10 * 3600).unwrap(), [0]);
        // 다른 프로세스가 고르는 중이면 이번에는 건너뛴다.
        let held = std::fs::OpenOptions::new()
            .write(true)
            .open(file.with_extension("lock"))
            .unwrap();
        held.lock().unwrap();
        assert!(claim_due(&file, &p, 2_000_000).unwrap().is_empty());
        held.unlock().unwrap();
        assert_eq!(claim_due(&file, &p, 2_000_000).unwrap(), [0, 1]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `d` 는 날짜, `24h` 는 경과 시간 — 자정을 넘긴 몇 분과 같은 날 안의 스물세 시간이 갈린다.
    #[test]
    fn days_count_dates_and_hours_count_elapsed_time() {
        use chrono::TimeZone;
        let at = |d: u32, h: u32, m: u32| {
            chrono::Local
                .with_ymd_and_hms(2026, 3, d, h, m, 0)
                .earliest()
                .unwrap()
                .timestamp() as u64
        };
        let day = parse_every("1d").unwrap();
        let hours = parse_every("24h").unwrap();
        // 자정을 넘긴 20분 뒤.
        let (last, now) = (at(10, 23, 50), at(11, 0, 10));
        assert!(is_ready("1d", day, last, now));
        assert!(!is_ready("24h", hours, last, now));
        // 같은 날 안의 23시간 뒤.
        let (last, now) = (at(10, 0, 10), at(10, 23, 10));
        assert!(!is_ready("1d", day, last, now));
        assert!(!is_ready("24h", hours, last, now));
        // 25시간 뒤.
        assert!(is_ready("1d", day, at(10, 9, 0), at(11, 10, 0)));
        assert!(is_ready("24h", hours, at(10, 9, 0), at(11, 10, 0)));
        // 이틀 — 다음 날은 아직, 그다음 날 자정을 넘기면.
        let two = parse_every("2d").unwrap();
        assert!(!is_ready("2d", two, at(10, 23, 50), at(11, 23, 50)));
        assert!(is_ready("2d", two, at(10, 23, 50), at(12, 0, 10)));
        // 시계가 뒤로 가면 안 돈다.
        assert!(!is_ready("1d", day, at(11, 0, 10), at(10, 23, 50)));
    }

    /// 실제 길드 — 맞는 퀘스트만 실려 그 줄이 불리고, 맞는 것이 없으면 안 불린다.
    #[tokio::test]
    async fn comment_items_carry_the_body_and_where_it_is() {
        let root = tmp("comment-items");
        crate::repo::seed_guild_dir(&root).unwrap();
        let store = crate::Store::open(&root).await.unwrap();
        let q = crate::ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "댓글 달린 것".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        crate::ops::comments::add_comment_entry(
            &store,
            &q.quest_id,
            "claude".into(),
            "결정 필요".into(),
            None,
            true,
        )
        .await
        .unwrap();
        let found = items(&store, Of::Comment, "discussion:unresolved")
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["id"], 1);
        assert_eq!(
            found[0]["target"],
            serde_json::json!({ "kind": "quest", "id": q.quest_id })
        );
        assert_eq!(found[0]["author"], "claude");
        assert_eq!(found[0]["body"], "결정 필요");
        assert_eq!(found[0]["discussion"], true);
        assert_eq!(found[0]["resolved"], false);
        assert!(found[0]["edited_at"].is_null());
        assert_eq!(found[0]["reactions"], serde_json::json!([]));
        // 반응은 파일에만 쓰고 끝내면 안 된다 — 이 목록은 index 를 읽는다.
        crate::ops::comments::toggle_comment_reaction(&store, &q.quest_id, 1, "👍", "kim")
            .await
            .unwrap();
        crate::ops::comments::toggle_comment_reaction(&store, &q.quest_id, 1, "👍", "lee")
            .await
            .unwrap();
        let found = items(&store, Of::Comment, "discussion:unresolved")
            .await
            .unwrap();
        assert_eq!(
            found[0]["reactions"],
            serde_json::json!([{ "emoji": "👍", "authors": ["kim", "lee"] }])
        );
        crate::ops::comments::toggle_comment_reaction(&store, &q.quest_id, 1, "👍", "kim")
            .await
            .unwrap();
        crate::ops::comments::toggle_comment_reaction(&store, &q.quest_id, 1, "👍", "lee")
            .await
            .unwrap();
        let found = items(&store, Of::Comment, "discussion:unresolved")
            .await
            .unwrap();
        assert_eq!(found[0]["reactions"], serde_json::json!([]));
        assert!(
            items(&store, Of::Comment, "author:nobody")
                .await
                .unwrap()
                .is_empty()
        );
        // 줄을 읽을 때 댓글의 키로 검사한다.
        assert!(crate::query::parse_for(Of::Comment.target(), "status:open").is_err());
    }

    /// BUG-389: 켜 둔 앱이 모르는 사이 밖에서 고친 댓글도 다음 차례의 목록에 실린다.
    /// BUG-390: 시간 줄은 런타임 안에서 돈다 — 거기서 부른 `backup()` 도 실제로 백업을 만든다.
    #[tokio::test]
    async fn backup_runs_from_a_timed_line() {
        let root = tmp("tick-backup");
        crate::repo::seed_guild_dir(&root).unwrap();
        let store = crate::Store::open(&root).await.unwrap();
        crate::ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "퀘스트".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let mut p = plugin(
            "name = \"s\"\nscope = [\"gui\"]\nscripts = [\"main.rhai\"]\npermissions = [\"backup\"]\n[[handlers]]\nid = \"백업\"\nevery = \"1h\"\nquery = \"status:open\"\ncall = \"keep\"\n",
            &root,
        );
        p.compiled = Some(Arc::new(
            super::super::script::Script::compile_source("fn keep(e) { backup() }").unwrap(),
        ));
        let delivery = super::super::guild::WithGuild::new(
            Arc::new(super::super::testing::Recorder::default()),
            store.clone(),
            Arc::new(super::super::guild::Stderr),
        );
        let log = Mutex::new(Vec::new());
        let state = root.join("state");
        tick(
            &store,
            std::slice::from_ref(&p),
            &delivery,
            &log,
            5_000_000,
            Some(&state),
        )
        .await;
        assert!(log.lock().unwrap().is_empty(), "{:?}", log.lock().unwrap());
        assert_eq!(
            crate::snapshot::list_snapshots(&store.paths).unwrap().len(),
            1,
            "백업이 안 만들어졌다"
        );
    }

    #[tokio::test]
    async fn tick_reads_the_files_before_handing_over_the_list() {
        let root = tmp("tick-outside");
        crate::repo::seed_guild_dir(&root).unwrap();
        let store = crate::Store::open(&root).await.unwrap();
        let q = crate::ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "퀘스트".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        crate::ops::comments::add_comment_entry(
            &store,
            &q.quest_id,
            "claude".into(),
            "원래".into(),
            None,
            false,
        )
        .await
        .unwrap();
        let path = store.paths.comments_path(&q.quest_id);
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace("원래", "밖댓글")).unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(later).unwrap();

        let p = plugin(
            &format!(
                "{HEAD}[[handlers]]\nid = \"댓글\"\nevery = \"1h\"\nof = \"comment\"\nquery = \"author:claude\"\naction = \"out\"\n"
            ),
            &root,
        );
        let rec = super::super::testing::Recorder::default();
        let log = Mutex::new(Vec::new());
        let state = root.join("state");
        tick(
            &store,
            std::slice::from_ref(&p),
            &rec,
            &log,
            5_000_000,
            Some(&state),
        )
        .await;
        let sent = rec.take();
        assert_eq!(sent.len(), 1, "{:?}", log.lock().unwrap());
        assert_eq!(sent[0]["body"]["items"][0]["body"], "밖댓글");
    }

    /// 같은 문서가 이벤트로 올 때와 시간으로 올 때 칸 이름 · 값의 모양이 같다. 목록은 칸을 더할 수만 있다.
    #[tokio::test]
    async fn items_have_every_field_of_the_event_with_the_same_shape() {
        use crate::events::payload;
        let root = tmp("item-shapes");
        crate::repo::seed_guild_dir(&root).unwrap();
        let store = crate::Store::open(&root).await.unwrap();
        let quest = crate::ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "퀘스트".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let campaign = crate::ops::campaigns::create_campaign(
            &store,
            crate::models::CreateCampaignRequest {
                title: "캠페인".into(),
                description: None,
                started_at: Some("2030-01-01".into()),
                ended_at: None,
            },
        )
        .await
        .unwrap();
        let book = crate::ops::library::create_book(&store, "문서", "", "")
            .await
            .unwrap();
        let comment = crate::ops::comments::add_comment_entry(
            &store,
            &quest.quest_id,
            "claude".into(),
            "본문".into(),
            None,
            true,
        )
        .await
        .unwrap();
        let check = |label: &str, event: Value, item: &Value| {
            for (key, want) in event.as_object().unwrap() {
                assert_eq!(item.get(key), Some(want), "{label} 의 {key}");
            }
        };
        let quest = crate::services::quests::list(&store.index_pool, &Default::default())
            .await
            .unwrap()
            .remove(0);
        let query = format!("id:{}", quest.quest_id);
        let found = items(&store, Of::Quest, &query).await.unwrap();
        check("퀘스트", payload::quest(&quest), &found[0]);
        let found = items(&store, Of::Campaign, "start:2030-01-01")
            .await
            .unwrap();
        check("캠페인", payload::campaign(&campaign), &found[0]);
        let found = items(&store, Of::Library, "title:문서").await.unwrap();
        check("문서", payload::book(&book), &found[0]);
        let found = items(&store, Of::Comment, "author:claude").await.unwrap();
        check("댓글", payload::comment(&comment), &found[0]);
    }

    #[tokio::test]
    async fn campaign_comment_items_carry_reactions_too() {
        let root = tmp("campaign-comment-reactions");
        crate::repo::seed_guild_dir(&root).unwrap();
        let store = crate::Store::open(&root).await.unwrap();
        let c = crate::ops::campaigns::create_campaign(
            &store,
            crate::models::CreateCampaignRequest {
                title: "캠페인".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        crate::ops::campaign_comments::add_entry(
            &store,
            &c.campaign_slug,
            "claude".into(),
            "확인".into(),
            None,
        )
        .await
        .unwrap();
        crate::ops::campaign_comments::toggle_reaction(&store, &c.campaign_slug, 1, "🎉", "lee")
            .await
            .unwrap();
        let found = items(&store, Of::Comment, "author:claude").await.unwrap();
        assert_eq!(
            found[0]["reactions"],
            serde_json::json!([{ "emoji": "🎉", "authors": ["lee"] }])
        );
    }

    #[tokio::test]
    async fn tick_runs_the_due_line_with_the_matching_items() {
        let root = tmp("tick");
        crate::repo::seed_guild_dir(&root).unwrap();
        let store = crate::Store::open(&root).await.unwrap();
        crate::ops::create_quest(
            &store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "열린 것".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let p = plugin(
            &format!(
                "{HEAD}[[handlers]]\nid = \"열린 것\"\nevery = \"1h\"\nquery = \"status:open\"\naction = \"out\"\n\
                 [[handlers]]\nid = \"없는 것\"\nevery = \"1h\"\nquery = \"status:done\"\naction = \"out\"\n"
            ),
            &root,
        );
        let rec = super::super::testing::Recorder::default();
        let log = Mutex::new(Vec::new());
        let state = root.join("state");
        tick(
            &store,
            std::slice::from_ref(&p),
            &rec,
            &log,
            5_000_000,
            Some(&state),
        )
        .await;
        let sent = rec.take();
        assert!(log.lock().unwrap().is_empty(), "{:?}", log.lock().unwrap());
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0]["body"]["event"], EVENT);
        assert_eq!(sent[0]["body"]["line"], "열린 것");
        assert_eq!(sent[0]["body"]["count"], 1);
        assert_eq!(sent[0]["body"]["items"][0]["title"], "열린 것");
        // 다음 확인은 시간이 안 됐다.
        tick(
            &store,
            std::slice::from_ref(&p),
            &rec,
            &log,
            5_000_030,
            Some(&state),
        )
        .await;
        assert!(rec.take().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
