//! DEV-380: 컴포넌트가 사용자에게 보여주는 플러그인 상태.
//!
//! GUI 와 서버가 같은 모양을 내놓아야 한다 — 프런트는 하나뿐인데 어느 쪽이
//! 답하느냐에 따라 필드가 달라지면 화면이 갈린다. 그래서 여기 둔다.
//!
//! # 조회와 관리는 갈라져 있다
//!
//! 조회는 **누구나** 한다(브라우저 포함). 플러그인 정의는 어차피 길드 파일이라
//! 그 길드를 읽을 수 있는 사람은 이미 볼 수 있고, "이 공유 서버에 어떤 훅이
//! 걸려 있나" 는 오히려 보여야 하는 정보다.
//!
//! 관리(허용/철회/신뢰)는 다르다. 팀이 공유하는 주소로 동의를 받으면 "누구의
//! 동의인가" 가 흐려지고 `run` 을 열어 주는 원격 구멍이 된다. 그래서
//! [`PluginStatus::manageable`] 이 false 면 프런트는 버튼 자체를 안 그리고,
//! 서버는 애초에 그 경로를 열지 않는다.

use super::{Action, ActionRef, InputType, Plugin, Scope};
use crate::error::AppResult;
use serde::{Deserialize, Serialize};

/// 프런트로 넘기는 플러그인 한 건. 내부 구조체를 그대로 흘리지 않는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginView {
    pub name: String,
    /// REQ-020: 정의가 적어 둔 사람 말 설명. 없을 수 있다 — 선택 필드다.
    pub description: Option<String>,
    /// 모든 줄의 이벤트 패턴(바뀌기 전 단계는 `pre:` 를 붙인다).
    pub on: Vec<String>,
    pub scope: Vec<String>,
    /// DEV-403: 줄 목록 — 언제 → 무엇. 적힌 순서 그대로.
    pub handlers: Vec<HandlerView>,
    /// 이 플러그인이 내보낼 수 있는 곳 전부 — **무엇에 동의하는지**의 핵심.
    /// 정의에 적힌 그대로다(환경변수 참조는 안 푼다 — 값은 보여주지 않는다).
    pub actions: Vec<ActionView>,
    pub scripts: Vec<String>,
    /// DEV-408: 스크립트가 불러오는 파일 — 폴더 밖 것도 포함. 처음 한 번만 허락받고, 그 파일이
    /// 바뀌어도 다시 묻지 않는다.
    pub imports: Vec<String>,
    /// DEV-406: 스크립트가 길드에 시킬 수 있는 일 — 허용 화면이 보여 준다.
    pub permissions: Vec<String>,
    /// DEV-410: 정의가 지목한 환경변수 **이름**(헤더의 `${VAR}` 와 `body_env`). 어떤 비밀값을
    /// 쓰는지는 허용 전에 알아야 하고, **값은 절대 싣지 않는다** — 이 뷰는 HTTP 로도 나간다.
    pub env: Vec<String>,
    /// BUG-279: `run` 훅이 파일을 쓰는 자리. `post` 는 작업 디렉터리가 없으므로
    /// `None` 이다 — 안 쓰는 경로를 보여주면 "여기 뭐가 생기나" 하고 찾게 된다.
    ///
    /// 경로만 만든다(폴더는 안 만든다). 목록을 여는 것만으로 안 돌 플러그인의
    /// 폴더까지 생기면 안 된다.
    pub data_dir: Option<String>,
    /// REQ-021: 이 플러그인이 사용자에게 받아야 하는 값들 — 선언 + 지금 상태.
    pub inputs: Vec<InputView>,
    /// 스크립트 원문. 이걸 안 보여주면 동의가 형식만 남는다.
    pub script_src: Option<String>,
    pub granted: bool,
    /// 이 컴포넌트에서 도는가. scope 가 안 맞으면 허용해도 여기선 안 돈다.
    pub runs_here: bool,
    /// DEV-399: 어디서 온 것인가 — `None` 이면 이 길드의 `.guild/plugins/`,
    /// `Some(이름)` 이면 그 소스. 한 목록에 섞여 나오므로 출처가 보여야 한다.
    pub source: Option<String>,
    /// 그 폴더의 실제 경로 — 소스에서 온 것은 길드 밖이라 어디인지 알아야 한다.
    pub dir: String,
}

