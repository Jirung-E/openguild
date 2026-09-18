//! BUG-287: ops 의 공개 함수가 **전부** 잠금 분류를 갖는지 강제한다.
//!
//! 잠금을 빠뜨린 쓰기는 조용히 데이터를 지우고, 잠금을 겹쳐 잡은 쓰기는 제자리에서
//! 멈춘다. 둘 다 평소 시험으로는 안 보인다(동시에 두 프로세스가 있어야 드러난다).
//! 그래서 소스를 읽어 모양으로 확인한다 — 새 공개 함수는 아래 표에 분류를 적기
//! 전까지 이 시험이 실패한다.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// 몸통 맨 앞에서 `store.mutation_guard()` 를 잡는다.
    Guarded,
    /// 이 모듈의 비공개 함수가 대신 잡는다 — 공개 함수는 경로만 골라 넘긴다.
    Via(&'static str),
    /// 잠금을 잡는 진입점들을 **차례로** 부른다. 사이에 오래 걸리는 복사가 있어
    /// 한 번에 쥐면 그동안 길드 전체가 멈춘다.
    Composes,
    /// 읽기만 한다(DB 캐시 자가 치유 UPDATE 는 파일을 안 건드리므로 읽기로 친다).
    Read,
    /// 잠금을 이미 쥔 진입점 안에서만 불린다 — 여기서 잡으면 교착.
    Helper,
    /// 고유한 새 이름의 파일만 만든다. 기존 것을 읽어 고치지 않으니 잃을 것이 없고,
    /// 업로드 내내 쥐면 그동안 모든 쓰기가 멈춘다.
    NewFile,
}
use Kind::*;

const SOURCES: &[(&str, &str)] = &[
    ("attachments", include_str!("attachments.rs")),
    ("backlinks", include_str!("backlinks.rs")),
    ("campaign_comments", include_str!("campaign_comments.rs")),
    ("campaigns", include_str!("campaigns.rs")),
    ("comments", include_str!("comments.rs")),
    ("counter", include_str!("counter.rs")),
    ("doc_history", include_str!("doc_history.rs")),
    ("library", include_str!("library.rs")),
    ("meta", include_str!("meta.rs")),
    ("positions", include_str!("positions.rs")),
    ("quests", include_str!("quests.rs")),
    ("rules", include_str!("rules.rs")),
    ("search", include_str!("search.rs")),
    ("worklog", include_str!("worklog.rs")),
];

