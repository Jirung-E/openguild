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
pub mod delivery;
pub mod runtime;
pub mod script;
#[cfg(test)]
mod tests;
pub mod values;
pub mod view;

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
        /// DEV-384: 본문에 끼워 넣을 환경변수 — `{ "chat_id": "TELEGRAM_CHAT_ID" }`.
        ///
        /// **왜 필요한가.** 스크립트에는 I/O 가 없다(그게 [[DEV-376]] 의 전부다)
        /// — 그래서 `payload()` 는 환경변수를 읽을 수 없다. 그런데 텔레그램의
        /// `sendMessage` 처럼 **본문에 개인 식별자를 요구하는** API 가 흔하다.
        /// url 과 헤더만 확장해서는 그런 API 를 못 쓴다.
        ///
        /// 본문 전체를 훑어 `${VAR}` 를 푸는 방법도 있었지만 안 했다 — 사용자가
        /// 쓴 댓글 본문이 우연히 `${HOME}` 이면 그게 확장돼 나간다. 여기서는
        /// **정의가 지목한 키에만** 넣으므로 그런 일이 없다.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        body_env: BTreeMap<String, String>,
        /// 응답을 기다리는 한도(ms). 비동기라 길드는 안 멈추지만 무한정
        /// 붙들면 전달 스레드가 막힌다. 없으면 [`DEFAULT_TIMEOUT_MS`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
    /// 프로세스를 띄우고 이벤트 JSON 을 **stdin** 으로 넘긴다.
    /// 인자로 넘기면 길이 제한과 이스케이프 문제가 생긴다.
    Run {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
}

/// 전달 시한 기본값. 넉넉하되 무한은 아니다 — AI 응답이 느릴 수 있다.
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;
/// 정의가 아무리 크게 적어도 여기까지. 하나가 스레드를 영원히 잡으면 뒤가 다
/// 막힌다.
pub const MAX_TIMEOUT_MS: u64 = 60_000;

impl Action {
    /// 실제로 쓸 시한.
    pub fn timeout(&self) -> std::time::Duration {
        let ms = match self {
            Action::Post { timeout_ms, .. } | Action::Run { timeout_ms, .. } => *timeout_ms,
        };
        std::time::Duration::from_millis(ms.unwrap_or(DEFAULT_TIMEOUT_MS).min(MAX_TIMEOUT_MS))
    }
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
    /// REQ-020: 이 플러그인이 무슨 일을 하는지 **사람 말로**. 선택이다.
    ///
    /// 나머지 필드는 전부 기계가 읽는 값이라, 처음 보는 플러그인 앞에서
    /// "이게 무슨 일을 하는가" 를 알려주는 것이 하나도 없었다. 이 화면의 존재
    /// 이유가 *무엇에 동의하는지 보여주는 것*([[DEV-383]])인데 정작 "무엇을" 에
    /// 해당하는 문장이 없었다. 스크립트 원문을 펼칠 수는 있지만 그건 읽을 수
    /// 있는 사람에게만 답이다.
    ///
    /// **없을 때 직렬화에 나타나지 않는다**(`skip_serializing_if`). 동의 지문이
    /// 정의를 통째로 담으므로([`consent::fingerprint`]), 나타났다면 이 필드가
    /// 생긴 것만으로 기존 동의가 전부 무효가 됐을 것이다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 구독 패턴 — `quest.created`, `quest.*`, `*.created`, `pre:quest.*`.
    pub on: Vec<String>,
    /// **비어 있으면 적재하지 않는다.** 위 [`Scope`] 주석 참고.
    pub scope: Vec<Scope>,
    pub action: Action,
    /// 판단·가공 스크립트(폴더 기준 상대 경로). [[DEV-376]] 이 실행한다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// REQ-021: 이 플러그인이 사용자에게 받아야 하는 값들.
    ///
    /// 이름·설명·선택지는 **플러그인 작성자만 안다.** 코어가 `${GITHUB_TOKEN}`
    /// 이라는 참조만 보고 "repo 스코프가 필요하고 github.com/settings/tokens
    /// 에서 만든다" 를 알아낼 방법은 없다. 그래서 정의가 선언한다.
    ///
    /// [`description`](Self::description) 과 마찬가지로 **없으면 직렬화에
    /// 안 나타난다** — 동의 지문이 정의를 통째로 담기 때문이다.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<Input>,
}