/// 핸들러 한 줄.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerView {
    /// `id`, 없으면 "N번째 줄".
    pub label: String,
    /// `pre` | `post`
    pub stage: String,
    pub events: Vec<String>,
    /// 부르는 스크립트 함수.
    pub call: Option<String>,
    /// DEV-405: 함수가 받는 연결 데이터 — 허용 화면이 "퀘스트를 읽음" 을 보여 준다.
    pub with: Vec<String>,
    /// DEV-407: 이 줄을 기다리나(명령이 끝나기 전에 끝난다).
    pub wait: bool,
    /// REQ-025: 이 줄이 불릴 조건 — `["change.to = done, closed"]` 처럼 사람이 읽을 한 줄씩.
    pub when: Vec<String>,
    /// 바로 실행하는 동작 — 이름 붙인 것이면 그 이름, 줄에 적은 것이면 `None` 이고 대신
    /// `action_kind`/`action_target` 이 찬다.
    pub action: Option<String>,
    pub action_kind: Option<String>,
    pub action_target: Option<String>,
}

/// 동작 하나.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionView {
    pub name: String,
    /// `post` | `run`
    pub kind: String,
    /// `post` 의 목적지 또는 `run` 의 명령(이 기계에서 실제로 띄울 것).
    pub target: String,
}

/// DEV-410: 정의가 지목한 환경변수 이름들. 값은 읽지 않는다 — 이름만으로 충분하고, 읽으면
/// 실수로 내보낼 길이 생긴다.
fn env_names(p: &Plugin) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |s: String| {
        if !out.contains(&s) {
            out.push(s);
        }
    };
    for (_, a) in p.def.all_actions() {
        if let Action::Post { headers, body_env, .. } = a {
            for v in headers.values() {
                for name in env_refs(v) {
                    add(name);
                }
            }
            for v in body_env.values() {
                add(v.clone());
            }
        }
    }
    out
}

/// `"Bearer ${TOKEN}"` → `["TOKEN"]`.
fn env_refs(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("${") {
        rest = &rest[i + 2..];
        let Some(end) = rest.find('}') else { break };
        let name = rest[..end].trim();
        if !name.is_empty() {
            out.push(name.to_string());
        }
        rest = &rest[end + 1..];
    }
    out
}

fn action_target(a: &Action) -> String {
    match a {
        Action::Post { url, .. } => url.clone(),
        Action::Run { .. } => {
            let (command, args) = a.run_command().expect("run 동작이다");
            format!("{command} {}", args.join(" ")).trim_end().to_string()
        }
    }
}

/// REQ-021: 입력 하나를 화면이 그릴 수 있는 모양으로.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputView {
    pub key: String,
    /// `label` 이 없으면 `key` 로 채워 보낸다 — 화면이 다시 판단하지 않게.
    pub label: String,
    /// `text` | `checkbox` | `select` | `number`
    #[serde(rename = "type")]
    pub input_type: String,
    pub help: Option<String>,
    pub secret: bool,
    pub options: Vec<InputOptionView>,
    /// 지금 값. **`secret` 이면 언제나 `None` 이다** — 화면에 뿌릴 이유가 없고,
    /// HTTP 로 조회하는 경로도 같은 모양을 쓴다(브라우저로 토큰이 나가면 안
    /// 된다). 값이 있는지는 [`has_value`](Self::has_value) 로 알린다.
    pub value: Option<serde_json::Value>,
    pub has_value: bool,
    /// `stored` | `env` | `default` | `missing` — 사용자가 **덮어쓸지** 판단
    /// 하려면 값이 어디서 왔는지 알아야 한다.
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputOptionView {
    pub value: String,
    pub label: String,
}

/// 한 길드의 플러그인 상태 전부.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatus {
    pub plugins: Vec<PluginView>,
    /// 읽거나 검증하다 실패한 것 — `(이름, 이유)`.
    pub errors: Vec<(String, String)>,
    /// BUG-288: 자동 허용이 켜져 있나 — 새로 오거나 바뀐 것도 묻지 않고 돈다.
    pub auto_allow: bool,
    /// 이 경로로 허용/철회까지 할 수 있나. HTTP 로 답할 때는 **항상 false**.
    pub manageable: bool,
    /// DEV-383: 아직 길드를 안 열었나. **문장이 아니라 상태로 넘긴다** — 예전엔
    /// Rust 가 만든 한국어 문장을 그대로 렌더해서, 영어 UI 한가운데 한국어가
    /// 한 줄 끼었다. 문구는 프런트가 자기 언어로 만든다.
    pub no_guild: bool,
    /// 전달 중 쌓인 문제(최근 것부터 잘림). 비어 있는 것이 정상이다.
    #[serde(default)]
    pub problems: Vec<String>,
}

impl PluginStatus {
    /// 아직 길드를 안 열었다. 문구는 프런트가 만든다.
    pub fn no_guild() -> Self {
        Self {
            plugins: Vec::new(),
            errors: Vec::new(),
            auto_allow: false,
            manageable: false,
            no_guild: true,
            problems: Vec::new(),
        }
    }
}

