//! DEV-415: 줄 묶음 — 여러 줄이 같은 조건을 되풀이할 때 묶음에 한 번만 적는다(Bevy 의 SystemSet).
//!
//! ```toml
//! [[groups]]
//!     id   = "알림 퀘스트"
//!     with = ["subject"]
//!     [groups.when]
//!         "subject.tags" = "notify"
//!
//! [[handlers]]
//!     group = "알림 퀘스트"
//!     post  = ["comment.added"]
//!     call  = "on_comment"
//! ```
//!
//! 묶음이 갖는 것은 줄과 같은 이름의 칸 넷 — `when`(조건), `with`(읽을 데이터), `run_after` · `run_before`(순서).
//!
//! # 불러올 때 줄마다 합친다
//!
//! 묶음의 `when` · `with` 는 **적재 때** 들어 있는 줄마다 합친다. 그래서 실행기 · 검사 · 허용 화면 · `plugin test` 는
//! 묶음을 몰라도 된다 — 줄 하나만 보면 그 줄이 언제 도는지 다 보인다(허용 화면이 줄마다 합친 조건을 보여 준다).
//! 합치기는 여러 번 해도 같다: 같은 칸에 같은 값이면 그대로 두고, 다른 값이면 오류다(조건 둘이 부딪히면 그 줄은
//! 영영 안 돈다 — 조용히 하나를 버리면 왜 안 도는지 모른다).
//!
//! 순서는 합치지 않는다 — 묶음 이름으로 적은 순서는 [`super::order`] 가 정렬할 때 그 묶음의 줄들로 푼다.
use super::{PluginDef, when};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

/// 묶음 하나.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    /// 묶음 이름 — 줄의 `group` 과 순서(`run_after` · `run_before`)가 이 이름으로 가리킨다. 줄 이름과 겹칠 수 없다.
    pub id: String,
    /// 들어 있는 줄마다 더해지는 조건.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub when: when::When,
    /// 들어 있는 줄마다 더해지는 연결 데이터.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub with: Vec<String>,
    /// 묶음의 줄들이 이 줄 · 묶음보다 **먼저** 돌아야 한다.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run_after: Vec<String>,
    /// 묶음의 줄들이 이 줄 · 묶음보다 **나중에** 돌아야 한다.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run_before: Vec<String>,
}

/// 묶음을 검사하고 `when` · `with` 를 줄마다 합친다. [`super::parse_def`] 가 검증 전에 부른다.
pub fn expand(def: &mut PluginDef) -> AppResult<()> {
    let name = def.name.clone();
    let bad = |m: String| Err(AppError::BadRequest(format!("{name}: {m}")));
    let line_ids: std::collections::HashSet<&str> = def
        .handlers
        .iter()
        .filter_map(|h| h.id.as_deref())
        .collect();
    let mut seen = std::collections::HashSet::new();
    for g in &def.groups {
        if g.id.trim().is_empty() {
            return bad(crate::tf!(
                "묶음의 `id` 가 비어 있습니다",
                "a group has an empty `id`"
            ));
        }
        if !seen.insert(g.id.as_str()) {
            return bad(crate::tf!(
                "묶음 이름 `{}` 가 겹칩니다",
                "group name `{}` is duplicated",
                g.id
            ));
        }
        if line_ids.contains(g.id.as_str()) {
            return bad(crate::tf!(
                "묶음 이름 `{}` 가 줄 이름과 겹칩니다 — 순서를 적을 때 어느 쪽인지 알 수 없습니다",
                "group name `{}` is also a line name — an order naming it would be ambiguous",
                g.id
            ));
        }
    }
    // 순서에 적은 이름은 줄이나 묶음이어야 한다 — 줄이 없는 묶음의 순서는 정렬이 안 보므로 여기서 본다.
    for g in &def.groups {
        for x in g.run_after.iter().chain(&g.run_before) {
            if x == &g.id {
                return bad(crate::tf!(
                    "묶음 `{}`: 자기 자신과의 순서는 적을 수 없습니다",
                    "group `{}`: a group cannot be ordered against itself",
                    g.id
                ));
            }
            if !line_ids.contains(x.as_str()) && !seen.contains(x.as_str()) {
                return bad(crate::tf!(
                    "묶음 `{}`: 순서에 적은 `{x}` 는 줄도 묶음도 아닙니다",
                    "group `{}`: `{x}` in its order is neither a line nor a group",
                    g.id
                ));
            }
        }
    }
    let groups = def.groups.clone();
    for (i, h) in def.handlers.iter_mut().enumerate() {
        let Some(gid) = h.group.clone() else { continue };
        let who = h.label(i);
        let Some(g) = groups.iter().find(|g| g.id == gid) else {
            return bad(crate::tf!(
                "{who}: 묶음 `{gid}` 가 없습니다 — `[[groups]]` 에 그 `id` 를 적으세요",
                "{who}: there is no group `{gid}` — add it to `[[groups]]`"
            ));
        };
        for (k, v) in &g.when {
            match h.when.get(k) {
                Some(mine) if mine != v => {
                    return bad(crate::tf!(
                        "{who}: 조건 `{k}` 를 줄과 묶음 `{gid}` 가 다르게 적었습니다 — 둘 다 만족할 수 없어 이 줄은 안 돕니다",
                        "{who}: the line and group `{gid}` give different values for condition `{k}` — both cannot hold, so the line would never run"
                    ));
                }
                Some(_) => {}
                None => {
                    h.when.insert(k.clone(), v.clone());
                }
            }
        }
        // 묶음의 것을 앞에 — 함수 인자 순서는 `fn f(e, 묶음의 것…, 줄의 것…)` 이다.
        let mut with: Vec<String> = g.with.clone();
        for w in &h.with {
            if !with.contains(w) {
                with.push(w.clone());
            }
        }
        h.with = with;
    }
    Ok(())
}

