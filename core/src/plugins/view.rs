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

use super::{Action, Plugin, Scope};
use crate::error::AppResult;
use serde::{Deserialize, Serialize};

/// 프런트로 넘기는 플러그인 한 건. 내부 구조체를 그대로 흘리지 않는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginView {
    pub name: String,
    pub on: Vec<String>,
    pub scope: Vec<String>,
    /// `post` | `run`
    pub action: String,
    /// `post` 의 목적지 또는 `run` 의 명령 — **무엇에 동의하는지**의 핵심.
    /// 정의에 적힌 그대로다(환경변수 참조는 안 푼다 — 값은 보여주지 않는다).
    pub target: String,
    pub script: Option<String>,
    /// 스크립트 원문. 이걸 안 보여주면 동의가 형식만 남는다.
    pub script_src: Option<String>,
    pub granted: bool,
    /// 이 컴포넌트에서 도는가. scope 가 안 맞으면 허용해도 여기선 안 돈다.
    pub runs_here: bool,
}

/// 한 길드의 플러그인 상태 전부.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatus {
    pub plugins: Vec<PluginView>,
    /// 읽거나 검증하다 실패한 것 — `(이름, 이유)`.
    pub errors: Vec<(String, String)>,
    /// 길드 통째 신뢰가 켜져 있나.
    pub trusted: bool,
    /// 이 경로로 허용/철회까지 할 수 있나. HTTP 로 답할 때는 **항상 false**.
    pub manageable: bool,
    /// 보여줄 안내 — 아직 길드를 안 열었다든지. 비어 있는 것이 정상이다.
    pub notes: Vec<String>,
    /// 전달 중 쌓인 문제(최근 것부터 잘림). 비어 있는 것이 정상이다.
    #[serde(default)]
    pub problems: Vec<String>,
}

impl PluginStatus {
    /// 보여줄 게 없다고 답한다(길드를 아직 안 열었을 때 등).
    pub fn empty(note: impl Into<String>) -> Self {
        Self {
            plugins: Vec::new(),
            errors: Vec::new(),
            trusted: false,
            manageable: false,
            notes: vec![note.into()],
            problems: Vec::new(),
        }
    }
}

fn view(p: &Plugin, granted: bool, scope: Scope) -> PluginView {
    let (action, target) = match &p.def.action {
        Action::Post { url, .. } => ("post", url.clone()),
        Action::Run { command, args, .. } => (
            "run",
            format!("{command} {}", args.join(" "))
                .trim_end()
                .to_string(),
        ),
    };
    PluginView {
        name: p.def.name.clone(),
        on: p.def.on.clone(),
        scope: p.def.scope.iter().map(scope_label).collect(),
        action: action.into(),
        target,
        script: p.def.script.clone(),
        script_src: p.script_src.clone(),
        granted,
        runs_here: p.def.scope.contains(&scope),
    }
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
        trusted: granted.trusted,
        manageable,
        notes: Vec::new(),
        // DEV-381: 여기까지 올려야 사용자가 본다. 예전엔 모아만 두고 아무도
        // 안 읽어서, 훅이 조용히 실패해도 화면에 아무것도 안 떴다.
        problems: store.plugin_problems(),
    })
}
