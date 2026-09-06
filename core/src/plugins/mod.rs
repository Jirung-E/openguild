//! DEV-375: 플러그인 **정의와 적재**.
//!
//! 길드의 동작에 사용자가 자기 동작을 붙인다([[DEV-373]]). 세 층 중 첫째 —
//! *언제 / 어디서 / 무엇으로 내보낼지* 를 JSON 으로 적는다.
//!
//! ```text
//! .guild/plugins/{name}/plugin.json    ← 정의 (git 으로 공유된다)
//! .guild/plugins/{name}/*.rhai         ← 판단·가공 ([[DEV-376]])
//! ```
//!
//! # 코어가 소유하는 동작은 둘뿐이다
//!
//! `post`(HTTP)와 `run`(프로세스). "밖으로 내보낸다" 의 최소 완전집합이고
//! 나머지는 사용자 프로그램 몫이다. slack/discord/email 같은 통합을 코어에
//! 쌓기 시작하면 끝없이 따라다녀야 한다.
//!
//! # 정의는 git 으로 따라온다
//!
//! 그래서 **동료가 만든 정의가 내 기계로 온다.** `scope` 에 `cli`/`gui` 가
//! 있으면 내 기계에서 도는 것이고, `run` 이면 임의 실행이다. 그래서 적재는
//! 동의를 요구한다([`consent`]) — 코어는 **묻지 않고** "동의가 필요하다" 고
//! 알려만 준다. 묻는 방법은 컴포넌트마다 다르고(CLI 는 비대화형일 수 있다),
//! 아무도 답하지 않으면 그 플러그인은 **돌지 않는다**.

pub mod consent;
pub mod runtime;
pub mod script;
#[cfg(test)]
mod tests;

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 어느 컴포넌트에서 돌 것인가. **명시 필수** — 기본값을 두면 무엇이든
/// 놀라는 사람이 생긴다("GUI 에서만 돌 줄 알았는데 CI 에서도 돌았다").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Cli,
    Gui,
    /// 서버에 걸면 **그 서버를 쓰는 모두**에게 적용된다. 공유 범위는 파일
    /// 위치가 아니라 이 값이 정한다.
    Server,
}

/// 코어가 할 줄 아는 두 가지.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// HTTP 로 이벤트를 보낸다.
    Post {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
    /// 프로세스를 띄우고 이벤트 JSON 을 **stdin** 으로 넘긴다.
    /// 인자로 넘기면 길이 제한과 이스케이프 문제가 생긴다.
    Run {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Post { .. } => "post",
            Action::Run { .. } => "run",
        }
    }
}

/// `plugin.json` 의 모양.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDef {
    pub name: String,
    /// 구독 패턴 — `quest.created`, `quest.*`, `*.created`, `pre:quest.*`.
    pub on: Vec<String>,
    /// **비어 있으면 적재하지 않는다.** 위 [`Scope`] 주석 참고.
    pub scope: Vec<Scope>,
    pub action: Action,
    /// 판단·가공 스크립트(폴더 기준 상대 경로). [[DEV-376]] 이 실행한다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
}

/// 적재된 플러그인 하나.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub def: PluginDef,
    /// `.guild/plugins/{name}/` — 스크립트 경로 해석의 기준.
    pub dir: PathBuf,
    /// DEV-376: 적재 때 컴파일해 둔 판단·가공 스크립트. 문법 오류는 **적재
    /// 때** 드러나야지 첫 이벤트 때 드러나면 안 된다. 없으면 전부 보내고
    /// 이벤트 JSON 을 그대로 쓴다.
    pub compiled: Option<std::sync::Arc<script::Script>>,
    /// 그 스크립트의 원문 — 동의 지문에 들어간다([`consent`]). 정의만 보면
    /// 스크립트를 갈아끼우는 것으로 동의를 우회할 수 있다.
    pub script_src: Option<String>,
}

impl Plugin {
    /// 이 플러그인이 그 이벤트를 원하나.
    pub fn wants(&self, name: &str, phase: crate::events::Phase) -> bool {
        self.def
            .on
            .iter()
            .any(|p| crate::events::names::matches(p, name, phase))
    }

    pub fn script_path(&self) -> Option<PathBuf> {
        self.def.script.as_ref().map(|s| self.dir.join(s))
    }
}

/// 적재 결과. **오류가 있어도 나머지는 돈다** — 정의 하나가 깨졌다고 길드
/// 전체의 플러그인이 죽으면 안 된다.
#[derive(Debug, Default)]
pub struct Loaded {
    /// 지금 이 컴포넌트에서 돌 수 있는 것.
    pub active: Vec<Plugin>,
    /// 정의는 멀쩡한데 **아직 동의를 안 받은 것.** 호출자가 물어볼 대상이다.
    /// 아무도 묻지 않으면 그냥 안 돈다.
    pub needs_consent: Vec<Plugin>,
    /// 이 컴포넌트의 `scope` 가 아니라 건너뛴 것(이름만).
    pub out_of_scope: Vec<String>,
    /// 읽거나 검증하다 실패한 것 — `(이름 또는 경로, 이유)`.
    pub errors: Vec<(String, String)>,
}

