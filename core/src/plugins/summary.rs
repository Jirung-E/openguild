//! DEV-410: **무엇에 동의하는지**를 사람 말로 적어 준다. `openguild plugin allow` 가 보여 주는 것.
//!
//! # 왜 정의 원문으로는 부족한가
//!
//! 예전엔 `plugin.toml` 을 그대로 찍었다. 정의가 한 줄짜리일 때는 그것으로 충분했지만, 이제 한
//! 플러그인이 여러 줄을 갖고 줄마다 조건·읽는 데이터·기다림이 붙는다([[DEV-403]]). TOML 을 읽어
//! "이게 내 퀘스트를 읽나?" 를 스스로 조립하게 하면, 결국 **안 읽고 허용하게** 된다.
//!
//! 그래서 같은 내용을 이렇게 바꿔 적는다.
//!
//! ```text
//! 하는 일
//!   1. 바뀐 뒤 quest.status_changed → on_done() (조건: change.to = done) · 퀘스트를 읽음(subject) · 기다림
//!   2. 바뀌기 전 comment.added → 보내기 https://example.test/hook
//! 길드에 시킬 수 있는 일: 알림, 백업
//! 내보내는 곳
//!   out  post  https://example.test/hook
//! 불러오는 파일: ../공통/fmt.rhai (처음 한 번만 묻습니다 — 이 파일이 바뀌어도 다시 묻지 않습니다)
//! ```
//!
//! GUI 는 같은 것을 [`super::view::PluginView`] 로 그린다. 두 화면이 **같은 사실**을 말하도록,
//! 여기서 쓰는 재료도 그 뷰와 같다.

use super::{Plugin, Scope, view};

/// 허용 화면에 그대로 찍을 줄들. 사람이 읽는 순서 — 하는 일 → 시킬 수 있는 일 → 나가는 곳 →
/// 읽고 쓰는 파일.
pub fn consent_lines(p: &Plugin, scope: Scope) -> Vec<String> {
    let v = view::view(p, false, scope);
    let mut out = Vec::new();
    if let Some(d) = &v.description {
        out.push(d.clone());
    }
    out.push(crate::tf!(
        "어디서 도나: {}",
        "runs in: {}",
        v.scope.join(", ")
    ));

    out.push(crate::tf!("하는 일", "what it does"));
    for (i, h) in v.handlers.iter().enumerate() {
        out.push(format!("  {}. {}", i + 1, handler_line(h)));
    }
    if v.handlers.is_empty() {
        out.push(crate::tf!("  (줄이 없습니다 — 아무것도 안 합니다)", "  (no lines — it does nothing)"));
    }

    // DEV-406: 길드에 시키는 일은 `[actions]` 에 안 보인다 — 권한으로만 드러난다.
    if !v.permissions.is_empty() {
        let named: Vec<String> = v.permissions.iter().map(|p| permission_name(p)).collect();
        out.push(crate::tf!(
            "길드에 시킬 수 있는 일: {}",
            "may ask the guild to: {}",
            named.join(", ")
        ));
    }

    if !v.actions.is_empty() {
        out.push(crate::tf!("내보내는 곳", "where it sends"));
        for a in &v.actions {
            out.push(format!("  {}  {}  {}", a.name, a.kind, a.target));
        }
    }
    // 헤더·본문에 끼워 넣는 환경변수 — **값은 안 보여 준다**. 어떤 비밀값을 쓰는지는 알아야 한다.
    if !v.env.is_empty() {
        out.push(crate::tf!(
            "쓰는 환경변수: {}",
            "environment variables used: {}",
            v.env.join(", ")
        ));
    }

    if !v.scripts.is_empty() {
        out.push(crate::tf!("스크립트: {}", "scripts: {}", v.scripts.join(", ")));
    }
    // DEV-408: 폴더 밖 파일도 불러올 수 있다. **바뀌어도 다시 안 묻는다** 는 것이 여기서 유일하게
    // 알려지는 사실이라, 빼면 안 된다.
    if !v.imports.is_empty() {
        out.push(crate::tf!(
            "불러오는 파일: {} (처음 한 번만 묻습니다 — 이 파일이 바뀌어도 다시 묻지 않습니다)",
            "imports: {} (asked once — you will not be asked again if these files change)",
            v.imports.join(", ")
        ));
    }
    if let Some(d) = &v.data_dir {
        out.push(crate::tf!("작업 폴더: {}", "working folder: {}", d));
    }
    if !v.inputs.is_empty() {
        let keys: Vec<&str> = v.inputs.iter().map(|i| i.key.as_str()).collect();
        out.push(crate::tf!(
            "설정값: {}",
            "settings: {}",
            keys.join(", ")
        ));
    }
    out.push(crate::tf!("폴더: {}", "folder: {}", v.dir));
    out
}

