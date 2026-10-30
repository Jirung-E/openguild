//! DEV-414: 한 이벤트에 여러 줄이 걸릴 때의 순서 — Bevy 의 `before` / `after` 와 같은 생각.
//!
//! 줄은 `run_after = ["다른 줄 id"]` · `run_before = [...]` 로 **같은 플러그인 안의** 다른 줄과의 순서를 적는다.
//! 플러그인끼리의 순서는 정의 맨 위에 같은 이름의 칸으로 다른 플러그인의 `name` 을 적는다([`plugin_order`], DEV-469).
//!
//! # 적지 않은 줄끼리는 적힌 순서
//!
//! 순서를 적지 않은 줄끼리는 **적힌 순서대로** 돈다 — 그동안 만든 플러그인이 그대로 돌게, 그리고 결과가 늘 하나로
//! 정해지게. 그래서 바뀌기 전(`pre`) 줄 둘이 같은 칸(예: 제목)을 고쳐도 문제가 없다: 이 순서에서 **뒤에 도는 줄의
//! 값이 남는다.** (BOOK-004 가 "그때 다시 정한다" 고 미뤄 둔 것을 이렇게 정했다 — admin 2026-10-27 "다 진행해라".)
//!
//! 정렬은 위상 정렬이고, 동시에 돌 수 있는 줄 가운데 **적힌 순서가 앞인 것**을 먼저 고른다. 순서를 하나도 안 적으면
//! 적힌 순서 그대로다.
//!
//! # 같은 자리에서 도는 줄끼리만
//!
//! 줄은 도는 자리가 셋으로 나뉜다 — 바뀌기 전(`pre`, 변경 안에서), 바뀐 뒤(`post`, 전용 스레드), 바뀐 뒤 기다리는 줄
//! (`post` + `wait`, 잠금이 풀린 뒤 부른 자리에서). 자리가 다르면 이미 순서가 정해져 있거나(`pre` 가 늘 먼저) 서로
//! 다른 실이라 맞출 수 없다. 그래서 자리가 다른 줄 사이의 순서는 적재 때 오류다.
use super::{Handler, Plugin, PluginDef};
use std::collections::{BTreeSet, HashMap};

/// 줄이 도는 자리.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Pre,
    Post,
    PostWait,
    /// DEV-458: 시간마다 — 줄마다 따로 돈다.
    Every,
}

fn slot(h: &Handler) -> Slot {
    if h.every.is_some() {
        Slot::Every
    } else if !h.pre.is_empty() {
        Slot::Pre
    } else if h.wait {
        Slot::PostWait
    } else {
        Slot::Post
    }
}

fn slot_name(s: Slot) -> String {
    match s {
        Slot::Pre => crate::tf!("바뀌기 전(pre)", "before (pre)"),
        Slot::Post => crate::tf!("바뀐 뒤(post)", "after (post)"),
        Slot::Every => crate::tf!("시간마다(every)", "scheduled (every)"),
        Slot::PostWait => crate::tf!(
            "바뀐 뒤 기다리는 줄(post + wait)",
            "after, waited for (post + wait)"
        ),
    }
}