const TABLE: &[(&str, &str, Kind)] = &[
    ("attachments", "validate_guild_rel", Read),
    ("attachments", "save_attachment", NewFile),
    ("attachments", "new_attachment_dest", NewFile),
    ("attachments", "save_attachment_from_file", NewFile),
    ("attachments", "save_attachment_from_file_with_progress", NewFile),
    ("attachments", "save_attachment_from_file_cancellable", NewFile),
    ("attachments", "list_quest_attachments", Read),
    ("attachments", "list_campaign_attachments", Read),
    ("attachments", "list_book_attachments", Read),
    ("attachments", "add_quest_attachment", Via("add_attachment")),
    ("attachments", "add_campaign_attachment", Via("add_attachment")),
    ("attachments", "add_book_attachment", Via("add_attachment")),
    ("attachments", "remove_quest_attachment", Via("remove_attachment")),
    ("attachments", "remove_campaign_attachment", Via("remove_attachment")),
    ("attachments", "remove_book_attachment", Via("remove_attachment")),
    ("backlinks", "list_backlinks", Read),
    ("backlinks", "refresh_for", Helper),
    ("campaign_comments", "list_entries", Read),
    ("campaign_comments", "add_entry", Guarded),
    ("campaign_comments", "update_entry", Guarded),
    ("campaign_comments", "delete_entry", Guarded),
    ("campaign_comments", "toggle_reaction", Guarded),
    ("campaign_comments", "toggle_pinned", Guarded),
    ("campaign_comments", "get_memo", Read),
    ("campaign_comments", "set_memo", Guarded),
    ("campaigns", "fetch_detail", Read),
    ("campaigns", "create_campaign", Guarded),
    ("campaigns", "update_campaign", Guarded),
    ("campaigns", "begin_banner_image", Guarded),
    ("campaigns", "commit_banner_image", Guarded),
    ("campaigns", "set_banner_image", Composes),
    ("campaigns", "clear_banner_image", Guarded),
    ("campaigns", "delete_campaign", Guarded),
    ("campaigns", "link_quest_by_slug", Guarded),
    ("campaigns", "unlink_quest_by_slug", Guarded),
    ("campaigns", "add_checklist_line", Guarded),
    ("campaigns", "set_checklist_checked_by_index", Guarded),
    ("campaigns", "remove_checklist_by_index", Guarded),
    ("campaigns", "write_campaign_file", Helper),
    ("counter", "check_and_fix_counters", Guarded),
    ("comments", "get_comments", Read),
    ("comments", "set_comments", Guarded),
    ("comments", "list_comment_entries", Read),
    ("comments", "add_comment_entry", Guarded),
    ("comments", "update_comment_entry", Guarded),
    ("comments", "toggle_comment_reaction", Guarded),
    ("comments", "toggle_comment_discussion", Guarded),
    ("comments", "toggle_comment_resolved", Guarded),
    ("comments", "toggle_comment_pinned", Guarded),
    ("comments", "delete_comment_entry", Guarded),
    ("comments", "get_memo", Read),
    ("comments", "set_memo", Guarded),
    ("doc_history", "record", Helper),
    ("doc_history", "rename", Helper),
    ("doc_history", "purge", Helper),
    ("library", "list_books", Read),
    ("library", "list_books_in", Read),
    ("library", "get_book", Read),
    ("library", "set_book_tags", Guarded),
    ("library", "edit_book_tags", Guarded),
    ("library", "history", Read),
    ("library", "create_book", Guarded),
    ("library", "update_book", Guarded),
    ("library", "delete_book", Guarded),
    ("library", "list_folders", Read),
    ("library", "create_folder", Guarded),
    ("library", "move_folder", Guarded),
    ("library", "delete_folder", Guarded),
    ("meta", "create_type", Guarded),
    ("meta", "update_type", Guarded),
    ("meta", "delete_type", Guarded),
    ("meta", "rename_type", Guarded),
    ("meta", "rename_status_slug", Guarded),
    ("meta", "count_quests_by_type", Read),
    ("meta", "create_status", Guarded),
    ("meta", "update_status", Guarded),
    ("meta", "delete_status", Guarded),
    ("meta", "count_quests_by_status", Read),
    ("meta", "upsert_tag_def", Guarded),
    ("meta", "delete_tag_def", Guarded),
    ("positions", "update_position", Guarded),
    ("positions", "update_positions", Guarded),
    ("quests", "create_quest", Guarded),
    ("quests", "list_quests", Read),
    ("quests", "update_quest", Guarded),
    ("quests", "set_due_dates", Guarded),
    ("quests", "set_quest_tags", Guarded),
    ("quests", "edit_quest_tags", Guarded),
    ("quests", "list_quest_tags", Read),
    ("quests", "change_status", Guarded),
    ("quests", "change_quest_type", Guarded),
    ("quests", "change_parent", Guarded),
    ("quests", "delete_quest", Guarded),
    ("quests", "restore_quest", Guarded),
    ("quests", "add_prerequisite", Guarded),
    ("quests", "remove_prerequisite", Guarded),
    ("quests", "write_quest_file", Helper),
    ("rules", "list_rules", Read),
    ("rules", "get_rule", Read),
    ("rules", "history", Read),
    ("rules", "get_rule_entry", Read),
    ("rules", "set_rule", Guarded),
    ("rules", "create_rule", Guarded),
    ("rules", "delete_rule", Guarded),
    ("rules", "rename_rule", Guarded),
    ("rules", "set_rule_tags", Guarded),
    ("rules", "edit_rule_tags", Guarded),
    ("rules", "get_rules", Read),
    ("rules", "set_rules", Guarded),
    ("search", "search", Read),
    ("worklog", "validate_date", Read),
    ("worklog", "activities", Read),
    ("worklog", "daily_summary", Read),
    ("worklog", "get_note", Read),
    ("worklog", "set_note", Guarded),
    ("worklog", "list_notes", Read),
];

