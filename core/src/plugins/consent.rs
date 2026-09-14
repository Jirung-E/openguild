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
    /// 자동 허용 중인 길드 — 새로 오거나 바뀐 플러그인도 묻지 않고 돈다. 혼자 쓰는
    /// 길드에서 매번 묻는 건 성가시다.
    ///
    /// BUG-288: 뜻은 "통째 신뢰" 에서 "자동 허용" 으로 바뀌었지만 **파일 필드 이름은
    /// 그대로 둔다** — 설치된 앱과 개발 빌드가 같은 파일을 읽는다.
    #[serde(default)]
    pub trusted_guilds: Vec<String>,
    /// BUG-288: 길드 경로 → 사용자가 **직접 철회한** 플러그인 이름.
    ///
    /// 철회를 "동의 지우기" 로만 남기면, 자동 허용이 그 자리에서 다시 살려 버린다
    /// (예전 "전체 허용 중 개별 철회 불가" 가 정확히 그 모양이었다).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub denied: BTreeMap<String, std::collections::BTreeSet<String>>,
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
        auto_allow: file.trusted_guilds.contains(&key),
        entries: file.guilds.get(&key).cloned().unwrap_or_default(),
        denied: file.denied.get(&key).cloned().unwrap_or_default(),
    })
}

/// 한 길드에 대한 동의 상태.
#[derive(Debug, Default, Clone)]
pub struct Granted {
    /// 자동 허용 — 동의 기록이 없거나 옛 지문이어도 돈다. 직접 철회한 것은 빼고.
    pub auto_allow: bool,
    pub entries: BTreeMap<String, serde_json::Value>,
    /// 직접 철회한 것. 자동 허용보다 **먼저** 본다.
    pub denied: std::collections::BTreeSet<String>,
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
///
/// 판정 순서(BUG-288): **직접 철회 → 지금 모양에 대한 동의 → 자동 허용.** 예전엔
/// 자동 허용(당시 "신뢰")이 맨 앞이라 개별 철회가 아무 일도 안 했다.
pub fn is_granted(granted: &Granted, plugin: &super::Plugin) -> bool {
    if granted.denied.contains(&plugin.def.name) {
        return false;
    }
    if granted.entries.get(&plugin.def.name) == Some(&fingerprint(&plugin.def, &plugin.folder)) {
        return true;
    }
    granted.auto_allow
}

/// 동의를 남긴다. 호출자(컴포넌트)가 사용자에게 물어본 **뒤에** 부른다 —
/// 코어는 묻지 않는다(CLI 는 비대화형일 수 있고, GUI 는 대화상자가 있다).
pub fn grant(guild_root: &Path, plugin: &super::Plugin) -> AppResult<()> {
    grant_all(guild_root, &[plugin])
}

/// BUG-288: 여럿을 한 번에 허용 — 파일을 한 번만 쓴다. "전체 허용" 버튼이 이것이다.
/// **모드가 아니다** — 지금 있는 것들에 개별 동의를 남길 뿐이라, 그 뒤 개별 철회가 먹는다.
pub fn grant_all(guild_root: &Path, plugins: &[&super::Plugin]) -> AppResult<()> {
    update(|file| {
        let key = guild_key(guild_root);
        let entries = file.guilds.entry(key.clone()).or_default();
        for p in plugins {
            entries.insert(p.def.name.clone(), fingerprint(&p.def, &p.folder));
        }
        if let Some(d) = file.denied.get_mut(&key) {
            for p in plugins {
                d.remove(&p.def.name);
            }
            if d.is_empty() {
                file.denied.remove(&key);
            }
        }
    })
}

/// 동의 철회 — 동의를 지우고 **철회했다는 사실을 남긴다**(자동 허용이 되살리지 않게).
pub fn revoke(guild_root: &Path, name: &str) -> AppResult<()> {
    revoke_all(guild_root, &[name])
}

/// BUG-288: 여럿을 한 번에 철회. "전체 해제" 버튼이 이것이다.
pub fn revoke_all(guild_root: &Path, names: &[&str]) -> AppResult<()> {
    update(|file| {
        let key = guild_key(guild_root);
        if let Some(m) = file.guilds.get_mut(&key) {
            for n in names {
                m.remove(*n);
            }
        }
        let d = file.denied.entry(key).or_default();
        d.extend(names.iter().map(|n| n.to_string()));
    })
}

/// BUG-288: 자동 허용 켜기 — 새로 오거나 바뀐 플러그인도 묻지 않고 돈다.
/// 직접 철회한 것은 계속 안 돈다.
pub fn enable_auto_allow(guild_root: &Path) -> AppResult<()> {
    update(|file| {
        let key = guild_key(guild_root);
        if !file.trusted_guilds.contains(&key) {
            file.trusted_guilds.push(key);
        }
    })
}

/// BUG-288: 자동 허용 끄기 — **지금 돌던 것은 그대로 돈다.** 모드는 앞으로 올 것에
/// 대한 것이라, 끄는 순간 돌던 것이 멈추면 "끄기" 가 "전체 해제" 를 겸하게 된다.
/// 그래서 자동 허용 덕에 돌던 것들의 지금 모양을 개별 동의로 굳힌 뒤 끈다. 그 뒤에
/// 바뀌거나 새로 온 것은 다시 묻는다.
pub fn disable_auto_allow(guild_root: &Path) -> AppResult<()> {
    let loaded = super::load_all(guild_root);
    update(|file| {
        let key = guild_key(guild_root);
        let entries = file.guilds.entry(key.clone()).or_default();
        for p in &loaded.active {
            entries.insert(p.def.name.clone(), fingerprint(&p.def, &p.folder));
        }
        if entries.is_empty() {
            file.guilds.remove(&key);
        }
        file.trusted_guilds.retain(|k| k != &key);
    })
}

/// 읽고 → 고치고 → **통째로 덮어쓴다.** 이 파일에는 이 기계의 모든 동의가
/// 들어 있으므로 쓰다가 죽으면 전부 잃는다. 두 가지를 지킨다.
///
/// 1. **못 읽는 파일은 덮어쓰지 않는다.** 예전에는 파싱 실패를 빈 상태로
///    갈음하고 그대로 덮어써서, 한 번 깨진 파일이 곧 "동의 전부 소멸" 이었다.
///    이제는 오류로 돌려준다 — 사용자가 파일을 보고 판단할 기회를 준다.
///    (읽기 전용인 [`load`] 는 여전히 빈 상태로 떨어진다. 그쪽은 "아무것도
///    안 돈다" 라 안전한 방향이다.)
/// 2. **임시 파일에 쓰고 rename.** 같은 디렉터리 안의 rename 은 원자적이라,
///    중간에 죽어도 예전 파일이 그대로 남는다.
fn update(f: impl FnOnce(&mut ConsentFile)) -> AppResult<()> {
    let p = path()?;
    let mut file = match std::fs::read_to_string(&p) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| {
            crate::error::AppError::BadRequest(format!(
                "{} 를 읽을 수 없습니다: {e}\n                 덮어쓰면 이 기계의 동의가 전부 사라지므로 멈춥니다 —                  파일을 고치거나 지운 뒤 다시 시도하세요.",
                p.display()
            ))
        })?,
        // 아직 없는 것은 정상이다(처음 동의).
        Err(_) => ConsentFile::default(),
    };
    f(&mut file);
    let body = serde_json::to_string_pretty(&file)
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?;
    std::fs::rename(&tmp, &p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        crate::error::AppError::Internal(anyhow::anyhow!(e))
    })?;
    Ok(())
}
