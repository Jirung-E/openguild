//! Recent guild 목록 — desktop / CLI 가 사용자 친화 진입 시 사용.
//!
//! 저장 위치 (OS 별, `directories` crate 의 `data_local_dir` 기반).
//! BUG-014 (2026-05-25): Windows 에서 Roaming → Local 로 이동. Linux /
//! macOS 는 둘이 같은 경로라 영향 없음.
//! - Linux:   `~/.local/share/openguild/recents.json` 또는 `$XDG_DATA_HOME/openguild/`
//! - macOS:   `~/Library/Application Support/openguild/recents.json`
//! - Windows: `%LOCALAPPDATA%\openguild\openguild\data\recents.json`
//!   (이전 BUG-014: `%APPDATA%\...` = Roaming. 도메인 환경에서 다른 PC 로
//!   sync 되어 부적절. 마이그레이션 없이 새 경로에서 신규 파일로 시작 —
//!   옛 Roaming 의 파일은 그대로 둠.)
//!
//! 형식: JSON array, LRU 순서 (최근 = 0번째).
//! ```json
//! [
//!   { "path": "/abs/path", "name": "guild-name", "last_opened": "2026-05-19T13:47:30Z" },
//!   ...
//! ]
//! ```
//!
//! 자동 호출: `cli::Backend::Local` open / `gui::resolve_guild_path` 성공 시
//! `add(path)`. 사용자는 별도 `clear()` / `list()` 통해 관리.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 최대 보관 개수.
pub const MAX_RECENTS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Recent {
    /// 절대 경로 (canonicalize 후).
    pub path: String,
    /// 길드 이름 (`{name}.guild` 파일의 `name` 필드 또는 디렉토리명).
    pub name: String,
    /// 마지막 open ISO 8601 timestamp.
    pub last_opened: String,
}

/// OS 별 user data dir 기반 recents.json 경로.
/// test 환경 (`OPENGUILD_RECENTS_DIR` env 설정) 면 그 경로 사용.
pub fn recents_path() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("OPENGUILD_RECENTS_DIR") {
        let p = PathBuf::from(dir);
        std::fs::create_dir_all(&p)
            .with_context(|| format!("create test recents dir: {}", p.display()))?;
        return Ok(p.join("recents.json"));
    }
    let dirs = directories::ProjectDirs::from("io", "openguild", "openguild")
        .context("ProjectDirs::from failed — HOME / APPDATA 환경변수 미설정?")?;
    // BUG-014: data_dir() → data_local_dir(). Windows 에서 Roaming →
    // Local. Linux / macOS 는 두 메서드가 같은 경로라 영향 없음.
    let data_dir = dirs.data_local_dir();
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("create recents dir: {}", data_dir.display()))?;
    Ok(data_dir.join("recents.json"))
}