/// 위젯 종류. 값이 코어로 들어오는 통로(`${...}`)는 하나지만, **사람이 값을
/// 넣는 방법**은 값의 성격마다 다르다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
    #[default]
    Text,
    Checkbox,
    Select,
    Number,
}

/// `select` 의 선택지 하나. `value` 가 실제 값이고 `label` 이 보이는 말이다 —
/// "5분" 을 그대로 값으로 쓰면 문구를 고치는 순간 저장된 값이 고아가 된다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputOption {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl InputOption {
    pub fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.value)
    }
}

/// REQ-021: 사용자에게 받을 값 하나.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    /// 스크립트의 `config("KEY")` 와 정의의 `${KEY}` 가 쓰는 이름.
    pub key: String,
    /// 화면에 뜨는 이름. 없으면 `key` 를 쓴다 — `TELEGRAM_BOT_TOKEN` 보다
    /// "봇 토큰" 이 낫지만, 안 적었다고 화면이 비면 더 나쁘다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(
        default,
        rename = "type",
        skip_serializing_if = "is_default_input_type"
    )]
    pub input_type: InputType,
    /// 위젯 아래 한 줄 — 토큰을 어디서 받는지 같은 것.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// **화면에서 가린다**는 뜻이다. 스크립트는 그래도 읽는다 — 스크립트가
    /// 만든 payload 는 정의가 적은 그 주소로만 나가고 사용자는 그 주소에
    /// 동의했으므로, 못 읽게 해도 막는 것이 없으면서 "왜 이 값만 안 읽히지"
    /// 를 만든다.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
    /// 사용자가 아직 안 정했을 때의 값. 체크박스는 `true`/`false`, 숫자는
    /// 숫자, 나머지는 문자열.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// `select` 전용.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<InputOption>,
}

fn is_default_input_type(t: &InputType) -> bool {
    *t == InputType::Text
}

impl Input {
    pub fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.key)
    }
}

/// 적재된 플러그인 하나.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub def: PluginDef,
    /// `.guild/plugins/{name}/` — 스크립트 경로 해석의 기준. **지문의 대상**
    /// 이므로 훅이 여기에 파일을 쓰면 안 된다([`Plugin::data_dir`]).
    pub dir: PathBuf,
    /// BUG-279: 어느 길드의 플러그인인가 — 데이터 폴더를 가르는 데 쓴다.
    pub guild_root: PathBuf,
    /// DEV-376: 적재 때 컴파일해 둔 판단·가공 스크립트. 문법 오류는 **적재
    /// 때** 드러나야지 첫 이벤트 때 드러나면 안 된다. 없으면 전부 보내고
    /// 이벤트 JSON 을 그대로 쓴다.
    pub compiled: Option<std::sync::Arc<script::Script>>,
    /// 그 스크립트의 원문 — 동의 지문에 들어간다([`consent`]). 정의만 보면
    /// 스크립트를 갈아끼우는 것으로 동의를 우회할 수 있다.
    pub script_src: Option<String>,
    /// DEV-381: 폴더 안의 **모든** 파일(정의 제외). `run` 은 이 폴더를 작업
    /// 디렉터리로 삼고 도므로, 옆에 있는 `hook.py` 도 실행되는 코드다 —
    /// 지문이 그것까지 봐야 갈아끼우기가 안 통한다.
    pub folder: BTreeMap<String, String>,
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

    /// BUG-279: 훅이 **파일을 쓰는 자리.** 없으면 만든다.
    pub fn data_dir(&self) -> AppResult<PathBuf> {
        data_dir(&self.guild_root, &self.def.name)
    }
}