/// 줄 목록만 — 적재 목록(`plugin list`)이 이름 아래에 적는다.
pub fn handler_lines(p: &Plugin, scope: Scope) -> Vec<String> {
    view::view(p, false, scope).handlers.iter().map(handler_line).collect()
}

/// 줄 하나 — "언제 → 무엇" 에 조건·읽는 데이터·기다림을 뒤에 붙인다.
fn handler_line(h: &view::HandlerView) -> String {
    let stage = if h.stage == "pre" {
        crate::tf!("바뀌기 전", "before")
    } else {
        crate::tf!("바뀐 뒤", "after")
    };
    let what = match (&h.call, &h.action) {
        (Some(f), _) => format!("{f}()"),
        (None, Some(name)) => name.clone(),
        (None, None) => match (&h.action_kind, &h.action_target) {
            (Some(k), Some(t)) => format!("{k} {t}"),
            _ => crate::tf!("(없음)", "(none)"),
        },
    };
    let mut line = format!("{stage} {} → {what}", h.events.join(" "));
    if !h.when.is_empty() {
        line.push_str(&crate::tf!(
            " (조건: {})",
            " (only when: {})",
            h.when.join(", ")
        ));
    }
    if !h.with.is_empty() {
        line.push_str(&crate::tf!(
            " · 읽음: {}",
            " · reads: {}",
            h.with.join(", ")
        ));
    }
    // `pre` 줄은 언제나 기다린다 — 그래야 막거나 값을 바꿀 수 있다.
    if h.wait || h.stage == "pre" {
        line.push_str(&crate::tf!(" · 끝날 때까지 기다림", " · waits for it to finish"));
    }
    if h.stage == "pre" {
        line.push_str(&crate::tf!(
            " · 막거나 값을 바꿀 수 있음",
            " · may block it or change values"
        ));
    }
    line
}

fn permission_name(p: &str) -> String {
    match p {
        "notify" => crate::tf!("알림 띄우기", "show notifications"),
        "backup" => crate::tf!("백업 만들기", "make backups"),
        other => other.to_string(),
    }
}

/// 여럿을 한 번에 허용할 때의 한 줄 요약 — 이름과 "가장 센 것" 만.
pub fn one_liner(p: &Plugin, scope: Scope) -> String {
    let v = view::view(p, false, scope);
    let mut marks: Vec<String> = Vec::new();
    if v.handlers.iter().any(|h| h.stage == "pre") {
        marks.push(crate::tf!("막을 수 있음", "can block"));
    }
    for perm in &v.permissions {
        marks.push(permission_name(perm));
    }
    let dests: Vec<String> = v.actions.iter().map(|a| format!("{} {}", a.kind, a.target)).collect();
    if !dests.is_empty() {
        marks.push(dests.join(", "));
    }
    if marks.is_empty() {
        marks.push(crate::tf!("밖으로 안 나감", "nothing leaves this machine"));
    }
    format!(
        "{}  [{}]  {}",
        p.def.name,
        v.scope.join(","),
        marks.join(" · ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{MANIFEST, load_all, plugins_dir};

    fn guild(label: &str, manifest: &str, script: Option<&str>) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let g = std::env::temp_dir().join(format!("og-sum-{label}-{ns}"));
        let dir = plugins_dir(&g).join("p");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST), manifest).unwrap();
        if let Some(s) = script {
            std::fs::write(dir.join("main.rhai"), s).unwrap();
        }
        g
    }

    const FULL: &str = r#"