const GUARD: &str = "store.mutation_guard().await";

struct Fn {
    public: bool,
    body: String,
}

/// 모듈 최상위 함수(들여쓰기 없는 `fn`)를 이름 → 몸통으로. 시험 모듈은 뺀다.
fn functions(src: &str) -> BTreeMap<String, Fn> {
    let src = match src.find("#[cfg(test)]") {
        Some(i) => &src[..i],
        None => src,
    };
    let mut out = BTreeMap::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let public = line.starts_with("pub ") || line.starts_with("pub(");
        let rest = line
            .trim_start_matches("pub(crate) ")
            .trim_start_matches("pub(super) ")
            .trim_start_matches("pub ")
            .trim_start_matches("async ");
        if let Some(sig) = rest.strip_prefix("fn ") {
            let name: String = sig
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // 최상위 함수의 끝은 들여쓰기 없는 `}` 한 줄이다.
            let mut body = String::new();
            let mut j = i;
            while j < lines.len() {
                body.push_str(lines[j]);
                body.push('\n');
                if lines[j] == "}" {
                    break;
                }
                j += 1;
            }
            out.insert(name, Fn { public, body });
            i = j;
        }
        i += 1;
    }
    out
}

/// 시그니처 뒤 — 실제 몸통만.
fn inner(body: &str) -> &str {
    let mut depth = 0i32;
    for (i, c) in body.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '{' if depth == 0 => return &body[i + 1..],
            _ => {}
        }
    }
    ""
}

