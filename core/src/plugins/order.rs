//! DEV-414: 한 이벤트에 여러 줄이 걸릴 때의 순서 — Bevy 의 `before` / `after` 와 같은 생각.
//!
//! 줄은 `run_after = ["다른 줄 id"]` · `run_before = [...]` 로 **같은 플러그인 안의** 다른 줄과의 순서를 적는다.
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
use super::{Handler, PluginDef};
use std::collections::{BTreeSet, HashMap};

/// 줄이 도는 자리.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Pre,
    Post,
    PostWait,
}

fn slot(h: &Handler) -> Slot {
    if !h.pre.is_empty() {
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
}
