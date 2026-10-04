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

// DEV-254: server 는 CLI(전역 static)와 달리 요청마다 다른 언어일 수 있음
// (Accept-Language 헤더). tokio task-local 로 요청 스코프에 override 를
// 심어두고, `effective()` 가 이걸 우선 사용 — CLI/기존 코드 경로는 task-local
// 이 없으므로 그대로 `current()` (env > 저장 파일 > 기본 ko) 로 fallback.
tokio::task_local! {
    static REQUEST_LOCALE: Locale;
}

/// axum handler 를 이 locale 로 스코프 — server 미들웨어가 요청 시작 시 호출.
pub async fn scoped<F: std::future::Future>(locale: Locale, f: F) -> F::Output {
    REQUEST_LOCALE.scope(locale, f).await
}

/// 현재 유효 언어 — 요청 스코프 override(server) > `current()`(CLI 등).
/// core::ops 의 사용자-노출 에러 메시지(`AppError::NotFound`/`BadRequest`)는
/// 이걸로 분기해야 서버 응답이 요청자의 Accept-Language 를 따른다.
pub fn effective() -> Locale {
    REQUEST_LOCALE.try_with(|l| *l).unwrap_or_else(|_| current())
}

/// CLI 의 `tf!`(cli/src/main.rs) 와 동일한 최소 침습 이중 언어 헬퍼 — core 의
/// 사용자-노출 에러 메시지용. `effective()` 로 분기(요청 스코프 > 전역).
#[macro_export]
macro_rules! tf {
    ($ko:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::locale::effective() == $crate::locale::Locale::En {
            format!($en $(, $arg)*)
        } else {
            format!($ko $(, $arg)*)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    // BUG-250: 파일 전용 잠금이던 것을 프로세스 전역 하나로 — 다른 파일의
    // env 조작과 겹치던 것을 막는다.
    use crate::test_env::env_lock;

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
        let _guard = env_lock();
        let dir = fresh_dir("home");
        // SAFETY: 테스트는 env_lock() 으로 직렬화되어 동시 env 변경 없음.
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
            // SAFETY: 직렬화됨(env_lock).
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

    fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                rust_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    fn hangul(s: &str) -> bool {
        s.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c))
    }

    /// 시험 코드(`#[cfg(test)]` 가 붙은 항목)를 줄 수는 남긴 채 지운다.
    /// BUG-350: 예전엔 첫 `#[cfg(test)]` 뒤를 통째로 버려서, 파일 위쪽에 `#[cfg(test)] mod tests;` 가 있는
    /// `plugins/mod.rs` 는 아예 안 봤다 — 그 파일의 한국어 오류 50곳을 놓쳤다.
    fn without_tests(src: &str) -> String {
        const MARK: &str = "#[cfg(test)]";
        let mut out = String::with_capacity(src.len());
        let mut rest = src;
        while let Some(at) = rest.find(MARK) {
            out.push_str(&rest[..at]);
            let after = &rest[at..];
            let semi = after.find(';');
            let open = after.find('{');
            let end = match (semi, open) {
                (Some(s), o) if o.is_none_or(|o| s < o) => s + 1,
                (_, Some(o)) => {
                    let mut depth = 0usize;
                    let mut end = after.len();
                    for (i, c) in after[o..].char_indices() {
                        match c {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = o + i + 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    end
                }
                _ => after.len(),
            };
            out.extend(std::iter::repeat_n('\n', after[..end].matches('\n').count()));
            rest = &after[end..];
        }
        out.push_str(rest);
        out
    }

    /// 문자열 리터럴(내용, 시작 위치) — 주석 · 문자 리터럴 · 수명 표시는 건너뛴다.
    fn string_literals(code: &str) -> Vec<(usize, &str)> {
        let b = code.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if code[i..].starts_with("//") {
                i = code[i..].find('\n').map_or(b.len(), |n| i + n);
            } else if code[i..].starts_with("/*") {
                i = code[i..].find("*/").map_or(b.len(), |n| i + n + 2);
            } else if b[i] == b'\'' {
                // 'x' 나 '\'' 는 넘기고, 수명('a)은 한 글자만.
                let tail = &code[i + 1..];
                let mut cs = tail.char_indices();
                i += match (cs.next(), cs.next()) {
                    (Some((_, '\\')), _) => tail[1..].find('\'').map_or(1, |n| n + 3),
                    (Some(_), Some((n, '\''))) => n + 2,
                    _ => 1,
                };
            } else if b[i] == b'r' && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) && {
                let h = code[i + 1..].bytes().take_while(|c| *c == b'#').count();
                code[i + 1 + h..].starts_with('"')
            } {
                let h = code[i + 1..].bytes().take_while(|c| *c == b'#').count();
                let start = i + 2 + h;
                let close = format!("\"{}", "#".repeat(h));
                let end = code[start..].find(&close).map_or(b.len(), |n| start + n);
                out.push((i, &code[start..end]));
                i = end + close.len();
            } else if b[i] == b'"' {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                out.push((i, &code[i + 1..j.min(b.len())]));
                i = j + 1;
            } else {
                i += 1;
            }
        }
        out
    }

    /// BUG-274: 사람에게 가는 오류(`AppError::BadRequest` · `NotFound` · `Cancelled`)에 한국어를 **그대로** 적으면 영어
    /// 화면에 한국어가 낀다 — `tf!` 로 두 말을 함께 적어야 한다. core 소스를 훑어 막는다(시험 코드는 뺀다).
    #[test]
    fn user_facing_errors_are_bilingual() {
        let mut files = Vec::new();
        rust_files(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut bad = Vec::new();
        for f in files {
            let code = without_tests(&std::fs::read_to_string(&f).unwrap());
            for kind in ["AppError::BadRequest(", "AppError::NotFound(", "AppError::Cancelled("] {
                for (i, _) in code.match_indices(kind) {
                    let rest = code[i + kind.len()..].trim_start();
                    let rest = rest.strip_prefix("format!(").map(str::trim_start).unwrap_or(rest);
                    if let Some(lit) = rest.strip_prefix('"') {
                        let lit = lit.split('"').next().unwrap_or("");
                        if hangul(lit) {
                            let line = code[..i].matches('\n').count() + 1;
                            bad.push(format!("{}:{line}: {}", f.display(), lit.chars().take(40).collect::<String>()));
                        }
                    }
                }
            }
        }
        assert!(bad.is_empty(), "한국어만 적힌 오류 — tf!(한국어, 영어) 로 감쌀 것:\n{}", bad.join("\n"));
    }

    /// BUG-350: 영어 화면에서는 플러그인 정의 오류도 영어로 나온다(고치기 전엔 한국어만).
    #[test]
    fn plugin_definition_errors_follow_the_locale() {
        let raw = "name = \"demo\"\nscope = [\"cli\"]\nhandlers = []\n";
        let msg = |l: Locale| {
            REQUEST_LOCALE.sync_scope(l, || crate::plugins::parse_def(raw).unwrap_err().to_string())
        };
        let en = msg(Locale::En);
        assert!(en.contains("no `[[handlers]]`"), "{en}");
        assert!(!hangul(&en), "{en}");
        let ko = msg(Locale::Ko);
        assert!(ko.contains("`[[handlers]]` 가 없습니다"), "{ko}");
    }

    /// BUG-350: 플러그인 쪽 문장은 `AppError` 말고도 문제 칸 · 전달 실패 · 정의 검사 · 시험 도구 출력으로 사람에게 간다.
    /// 그래서 `plugins/` 는 **모든** 한국어 리터럴이 `tf!` 의 첫 인자여야 한다.
    /// 빼는 것: 시험 코드, `expect(...)`(코드가 틀렸을 때만 나는 패닉), `schema.rs`(편집기용 JSON 스키마 설명 — 문서다).
    #[test]
    fn plugin_messages_are_bilingual() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("plugins");
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        let mut bad = Vec::new();
        for f in files {
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "tests.rs" || name == "schema.rs" {
                continue;
            }
            let code = without_tests(&std::fs::read_to_string(&f).unwrap());
            for (at, lit) in string_literals(&code) {
                if !hangul(lit) {
                    continue;
                }
                let before = code[..at].trim_end();
                if before.ends_with("tf!(") || before.ends_with("expect(") {
                    continue;
                }
                let line = code[..at].matches('\n').count() + 1;
                bad.push(format!("{}:{line}: {}", f.display(), lit.chars().take(40).collect::<String>()));
            }
        }
        assert!(bad.is_empty(), "한국어만 적힌 플러그인 문장 — tf!(한국어, 영어) 로 감쌀 것:\n{}", bad.join("\n"));
    }
}