/// 줄을 부를 순서 — `def.handlers` 의 번호들. 검증을 거친 정의라면 실패하지 않는다.
pub fn run_order(def: &PluginDef) -> Result<Vec<usize>, String> {
    let n = def.handlers.len();
    let by_id: HashMap<&str, usize> = def
        .handlers
        .iter()
        .enumerate()
        .filter_map(|(i, h)| h.id.as_deref().map(|id| (id, i)))
        .collect();
    // DEV-415: 묶음 이름 → 든 줄들.
    let members = |gid: &str| -> Vec<usize> {
        def.handlers
            .iter()
            .enumerate()
            .filter(|(_, h)| h.group.as_deref() == Some(gid))
            .map(|(i, _)| i)
            .collect()
    };
    let is_group = |x: &str| def.groups.iter().any(|g| g.id == x);
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for (i, h) in def.handlers.iter().enumerate() {
        let who = h.label(i);
        // 줄이 직접 적은 것, 그리고 든 묶음이 적은 것(`inherited`). 묶음의 순서는 그 묶음의 줄 가운데 **같은 자리**에서
        // 도는 줄에만 걸린다 — 묶음에는 `pre` 줄과 `post` 줄이 섞일 수 있다.
        let group = h
            .group
            .as_deref()
            .and_then(|g| def.groups.iter().find(|x| x.id == g));
        let mut refs: Vec<(&str, bool, bool)> = Vec::new(); // (이름, 이 줄이 뒤인가, 묶음에서 왔나)
        refs.extend(h.run_after.iter().map(|x| (x.as_str(), true, false)));
        refs.extend(h.run_before.iter().map(|x| (x.as_str(), false, false)));
        if let Some(g) = group {
            refs.extend(g.run_after.iter().map(|x| (x.as_str(), true, true)));
            refs.extend(g.run_before.iter().map(|x| (x.as_str(), false, true)));
        }
        let mut edges = Vec::new();
        for (x, after, inherited) in refs {
            let targets: Vec<usize> = if let Some(&j) = by_id.get(x) {
                if inherited {
                    // 묶음이 적은 순서는 그 줄 자신 · 자리가 다른 줄에는 그냥 안 걸린다.
                    if j == i || slot(&def.handlers[j]) != slot(h) {
                        continue;
                    }
                }
                vec![j]
            } else if is_group(x) {
                if h.group.as_deref() == Some(x) {
                    return Err(crate::tf!(
                        "{who}: 자기가 든 묶음 `{x}` 와는 순서를 적을 수 없습니다",
                        "{who}: a line cannot be ordered against its own group `{x}`"
                    ));
                }
                members(x)
                    .into_iter()
                    .filter(|&j| j != i && slot(&def.handlers[j]) == slot(h))
                    .collect()
            } else {
                return Err(crate::tf!(
                    "{who}: 순서에 적은 `{x}` 가 없습니다 — 같은 플러그인의 줄 `id` 나 묶음 `id` 를 적으세요",
                    "{who}: `{x}` named in the order does not exist — use a line or group `id` from the same plugin"
                ));
            };
            for j in targets {
                edges.push(if after { (j, i) } else { (i, j) });
            }
        }
        for (a, b) in edges {
            if a == b {
                return Err(crate::tf!(
                    "{who}: 자기 자신과의 순서는 적을 수 없습니다",
                    "{who}: a line cannot be ordered against itself"
                ));
            }
            let (sa, sb) = (slot(&def.handlers[a]), slot(&def.handlers[b]));
            if sa != sb {
                let (x, y) = (def.handlers[a].label(a), def.handlers[b].label(b));
                return Err(crate::tf!(
                    "{who}: `{x}` 은 {} 줄이고 `{y}` 는 {} 줄입니다 — 도는 자리가 달라 순서를 정할 수 없습니다",
                    "{who}: `{x}` is a {} line and `{y}` is a {} line — they run in different places, so they cannot be ordered",
                    slot_name(sa),
                    slot_name(sb)
                ));
            }
            if !succ[a].contains(&b) {
                succ[a].push(b);
                indeg[b] += 1;
            }
        }
    }
    // 동시에 돌 수 있는 줄 가운데 적힌 순서가 앞인 것부터.
    let mut ready: BTreeSet<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut out = Vec::with_capacity(n);
    while let Some(i) = ready.pop_first() {
        out.push(i);
        for &j in &succ[i] {
            indeg[j] -= 1;
            if indeg[j] == 0 {
                ready.insert(j);
            }
        }
    }
    if out.len() < n {
        // 남은 줄에는 고리 뒤에 매달린 줄도 있다 — 뒤로 이어지는 줄이 더 없는 것부터 걷어 내면 고리만 남는다.
        let mut left: Vec<bool> = (0..n).map(|i| indeg[i] > 0).collect();
        loop {
            let tail = (0..n).find(|&i| left[i] && !succ[i].iter().any(|&j| left[j]));
            match tail {
                Some(i) => left[i] = false,
                None => break,
            }
        }
        let ring: Vec<String> = (0..n)
            .filter(|&i| left[i])
            .map(|i| def.handlers[i].label(i))
            .collect();
        return Err(crate::tf!(
            "줄 순서가 돌고 돕니다 — {} 의 run_after · run_before 가 서로를 가리킵니다",
            "the line order is circular — the run_after / run_before of {} point at one another",
            ring.join(", ")
        ));
    }
    Ok(out)
}