name = "p"
description = "하는 일 설명"
scope = ["cli"]
permissions = ["notify", "backup"]
scripts = ["main.rhai"]

[actions.out.post]
url = "https://example.test/hook"
headers = { Authorization = "Bearer ${MY_TOKEN}" }
body_env = { chat_id = "MY_CHAT" }

[[handlers]]
post = ["quest.status_changed"]
call = "on_done"
with = ["subject"]
wait = true
[handlers.when]
"change.to" = ["done"]

[[handlers]]
pre = ["comment.added"]
action = "out"
"#;

    /// 허용 화면은 **줄마다** 언제·무엇·조건·읽는 것·기다림을 말한다.
    #[test]
    fn every_line_is_spelled_out() {
        let g = guild("full", FULL, Some("fn on_done(e, subject) { }"));
        let l = load_all(&g);
        assert!(l.errors.is_empty(), "{:?}", l.errors);
        let p = &l.needs_consent[0];
        let text = consent_lines(p, Scope::Cli).join("\n");
        assert!(text.contains("하는 일 설명"), "{text}");
        assert!(text.contains("quest.status_changed"), "{text}");
        assert!(text.contains("on_done()"), "{text}");
        assert!(text.contains("change.to"), "조건이 안 보인다:\n{text}");
        assert!(text.contains("subject"), "읽는 데이터가 안 보인다:\n{text}");
        assert!(text.contains("기다림"), "기다리는지가 안 보인다:\n{text}");
        // `pre` 줄은 막을 수 있다 — 이것을 모르고 허용하면 안 된다.
        assert!(text.contains("막거나"), "막을 수 있다는 말이 없다:\n{text}");
        let _ = std::fs::remove_dir_all(&g);
    }

    /// 권한·나가는 곳·환경변수 이름은 나오고, **값은 안 나온다**.
    #[test]
    fn permissions_and_destinations_are_named_but_not_values() {
        unsafe { std::env::set_var("MY_TOKEN", "비밀") };
        let g = guild("perm", FULL, Some("fn on_done(e, subject) { }"));
        let l = load_all(&g);
        let text = consent_lines(&l.needs_consent[0], Scope::Cli).join("\n");
        assert!(text.contains("알림 띄우기") && text.contains("백업 만들기"), "{text}");
        assert!(text.contains("https://example.test/hook"), "{text}");
        assert!(text.contains("MY_TOKEN") && text.contains("MY_CHAT"), "{text}");
        assert!(!text.contains("비밀"), "환경변수 값이 새어 나왔다:\n{text}");
        unsafe { std::env::remove_var("MY_TOKEN") };
        let _ = std::fs::remove_dir_all(&g);
    }

    /// 여럿을 한 번에 허용할 때는 한 줄 — 그래도 "막을 수 있음" 은 빠지지 않는다.
    #[test]
    fn the_one_liner_still_says_the_strongest_thing() {
        let g = guild("one", FULL, Some("fn on_done(e, subject) { }"));
        let l = load_all(&g);
        let line = one_liner(&l.needs_consent[0], Scope::Cli);
        assert!(line.starts_with("p  [cli]"), "{line}");
        assert!(line.contains("막을 수 있음"), "{line}");
        assert!(line.contains("알림 띄우기"), "{line}");
        let _ = std::fs::remove_dir_all(&g);
    }

    /// 나가는 곳도 권한도 없으면 그렇다고 말한다 — 빈 줄은 "모름" 으로 읽힌다.
    #[test]
    fn a_plugin_that_sends_nowhere_says_so() {
        let g = guild(
            "quiet",
            "name = \"p\"\nscope = [\"cli\"]\nscripts = [\"main.rhai\"]\n\n[[handlers]]\npost = [\"quest.created\"]\ncall = \"h\"\n",
            Some("fn h(e) { }"),
        );
        let l = load_all(&g);
        assert!(l.errors.is_empty(), "{:?}", l.errors);
        let line = one_liner(&l.needs_consent[0], Scope::Cli);
        assert!(line.contains("밖으로 안 나감"), "{line}");
        let _ = std::fs::remove_dir_all(&g);
    }
}
