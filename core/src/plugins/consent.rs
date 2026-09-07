//! DEV-375: **이 기계에서** 어떤 플러그인을 돌려도 되는지.
//!
//! `.guild/plugins/` 는 git 으로 공유되므로 동료가 만든 정의가 내 기계로
//! 온다. `scope` 에 `cli`/`gui` 가 있으면 내 기계에서 돌고, `run` 이면 임의
//! 실행이다. 그래서 **정의가 오는 것과 도는 것을 갈라 놓는다** — 동의는
//! git 이 아니라 `~/.openguild/` 에 남는다(기계마다 따로).
//!
//! # 해시 대신 정의 원문을 저장한다
//!
//! 정의가 바뀌면 다시 물어야 한다. 지문으로 해시를 쓰는 것이 보통이지만
//! 이 저장소에는 해시 크레이트가 없고, **테스트 하나 때문에 의존성을 들이지
//! 않는다**([[BUG-267]] 에서 같은 판단을 했다). 정의를 정규화한 JSON 그대로
//! 저장하면 비교가 정확하고, 덤으로 **사용자가 무엇에 동의했는지 직접 볼 수
//! 있다**. 플러그인은 길드당 몇 개뿐이라 크기도 문제되지 않는다.
//!
//! # 지문에는 **폴더 전체**가 들어간다 ([[DEV-376]], [[DEV-381]])
//!
//! `plugin.json` 은 그대로 두고 `transform.rhai` 만 고치면 **보내는 내용이
//! 통째로 바뀐다.** 스크립트에 I/O 는 없지만 이미 동의한 목적지로 무엇을
//! 실어 보낼지는 정할 수 있다 — 동의의 대상이 정의뿐이면 그 구멍으로
//! 빠져나간다.
//!
//! 스크립트 하나만 봐도 부족했다. `run` 은 플러그인 폴더를 작업 디렉터리로
//! 삼고 돌기 때문에, 옆에 있는 `hook.py` / `notify.sh` 는 **git 으로 따라오는
//! 실행되는 코드**다. 그걸 갈아끼우면 `plugin.json` 은 그대로여서 다시 묻지
//! 않았다. 그래서 폴더 안 파일 전부를 본다.

use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::PluginDef;

/// `~/.openguild/plugin-consent.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConsentFile {
    /// 길드 경로 → (플러그인 이름 → 동의한 정의 원문).
    #[serde(default)]
    pub guilds: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    /// "이 길드는 다 허용" — 혼자 쓰는 길드에서 매번 묻는 건 성가시다.
    #[serde(default)]
    pub trusted_guilds: Vec<String>,
}

/// 이 기계의 동의 파일 경로.
pub fn path() -> AppResult<PathBuf> {
    let home = crate::user_dirs::openguild_home()
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?;
    Ok(home.join("plugin-consent.json"))
}

/// 길드를 식별하는 키. 경로 표기가 흔들리지 않게 정규화한다
/// (`/var` ↔ `/private/var` — [[BUG-270]] 에서 겪은 문제와 같은 계열).
fn guild_key(guild_root: &Path) -> String {
    crate::recents::normalize_abs(guild_root)
}

/// 이 기계의 동의 상태. 파일이 없거나 깨져 있으면 **빈 상태** — 동의가
/// 없다는 뜻이고, 그러면 아무것도 안 돈다(안전한 쪽).
pub fn load(guild_root: &Path) -> AppResult<Granted> {
    let p = path()?;
    let file: ConsentFile = std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let key = guild_key(guild_root);
    Ok(Granted {
        trusted: file.trusted_guilds.contains(&key),
        entries: file.guilds.get(&key).cloned().unwrap_or_default(),
    })
}

/// 한 길드에 대한 동의 상태.
#[derive(Debug, Default, Clone)]
pub struct Granted {
    pub trusted: bool,
    pub entries: BTreeMap<String, serde_json::Value>,
}

/// 동의 대상을 비교용으로 정규화 — 필드 순서에 흔들리지 않게 한다.
///
/// `folder` 는 `BTreeMap` 이라 순서가 고정된다.
pub fn fingerprint(def: &PluginDef, folder: &BTreeMap<String, String>) -> serde_json::Value {
    serde_json::json!({
        "def": serde_json::to_value(def).unwrap_or(serde_json::Value::Null),
        "folder": folder,
    })
}

/// 이걸 돌려도 되나.
///
/// **정의나 스크립트가 바뀌면 false 다** — 어제 동의한 것이 오늘 다른 URL 로
/// 보내거나 다른 내용을 실어 보내고 있을 수 있다. 그게 이 파일이 원문을 들고
/// 있는 이유다.
pub fn is_granted(granted: &Granted, plugin: &super::Plugin) -> bool {
    if granted.trusted {
        return true;
    }
    granted.entries.get(&plugin.def.name) == Some(&fingerprint(&plugin.def, &plugin.folder))
}

/// 동의를 남긴다. 호출자(컴포넌트)가 사용자에게 물어본 **뒤에** 부른다 —
/// 코어는 묻지 않는다(CLI 는 비대화형일 수 있고, GUI 는 대화상자가 있다).
pub fn grant(guild_root: &Path, plugin: &super::Plugin) -> AppResult<()> {
    update(|file| {
        file.guilds
            .entry(guild_key(guild_root))
            .or_default()
            .insert(
                plugin.def.name.clone(),
                fingerprint(&plugin.def, &plugin.folder),
            );
    })
}

/// 이 길드의 플러그인을 전부 허용.
pub fn trust_guild(guild_root: &Path) -> AppResult<()> {
    update(|file| {
        let key = guild_key(guild_root);
        if !file.trusted_guilds.contains(&key) {
            file.trusted_guilds.push(key);
        }
    })
}

/// DEV-380: 길드 통째 신뢰 해제. **`trust_guild` 에 되돌리기가 없었다** —
/// `is_granted` 가 `trusted` 에서 단락되므로, 신뢰를 켠 뒤에는 개별 철회가
/// 아무 일도 안 하고 그걸 끌 수단도 없었다.
pub fn untrust_guild(guild_root: &Path) -> AppResult<()> {
    update(|file| {
        let key = guild_key(guild_root);
        file.trusted_guilds.retain(|k| k != &key);
    })
}

/// 동의 철회 — 이름을 지운다.
pub fn revoke(guild_root: &Path, name: &str) -> AppResult<()> {
    update(|file| {
        if let Some(m) = file.guilds.get_mut(&guild_key(guild_root)) {
            m.remove(name);
        }
    })
}

fn update(f: impl FnOnce(&mut ConsentFile)) -> AppResult<()> {
    let p = path()?;
    let mut file: ConsentFile = std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    f(&mut file);
    let body = serde_json::to_string_pretty(&file)
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?;
    std::fs::write(&p, body).map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?;
    Ok(())
}
