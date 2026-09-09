//! REQ-021: 플러그인이 요구하는 값 — **이 기계에 저장한다.**
//!
//! 정의(`plugin.json`)는 "봇 토큰이 필요하다" 까지만 말한다([`super::Input`]).
//! 실제 토큰은 사용자가 넣고, 그것은 git 으로 공유되면 안 되므로 길드가 아니라
//! `~/.openguild/` 에 남는다 — 동의([`super::consent`])와 정확히 같은 자리,
//! 같은 이유다.
//!
//! # 키가 `{길드}/{플러그인}/{key}` 인 이유
//!
//! 같은 플러그인이라도 길드마다 다른 값을 쓴다. 알림을 보낼 방이 프로젝트마다
//! 다르고, 저장소마다 다른 토큰을 쓰는 것도 흔하다.
//!
//! # 해석 순서 — 저장값 → 기본값 → 프로세스 환경변수
//!
//! 마지막 칸이 중요하다. **진짜 환경변수가 "모든 길드 공통" 자리 노릇을 한다** —
//! 토큰 하나를 여러 길드에서 쓰려면 예전처럼 `export` 하면 되고, 한 길드만
//! 다르게 하려면 화면에서 덮어쓴다. 층을 새로 만들 필요가 없었다.
//!
//! # 값은 동의 지문에 안 들어간다
//!
//! 지문은 *무엇이 도는가* 이고 값은 *누구로서 도는가* 다. 값이 지문에 있으면
//! 토큰을 갱신할 때마다 재동의를 물어야 한다. 반대로 `inputs` **선언**은
//! 정의의 일부라 지문에 들어간다 — 요구하는 값이 바뀌는 것은 사용자가 다시
//! 봐야 할 변화다.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{Input, InputType, PluginDef};

/// `~/.openguild/plugin-values.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ValuesFile {
    /// 길드 경로 → (플러그인 이름 → (key → 값)).
    #[serde(default)]
    pub guilds: BTreeMap<String, BTreeMap<String, BTreeMap<String, serde_json::Value>>>,
}

pub fn path() -> AppResult<PathBuf> {
    Ok(crate::user_dirs::openguild_home()?.join("plugin-values.json"))
}

/// 동의 파일과 **같은 정규화**를 쓴다. 두 파일이 같은 길드를 다른 키로 부르면
/// 동의는 살아 있는데 값은 사라지는 상태가 생긴다.
fn guild_key(guild_root: &Path) -> String {
    crate::recents::normalize_abs(guild_root)
}

/// 이 길드·플러그인에 저장된 값. 파일이 없거나 깨져 있으면 빈 상태다 —
/// 읽기는 안전한 쪽으로 떨어진다(값이 없으면 기본값·환경변수로 간다).
pub fn stored(guild_root: &Path, plugin: &str) -> BTreeMap<String, serde_json::Value> {
    let Ok(p) = path() else {
        return BTreeMap::new();
    };
    let file: ValuesFile = std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    file.guilds
        .get(&guild_key(guild_root))
        .and_then(|g| g.get(plugin))
        .cloned()
        .unwrap_or_default()
}

/// 값 하나를 저장한다. `None` 이면 지운다("기본값으로 되돌리기").
pub fn set(
    guild_root: &Path,
    plugin: &str,
    key: &str,
    value: Option<serde_json::Value>,
) -> AppResult<()> {
    update(|file| {
        let g = file.guilds.entry(guild_key(guild_root)).or_default();
        let p = g.entry(plugin.to_string()).or_default();
        match value {
            Some(v) => {
                p.insert(key.to_string(), v);
            }
            None => {
                p.remove(key);
            }
        }
        // 빈 껍데기를 남기지 않는다 — 파일을 열어 본 사람이 "이 플러그인에
        // 값이 있나" 를 눈으로 판단할 수 있어야 한다.
        if p.is_empty() {
            g.remove(plugin);
        }
        if g.is_empty() {
            file.guilds.remove(&guild_key(guild_root));
        }
    })
}

/// 이 플러그인의 값을 통째로 지운다 — 동의 철회와 함께 부른다.
pub fn clear(guild_root: &Path, plugin: &str) -> AppResult<()> {
    update(|file| {
        if let Some(g) = file.guilds.get_mut(&guild_key(guild_root)) {
            g.remove(plugin);
            if g.is_empty() {
                file.guilds.remove(&guild_key(guild_root));
            }
        }
    })
}