/// 디스크에서 list 읽기. 파일 없으면 빈 vec.
pub fn list() -> Result<Vec<Recent>> {
    let path = recents_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("read recents: {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let recents: Vec<Recent> = serde_json::from_str(&content)
        .with_context(|| format!("parse recents JSON: {}", path.display()))?;
    Ok(recents)
}

/// 디스크에 쓰기 (atomic — tmp → rename).
pub fn write(recents: &[Recent]) -> Result<()> {
    let path = recents_path()?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(recents).context("serialize recents")?;
    std::fs::write(&tmp, json).with_context(|| format!("write tmp: {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

/// 길드 한 개 등록 — LRU 갱신 (이미 있으면 최상단으로, 없으면 추가, MAX 초과 제거).
///
/// `guild_path` 는 canonicalize 시도 (상대 / `..` 정리). 실패 시 그대로 사용.
pub fn add<P: AsRef<Path>>(guild_path: P) -> Result<()> {
    let path = guild_path.as_ref();
    let abs = normalize_abs(path);

    // 길드 이름 — `{name}.guild` 파일 또는 디렉토리명.
    let name = guess_name(path);

    let now = now_iso();
    let mut list = list().unwrap_or_default();
    list.retain(|r| r.path != abs);
    list.insert(0, Recent { path: abs, name, last_opened: now });
    list.truncate(MAX_RECENTS);
    write(&list)?;
    Ok(())
}

/// 전체 비우기.
pub fn clear() -> Result<()> {
    write(&[])
}

/// 한 항목 제거 (path 기준). 없으면 no-op (Ok).
pub fn remove(path: &str) -> Result<()> {
    let mut list = list().unwrap_or_default();
    let before = list.len();
    list.retain(|r| r.path != path);
    if list.len() != before {
        write(&list)?;
    }
    Ok(())
}

/// path 를 절대 + 사용자 친화 형태로 정규화.
///
/// Windows: `canonicalize` 결과의 `\\?\` extended-length prefix 제거 — 그
/// 형태로 화면에 표시 / sqlite URL 에 쓰면 양쪽 모두 깨짐. canonicalize
/// 실패 시 (path 가 없거나 권한 부족 등) 원본 반환.
pub fn normalize_abs(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let raw = abs.to_string_lossy().to_string();
    // Windows extended-length prefix 제거.
    raw.trim_start_matches(r"\\?\")
        .trim_start_matches(r"\\\\?\\")
        .to_string()
}

/// 길드 디렉토리에서 이름 추측 — `*.guild` 파일이 있으면 그 stem, 아니면 디렉토리명.
fn guess_name(dir: &Path) -> String {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("guild")
                && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
            {
                return stem.to_string();
            }
        }
    }
    dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("(unnamed)")
        .to_string()
}

fn now_iso() -> String {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 단순 ISO 8601 (초 단위, UTC).
    // chrono 의존 회피 — 자체 변환.
    let secs_i = secs as i64;
    let days = secs_i / 86400;
    let rem = secs_i % 86400;
    let (y, m, d) = days_to_ymd(days);
    let h = rem / 3600;
    let mi = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Unix epoch 부터의 일수 → (year, month, day). 1970-01-01 = day 0.
fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    days += 719468; // 0000-03-01 기준으로 shift
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_env(label: &str) -> PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-recents-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        // 각 test 가 자기 dir 사용 (unique ns prefix). 단 env::set_var 는
        // process-global 이라 with_env 의 static Mutex 가 직렬화 — BUG-048.
        dir
    }

    /// BUG-048: 모든 recents 테스트가 같은 process-global env
    /// (`OPENGUILD_RECENTS_DIR`) 를 set/remove 하므로 병렬 실행 시 race.
    /// 한 테스트가 unset 한 순간 다른 테스트의 `add()` 가 호출되면 진짜
    /// `%LOCALAPPDATA%\openguild\openguild\data\recents.json` 를 건드려
    /// 사용자 머신을 오염시키고 본 테스트 결과도 비결정.
    /// → process 안의 static Mutex 로 직렬화. 단일 스레드 효과.
    fn env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn with_env<F: FnOnce()>(dir: &Path, f: F) {
        // BUG-048: env 변수 critical section. 다른 테스트가 unset 한 순간
        // 본 add() 가 default ProjectDirs 경로 (= 실제 사용자 recents) 를 쓰지
        // 않도록 보호. 한 테스트가 panic 해도 다른 테스트 안 깨지게 PoisonError
        // 도 흡수 (Mutex 의 데이터는 ()  — 의미 없음).
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: 위 lock 이 단일 스레드 실행 보장.
        unsafe {
            std::env::set_var("OPENGUILD_RECENTS_DIR", dir);
        }
        f();
        unsafe {
            std::env::remove_var("OPENGUILD_RECENTS_DIR");
        }
    }

    #[test]
    fn empty_list_when_no_file() {
        let dir = fresh_env("empty");
        with_env(&dir, || {
            let v = list().unwrap();
            assert!(v.is_empty());
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_creates_file_and_appears_in_list() {
        let dir = fresh_env("add");
        with_env(&dir, || {
            let guild = std::env::temp_dir().join("og-recents-target");
            std::fs::create_dir_all(&guild).unwrap();
            add(&guild).unwrap();
            let v = list().unwrap();
            assert_eq!(v.len(), 1);
            assert!(v[0].path.contains("og-recents-target"));
            let _ = std::fs::remove_dir_all(&guild);
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_existing_path_moves_to_top() {
        let dir = fresh_env("lru");
        with_env(&dir, || {
            let a = std::env::temp_dir().join("og-recents-a");
            let b = std::env::temp_dir().join("og-recents-b");
            std::fs::create_dir_all(&a).unwrap();
            std::fs::create_dir_all(&b).unwrap();
            add(&a).unwrap();
            add(&b).unwrap();
            add(&a).unwrap(); // a 가 최상단으로 이동
            let v = list().unwrap();
            assert_eq!(v.len(), 2);
            assert!(v[0].path.contains("og-recents-a"));
            assert!(v[1].path.contains("og-recents-b"));
            let _ = std::fs::remove_dir_all(&a);
            let _ = std::fs::remove_dir_all(&b);
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_truncates_to_max() {
        let dir = fresh_env("max");
        with_env(&dir, || {
            for i in 0..(MAX_RECENTS + 5) {
                let p = std::env::temp_dir().join(format!("og-recents-many-{i}"));
                std::fs::create_dir_all(&p).unwrap();
                add(&p).unwrap();
                let _ = std::fs::remove_dir_all(&p);
            }
            let v = list().unwrap();
            assert_eq!(v.len(), MAX_RECENTS);
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_empties_list() {
        let dir = fresh_env("clear");
        with_env(&dir, || {
            let p = std::env::temp_dir().join("og-recents-c");
            std::fs::create_dir_all(&p).unwrap();
            add(&p).unwrap();
            assert_eq!(list().unwrap().len(), 1);
            clear().unwrap();
            assert!(list().unwrap().is_empty());
            let _ = std::fs::remove_dir_all(&p);
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn name_from_guild_file_stem() {
        let dir = fresh_env("name-guild");
        with_env(&dir, || {
            let guild = std::env::temp_dir().join("og-recents-named");
            std::fs::create_dir_all(&guild).unwrap();
            std::fs::write(guild.join("my-project.guild"), "name = \"x\"\nversion = \"1.0\"\n")
                .unwrap();
            add(&guild).unwrap();
            let v = list().unwrap();
            assert_eq!(v[0].name, "my-project");
            let _ = std::fs::remove_dir_all(&guild);
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn name_falls_back_to_dir_basename() {
        let dir = fresh_env("name-dir");
        with_env(&dir, || {
            let guild = std::env::temp_dir().join("og-recents-noname");
            std::fs::create_dir_all(&guild).unwrap();
            add(&guild).unwrap();
            let v = list().unwrap();
            assert_eq!(v[0].name, "og-recents-noname");
            let _ = std::fs::remove_dir_all(&guild);
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
