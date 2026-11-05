//! 문서 · 스킬이 가르치는 것이 지금의 CLI 와 맞는지 — 실제 `openguild` 바이너리로 대조한다.
//!
//! 스킬은 에이전트가 그대로 믿고 쓴다. 틀려도 아무 오류가 안 나고, 지금까지는 릴리스 때 사람이
//! 볼 때만 걸렸다. 설명이 맞는지까지는 여기서 모른다 — 그건 릴리스 점검(`release-process`)이 읽어서 본다.

use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// 크레이트만 따로 배포된 경우엔 문서가 없다. 이 검사가 뜻을 갖는 건 저장소 안에서다.
fn in_repo() -> bool {
    repo().join("skills").exists()
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
}

/// 사용자 · 에이전트가 따라 하는 문서.
fn guide_files() -> Vec<PathBuf> {
    let r = repo();
    let mut out: Vec<PathBuf> = [
        "README.md",
        "AGENTS.md",
        "docs/USAGE.md",
        "examples/plugins/README.md",
        "gui/README.md",
        "packaging/macos/cli-server-README.md",
    ]
    .iter()
    .map(|f| r.join(f))
    .filter(|p| p.exists())
    .collect();
    walk(&r.join("skills"), "md", &mut out);
    walk(&r.join(".agents/skills"), "md", &mut out);
    walk(&r.join(".guild/rules"), "md", &mut out);
    out.sort();
    out
}

fn rel(p: &Path) -> String {
    p.strip_prefix(repo())
        .unwrap_or(p)
        .display()
        .to_string()
        .replace('\\', "/")
}

/// 빈 길드 + 격리 홈에서 CLI 를 돌린다. 도움말은 같은 명령을 여러 번 묻지 않게 담아 둔다.
struct Cli {
    base: PathBuf,
    help: HashMap<Vec<String>, Option<String>>,
}

impl Cli {
    fn new(label: &str) -> Cli {
        let base = std::env::temp_dir().join(format!(
            "og-docs-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for d in ["g", "home"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let cli = Cli {
            base,
            help: HashMap::new(),
        };
        assert!(cli.run(&["init", "--name", "docs"]).status.success());
        cli
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_openguild"))
            .args(args)
            .current_dir(self.base.join("g"))
            .env("OPENGUILD_HOME", self.base.join("home"))
            .env_remove("OPENGUILD_REMOTE")
            .output()
            .unwrap()
    }

    /// 없는 명령이면 None.
    fn help_of(&mut self, path: &[String]) -> Option<String> {
        if let Some(h) = self.help.get(path) {
            return h.clone();
        }
        let mut args: Vec<&str> = path.iter().map(String::as_str).collect();
        args.push("--help");
        let out = self.run(&args);
        let h = out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned());
        self.help.insert(path.to_vec(), h.clone());
        h
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// 셸처럼 낱말로 나눈다. 따옴표 밖의 `|` · `;` · `&` · `)` · ` #` 에서 그 명령이 끝난다.
fn shell_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    let mut started = false;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    started = true;
                }
                '|' | ';' | '&' | ')' => break,
                '#' if !started => break,
                c if c.is_whitespace() => {
                    if started {
                        out.push(std::mem::take(&mut cur));
                        started = false;
                    }
                }
                c => {
                    cur.push(c);
                    started = true;
                }
            },
        }
    }
    if started {
        out.push(cur);
    }
    out
}

/// 문서의 코드(펜스 안 줄 · 인라인 `…`)만 준다 — 줄 번호와 함께. 일반 문장의 "openguild" 는 명령이 아니다.
fn code_of(body: &str) -> Vec<(usize, String)> {
    let span = Regex::new(r"`([^`]+)`").unwrap();
    let mut fence = false;
    let mut out = Vec::new();
    for (i, line) in body.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
            continue;
        }
        if fence {
            out.push((i + 1, line.to_string()));
        } else {
            out.extend(span.captures_iter(line).map(|c| (i + 1, c[1].to_string())));
        }
    }
    out
}