fn calls(body: &str, module: &str, target_module: &str, name: &str) -> bool {
    let body = inner(body);
    let qualified = format!("{target_module}::{name}(");
    if body.contains(&qualified) {
        return true;
    }
    if module != target_module {
        return false;
    }
    let bare = format!("{name}(");
    body.match_indices(&bare).any(|(i, _)| {
        let before = body[..i].chars().next_back();
        !matches!(before, Some(c) if c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
    })
}

#[test]
fn every_ops_function_is_classified() {
    let classified: BTreeSet<(&str, &str)> = TABLE.iter().map(|(m, f, _)| (*m, *f)).collect();
    let mut missing = Vec::new();
    let mut stale = Vec::new();
    for (module, src) in SOURCES {
        let fns = functions(src);
        for (name, f) in &fns {
            if f.public && !classified.contains(&(*module, name.as_str())) {
                missing.push(format!("{module}::{name}"));
            }
        }
        for (m, name, _) in TABLE.iter().filter(|(m, _, _)| m == module) {
            if !fns.get(*name).is_some_and(|f| f.public) {
                stale.push(format!("{m}::{name}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "잠금 분류가 없는 ops 공개 함수 — lock_coverage.rs 의 TABLE 에 적을 것: {missing:#?}"
    );
    assert!(stale.is_empty(), "TABLE 에 있는데 공개 함수가 아니다: {stale:#?}");
}

/// DEV-407: 잠금 전에 허용되는 것 — 플러그인에게 묻는 호출과 그 물음이 필요한지 확인하는 줄.
fn strip_pre_ask(src: &str) -> String {
    let mut out = String::new();
    let mut rest = src;
    while let Some(i) = rest
        .find("store.ask_pre(")
        .or_else(|| rest.find("store.events_wanted("))
        // 묻기를 감싼 헬퍼(`ask_…`)도 같은 이유로 잠금 전에 둔다.
        .or_else(|| rest.find("ask_"))
    {
        out.push_str(&rest[..i]);
        // 그 호출의 괄호가 닫힐 때까지 건너뛴다.
        let mut depth = 0usize;
        let mut end = i;
        for (j, c) in rest[i..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + j + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[test]
fn guards_match_the_classification() {
    let mut wrong = Vec::new();
    for (module, name, kind) in TABLE {
        let src = SOURCES.iter().find(|(m, _)| m == module).unwrap().1;
        let fns = functions(src);
        let Some(f) = fns.get(*name) else { continue };
        let body = inner(&f.body);
        let holds = body.contains(GUARD);
        match kind {
            Guarded => {
                let Some(at) = body.find(GUARD) else {
                    wrong.push(format!("{module}::{name}: 잠금이 없다"));
                    continue;
                };
                // 검증용 읽기까지 잠금 안에 있어야 남이 막 바꾼 상태를 보고 판단한다.
                //
                // DEV-407: 플러그인에게 **묻는 것**(`ask_pre`)은 예외다 — 답이 밖으로 나갔다
                // 올 수도 있는 시간이라, 그동안 잠금을 쥐고 있으면 다른 작업이 전부 선다.
                // 그래서 잠금 전에 묻는다. 검증용 읽기가 아니므로 이 규칙의 대상이 아니다.
                let before = strip_pre_ask(&body[..at]);
                if before.contains("store.") || before.contains("store,") {
                    wrong.push(format!("{module}::{name}: 잠금 전에 store 를 만진다"));
                }
            }
            Via(private) => {
                if holds {
                    wrong.push(format!("{module}::{name}: Via 인데 직접 잡는다"));
                }
                match fns.get(*private) {
                    Some(p) if inner(&p.body).contains(GUARD) => {}
                    _ => wrong.push(format!("{module}::{name}: {private} 가 잠금을 안 잡는다")),
                }
                if !calls(&f.body, module, module, private) {
                    wrong.push(format!("{module}::{name}: {private} 를 안 부른다"));
                }
            }
            Composes | Read | Helper | NewFile => {
                if holds {
                    wrong.push(format!("{module}::{name}: {kind:?} 인데 잠금을 잡는다"));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn locked_entry_points_do_not_nest() {
    // 잠금은 재진입이 안 된다 — 쥔 채 다른 진입점을 부르면 영원히 기다린다.
    let takes_lock: Vec<(&str, &str)> = TABLE
        .iter()
        .filter(|(_, _, k)| matches!(k, Guarded | Via(_) | Composes))
        .map(|(m, f, _)| (*m, *f))
        .collect();
    let mut nested = Vec::new();
    for (module, src) in SOURCES {
        for (name, f) in functions(src) {
            if !inner(&f.body).contains(GUARD) {
                continue;
            }
            for (tm, tf) in &takes_lock {
                if (*tm, *tf) != (*module, name.as_str()) && calls(&f.body, module, tm, tf) {
                    nested.push(format!("{module}::{name} → {tm}::{tf}"));
                }
            }
        }
    }
    assert!(nested.is_empty(), "잠금을 쥔 채 잠그는 진입점을 부른다: {nested:#?}");
}

#[test]
fn nobody_takes_the_bare_in_process_lock() {
    // 프로세스 안 뮤텍스만 잡으면 다른 프로세스를 못 막는다.
    let offenders: Vec<&str> = SOURCES
        .iter()
        .filter(|(_, src)| {
            let code = src.find("#[cfg(test)]").map_or(*src, |i| &src[..i]);
            code.contains("write_lock.lock")
        })
        .map(|(m, _)| *m)
        .collect();
    assert!(offenders.is_empty(), "store.write_lock 을 직접 잡는다: {offenders:?}");
}

#[test]
fn the_scanner_sees_what_it_claims() {
    // 스캐너가 아무것도 못 찾으면 위 시험들이 전부 공허하게 통과한다.
    let fns = functions(include_str!("quests.rs"));
    let f = fns.get("create_quest").expect("create_quest 를 찾는다");
    assert!(f.public);
    assert!(inner(&f.body).contains("journal::append"));
    assert!(!fns.contains_key("fresh"), "시험 모듈 안 함수는 빼야 한다");
    // `store: &Store` 를 쓰지 않는다 — events/catalog 의 op 스캐너가 이 파일도 읽는다.
    let sample = "pub async fn a(s: &S) -> R {\n    b(s);\n    x.b(s);\n}\n";
    let fns = functions(sample);
    assert!(calls(&fns["a"].body, "m", "m", "b"));
    assert!(!calls("fn a() {\n    x.c(1);\n}\n", "m", "m", "c"));
}