pub(super) fn view(p: &Plugin, granted: bool, scope: Scope) -> PluginView {
    let handlers = p
        .def
        .handlers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let (action, action_kind, action_target) = match &h.action {
                Some(ActionRef::Named(n)) => (Some(n.clone()), None, None),
                Some(ActionRef::Inline(a)) => {
                    (None, Some(a.kind().to_string()), Some(action_target(a)))
                }
                None => (None, None, None),
            };
            HandlerView {
                label: h.label(i),
                stage: if h.pre.is_empty() { "post" } else { "pre" }.into(),
                events: h.patterns().to_vec(),
                call: h.call.clone(),
                with: h.with.clone(),
                wait: h.wait,
                when: h
                    .when
                    .iter()
                    .map(|(k, v)| {
                        let shown = match v {
                            serde_json::Value::Array(a) => a
                                .iter()
                                .map(|x| x.to_string().trim_matches('"').to_string())
                                .collect::<Vec<_>>()
                                .join(", "),
                            other => other.to_string().trim_matches('"').to_string(),
                        };
                        format!("{k} = {shown}")
                    })
                    .collect(),
                action,
                action_kind,
                action_target,
            }
        })
        .collect();
    let actions = p
        .def
        .actions
        .iter()
        .map(|(name, a)| ActionView {
            name: name.clone(),
            kind: a.kind().into(),
            target: action_target(a),
        })
        .collect();
    PluginView {
        name: p.def.name.clone(),
        description: p.def.description.clone(),
        on: p.def.subscriptions(),
        scope: p.def.scope.iter().map(scope_label).collect(),
        handlers,
        actions,
        scripts: p.def.scripts.clone(),
        imports: p.imports.clone(),
        permissions: p.def.permissions.clone(),
        env: env_names(p),
        data_dir: if p.def.has_run() {
            super::data_dir_path(&p.guild_root, &p.def.name)
                .ok()
                .map(|d| d.display().to_string())
        } else {
            None
        },
        inputs: input_views(p),
        source: p.source.clone(),
        dir: p.dir.display().to_string(),
        script_src: p.script_src.clone(),
        granted,
        runs_here: p.def.scope.contains(&scope),
    }
}

fn input_views(p: &Plugin) -> Vec<InputView> {
    let inputs = p.def.effective_inputs();
    if inputs.is_empty() {
        return Vec::new();
    }
    let resolved = super::values::resolve(&p.guild_root, &p.def);
    inputs
        .iter()
        .map(|i| {
            let r = resolved.get(&i.key);
            let has = r.is_some_and(|r| r.value.is_some());
            InputView {
                key: i.key.clone(),
                label: i.label().to_string(),
                input_type: match i.input_type {
                    InputType::Text => "text",
                    InputType::Checkbox => "checkbox",
                    InputType::Select => "select",
                    InputType::Number => "number",
                }
                .into(),
                help: i.help.clone(),
                secret: i.secret,
                options: i
                    .options
                    .iter()
                    .map(|o| InputOptionView {
                        value: o.value.clone(),
                        label: o.label().to_string(),
                    })
                    .collect(),
                // 비밀은 여기서 끊는다. 화면이 실수로 그릴 기회조차 없어야 한다.
                value: if i.secret {
                    None
                } else {
                    r.and_then(|r| r.value.clone())
                },
                has_value: has,
                source: match r.map(|r| r.source) {
                    Some(super::values::Source::Stored) => "stored",
                    Some(super::values::Source::Env) => "env",
                    Some(super::values::Source::Default) => "default",
                    _ => "missing",
                }
                .into(),
            }
        })
        .collect()
}

fn scope_label(s: &Scope) -> String {
    match s {
        Scope::Cli => "cli",
        Scope::Gui => "gui",
        Scope::Server => "server",
    }
    .to_string()
}

/// 이 길드의 상태를 읽는다. **scope 를 가리지 않는다** — 목록에서 걸러 버리면
/// 다른 컴포넌트 전용 플러그인은 존재조차 안 보이고 허용할 방법이 사라진다.
/// `scope` 는 "여기서 도는가"(`runs_here`) 판정에만 쓴다.
pub fn status(store: &crate::Store, scope: Scope, manageable: bool) -> AppResult<PluginStatus> {
    let guild_root = &store.paths.guild_root;
    let loaded = super::load_all(guild_root);
    let granted = super::consent::load(guild_root)?;
    Ok(PluginStatus {
        plugins: loaded
            .active
            .iter()
            .map(|p| view(p, true, scope))
            .chain(loaded.needs_consent.iter().map(|p| view(p, false, scope)))
            .collect(),
        errors: loaded.errors,
        auto_allow: granted.auto_allow,
        manageable,
        no_guild: false,
        // DEV-381: 여기까지 올려야 사용자가 본다. 예전엔 모아만 두고 아무도
        // 안 읽어서, 훅이 조용히 실패해도 화면에 아무것도 안 떴다.
        problems: store.plugin_problems(),
    })
}