/// BUG-279: `~/.openguild/plugin-data/{길드}/{플러그인}/`.
///
/// # 왜 플러그인 폴더가 아닌가
///
/// `run` 은 예전에 **플러그인 폴더**를 작업 디렉터리로 삼았다. 옆 파일
/// (`notify.sh`)을 부르려면 그래야 했다. 그런데 동의 지문은 그 폴더의 **모든
/// 파일**을 본다([[DEV-381]]) — `hook.py` 갈아끼우기를 막으려던 것이다.
///
/// 둘이 겹치면 덫이 된다. 훅이 출력을 옆에 쓰는 순간 폴더 지문이 바뀌어
/// **자기 동의를 스스로 깬다.** 한 번은 돌고 그 뒤로 조용히 안 돈다.
/// admin 이 `deleted-audit` 으로 실제로 밟았고, 덤으로 지워진 퀘스트 내용이
/// `plugin-consent.json` 안에 쌓이고 있었다(지문이 텍스트를 원문으로 담는다).
///
/// 그래서 **코드와 데이터를 가른다.** 폴더는 git 으로 따라오는 코드이고 지문의
/// 대상이다. 훅이 만드는 것은 그 기계의 데이터고, 동의가 사는 곳 옆에 둔다 —
/// "정의는 git 으로 오고 나머지는 기계에 남는다"([[DEV-375]])를 그대로 잇는다.
/// 코드 폴더는 `OPENGUILD_PLUGIN_DIR` 로 알려주므로 옆 파일도 여전히 부른다.
pub fn data_dir(guild_root: &Path, plugin_name: &str) -> AppResult<PathBuf> {
    let p = data_dir_path(guild_root, plugin_name)?;
    std::fs::create_dir_all(&p).map_err(|e| {
        AppError::Internal(anyhow::anyhow!(
            "플러그인 데이터 폴더를 만들지 못했습니다 {}: {e}",
            p.display()
        ))
    })?;
    Ok(p)
}

/// 경로만 — **만들지는 않는다.** 화면이 "여기에 씁니다" 라고 보여줄 때 쓴다.
/// 조회하는 것만으로 안 쓸 폴더까지 만들면 `post` 플러그인 몫까지 생긴다.
pub fn data_dir_path(guild_root: &Path, plugin_name: &str) -> AppResult<PathBuf> {
    Ok(crate::user_dirs::openguild_home()?
        .join("plugin-data")
        .join(guild_data_key(guild_root))
        .join(safe_segment(plugin_name)))
}

/// 길드 하나를 가리키는 폴더 이름. 읽을 수 있게 마지막 경로 조각을 앞에 두고,
/// **경로 전체의 해시**를 붙여 이름이 같은 다른 길드와 안 섞이게 한다.
fn guild_data_key(guild_root: &Path) -> String {
    let norm = crate::recents::normalize_abs(guild_root);
    let label = Path::new(&norm)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("guild");
    format!("{}-{:016x}", safe_segment(label), checksum(norm.as_bytes()))
}

/// 경로 조각 하나로 안전한 문자열. **`name` 은 사용자가 적는 값**이라
/// `../../..` 이나 `/` 가 들어올 수 있고, 그대로 이어 붙이면 폴더를 벗어난다.
fn safe_segment(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    // 전부 점이면(`.`, `..`) 상위 폴더를 뜻한다.
    if out.trim_matches('.').is_empty() {
        return "_".into();
    }
    out.chars().take(64).collect()
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
    load_scoped(guild_root, Some(scope))
}

/// scope 를 가리지 않고 전부 읽는다 — **동의를 관리하는 쪽**이 쓴다.
///
/// GUI 전용 플러그인의 허용을 CLI 에서 할 수 있어야 한다. scope 로 걸러 버리면
/// 그 플러그인은 목록에도 안 나오고 허용할 방법이 없다.
pub fn load_all(guild_root: &Path) -> Loaded {
    load_scoped(guild_root, None)
}

