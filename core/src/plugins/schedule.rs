//! DEV-458(REQ-026): 이벤트가 아니라 **시간이 되면** 도는 줄.
//!
//! ```toml
//! [[handlers]]
//!     id    = "마감 알림"
//!     every = "1h"                                  # 분 m · 시간 h · 하루 d, 최소 1m
//!     query = "status:open,in_progress due:overdue" # REQ-039 의 쿼리 문법
//!     of    = "quest"                               # quest(기본) · campaign · library
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
//! 기록이 없으면(처음 허용) 곧 한 번 돈다. 앱이 꺼져 있던 동안 놓친 것은 켜질 때 **한 번만** 돈다 — 쌓아 두었다가
//! 여러 번 돌지 않는다.
//!
//! # 줄이 받는 것
//!
//! 다른 줄과 같은 `fn 이름(e)`. `e.event` 는 `"every"`, `e.items` 는 맞는 목록(짧은 요약), `e.count` 는 개수, `e.line` ·
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
}

impl Of {
    pub fn as_str(self) -> &'static str {
        match self {
            Of::Quest => "quest",
            Of::Campaign => "campaign",
            Of::Library => "library",
        }
    }

    fn target(self) -> crate::query::Target {
        match self {
            Of::Quest => crate::query::Target::Quest,
            Of::Campaign => crate::query::Target::Campaign,
            Of::Library => crate::query::Target::Library,
        }
    }
}

/// `30m` · `1h` · `1d` · `90s` 같은 간격.
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
    let mut state: BTreeMap<String, u64> = std::fs::read_to_string(file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut due = Vec::new();
    for (i, h) in every_lines(p) {
        let Ok(period) = parse_every(h.every.as_deref().unwrap_or_default()) else {
            continue; // 적재가 막는다.
        };
        let k = key(h, i);
        let ready = state
            .get(&k)
            .is_none_or(|&last| now.saturating_sub(last) >= period.as_secs());
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

/// 쿼리에 맞는 목록 — 한 건은 짧은 요약이다(본문 같은 큰 칸은 뺀다).
async fn items(store: &crate::Store, of: Of, q: &str) -> crate::error::AppResult<Vec<Value>> {
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
                    json!({
                        "id": r.quest_id,
                        "title": r.title,
                        "status": r.status_slug,
                        "type": r.type_prefix,
                        "urgency": r.urgency,
                        "tags": r.tags,
                        "desired_due": r.desired_due,
                        "required_due": r.required_due,
                        "campaign_due": r.earliest_campaign_due,
                        "updated_at": r.updated_at,
                        "comment_count": r.comment_count,
                    })
                })
                .collect()
        }
        Of::Campaign => crate::services::campaigns::list_matching(&store.index_pool, None, Some(q))
            .await?
            .into_iter()
            .map(|c| {
                json!({
                    "id": c.campaign_slug,
                    "title": c.title,
                    "status": c.status,
                    "start": c.started_at,
                    "end": c.ended_at,
                })
            })
            .collect(),
        Of::Library => crate::ops::library::list_books_query(store, None, &[], Some(q))
            .await?
            .into_iter()
            .map(|d| {
                json!({
                    "id": d.book_id(),
                    "title": d.title,
                    "folder": d.path,
                    "tags": d.tags,
                    "updated_at": d.updated_at,
                })
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
                 [[handlers]]\nid = \"매일\"\nevery = \"1d\"\nquery = \"status:open\"\naction = \"out\"\n"
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

    /// 실제 길드 — 맞는 퀘스트만 실려 그 줄이 불리고, 맞는 것이 없으면 안 불린다.
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