fn is_placeholder(q: &str) -> bool {
    q.contains('<') && !q.contains(":<") || q.contains('$') || q.contains('{') || q.contains("...")
}

const GLOBAL: [&str; 6] = [
    "--help",
    "--json",
    "--compact",
    "--remote",
    "--guild",
    "--version",
];

fn has_option(help: &str, opt: &str) -> bool {
    help.match_indices(opt).any(|(i, _)| {
        !help[i + opt.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// 한 줄의 명령을 대조한다.
fn check_command(cli: &mut Cli, words: &[String], at: &str, bad: &mut Vec<String>) {
    let mut i = 0;
    while let Some(w) = words.get(i) {
        match w.as_str() {
            "--guild" | "--remote" => i += 2,
            "--json" | "--compact" => i += 1,
            _ => break,
        }
    }
    let word = Regex::new(r"^[a-z][a-z-]*$").unwrap();
    let mut path: Vec<String> = Vec::new();
    for w in &words[i..] {
        if !word.is_match(w) {
            break;
        }
        let Some(help) = cli.help_of(&path) else {
            break;
        };
        let listed = Regex::new(&format!(r"(?m)^\s+{}(\s|,|$)", regex::escape(w))).unwrap();
        // 맨 위 명령은 별칭(`types` · `statuses`)도 있다 — 도움말 목록에 안 나온다.
        let alias = path.is_empty() && cli.help_of(std::slice::from_ref(w)).is_some();
        if !listed.is_match(&help) && !alias {
            break;
        }
        path.push(w.clone());
    }
    let Some(help) = (!path.is_empty()).then(|| cli.help_of(&path)).flatten() else {
        if let Some(first) = words.get(i).filter(|w| word.is_match(w)) {
            bad.push(format!("{at}: 없는 명령 `openguild {first}`"));
        }
        return;
    };
    for w in words {
        let opt = w.split('=').next().unwrap_or(w);
        if opt.starts_with("--")
            && opt.len() > 2
            && opt[2..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase())
            && !GLOBAL.contains(&opt)
            && !has_option(&help, opt)
        {
            bad.push(format!(
                "{at}: `openguild {}` 에 없는 옵션 {opt}",
                path.join(" ")
            ));
        }
    }
    for (j, w) in words.iter().enumerate() {
        if w == "-q" || w == "--query" {
            let Some(q) = words.get(j + 1).filter(|q| !is_placeholder(q)) else {
                continue;
            };
            let mut args: Vec<&str> = path.iter().map(String::as_str).collect();
            args.extend(["-q", q.as_str()]);
            let out = cli.run(&args);
            if !out.status.success() {
                bad.push(format!(
                    "{at}: 쿼리가 안 돈다 `{q}` — {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
        }
    }
}

/// 설명하느라 일부러 적은 것 — 없어진 명령을 없어졌다고 말하는 문장.
const NOT_COMMANDS: [(&str, &str); 1] = [(".guild/rules/cli-command-naming.md", "rules")];

#[test]
fn documented_commands_options_and_queries_exist() {
    if !in_repo() {
        return;
    }
    let mut cli = Cli::new("cmd");
    let call = Regex::new(r"(?:^\s*|[$(|&;]\s*)openguild\s+(.*)$").unwrap();
    let script = Regex::new(r#"query\("(quest|campaign|library|comment)",\s*"([^"]+)"\)"#).unwrap();
    let mut bad = Vec::new();
    let mut commands = 0;
    let mut scripts = 0;
    for f in guide_files() {
        let name = rel(&f);
        let body = std::fs::read_to_string(&f).unwrap();
        for (ln, code) in code_of(&body) {
            let at = format!("{name}:{ln}");
            if let Some(m) = call.captures(&code) {
                let words = shell_words(&m[1]);
                let skip = words.first().is_some_and(|w| {
                    w.starts_with(['<', '[', '.'])
                        || NOT_COMMANDS.contains(&(name.as_str(), w.as_str()))
                });
                if !words.is_empty() && !skip {
                    commands += 1;
                    check_command(&mut cli, &words, &at, &mut bad);
                }
            }
            for m in script.captures_iter(&code) {
                scripts += 1;
                let list: &[&str] = match &m[1] {
                    "quest" => &["quest", "list"],
                    "campaign" => &["campaign", "list"],
                    "library" => &["library", "list"],
                    _ => &["comments"],
                };
                let mut args = list.to_vec();
                args.extend(["-q", &m[2]]);
                let out = cli.run(&args);
                if !out.status.success() {
                    bad.push(format!(
                        "{at}: query() 가 안 돈다 `{}` — {}",
                        &m[2],
                        String::from_utf8_lossy(&out.stderr).trim()
                    ));
                }
            }
        }
    }
    // 추출이 망가져 아무것도 안 보는데 통과하는 일이 없게.
    assert!(
        commands > 300,
        "대조한 명령이 {commands}개뿐 — 추출이 망가졌다"
    );
    assert!(
        scripts > 3,
        "대조한 query() 가 {scripts}개뿐 — 추출이 망가졌다"
    );
    assert!(
        bad.is_empty(),
        "문서가 지금의 CLI 와 다르다 ({}건):\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// 없애거나 이름을 바꾼 것. 뺄 때 여기에 더한다 — 시험은 코드만 보므로 문서의 옛 이름은 못 잡는다.
/// CHANGELOG 는 안 본다 — 무엇이 무엇으로 바뀌었는지 옛 이름을 적는 곳이다.
const REMOVED: [(&str, &str); 7] = [
    (
        r"\be\.items\b",
        "시간 실행 함수는 받는 것이 없다 — query() 로 읽는다",
    ),
    (
        r"\be\.count\b",
        "시간 실행 함수는 받는 것이 없다 — query() 로 읽는다",
    ),
    (
        r#"\bof\s*=\s*"(quest|campaign|library|comment)""#,
        "줄에 `of` 는 없다 — query(대상, 쿼리) 로 읽는다",
    ),
    (
        r"\b(status|tag|type|title|body|campaign|folder|child|parent|prereq|successor|id|quest):=?\[",
        "`key:[a, b]` 문법은 없다 — any · all · only 로 적는다",
    ),
    (
        r"--has-sub\b|--no-sub\b",
        "`--has-child` · `--no-child` 로 바뀌었다",
    ),
    (
        r"\blibrary list[^`\n]*--path\b",
        "`library list` 는 `--folder` 다 — `--path` 는 new · update 만 받는다",
    ),
    (r"never blocks the guild", "`pre` 는 막을 수 있다"),
];

#[test]
fn docs_do_not_teach_removed_names() {
    if !in_repo() {
        return;
    }
    let r = repo();
    let mut files = guide_files();
    walk(&r.join(".guild/library"), "md", &mut files);
    walk(&r.join("examples/plugins"), "rhai", &mut files);
    walk(&r.join("examples/plugins"), "toml", &mut files);
    files.push(r.join("examples/plugins/plugin.schema.json"));
    files.push(r.join("gui/frontend/src/lib/stores/locale.ts"));
    let texts: Vec<(String, String)> = files
        .iter()
        .map(|f| (rel(f), std::fs::read_to_string(f).unwrap()))
        .collect();
    assert!(texts.len() > 20, "본 파일이 {}개뿐", texts.len());
    let mut bad = Vec::new();
    for (pat, why) in REMOVED {
        let re = Regex::new(pat).unwrap();
        for (name, body) in &texts {
            for (i, line) in body.lines().enumerate() {
                if let Some(m) = re.find(line) {
                    bad.push(format!("{name}:{}: `{}` — {why}", i + 1, m.as_str()));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "문서에 없어진 이름이 남았다 ({}건):\n{}",
        bad.len(),
        bad.join("\n")
    );
}