/// **못 읽는 파일은 덮어쓰지 않는다.** [[DEV-385]] 가 동의 파일에서 겪은 그대로
/// 다 — 파싱 실패를 빈 상태로 갈음하고 덮어쓰면 한 번 깨졌을 때 이 기계의 값이
/// 전부 사라지고, 사용자는 왜 플러그인이 갑자기 안 도는지 모른다.
fn update(f: impl FnOnce(&mut ValuesFile)) -> AppResult<()> {
    let p = path()?;
    let mut file = match std::fs::read_to_string(&p) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| {
            AppError::BadRequest(format!(
                "{} 를 읽을 수 없습니다: {e}\n\
                 덮어쓰면 이 기계에 저장한 플러그인 값이 전부 사라지므로 \
                 멈춥니다 — 파일을 고치거나 지운 뒤 다시 시도하세요.",
                p.display()
            ))
        })?,
        Err(_) => ValuesFile::default(),
    };
    f(&mut file);
    let body =
        serde_json::to_string_pretty(&file).map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    // 임시 파일 + rename — 쓰다 죽어도 예전 값이 남는다.
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    restrict(&tmp);
    std::fs::rename(&tmp, &p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::Internal(anyhow::anyhow!(e))
    })?;
    Ok(())
}

/// 소유자만 읽고 쓴다(0600). **평문으로 토큰이 들어 있는 파일이다** — 암호화를
/// 안 하기로 한 이상(admin 확정: 베타에서는 평문 + 0600 + 화면에 명시),
/// 최소한 같은 기계의 다른 사용자에게는 안 보여야 한다.
///
/// rename 전에 건다 — 나중에 걸면 그 사이에 열린 파일은 이미 넓은 권한이다.
#[cfg(unix)]
fn restrict(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
}

/// Windows 는 ACL 이라 모드 비트가 없다. 사용자 프로필 아래라 기본 ACL 이
/// 이미 소유자 중심이다 — 별도 처리는 하지 않는다.
#[cfg(not(unix))]
fn restrict(_p: &Path) {}

/// 값 하나가 어디서 왔나. 화면이 "저장됨 / 환경변수 / 기본값" 을 구분해 보여야
/// 사용자가 **덮어쓸지** 안다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// 이 기계에 저장된 값.
    Stored,
    /// 프로세스 환경변수 — 모든 길드 공통 자리.
    Env,
    /// 정의가 적어 둔 기본값.
    Default,
    /// 아무 데도 없다. 이 플러그인은 못 돈다.
    Missing,
}

/// 해석된 값 하나.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub value: Option<serde_json::Value>,
    pub source: Source,
}

impl Resolved {
    /// `${...}` 치환에 들어갈 모양 — **항상 문자열**이다. 체크박스는
    /// `true`/`false`, 숫자는 십진수. `to_string()` 을 그냥 쓰면 문자열에
    /// 따옴표가 붙어 URL 에 `"abc"` 가 들어간다.
    pub fn as_str(&self) -> Option<String> {
        match self.value.as_ref()? {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Null => None,
            v => Some(v.to_string()),
        }
    }
}

/// 이 플러그인이 쓸 값 전부 — 선언한 입력 + 저장돼 있는 그 밖의 키.
///
/// 선언에 없는데 저장된 키도 함께 돌려준다. 정의가 `${X}` 를 참조하면서
/// `inputs` 에 안 적는 경우가 있고, 그때도 사용자가 넣은 값은 쓰여야 한다.
pub fn resolve(guild_root: &Path, def: &PluginDef) -> BTreeMap<String, Resolved> {
    let stored = stored(guild_root, &def.name);
    let mut out = BTreeMap::new();
    // REQ-021: 선언 안 하고 쓰기만 한 값도 채울 수 있어야 한다.
    for input in &def.effective_inputs() {
        out.insert(
            input.key.clone(),
            resolve_one(input, stored.get(&input.key)),
        );
    }
    for (k, v) in &stored {
        out.entry(k.clone()).or_insert_with(|| Resolved {
            value: Some(v.clone()),
            source: Source::Stored,
        });
    }
    out
}

fn resolve_one(input: &Input, stored: Option<&serde_json::Value>) -> Resolved {
    if let Some(v) = stored {
        return Resolved {
            value: Some(v.clone()),
            source: Source::Stored,
        };
    }
    // 환경변수는 문자열뿐이라 선언한 형에 맞춰 읽는다 — 체크박스에
    // `"true"` 문자열이 들어가면 스크립트의 `if config(k)` 가 안 돈다.
    if let Ok(raw) = std::env::var(&input.key) {
        return Resolved {
            value: Some(coerce(&raw, input.input_type)),
            source: Source::Env,
        };
    }
    match &input.default {
        Some(d) => Resolved {
            value: Some(d.clone()),
            source: Source::Default,
        },
        None => Resolved {
            value: None,
            source: Source::Missing,
        },
    }
}

fn coerce(raw: &str, ty: InputType) -> serde_json::Value {
    match ty {
        InputType::Checkbox => serde_json::Value::Bool(matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )),
        InputType::Number => raw
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(raw.to_string())),
        _ => serde_json::Value::String(raw.to_string()),
    }
}