/// `check` 의 경고 — 순서를 적었는데 두 줄이 **같은 이벤트에 함께 걸리는 일이 없으면** 그 순서는 아무 일도 안 한다.
/// 대개 이벤트 이름을 잘못 적었거나 줄을 옮기다 남은 것이다.
pub fn unused_orders(def: &PluginDef) -> Vec<String> {
    use crate::events::names as ev;
    let by_id: HashMap<&str, usize> = def
        .handlers
        .iter()
        .enumerate()
        .filter_map(|(i, h)| h.id.as_deref().map(|id| (id, i)))
        .collect();
    let overlap = |a: &Handler, b: &Handler| {
        let phase = a.phase();
        let names: &[&str] = match phase {
            crate::events::Phase::Pre => ev::PRE_CAPABLE,
            crate::events::Phase::Post => ev::ALL,
        };
        names.iter().any(|n| a.wants(n, phase) && b.wants(n, phase))
    };
    let mut out = Vec::new();
    for (i, h) in def.handlers.iter().enumerate() {
        for id in h.run_after.iter().chain(&h.run_before) {
            // DEV-415: 묶음 이름이면 그 묶음의 줄 가운데 하나라도 함께 걸리면 된다.
            let targets: Vec<usize> = match by_id.get(id.as_str()) {
                Some(&j) => vec![j],
                None => def
                    .handlers
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| x.group.as_deref() == Some(id.as_str()))
                    .map(|(j, _)| j)
                    .collect(),
            };
            if targets.is_empty() {
                continue;
            }
            if !targets.iter().any(|&j| overlap(h, &def.handlers[j])) {
                out.push(crate::tf!(
                    "{}: `{id}` 와의 순서를 적었지만 두 줄이 같은 이벤트에 함께 걸리는 일이 없습니다 — 이 순서는 아무 일도 안 합니다",
                    "{}: it is ordered against `{id}`, but the two lines never fire on the same event — the order does nothing",
                    h.label(i)
                ));
            }
        }
    }
    out
}

/// DEV-469: **플러그인끼리**의 순서 — 정의 맨 위의 `run_after` · `run_before`(다른 플러그인의 `name`). 줄 순서와
/// 같은 생각이다: 적지 않은 것끼리는 받은 순서(적재 순서 — 길드 폴더 이름순, 그 뒤 소스)를 지키고, 동시에 돌 수
/// 있는 것 가운데 앞인 것을 먼저 고른다.
///
/// 순서는 바뀌기 전(`pre`) 단계에서 뜻이 크다 — 앞 플러그인이 막거나 대신 처리하면 뒤 플러그인은 그 이벤트를 못
/// 본다. 없는 이름은 넘어간다(그 플러그인이 이 길드에 없을 수 있다). 순서가 돌고 돌면 그 플러그인들은 받은 순서로
/// 두고 경고를 돌려준다 — 적재를 막으면 남이 만든 두 플러그인을 같이 못 쓰게 된다.
pub fn plugin_order(plugins: &mut Vec<Plugin>) -> Vec<CrossWarning> {
    let n = plugins.len();
    let idx: HashMap<&str, usize> = plugins
        .iter()
        .enumerate()
        .map(|(i, p)| (p.def.name.as_str(), i))
        .collect();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    let mut edge = |a: usize, b: usize| {
        if a != b && !succ[a].contains(&b) {
            succ[a].push(b);
            indeg[b] += 1;
        }
    };
    for (i, p) in plugins.iter().enumerate() {
        for x in &p.def.run_after {
            if let Some(&j) = idx.get(x.as_str()) {
                edge(j, i);
            }
        }
        for x in &p.def.run_before {
            if let Some(&j) = idx.get(x.as_str()) {
                edge(i, j);
            }
        }
    }
    let mut ready: BTreeSet<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut out = Vec::with_capacity(n);
    while let Some(i) = ready.pop_first() {
        out.push(i);
        for &j in &succ[i] {
            indeg[j] -= 1;
            if indeg[j] == 0 {
                ready.insert(j);
            }
        }
    }
    let mut warnings = Vec::new();
    if out.len() < n {
        let stuck: Vec<usize> = (0..n).filter(|i| !out.contains(i)).collect();
        let names: Vec<String> = stuck.iter().map(|&i| plugins[i].def.name.clone()).collect();
        warnings.push(CrossWarning {
            message: crate::tf!(
                "플러그인 순서가 돌고 돕니다 — {} 의 run_after · run_before 가 서로를 가리킵니다. 이 플러그인들은 읽은 순서로 돕니다",
                "the plugin order is circular — the run_after / run_before of {} point at one another. These plugins run in load order",
                names.join(", ")
            ),
            plugins: names,
        });
        out.extend(stuck);
    }
    let mut slots: Vec<Option<Plugin>> = std::mem::take(plugins).into_iter().map(Some).collect();
    plugins.extend(out.into_iter().filter_map(|i| slots[i].take()));
    warnings
}