impl Loaded {
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}

/// `.guild/plugins`.
pub fn plugins_dir(guild_root: &Path) -> PathBuf {
    guild_root.join(".guild").join("plugins")
}

/// 이 컴포넌트에서 돌릴 것들을 읽는다.
///
/// 폴더가 없으면 빈 결과 — 플러그인을 안 쓰는 길드가 대부분이고, 그 경우
/// 비용이 0 이어야 한다.
pub fn load_for(guild_root: &Path, scope: Scope) -> Loaded {
    let mut out = Loaded::default();
    let dir = plugins_dir(guild_root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let granted = consent::load(guild_root).unwrap_or_default();

    let mut dirs: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort(); // 적재 순서를 파일시스템 순서에 맡기지 않는다(재현 가능하게).

    for pdir in dirs {
        let manifest = pdir.join("plugin.json");
        let label = pdir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        if !manifest.is_file() {
            continue; // 플러그인 폴더가 아니다 — 조용히 넘어간다.
        }
        match read_def(&manifest) {
            Err(e) => out.errors.push((label, e.to_string())),
            Ok(def) => {
                if !def.scope.contains(&scope) {
                    out.out_of_scope.push(def.name);
                    continue;
                }
                let (compiled, script_src) = match compile_script(&pdir, &def) {
                    Ok(c) => c,
                    Err(e) => {
                        // 스크립트가 깨졌으면 그 플러그인만 끈다 — 판단
                        // 계층이 없는 채로 돌면 안 보낼 것을 보내게 된다.
                        out.errors.push((label, e.to_string()));
                        continue;
                    }
                };
                let plugin = Plugin {
                    def,
                    dir: pdir.clone(),
                    compiled,
                    script_src,
                };
                if consent::is_granted(&granted, &plugin) {
                    out.active.push(plugin);
                } else {
                    out.needs_consent.push(plugin);
                }
            }
        }
    }
    out
}

/// 정의가 가리키는 스크립트를 읽어 컴파일한다. `script` 가 없으면 둘 다 `None`.
/// 원문도 함께 돌려준다 — 동의 지문이 그것을 본다.
type Compiled = (Option<std::sync::Arc<script::Script>>, Option<String>);
fn compile_script(dir: &Path, def: &PluginDef) -> AppResult<Compiled> {
    let Some(rel) = def.script.as_deref() else {
        return Ok((None, None));
    };
    let path = dir.join(rel);
    let src = std::fs::read_to_string(&path).map_err(|e| {
        AppError::BadRequest(format!(
            "{}: 스크립트 {} 를 읽지 못했습니다: {e}",
            def.name,
            path.display()
        ))
    })?;
    let compiled = script::Script::compile_source(&src)
        .map_err(|e| AppError::BadRequest(format!("{}: {e}", def.name)))?;
    Ok((Some(std::sync::Arc::new(compiled)), Some(src)))
}

/// 파일 하나를 읽고 **검증까지** 한다. 검증에 걸리면 적재하지 않는다.
pub fn read_def(manifest: &Path) -> AppResult<PluginDef> {
    let raw = std::fs::read_to_string(manifest)
        .map_err(|e| AppError::BadRequest(format!("plugin.json 읽기 실패: {e}")))?;
    let def: PluginDef = serde_json::from_str(&raw)
        .map_err(|e| AppError::BadRequest(format!("plugin.json 형식 오류: {e}")))?;
    validate(&def)?;
    Ok(def)
}

/// 적재 전 검사.
///
/// **여기서 막지 못하면 나중에 못 막는다** — 특히 비밀값은 한 번 커밋되면
/// 이력에 남는다.
pub fn validate(def: &PluginDef) -> AppResult<()> {
    if def.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "plugin.json: name 이 비어 있습니다".into(),
        ));
    }
    if def.on.is_empty() {
        return Err(AppError::BadRequest(format!(
            "{}: `on` 이 비어 있습니다 — 구독할 이벤트를 하나 이상 적으세요",
            def.name
        )));
    }
    if def.scope.is_empty() {
        return Err(AppError::BadRequest(format!(
            "{}: `scope` 가 비어 있습니다 — cli / gui / server 중 어디서 돌릴지 \
             명시해야 합니다(기본값을 두지 않는 이유는 놀라지 않기 위해서입니다)",
            def.name
        )));
    }
    // 아무 이벤트와도 안 맞는 패턴은 오타일 가능성이 높다. 조용히 안 도는
    // 것보다 적재 때 알려주는 편이 낫다.
    for pat in &def.on {
        let hits = crate::events::names::ALL.iter().any(|n| {
            crate::events::names::matches(pat, n, crate::events::Phase::Post)
                || crate::events::names::matches(pat, n, crate::events::Phase::Pre)
        });
        if !hits {
            return Err(AppError::BadRequest(format!(
                "{}: `{pat}` 는 어떤 이벤트와도 맞지 않습니다 — \
                 `openguild plugin events` 로 이름을 확인하세요",
                def.name
            )));
        }
    }
    // 스크립트 경로는 플러그인 폴더 안이어야 한다. `../../..` 를 적으면
    // 정의만으로 임의 파일을 컴파일 대상으로 지정할 수 있게 된다.
    if let Some(rel) = def.script.as_deref() {
        let p = Path::new(rel);
        if p.is_absolute()
            || p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(AppError::BadRequest(format!(
                "{}: `script` 는 플러그인 폴더 안의 상대 경로여야 합니다 (받은 값: {rel})",
                def.name
            )));
        }
    }
    check_no_literal_secret(def)?;
    Ok(())
}

