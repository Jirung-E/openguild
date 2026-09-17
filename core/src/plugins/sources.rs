//! DEV-399: 길드 밖 플러그인 — **소스**(가져다 쓸 폴더)와 **사용 중**(이 길드에서 쓸 것).
//!
//! 지금까지 플러그인은 `.guild/plugins/` 하나였다. 그건 그대로 둔다 — 길드에 딸린 것이고
//! git 으로 동료에게 간다. 그 위에 두 가지를 얹는다:
//!
//! | 층 | 무엇 | 어디 |
//! |---|---|---|
//! | 소스 | 플러그인 여럿을 담은 폴더를 "여기서 가져다 쓴다" | 이 파일(기계) |
//! | 사용 중 | 그 소스의 것 중 **이 길드에서** 쓸 것 | 같은 파일, 길드별 |
//!
//! **복사하지 않는다.** 소스 폴더의 코드를 그 자리에서 적재한다 — 원본을 고치면 바로
//! 반영되고(동의 지문이 바뀌어 다시 묻는다), 여러 길드가 같은 플러그인을 나눠 쓴다.
//!
//! 기록은 git 이 아니라 `~/.openguild/` 에 남는다. 소스 경로는 그 기계의 사정이고, 동료의
//! 기계에는 그 폴더가 없다 — 동의를 기계별로 두는 것과 같은 이유다([`super::consent`]).

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `~/.openguild/plugin-sources.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SourcesFile {
    /// 등록된 소스 — 이름 → 폴더.
    #[serde(default)]
    pub sources: BTreeMap<String, String>,
    /// 길드 경로 → 그 길드에서 쓰는 `이름@소스` 목록.
    #[serde(default)]
    pub used: BTreeMap<String, Vec<Used>>,
}

/// 이 길드에서 쓰기로 한 플러그인 하나.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Used {
    /// 소스 이름.
    pub source: String,
    /// 그 소스 안의 폴더 이름(= 플러그인 폴더).
    pub folder: String,
}

/// 소스 하나와 그 안에서 찾은 플러그인들.
#[derive(Debug, Clone)]
pub struct SourceView {
    pub name: String,
    pub path: PathBuf,
    /// 폴더가 없거나 못 읽으면 그 이유. 조용히 빠지면 "왜 안 돌지" 가 된다.
    pub problem: Option<String>,
    /// 그 소스가 내놓는 플러그인들.
    pub plugins: Vec<SourcePlugin>,
}

/// 소스 안의 플러그인 하나.
#[derive(Debug, Clone)]
pub struct SourcePlugin {
    /// 정의가 적은 **이름** — 동의도 목록도 이것으로 구분하므로 사람이 부를 이름도 이것이다.
    /// 정의를 못 읽으면 폴더 이름으로 대신한다(적재가 이유를 따로 보여 준다).
    pub name: String,
    /// 소스 폴더 안의 폴더 이름 — 경로를 다시 만들 때 쓴다. 소스 **자체가** 플러그인
    /// 폴더면 `"."` 이다(소스 경로에 그대로 이어 붙여도 같은 폴더가 된다).
    pub folder: String,
    pub path: PathBuf,
}

pub fn path() -> AppResult<PathBuf> {
    let home = crate::user_dirs::openguild_home()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    Ok(home.join("plugin-sources.json"))
}

fn guild_key(guild_root: &Path) -> String {
    crate::recents::normalize_abs(guild_root)
}

/// 읽기 — 없거나 깨져 있으면 **빈 상태**. 아무것도 안 붙는 쪽이 안전하다.
pub fn load() -> SourcesFile {
    let Ok(p) = path() else {
        return SourcesFile::default();
    };
    let mut file: SourcesFile = std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    file.normalize();
    file
}

impl SourcesFile {
    /// BUG-294: 예전 빌드가 Windows 에서 `\\?\C:\…` 를 그대로 적어 두었다. 새로 등록할 때만
    /// 떼면 **이미 등록된 소스는 계속 그 형태로** 훅에 넘어간다 — Windows PowerShell 5.1 은 `-File`
    /// 의 `?` 를 와일드카드로 읽어 파일을 못 찾고 0xFFFD0000 으로 끝났다. 읽을 때마다 뗀다.
    fn normalize(&mut self) {
        for p in self.sources.values_mut() {
            *p = crate::recents::strip_verbatim_prefix(p);
        }
    }
}

/// 읽고 → 고치고 → **통째로 덮어쓴다.** 동의 파일과 같은 규칙으로 다룬다
/// ([`super::consent`]): 못 읽는 파일은 덮어쓰지 않고, 임시 파일에 쓰고 rename.
fn update(f: impl FnOnce(&mut SourcesFile) -> AppResult<()>) -> AppResult<()> {
    let p = path()?;
    let mut file: SourcesFile = match std::fs::read_to_string(&p) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| {
            AppError::BadRequest(format!(
                "{} 를 읽을 수 없습니다: {e}\n 덮어쓰면 등록해 둔 소스가 사라지므로 멈춥니다 \
                 — 파일을 고치거나 지운 뒤 다시 시도하세요.",
                p.display()
            ))
        })?,
        Err(_) => SourcesFile::default(),
    };
    // 옛 형태로 적힌 것은 고쳐 쓴다 — 비교(같은 경로면 같은 소스)도 뗀 형태끼리 해야 맞는다.
    file.normalize();
    f(&mut file)?;
    let body = serde_json::to_string_pretty(&file)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    std::fs::rename(&tmp, &p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::Internal(anyhow::anyhow!(e))
    })
}

/// 정의가 적은 이름. 못 읽으면 폴더 이름으로 — 목록에서 사라지는 것보다 낫다
/// (왜 안 읽히는지는 적재가 `errors` 로 따로 말한다).
fn declared_name(dir: &Path) -> String {
    let fallback = || {
        dir.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("plugin")
            .to_string()
    };
    std::fs::read_to_string(dir.join("plugin.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("name")?.as_str().map(str::to_string))
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(fallback)
}

fn entry_at(dir: &Path, folder: String) -> SourcePlugin {
    SourcePlugin {
        name: declared_name(dir),
        folder,
        path: dir.to_path_buf(),
    }
}

/// 소스 자체가 플러그인 폴더일 때의 `folder` 값.
const SELF_FOLDER: &str = ".";

/// 폴더 안에서 플러그인 폴더들을 찾는다 — `plugin.json` 이 있는 하위 폴더.
/// 폴더 자체가 플러그인이면 그것 하나.
pub fn plugins_in(dir: &Path) -> std::io::Result<Vec<SourcePlugin>> {
    if dir.join("plugin.json").is_file() {
        // DEV-399: 예전엔 폴더 이름을 적어 두어, 쓸 때 `소스/폴더이름` 으로 **한 번 더** 이어
        // 붙이는 바람에 "소스에 없다" 로 실패했다(실제 바이너리 시험이 잡았다).
        return Ok(vec![entry_at(dir, SELF_FOLDER.to_string())]);
    }
    let mut found: Vec<SourcePlugin> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("plugin.json").is_file())
        .filter_map(|p| {
            let folder = p.file_name()?.to_str()?.to_string();
            Some(entry_at(&p, folder))
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// 소스들이 내놓는 것 중 이 이름의 플러그인 — `(소스, 폴더)`.
pub fn find_by_name(name: &str) -> Vec<(String, String)> {
    list()
        .into_iter()
        .flat_map(|s| {
            s.plugins
                .into_iter()
                .filter(|p| p.name == name || p.folder == name)
                .map(move |p| (s.name.clone(), p.folder))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// 등록된 소스들 — 경로가 사라졌으면 `problem` 에 이유를 담아 **목록에는 남긴다**.
pub fn list() -> Vec<SourceView> {
    let file = load();
    file.sources
        .into_iter()
        .map(|(name, raw)| {
            let path = PathBuf::from(&raw);
            let (problem, plugins) = match plugins_in(&path) {
                Ok(v) if v.is_empty() => (
                    Some(crate::tf!(
                        "플러그인을 찾지 못했습니다(plugin.json 이 있는 폴더가 없음)",
                        "no plugins found (no folder with plugin.json)"
                    )),
                    v,
                ),
                Ok(v) => (None, v),
                Err(e) => (
                    Some(crate::tf!(
                        "폴더를 읽지 못했습니다: {e}",
                        "cannot read the folder: {e}"
                    )),
                    Vec::new(),
                ),
            };
            SourceView {
                name,
                path,
                problem,
                plugins,
            }
        })
        .collect()
}

/// 소스 등록. 이름을 안 주면 폴더 이름을 쓴다(겹치면 `-2`, `-3`).
pub fn add_source(guild_root: &Path, dir: &Path, name: Option<&str>) -> AppResult<String> {
    let canon = std::fs::canonicalize(dir).map_err(|e| {
        AppError::BadRequest(crate::tf!(
            "폴더를 찾을 수 없습니다: {} ({e})",
            "no such folder: {} ({e})",
            dir.display()
        ))
    })?;
    // 길드 안은 이미 훑는다 — 같은 플러그인이 두 경로로 들어오면 이름이 겹쳐 거부된다.
    let guild_plugins = super::plugins_dir(guild_root);
    if let Ok(gp) = std::fs::canonicalize(&guild_plugins)
        && (canon == gp || canon.starts_with(&gp))
    {
        return Err(AppError::BadRequest(crate::tf!(
            "이 길드의 플러그인 폴더는 이미 읽고 있습니다 — 소스로 더할 필요가 없습니다: {}",
            "this guild's plugin folder is already loaded — no need to add it as a source: {}",
            dir.display()
        )));
    }
    // BUG-294: Windows 의 canonicalize 는 `\\?\C:\…` 를 돌려준다. 그대로 적어 두면 그 경로가
    // `${OPENGUILD_PLUGIN_DIR}` 로 훅에 넘어가는데, Windows PowerShell 5.1 등은 이 형태를 제대로
    // 못 다룬다. 길드 경로와 같은 규칙([`crate::recents::normalize_abs`])으로 떼어 낸다.
    let dir = PathBuf::from(crate::recents::strip_verbatim_prefix(&canon.to_string_lossy()));
    let found = plugins_in(&dir).map_err(|e| {
        AppError::BadRequest(crate::tf!(
            "폴더를 읽지 못했습니다: {e}",
            "cannot read the folder: {e}"
        ))
    })?;
    if found.is_empty() {
        return Err(AppError::BadRequest(crate::tf!(
            "플러그인이 없습니다 — plugin.json 이 있는 폴더가 하나도 없습니다: {}",
            "no plugins there — not a single folder with plugin.json: {}",
            dir.display()
        )));
    }

    let base = name
        .map(str::to_string)
        .or_else(|| dir.file_name()?.to_str().map(str::to_string))
        .unwrap_or_else(|| "source".to_string());
    let mut file = load();
    // 같은 경로가 이미 있으면 그 이름을 그대로 돌려준다 — 두 번 등록해도 하나다.
    if let Some((existing, _)) = file
        .sources
        .iter()
        .find(|(_, p)| PathBuf::from(p) == dir)
    {
        return Ok(existing.clone());
    }
    let mut chosen = base.clone();
    let mut n = 2;
    while file.sources.contains_key(&chosen) {
        chosen = format!("{base}-{n}");
        n += 1;
    }
    let dir_str = dir.display().to_string();
    let saved = chosen.clone();
    update(move |f| {
        f.sources.insert(chosen, dir_str);
        Ok(())
    })?;
    file = load();
    let _ = file;
    Ok(saved)
}

/// 소스 등록 해제 — **파일은 안 지운다.** 그 소스에서 쓰던 것들도 함께 목록에서 빠진다.
pub fn remove_source(name: &str) -> AppResult<()> {
    let name = name.to_string();
    update(move |f| {
        if f.sources.remove(&name).is_none() {
            return Err(AppError::NotFound(crate::tf!(
                "그런 소스가 없습니다: {name}",
                "no such source: {name}"
            )));
        }
        for used in f.used.values_mut() {
            used.retain(|u| u.source != name);
        }
        f.used.retain(|_, v| !v.is_empty());
        Ok(())
    })
}

/// 이 길드에서 쓰는 것들.
pub fn used_in(guild_root: &Path) -> Vec<Used> {
    load()
        .used
        .get(&guild_key(guild_root))
        .cloned()
        .unwrap_or_default()
}

/// 이 길드에서 쓴다 — `folder` 는 소스 폴더 안의 폴더 이름. 이미 쓰고 있으면 그대로(멱등).
pub fn use_plugin(guild_root: &Path, source: &str, folder: &str) -> AppResult<()> {
    let key = guild_key(guild_root);
    let (source, folder) = (source.to_string(), folder.to_string());
    update(move |f| {
        let Some(dir) = f.sources.get(&source) else {
            return Err(AppError::NotFound(crate::tf!(
                "그런 소스가 없습니다: {source}",
                "no such source: {source}"
            )));
        };
        let dir = PathBuf::from(dir).join(&folder);
        if !dir.join("plugin.json").is_file() {
            return Err(AppError::NotFound(crate::tf!(
                "그 소스에 없는 플러그인입니다: {folder}",
                "not in that source: {folder}"
            )));
        }
        let entry = Used {
            source: source.clone(),
            folder: folder.clone(),
        };
        let list = f.used.entry(key).or_default();
        if !list.contains(&entry) {
            list.push(entry);
        }
        Ok(())
    })
}

/// 이 길드에서 안 쓴다 — **파일은 안 지운다.** 플러그인 이름이나 폴더 이름 어느 쪽이든 받는다.
pub fn stop_using(guild_root: &Path, name: &str) -> AppResult<()> {
    let key = guild_key(guild_root);
    // 사람이 부르는 이름은 정의의 `name` 이고, 기록은 `(소스, 폴더)` 다. **짝으로** 비교한다
    // — 폴더만 보면 폴더 하나짜리 소스들(`"."`)이 한꺼번에 빠진다.
    let pairs: Vec<(String, String)> = find_by_name(name);
    let folder = name.to_string();
    update(move |f| {
        let Some(list) = f.used.get_mut(&key) else {
            return Err(AppError::NotFound(crate::tf!(
                "이 길드에서 쓰고 있지 않습니다: {folder}",
                "not in use in this guild: {folder}"
            )));
        };
        let before = list.len();
        list.retain(|u| {
            u.folder != folder && !pairs.iter().any(|(s, f)| s == &u.source && f == &u.folder)
        });
        if list.len() == before {
            return Err(AppError::NotFound(crate::tf!(
                "이 길드에서 쓰고 있지 않습니다: {folder}",
                "not in use in this guild: {folder}"
            )));
        }
        f.used.retain(|_, v| !v.is_empty());
        Ok(())
    })
}

/// `(소스 이름, 플러그인 폴더)`.
pub type UsedDir = (String, PathBuf);
/// `(플러그인 이름, 이유)` — 적재의 `errors` 와 같은 모양.
pub type UsedProblem = (String, String);

/// 이 길드에서 쓰기로 한 플러그인 폴더들 — 적재가 훑을 경로. 경로가 사라졌으면
/// `(이름, 이유)` 로 돌려 목록에 남긴다(조용히 빠지지 않게).
pub fn used_dirs(guild_root: &Path) -> (Vec<UsedDir>, Vec<UsedProblem>) {
    let file = load();
    let mut dirs = Vec::new();
    let mut problems = Vec::new();
    for u in file
        .used
        .get(&guild_key(guild_root))
        .cloned()
        .unwrap_or_default()
    {
        match file.sources.get(&u.source) {
            None => problems.push((
                u.folder.clone(),
                crate::tf!(
                    "소스가 없습니다: {} — `plugin source list` 로 확인하세요",
                    "source is gone: {} — check `plugin source list`",
                    u.source
                ),
            )),
            Some(dir) => {
                let p = PathBuf::from(dir).join(&u.folder);
                if p.join("plugin.json").is_file() {
                    dirs.push((u.source.clone(), p));
                } else {
                    problems.push((
                        u.folder.clone(),
                        crate::tf!(
                            "경로에 없습니다(소스 {}): {}",
                            "missing from source {}: {}",
                            u.source,
                            p.display()
                        ),
                    ));
                }
            }
        }
    }
    (dirs, problems)
}

// ─────────────── DEV-400: 화면이 받는 모양 ───────────────
//
// CLI 는 위 함수들을 직접 엮어 쓰지만, 데스크톱 화면은 "폴더 하나 골랐다" 한 번으로 끝나야
// 한다. 그 판단(하나면 바로 쓰고, 여럿이면 고르게 한다)을 CLI 와 화면이 따로 갖지 않게
// 여기 둔다.

/// 화면에 보여 줄 소스 하나.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceStatus {
    pub name: String,
    pub path: String,
    /// 폴더가 사라졌거나 플러그인이 없으면 그 이유 — 목록에서 빼지 않고 보인다.
    pub problem: Option<String>,
    pub plugins: Vec<AvailablePlugin>,
}

/// 소스가 내놓는 플러그인 하나와, 이 길드에서 쓰고 있는지.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailablePlugin {
    pub name: String,
    pub folder: String,
    pub dir: String,
    pub used: bool,
}

/// 등록된 소스 전부와 그 플러그인들 — 이 길드에서 쓰는 것에 표시.
pub fn status(guild_root: &Path) -> Vec<SourceStatus> {
    let used = used_in(guild_root);
    list()
        .into_iter()
        .map(|s| SourceStatus {
            plugins: s
                .plugins
                .into_iter()
                .map(|p| AvailablePlugin {
                    used: used
                        .iter()
                        .any(|u| u.source == s.name && u.folder == p.folder),
                    name: p.name,
                    folder: p.folder,
                    dir: p.path.display().to_string(),
                })
                .collect(),
            name: s.name,
            path: s.path.display().to_string(),
            problem: s.problem,
        })
        .collect()
}

/// 폴더 하나를 더한 결과.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AddOutcome {
    /// 플러그인이 하나라 바로 이 길드에서 쓰기로 했다. `name` 은 정의의 이름.
    Used {
        source: String,
        folder: String,
        name: String,
    },
    /// 여럿이라 소스로만 등록했다 — 무엇을 쓸지는 사람이 고른다(통째로 켜지 않는다).
    Registered { source: String, plugins: usize },
}

/// 폴더를 더한다 — 소스로 등록하고, 플러그인이 하나면 이 길드에서 쓴다.
///
/// 이미 등록된 폴더면 그 소스를 그대로 쓴다([`add_source`] 가 경로로 중복을 거른다).
pub fn add_folder(guild_root: &Path, dir: &Path) -> AppResult<AddOutcome> {
    let source = add_source(guild_root, dir, None)?;
    let found = list()
        .into_iter()
        .find(|s| s.name == source)
        .map(|s| s.plugins)
        .unwrap_or_default();
    match found.as_slice() {
        [only] => {
            use_plugin(guild_root, &source, &only.folder)?;
            Ok(AddOutcome::Used {
                source,
                folder: only.folder.clone(),
                name: only.name.clone(),
            })
        }
        many => Ok(AddOutcome::Registered {
            source,
            plugins: many.len(),
        }),
    }
}

/// 이 길드에서 안 쓴다 — `(소스, 폴더)` 를 정확히 짚는다. 화면의 소스 목록은 이 짝을 알고
/// 있으므로 이름으로 다시 찾지 않는다(이름은 소스마다 겹칠 수 있다).
pub fn stop_using_entry(guild_root: &Path, source: &str, folder: &str) -> AppResult<()> {
    let key = guild_key(guild_root);
    let target = Used {
        source: source.to_string(),
        folder: folder.to_string(),
    };
    update(move |f| {
        let list = f.used.entry(key).or_default();
        let before = list.len();
        list.retain(|u| u != &target);
        if list.len() == before {
            return Err(AppError::NotFound(crate::tf!(
                "이 길드에서 쓰고 있지 않습니다: {}@{}",
                "not in use in this guild: {}@{}",
                target.folder,
                target.source
            )));
        }
        f.used.retain(|_, v| !v.is_empty());
        Ok(())
    })
}