fn load_scoped(guild_root: &Path, scope: Option<Scope>) -> Loaded {
    let mut out = Loaded::default();
    let dir = plugins_dir(guild_root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let granted = consent::load(guild_root).unwrap_or_default();

    // DEV-383: 같은 `name` 이 둘이면 동의도 화면도 그 이름으로 구분하는데 어느
    // 쪽인지 알 수 없다. 프런트의 `{#each}` 키도 이름이라 화면이 통째로 깨졌다.
    // 폴더를 복사하고 이름을 안 고치는 건 흔한 실수다 — 적재에서 걸러 준다.
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

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
                if let Some(sc) = scope
                    && !def.scope.contains(&sc)
                {
                    out.out_of_scope.push(def.name);
                    continue;
                }
                if !seen_names.insert(def.name.clone()) {
                    out.errors.push((
                        label,
                        format!(
                            "`name` 이 겹칩니다: {} — 폴더마다 다른 이름이어야 합니다\
                             (동의도 화면도 이름으로 구분합니다).",
                            def.name
                        ),
                    ));
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
                    guild_root: guild_root.to_path_buf(),
                    compiled,
                    script_src,
                    folder: folder_fingerprint(&pdir),
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

/// DEV-381: 플러그인 폴더 안의 **모든** 파일을 동의 지문용으로 읽는다.
///
/// 지문이 `plugin.json` + 지정한 `.rhai` 하나만 보던 것이 구멍이었다. `run` 은
/// 폴더를 작업 디렉터리로 삼고 도는데, 옆에 있는 `hook.py` / `notify.sh` 는
/// git 으로 따라오는 **실행되는 코드**다. 그걸 갈아끼우면 동의를 다시 묻지
/// 않았다.
///
/// 텍스트는 원문 그대로 담는다 — 사용자가 무엇에 동의하는지 볼 수 있어야
/// 한다는 [`consent`] 의 판단을 그대로 잇는다. 크거나 UTF-8 이 아닌 파일은
/// 길이와 체크섬만 담는다(내용을 보여줄 수도, 통째로 저장할 수도 없다).
const FINGERPRINT_TEXT_LIMIT: usize = 64 * 1024;

pub fn folder_fingerprint(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort(); // 파일시스템 순서에 맡기지 않는다.
    for p in paths {
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == "plugin.json" {
            continue; // 정의는 따로 담는다.
        }
        if p.is_dir() {
            // 하위 폴더는 이름만 — 재귀까지 가면 지문이 끝없이 커진다.
            out.insert(format!("{name}/"), "<dir>".into());
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        let text = std::str::from_utf8(&bytes).ok();
        let value = match text {
            Some(t) if bytes.len() <= FINGERPRINT_TEXT_LIMIT => t.to_string(),
            _ => format!("<bin len={} sum={:016x}>", bytes.len(), checksum(&bytes)),
        };
        out.insert(name.to_string(), value);
    }
    out
}

/// FNV-1a 64. 암호학적 해시가 아니다 — **바뀐 걸 알아채기 위한 것**이고,
/// 그 목적에는 충분하며 의존성을 안 늘린다([[BUG-267]] 과 같은 판단).
fn checksum(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
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
    // DEV-381: 스크립트도 git 으로 따라간다. `plugin.json` 의 리터럴은 막으면서
    // `.rhai` 안의 리터럴은 통과시키면 앞뒤가 안 맞는다 — 게다가 본문(payload)
    // 은 환경변수 확장을 안 거치므로, 스크립트에 박는 것이 토큰을 싣는 유일한
    // 길이었다.
    if looks_like_known_key(&src) {
        return Err(AppError::BadRequest(format!(
            "{}: 스크립트 {rel} 에 비밀값으로 보이는 리터럴이 있습니다. \
             `.guild/plugins/` 는 git 에 커밋됩니다 — 값은 환경변수로 넘기세요.",
            def.name
        )));
    }
    let compiled = script::Script::compile_source(&src)
        .map_err(|e| AppError::BadRequest(format!("{}: {e}", def.name)))?;
    Ok((Some(std::sync::Arc::new(compiled)), Some(src)))
}

/// 파일 하나를 읽고 **검증까지** 한다. 검증에 걸리면 적재하지 않는다.
pub fn read_def(manifest: &Path) -> AppResult<PluginDef> {
    let raw = std::fs::read_to_string(manifest)
        .map_err(|e| AppError::BadRequest(format!("plugin.json 읽기 실패: {e}")))?;
    let mut def: PluginDef = serde_json::from_str(&raw)
        .map_err(|e| AppError::BadRequest(format!("plugin.json 형식 오류: {e}")))?;
    normalize(&mut def);
    validate(&def)?;
    Ok(def)
}

/// REQ-020: 설명 길이 상한. 설정 화면의 한 줄짜리 목록에 그대로 그려지고
/// 동의 지문에도 들어간다 — 상한이 없으면 목록에 문단이 들어오고 지문 파일이
/// 정의 하나로 부푼다. 긴 설명은 플러그인 폴더의 README 자리다.
pub const MAX_DESCRIPTION_CHARS: usize = 500;

/// 읽은 직후의 정규화. **검증 전에** 돈다.
///
/// REQ-020: 공백뿐인 설명을 없는 것으로 만든다. 지문에서는 `""` 와 "없음" 이
/// 다른 값이라, 화면에는 똑같이 아무것도 안 보이는 두 상태가 동의를 다시
/// 묻게 만든다. 앞뒤 공백도 여기서 턴다 — 화면과 지문이 같은 문자열을 본다.
fn normalize(def: &mut PluginDef) {
    if let Some(d) = def.description.take() {
        let trimmed = d.trim();
        if !trimmed.is_empty() {
            def.description = Some(trimmed.to_string());
        }
    }
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
    // BUG-279: `name` 은 동의 파일의 키이자 데이터 폴더 이름이다. 경로 조각이
    // 될 수 없는 값을 적재 때 막는다 — 정의는 git 으로 남의 기계에 간다.
    if def.name.contains(['/', '\\']) || def.name.trim_matches('.').is_empty() {
        return Err(AppError::BadRequest(format!(
            "plugin.json: `name` 에 경로 구분자나 점만 쓸 수 없습니다 (받은 값: {})",
            def.name
        )));
    }
    if def.on.is_empty() {
        return Err(AppError::BadRequest(format!(
            "{}: `on` 이 비어 있습니다 — 구독할 이벤트를 하나 이상 적으세요",
            def.name
        )));
    }
    // REQ-020: 문자 수로 센다 — 바이트로 세면 한글 설명이 영어의 3분의 1 길이
    // 에서 막힌다.
    if let Some(d) = def.description.as_deref() {
        let n = d.chars().count();
        if n > MAX_DESCRIPTION_CHARS {
            return Err(AppError::BadRequest(format!(
                "{}: `description` 이 너무 깁니다 ({n}자, 최대 {MAX_DESCRIPTION_CHARS}자) — \
                 설정 화면의 한 줄 설명 자리입니다. 긴 설명은 플러그인 폴더의 \
                 README 에 두세요.",
                def.name
            )));
        }
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
        use crate::events::Phase;
        use crate::events::names as ev;
        let hits = ev::ALL
            .iter()
            .any(|n| ev::matches(pat, n, Phase::Post) || ev::matches(pat, n, Phase::Pre));
        if !hits {
            return Err(AppError::BadRequest(format!(
                "{}: `{pat}` 는 어떤 이벤트와도 맞지 않습니다 — \
                 `openguild plugin events` 로 이름을 확인하세요",
                def.name
            )));
        }
        // DEV-381: 이름은 맞는데 그 이벤트가 **pre 를 안 내는** 경우를 잡는다.
        // 예전엔 `pre:quest.created` 가 통과한 뒤 아무 오류 없이 영원히 안
        // 돌았다 — 오타를 적재 때 잡으면서 없는 phase 를 통과시키면 앞뒤가
        // 안 맞는다. 와일드카드(`pre:*`)는 하나라도 맞으면 통과시킨다.
        if pat.starts_with("pre:") {
            let any_pre = ev::PRE_CAPABLE
                .iter()
                .any(|n| ev::matches(pat, n, Phase::Pre));
            if !any_pre {
                return Err(AppError::BadRequest(format!(
                    "{}: `{pat}` — 그 이벤트는 관찰 pre 를 내지 않습니다. \
                     현재 pre 가 있는 것: {}",
                    def.name,
                    ev::PRE_CAPABLE.join(", ")
                )));
            }
        }
    }
    validate_inputs(def)?;
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

/// REQ-021: 입력 개수·길이 상한. 화면에 그대로 그려지고 동의 지문에도 들어간다.
pub const MAX_INPUTS: usize = 32;
pub const MAX_INPUT_OPTIONS: usize = 64;

/// REQ-021: `inputs` 검사.
///
/// 여기서 막지 못하면 **화면에서 드러난다** — 선택지 없는 선택상자, 저장할 수
/// 없는 기본값, 어느 칸에 넣어야 할지 모를 중복 키. 전부 적재 때 알 수 있는
/// 것들이다.
fn validate_inputs(def: &PluginDef) -> AppResult<()> {
    if def.inputs.len() > MAX_INPUTS {
        return Err(AppError::BadRequest(format!(
            "{}: `inputs` 가 너무 많습니다 ({}개, 최대 {MAX_INPUTS}개)",
            def.name,
            def.inputs.len()
        )));
    }
    let mut seen = std::collections::HashSet::new();
    for i in &def.inputs {
        if i.key.trim().is_empty() {
            return Err(AppError::BadRequest(format!(
                "{}: `inputs` 의 `key` 가 비어 있습니다",
                def.name
            )));
        }
        // 값 저장소의 키이자 `${...}` 안에 들어가는 이름이다. 아무 문자나
        // 받으면 `${A}B` 와 `${A}` + `B` 를 가를 수 없다.
        if !i
            .key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(AppError::BadRequest(format!(
                "{}: `inputs` 의 `key` 는 영숫자·`_`·`-` 만 됩니다 (받은 값: {})",
                def.name, i.key
            )));
        }
        if !seen.insert(&i.key) {
            return Err(AppError::BadRequest(format!(
                "{}: `inputs` 의 `key` 가 겹칩니다: {} — 화면도 저장소도 이 \
                 이름으로 구분합니다",
                def.name, i.key
            )));
        }
        if let Some(h) = &i.help
            && h.chars().count() > MAX_DESCRIPTION_CHARS
        {
            return Err(AppError::BadRequest(format!(
                "{}: `{}` 의 `help` 가 너무 깁니다 (최대 {MAX_DESCRIPTION_CHARS}자)",
                def.name, i.key
            )));
        }
        match i.input_type {
            InputType::Select => {
                if i.options.is_empty() {
                    return Err(AppError::BadRequest(format!(
                        "{}: `{}` 은 select 인데 `options` 가 비어 있습니다 — \
                         고를 것이 없는 선택상자가 됩니다",
                        def.name, i.key
                    )));
                }
                if i.options.len() > MAX_INPUT_OPTIONS {
                    return Err(AppError::BadRequest(format!(
                        "{}: `{}` 의 `options` 가 너무 많습니다 (최대 {MAX_INPUT_OPTIONS}개)",
                        def.name, i.key
                    )));
                }
                // 기본값이 목록에 없으면 화면이 아무것도 안 고른 채로 뜬다.
                if let Some(d) = i.default.as_ref().and_then(|v| v.as_str())
                    && !i.options.iter().any(|o| o.value == d)
                {
                    return Err(AppError::BadRequest(format!(
                        "{}: `{}` 의 `default`({d})가 `options` 에 없습니다",
                        def.name, i.key
                    )));
                }
            }
            // select 가 아닌데 선택지를 적었으면 type 을 빠뜨린 것이다.
            _ if !i.options.is_empty() => {
                return Err(AppError::BadRequest(format!(
                    "{}: `{}` 에 `options` 가 있는데 type 이 select 가 아닙니다",
                    def.name, i.key
                )));
            }
            InputType::Checkbox if matches!(&i.default, Some(v) if !v.is_boolean()) => {
                return Err(AppError::BadRequest(format!(
                    "{}: `{}` 은 checkbox 인데 `default` 가 true/false 가 아닙니다",
                    def.name, i.key
                )));
            }
            InputType::Number if matches!(&i.default, Some(v) if !v.is_number()) => {
                return Err(AppError::BadRequest(format!(
                    "{}: `{}` 은 number 인데 `default` 가 숫자가 아닙니다",
                    def.name, i.key
                )));
            }
            _ => {}
        }
    }
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
        Action::Post { url, headers, .. } => {
            let mut v: Vec<(String, &String)> = vec![("url".into(), url)];
            v.extend(headers.iter().map(|(k, val)| (format!("headers.{k}"), val)));
            v
        }
        Action::Run { command, args, .. } => {
            let mut v: Vec<(String, &String)> = vec![("command".into(), command)];
            v.extend(
                args.iter()
                    .enumerate()
                    .map(|(i, a)| (format!("args[{i}]"), a)),
            );
            v
        }
    };
    // REQ-020: 설명도 git 에 커밋되는 자유 텍스트다. 다른 필드는 막으면서
    // 여기만 열어 두면 토큰을 적을 자리를 하나 만들어 주는 셈이다. 필드 이름이
    // 비밀을 뜻하지는 않으므로 `looks_like_known_key` 쪽만 걸린다.
    for (field, value) in &strings {
        if looks_like_known_key(value) {
            return Err(reject(field));
        }
        if field_name_means_secret(field) && !is_env_ref(value) {
            return Err(reject(field));
        }
    }

    // ── 산문에는 **접두사 검사만** 건다 ──
    //
    // REQ-021: 사람에게 보여주는 글이라 "이름이 비밀을 뜻하면 값도 비밀" 규칙을
    // 걸면 안 된다. `inputs.TELEGRAM_BOT_TOKEN.help` 는 필드 이름에 token 이
    // 들어가므로, "봇 토큰은 @BotFather 에게 받습니다" 라는 **안내문이** 통째로
    // 거부된다. 실제로 예제를 쓰다 밟았다 — [[DEV-380]] 에서 `task-runner` 가
    // `sk-` 를 품어 거부되던 것과 같은 계열의 오탐이다.
    //
    // 진짜로 막아야 할 것은 "예시랍시고 진짜 토큰을 적는" 경우이고, 그건
    // 접두사 검사가 잡는다.
    let mut prose: Vec<(String, &String)> = Vec::new();
    if let Some(d) = def.description.as_ref() {
        prose.push(("description".into(), d));
    }
    for i in &def.inputs {
        if let Some(h) = &i.help {
            prose.push((format!("inputs.{}.help", i.key), h));
        }
        if let Some(l) = &i.label {
            prose.push((format!("inputs.{}.label", i.key), l));
        }
    }
    for (field, value) in &prose {
        if looks_like_known_key(value) {
            return Err(reject(field));
        }
    }
    // 기본값은 "일단 내 토큰을 default 로" 가 그대로 커밋되는 자리다.
    for i in &def.inputs {
        if let Some(d) = i.default.as_ref().and_then(|v| v.as_str())
            && looks_like_known_key(d)
        {
            return Err(reject(&format!("inputs.{}.default", i.key)));
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
    //
    // DEV-381: 예전엔 길이 검사(`stripped.len() > 8`)가 `any` **안**에 있었다.
    // 그건 문자마다 같은 값이라, 나머지가 8자 이하이면 `any` 가 통째로 false 가
    // 되어 `!false = true` — `Authorization: "Pa55word"` 같은 **짧은 비밀번호가
    // 환경변수 참조로 통과**했다. 길이로 봐주지 않는다.
    let rest = strip_env_refs(v);
    let rest = rest.trim();
    if rest.is_empty() || !rest.chars().any(|c| c.is_ascii_alphanumeric()) {
        return true;
    }
    // 인증 스킴 단어 하나만 남는 것은 정상이다 — `Bearer ${TOKEN}`.
    matches!(
        rest.to_ascii_lowercase().as_str(),
        "bearer" | "basic" | "token"
    )
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
    // DEV-380: `contains` 는 `sk-` 가 영어 단어 꼬리에 흔해서 오탐이 난다 —
    // `task-runner`, `desk-notify`, `risk-eval` 이 전부 "비밀값" 으로 적재
    // 거부됐다(실측). 토큰 시작에서만 본다.
    PREFIXES.iter().any(|p| {
        v.match_indices(p)
            .any(|(i, _)| i == 0 || !v.as_bytes()[i - 1].is_ascii_alphanumeric())
    })
}

/// `${VAR}` 를 실제 값으로. 없는 변수는 **빈 문자열이 아니라 오류** —
/// 조용히 인증 없이 요청을 보내면 원인을 찾기 어렵다.
pub fn expand_env(value: &str) -> AppResult<String> {
    expand_env_with(value, &BTreeMap::new())
}

/// BUG-279: `extra` 를 프로세스 환경변수보다 **먼저** 본다.
///
/// `run` 의 작업 디렉터리가 데이터 폴더로 바뀌면서 코드 폴더를 가리킬 수단이
/// 필요해졌다. 자식에게 `OPENGUILD_PLUGIN_DIR` 를 넘기지만, 그건 **자식의**
/// 환경이라 `args` 안의 `${OPENGUILD_PLUGIN_DIR}` 는 못 푼다 — 확장은 부모가
/// 하기 때문이다. 그대로 두면 "환경변수가 설정되지 않았습니다" 로 죽는다.
///
/// 그래서 같은 이름을 확장 쪽에도 심는다. `sh -c` 를 거치지 않는 명령
/// (`python ${OPENGUILD_PLUGIN_DIR}/hook.py`)도 옆 파일을 부를 수 있다.
pub fn expand_env_with(value: &str, extra: &BTreeMap<String, String>) -> AppResult<String> {
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
            let got = match extra.get(&name) {
                Some(v) => v.clone(),
                None => std::env::var(&name).map_err(|_| {
                    AppError::BadRequest(format!("환경변수 {name} 가 설정되지 않았습니다"))
                })?,
            };
            out.push_str(&got);
            i += rel + 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok(out)
}
