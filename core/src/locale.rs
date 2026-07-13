//! DEV-254: CLI/server 언어 설정 — GUI(DEV-205)와 같은 홈 디렉토리
//! (`~/.openguild/locale.json`)를 진리원으로 공유.
//!
//! GUI 는 브라우저 localStorage 를 쓰지만, CLI 는 세션 상태를 들고 있지
//! 않으므로 명시적 저장이 필요하다. `openguild locale set <ko|en>` 커맨드가
//! 이 파일에 쓰고, 이후 모든 CLI 출력(및 server 응답 기본값)이 여기서 읽는다.
//!
//! 우선순위: `OPENGUILD_LOCALE` 환경변수(테스트/일회성 오버라이드) >
//! `~/.openguild/locale.json` > 기본값 `ko`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    #[default]
    Ko,
    En,
}

impl Locale {
    pub fn as_str(self) -> &'static str {
        match self {
            Locale::Ko => "ko",
            Locale::En => "en",
        }
    }

    pub fn parse(s: &str) -> Option<Locale> {
        match s.to_lowercase().as_str() {
            "ko" | "kr" | "korean" => Some(Locale::Ko),
            "en" | "english" => Some(Locale::En),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocaleFile {
    locale: Locale,
}

/// `~/.openguild/locale.json` 경로. 테스트 환경(`OPENGUILD_HOME` env)은
/// user_dirs::openguild_home() 이 이미 격리해준다.
fn locale_path() -> Result<PathBuf> {
    Ok(crate::user_dirs::openguild_home()?.join("locale.json"))
}

/// 현재 유효 언어. `OPENGUILD_LOCALE` 환경변수가 있으면 그걸 최우선(파일
/// 미변경 — 일회성 오버라이드), 없으면 저장된 파일, 둘 다 없으면 기본 ko.
pub fn current() -> Locale {
    if let Ok(env_val) = std::env::var("OPENGUILD_LOCALE")
        && let Some(l) = Locale::parse(&env_val)
    {
        return l;
    }
    load_saved().unwrap_or_default()
}

/// 파일에 저장된 언어만(env override 무시) — `locale show` 등에서 "저장된
/// 값"을 보여줄 때 사용.
pub fn load_saved() -> Result<Locale> {
    let path = locale_path()?;
    if !path.is_file() {
        return Ok(Locale::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read locale file: {}", path.display()))?;
    let parsed: LocaleFile =
        serde_json::from_str(&raw).with_context(|| format!("parse locale file: {}", path.display()))?;
    Ok(parsed.locale)
}

/// 언어 저장 — `openguild locale set <ko|en>` 이 호출.
pub fn save(locale: Locale) -> Result<()> {
    let path = locale_path()?;
    let body = serde_json::to_string_pretty(&LocaleFile { locale })?;
    std::fs::write(&path, body).with_context(|| format!("write locale file: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn fresh_dir(label: &str) -> PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-locale-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn with_isolated_home<F: FnOnce()>(f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = fresh_dir("home");
        // SAFETY: 테스트는 ENV_LOCK 으로 직렬화되어 동시 env 변경 없음.
        unsafe {
            std::env::set_var("OPENGUILD_HOME", &dir);
            std::env::remove_var("OPENGUILD_LOCALE");
        }
        f();
        unsafe {
            std::env::remove_var("OPENGUILD_HOME");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_is_ko() {
        with_isolated_home(|| {
            assert_eq!(current(), Locale::Ko);
        });
    }

    #[test]
    fn save_and_load_roundtrip() {
        with_isolated_home(|| {
            save(Locale::En).unwrap();
            assert_eq!(load_saved().unwrap(), Locale::En);
            assert_eq!(current(), Locale::En);
        });
    }

    #[test]
    fn env_overrides_saved_file() {
        with_isolated_home(|| {
            save(Locale::En).unwrap();
            // SAFETY: 직렬화됨(ENV_LOCK).
            unsafe {
                std::env::set_var("OPENGUILD_LOCALE", "ko");
            }
            assert_eq!(current(), Locale::Ko);
            assert_eq!(load_saved().unwrap(), Locale::En); // 파일 자체는 안 바뀜.
            unsafe {
                std::env::remove_var("OPENGUILD_LOCALE");
            }
        });
    }

    #[test]
    fn parse_aliases() {
        assert_eq!(Locale::parse("KO"), Some(Locale::Ko));
        assert_eq!(Locale::parse("En"), Some(Locale::En));
        assert_eq!(Locale::parse("fr"), None);
    }
}