/// `check` 의 경고 — 줄이 하나도 안 든 묶음.
pub fn empty_groups(def: &PluginDef) -> Vec<String> {
    def.groups
        .iter()
        .filter(|g| {
            !def.handlers
                .iter()
                .any(|h| h.group.as_deref() == Some(g.id.as_str()))
        })
        .map(|g| {
            crate::tf!(
                "묶음 `{}` 에 든 줄이 없습니다 — 줄에 `group = \"{}\"` 를 적거나 묶음을 지우세요",
                "group `{}` has no lines — put `group = \"{}\"` on a line or remove the group",
                g.id,
                g.id
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::parse_def;

    const HEAD: &str =
        "name = \"g\"\nscope = [\"cli\"]\n[actions.out.post]\nurl = \"https://example.com\"\n";

    #[test]
    fn a_group_adds_its_condition_and_data_to_each_line() {
        let d = parse_def(&format!(
            "{HEAD}\
             [[groups]]\nid = \"알림\"\nwith = [\"subject\"]\n[groups.when]\n\"subject.tags\" = \"notify\"\n\
             [[handlers]]\nid = \"댓글\"\ngroup = \"알림\"\npost = [\"comment.added\"]\naction = \"out\"\n\
             [handlers.when]\n\"comment.discussion\" = true\n\
             [[handlers]]\nid = \"새것\"\npost = [\"quest.created\"]\naction = \"out\"\n"
        ))
        .unwrap();
        let c = &d.handlers[0];
        assert_eq!(c.with, ["subject"]);
        assert_eq!(c.when.len(), 2, "{:?}", c.when);
        assert_eq!(c.when["subject.tags"], "notify");
        // 묶음 밖 줄은 그대로.
        assert!(d.handlers[1].when.is_empty() && d.handlers[1].with.is_empty());
        // 합치기는 여러 번 해도 같다(동의 지문이 정의를 다시 읽어도 안 바뀐다).
        let mut again = d.clone();
        super::expand(&mut again).unwrap();
        assert_eq!(again, d);
    }

    #[test]
    fn unknown_group_conflicting_condition_and_name_clash_are_errors() {
        let line = "post = [\"quest.created\"]\naction = \"out\"\n";
        let e = parse_def(&format!("{HEAD}[[handlers]]\ngroup = \"없음\"\n{line}")).unwrap_err();
        assert!(e.to_string().contains("없음"), "{e}");

        let e = parse_def(&format!(
            "{HEAD}[[groups]]\nid = \"g1\"\n[groups.when]\n\"quest.urgency\" = 1\n\
             [[handlers]]\ngroup = \"g1\"\n{line}[handlers.when]\n\"quest.urgency\" = 2\n"
        ))
        .unwrap_err();
        assert!(e.to_string().contains("quest.urgency"), "{e}");

        let e = parse_def(&format!(
            "{HEAD}[[groups]]\nid = \"same\"\n[[handlers]]\nid = \"same\"\n{line}"
        ))
        .unwrap_err();
        assert!(e.to_string().contains("same"), "{e}");

        let e = parse_def(&format!(
            "{HEAD}[[groups]]\nid = \"g1\"\nrun_after = [\"zz\"]\n[[handlers]]\ngroup = \"g1\"\n{line}"
        ))
        .unwrap_err();
        assert!(e.to_string().contains("zz"), "{e}");
    }

    #[test]
    fn check_warns_about_an_empty_group() {
        let d = parse_def(&format!(
            "{HEAD}[[groups]]\nid = \"빈\"\n[[handlers]]\npost = [\"quest.created\"]\naction = \"out\"\n"
        ))
        .unwrap();
        let w = super::empty_groups(&d);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("빈"));
    }
}