/// 비밀값처럼 보이는 리터럴 거부(admin 확정).
///
/// `.guild/plugins/` 는 **git 에 커밋된다.** API 키를 적으면 이력에 남고,
/// 경고만 해서는 무시된다. 환경변수 참조(`${OPENAI_API_KEY}`)만 허용한다.
///
/// 판정은 **보수적으로** 한다 — 넓게 잡으면 멀쩡한 값이 막힌다. 두 갈래만 본다:
/// 1. 필드 이름이 비밀을 뜻하는데(authorization, api-key, token …) 값이
///    환경변수 참조가 아닌 경우.
/// 2. 어디에 있든 **알려진 키 접두사**(`sk-`, `ghp_`, `xox…`)로 시작하는 값.
fn check_no_literal_secret(def: &PluginDef) -> AppResult<()> {
    let reject = |what: &str| -> AppError {
        AppError::BadRequest(format!(
            "{}: {what} 에 비밀값으로 보이는 리터럴이 있습니다. \
             `.guild/plugins/` 는 git 에 커밋되므로 키를 그대로 적으면 이력에 남습니다 — \
             `${{ENV_VAR}}` 형태의 환경변수 참조를 쓰세요.",
            def.name
        ))
    };
    let strings: Vec<(String, &String)> = match &def.action {
        Action::Post { url, headers } => {
            let mut v: Vec<(String, &String)> = vec![("url".into(), url)];
            v.extend(headers.iter().map(|(k, val)| (format!("headers.{k}"), val)));
            v
        }
        Action::Run { command, args } => {
            let mut v: Vec<(String, &String)> = vec![("command".into(), command)];
            v.extend(
                args.iter()
                    .enumerate()
                    .map(|(i, a)| (format!("args[{i}]"), a)),
            );
            v
        }
    };
    for (field, value) in strings {
        if looks_like_known_key(value) {
            return Err(reject(&field));
        }
        if field_name_means_secret(&field) && !is_env_ref(value) {
            return Err(reject(&field));
        }
    }
    Ok(())
}

fn field_name_means_secret(field: &str) -> bool {
    let f = field.to_ascii_lowercase();
    [
        "authorization",
        "api-key",
        "api_key",
        "apikey",
        "token",
        "secret",
        "password",
    ]
    .iter()
    .any(|k| f.contains(k))
}

/// 값 전체가 환경변수 참조인가 — `${VAR}` 하나, 또는 접두사 + 참조
/// (`Bearer ${OPENAI_API_KEY}`).
fn is_env_ref(value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() {
        return true; // 빈 값은 비밀이 아니다.
    }
    // `${...}` 를 걷어낸 나머지에 영숫자 덩어리가 남으면 리터럴이 섞인 것이다.
    let stripped = strip_env_refs(v);
    !stripped
        .chars()
        .any(|c| c.is_ascii_alphanumeric() && stripped.len() > 8)
        || stripped.trim().eq_ignore_ascii_case("bearer")
        || stripped.trim().is_empty()
}

fn strip_env_refs(v: &str) -> String {
    let mut out = String::new();
    let bytes: Vec<char> = v.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '$'
            && i + 1 < bytes.len()
            && bytes[i + 1] == '{'
            && let Some(close) = bytes[i..].iter().position(|c| *c == '}')
        {
            i += close + 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// 알려진 키 접두사. 여기 없다고 안전한 것은 아니지만, **확실한 것만** 잡는다.
fn looks_like_known_key(value: &str) -> bool {
    let v = value.trim();
    const PREFIXES: &[&str] = &[
        "sk-",
        "sk_live_",
        "sk_test_",
        "ghp_",
        "gho_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "xoxa-",
        "AKIA",
    ];
    PREFIXES.iter().any(|p| v.contains(p))
}

/// `${VAR}` 를 실제 값으로. 없는 변수는 **빈 문자열이 아니라 오류** —
/// 조용히 인증 없이 요청을 보내면 원인을 찾기 어렵다.
pub fn expand_env(value: &str) -> AppResult<String> {
    let mut out = String::new();
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$'
            && i + 1 < chars.len()
            && chars[i + 1] == '{'
            && let Some(rel) = chars[i..].iter().position(|c| *c == '}')
        {
            let name: String = chars[i + 2..i + rel].iter().collect();
            let got = std::env::var(&name).map_err(|_| {
                AppError::BadRequest(format!("환경변수 {name} 가 설정되지 않았습니다"))
            })?;
            out.push_str(&got);
            i += rel + 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok(out)
}