/// DEV-469: 줄의 `command` 를 이 플러그인의 설정값으로 푼 것(앞뒤 공백 없이). 못 풀거나 비면 `None` — 그 줄은
/// 안 돈다. 설정값(`config`)을 보므로 시험의 `set_config` 도 먹는다.
pub fn resolved_command(
    command: &str,
    cfg: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Option<String> {
    let subst: std::collections::BTreeMap<String, String> = cfg
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect();
    super::expand_env_with(command, &subst)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// DEV-469: 댓글 본문이 그 명령인가 — 앞뒤 공백은 무시하고 나머지는 정확히 같아야 한다.
pub fn command_matches(
    command: &str,
    event: &serde_json::Value,
    cfg: &std::collections::BTreeMap<String, serde_json::Value>,
) -> bool {
    let Some(want) = resolved_command(command, cfg) else {
        return false;
    };
    event
        .pointer("/comment/body")
        .and_then(|v| v.as_str())
        .is_some_and(|b| b.trim() == want)
}

/// DEV-469: 여러 플러그인에 걸친 경고 하나와 걸린 플러그인들.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossWarning {
    pub plugins: Vec<String>,
    pub message: String,
}

/// DEV-469: 한 자리(컴포넌트)에서 같이 도는 플러그인들 사이의 경고 — 순서가 돌고 도는 것, 그리고 **같은 댓글
/// 명령을 둘 이상이 잡는 것.** 명령은 줄에 `command` 로 적은 것만 안다(스크립트 안의 `if` 는 못 본다).
///
/// 겹치면 둘 다 돌 수도, 앞의 것만 돌 수도 있다 — 앞 플러그인이 막거나 대신 처리하면 거기서 끝나고, 본문만 바꾸면
/// 뒤 플러그인도 **원래 본문을** 보고 또 처리한다. 어느 쪽이든 쓰는 사람이 기대한 것이 아니기 쉽다.
pub fn cross_warnings(plugins: &[Plugin]) -> Vec<CrossWarning> {
    let mut ordered = plugins.to_vec();
    let mut out = plugin_order(&mut ordered);
    // 명령 → 그 명령을 잡는 플러그인(순서대로, 한 번씩).
    let mut by_command: Vec<(String, Vec<String>)> = Vec::new();
    for p in &ordered {
        let (cfg, _) = super::runtime::values_for(p);
        for h in &p.def.handlers {
            let Some(c) = h.command.as_deref().and_then(|c| resolved_command(c, &cfg)) else {
                continue;
            };
            match by_command.iter_mut().find(|(k, _)| *k == c) {
                Some((_, names)) if names.contains(&p.def.name) => {}
                Some((_, names)) => names.push(p.def.name.clone()),
                None => by_command.push((c, vec![p.def.name.clone()])),
            }
        }
    }
    for (c, names) in by_command {
        if names.len() < 2 {
            continue;
        }
        out.push(CrossWarning {
            message: crate::tf!(
                "댓글 명령 `{c}` 를 {} 가 같이 받습니다 — {} 가 먼저 돌고, 그쪽이 막거나 대신 처리하면 뒤 플러그인은 돌지 않습니다(본문만 바꾸면 뒤 플러그인도 또 처리합니다). 한쪽의 명령을 바꾸거나 `run_after` · `run_before` 로 순서를 정하세요",
                "{} all take the comment command `{c}` — {} runs first; if it blocks or handles it instead, the later ones do not run (if it only changes the body, the later ones handle it again). Change one command, or set the order with `run_after` / `run_before`",
                names.join(", "),
                names[0]
            ),
            plugins: names,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(lines: &str) -> PluginDef {
        let raw = format!(
            "name = \"o\"\nscope = [\"cli\"]\n[actions.out.post]\nurl = \"https://example.com\"\n{lines}"
        );
        toml::from_str(&raw).unwrap()
    }

    fn ids(d: &PluginDef, order: &[usize]) -> Vec<String> {
        order.iter().map(|&i| d.handlers[i].label(i)).collect()
    }

    const LINE: &str = "post = [\"quest.created\"]\naction = \"out\"\n";

    #[test]
    fn without_orders_it_is_the_written_order() {
        let d = def(&format!(
            "[[handlers]]\nid = \"a\"\n{LINE}[[handlers]]\nid = \"b\"\n{LINE}[[handlers]]\nid = \"c\"\n{LINE}"
        ));
        assert_eq!(ids(&d, &run_order(&d).unwrap()), ["a", "b", "c"]);
    }

    #[test]
    fn run_after_and_run_before_move_lines() {
        let d = def(&format!(
            "[[handlers]]\nid = \"a\"\nrun_after = [\"c\"]\n{LINE}\
             [[handlers]]\nid = \"b\"\n{LINE}\
             [[handlers]]\nid = \"c\"\n{LINE}\
             [[handlers]]\nid = \"d\"\nrun_before = [\"b\"]\n{LINE}"
        ));
        // c 다음 a, b 앞에 d — 나머지는 적힌 순서를 지킨다.
        assert_eq!(ids(&d, &run_order(&d).unwrap()), ["c", "a", "d", "b"]);
    }

    #[test]
    fn a_cycle_is_an_error_naming_the_lines() {
        // c 는 고리에 들지 않고 뒤에 매달렸을 뿐이다 — 이름에 안 나와야 한다.
        let d = def(&format!(
            "[[handlers]]\nid = \"a\"\nrun_after = [\"b\"]\n{LINE}\
             [[handlers]]\nid = \"b\"\nrun_after = [\"a\"]\n{LINE}\
             [[handlers]]\nid = \"c\"\nrun_after = [\"a\"]\n{LINE}"
        ));
        let e = run_order(&d).unwrap_err();
        assert!(
            e.contains('a') && e.contains('b') && !e.contains('c'),
            "{e}"
        );
    }

    #[test]
    fn unknown_and_self_references_are_errors() {
        let d = def(&format!(
            "[[handlers]]\nid = \"a\"\nrun_after = [\"zz\"]\n{LINE}"
        ));
        assert!(run_order(&d).unwrap_err().contains("zz"));
        let d = def(&format!(
            "[[handlers]]\nid = \"a\"\nrun_before = [\"a\"]\n{LINE}"
        ));
        assert!(run_order(&d).is_err());
    }

    #[test]
    fn lines_in_different_places_cannot_be_ordered() {
        let d = def(&format!(
            "[[handlers]]\nid = \"a\"\n{LINE}\
             [[handlers]]\nid = \"w\"\nwait = true\nrun_after = [\"a\"]\n{LINE}"
        ));
        let e = run_order(&d).unwrap_err();
        assert!(e.contains("wait"), "{e}");
    }

    /// DEV-415: 묶음 이름으로 적은 순서는 그 묶음의 줄들에, 묶음이 적은 순서는 든 줄마다 걸린다.
    #[test]
    fn group_names_order_their_lines() {
        let raw = format!(
            "name = \"o\"\nscope = [\"cli\"]\n[actions.out.post]\nurl = \"https://example.com\"\n\
             [[groups]]\nid = \"G\"\nrun_after = [\"x\"]\n\
             [[handlers]]\nid = \"a\"\ngroup = \"G\"\n{LINE}\
             [[handlers]]\nid = \"c\"\nrun_before = [\"G\"]\n{LINE}\
             [[handlers]]\nid = \"b\"\ngroup = \"G\"\n{LINE}\
             [[handlers]]\nid = \"x\"\n{LINE}"
        );
        let d = crate::plugins::parse_def(&raw).unwrap();
        // G(a, b) 는 x 다음, c 는 G 앞 — c, x 가 먼저(적힌 순서), 그다음 a, b.
        assert_eq!(ids(&d, &run_order(&d).unwrap()), ["c", "x", "a", "b"]);

        // 자기가 든 묶음과의 순서는 오류.
        let bad = raw.replace(
            "id = \"a\"\ngroup = \"G\"\n",
            "id = \"a\"\ngroup = \"G\"\nrun_after = [\"G\"]\n",
        );
        assert!(crate::plugins::parse_def(&bad).is_err());
    }

    #[test]
    fn check_warns_when_ordered_lines_never_meet() {
        let d = def(
            "[[handlers]]\nid = \"a\"\npost = [\"quest.created\"]\naction = \"out\"\n\
             [[handlers]]\nid = \"b\"\nrun_after = [\"a\"]\npost = [\"comment.added\"]\naction = \"out\"\n\
             [[handlers]]\nid = \"c\"\nrun_after = [\"a\"]\npost = [\"quest.*\"]\naction = \"out\"\n",
        );
        let w = unused_orders(&d);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].starts_with("b:"), "{w:?}");
    }

    fn plugin(name: &str, after: &[&str], before: &[&str]) -> Plugin {
        let mut d = def(&format!("[[handlers]]\n{LINE}"));
        d.name = name.into();
        d.run_after = after.iter().map(|s| s.to_string()).collect();
        d.run_before = before.iter().map(|s| s.to_string()).collect();
        Plugin {
            def: d,
            dir: Default::default(),
            guild_root: Default::default(),
            compiled: None,
            script_src: None,
            folder: Default::default(),
            imports: Vec::new(),
            source: None,
        }
    }

    fn names(ps: &[Plugin]) -> Vec<&str> {
        ps.iter().map(|p| p.def.name.as_str()).collect()
    }

    /// DEV-469: 적지 않은 것끼리는 받은 순서, 적은 것은 그대로 — 없는 이름은 넘어간다.
    #[test]
    fn plugins_keep_load_order_unless_told_otherwise() {
        let mut ps = vec![
            plugin("a", &[], &[]),
            plugin("b", &[], &[]),
            plugin("c", &[], &[]),
        ];
        assert!(plugin_order(&mut ps).is_empty());
        assert_eq!(names(&ps), ["a", "b", "c"]);

        let mut ps = vec![
            plugin("a", &["c", "nobody"], &[]),
            plugin("b", &[], &[]),
            plugin("c", &[], &[]),
        ];
        assert!(plugin_order(&mut ps).is_empty());
        assert_eq!(names(&ps), ["b", "c", "a"]);

        let mut ps = vec![
            plugin("a", &[], &[]),
            plugin("b", &[], &[]),
            plugin("c", &[], &["a"]),
        ];
        assert!(plugin_order(&mut ps).is_empty());
        assert_eq!(names(&ps), ["b", "c", "a"]);
    }

    /// DEV-469: 순서가 돌고 돌아도 아무것도 빠지지 않는다 — 걸린 것들은 받은 순서로 두고 경고한다.
    #[test]
    fn a_plugin_cycle_keeps_everyone_and_warns() {
        let mut ps = vec![
            plugin("a", &["b"], &[]),
            plugin("b", &["a"], &[]),
            plugin("c", &[], &[]),
        ];
        let w = plugin_order(&mut ps);
        assert_eq!(names(&ps), ["c", "a", "b"]);
        assert_eq!(w.len(), 1, "{w:?}");
        assert_eq!(w[0].plugins, ["a", "b"]);
    }
}
