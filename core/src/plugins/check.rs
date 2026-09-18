//! DEV-411: `openguild plugin check` — **돌리기 전에** 정의를 검사한다.
//!
//! # 왜 따로 있나
//!
//! 적재는 깨진 플러그인을 조용히 끄고 넘어간다(다른 플러그인과 길드 동작을 세우지 않으려고).
//! 그래서 만드는 사람은 "왜 안 도는지" 를 목록 화면의 한 줄로만 본다. 여기서는 반대로,
//! **한 폴더만 붙잡고** 적재가 하는 검사를 그대로 돌린 뒤 무엇이 걸렸는지 다 말한다.
//!
//! 에이전트가 플러그인을 써 줄 때도 이걸로 확인한다 — 길드를 건드리지 않고, 파일만 읽는다.
//!
//! # 오류와 경고
//!
//! **오류**는 적재가 거절하는 것이다(형식, 없는 함수, 못 쓰는 `with`, 못 찾는 파일 …).
//! **경고**는 돌기는 하지만 거의 실수인 것이다 — 아무 줄도 안 부르는 함수, 선언만 하고 안 쓰는
//! 권한, 반대로 스크립트가 쓰는데 선언하지 않은 권한(그건 돌 때 실패한다).

use super::{MANIFEST, OLD_MANIFEST, PluginDef, script};
use std::path::{Path, PathBuf};

/// 폴더 하나의 검사 결과.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Report {
    /// 정의를 읽었으면 그 이름, 못 읽었으면 폴더 이름.
    pub name: String,
    pub dir: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// 사람이 읽을 "하는 일" 줄들 — 검사에 통과했을 때만 찬다.
    pub lines: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// 폴더 하나를 검사한다. `guild_root` 는 이름·링크를 읽을 길드(없으면 `None`).
pub fn check_dir(dir: &Path, guild_root: Option<&Path>) -> Report {
    let mut r = Report {
        name: dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string(),
        dir: dir.display().to_string(),
        ..Default::default()
    };
    let manifest = dir.join(MANIFEST);
    if !manifest.is_file() {
        r.errors.push(if dir.join(OLD_MANIFEST).is_file() {
            crate::tf!(
                "plugin.json 은 더 이상 읽지 않습니다 — plugin.toml 의 [[handlers]] 형식으로 옮기세요",
                "plugin.json is no longer read — move it to plugin.toml with [[handlers]]"
            )
        } else {
            crate::tf!(
                "{} 가 없습니다 — 플러그인 폴더가 아닙니다",
                "no {} here — this is not a plugin folder",
                MANIFEST
            )
        });
        return r;
    }
    let def = match super::read_def(&manifest) {
        Ok(d) => d,
        Err(e) => {
            r.errors.push(e.to_string());
            return r;
        }
    };
    r.name = def.name.clone();

    // 적재와 **같은 길**로 컴파일한다 — 여기서 통과하고 적재에서 걸리면 이 명령이 쓸모없다.
    let compiled = match super::compile_script(dir, &def) {
        Ok((c, _, imports)) => {
            if !imports.is_empty() {
                r.lines.push(crate::tf!(
                    "불러오는 파일: {}",
                    "imports: {}",
                    imports.join(", ")
                ));
            }
            c
        }
        Err(e) => {
            r.errors.push(e.to_string());
            None
        }
    };

    let src = script_text(dir, &def);
    warn_unused_functions(&def, compiled.as_deref(), &src, &mut r);
    warn_permissions(&def, &src, &mut r);
    warn_stray_scripts(dir, &def, &mut r);

    if r.ok() {
        // 검사에 통과했으면 **무엇을 하게 되는지**까지 보여 준다. 허용 화면과 같은 말이다.
        let plugin = super::Plugin {
            def,
            imports: Vec::new(),
            dir: dir.to_path_buf(),
            guild_root: guild_root.unwrap_or(dir).to_path_buf(),
            compiled,
            script_src: None,
            folder: Default::default(),
            source: None,
        };
        for (i, line) in super::summary::handler_lines(&plugin, super::Scope::Cli)
            .iter()
            .enumerate()
        {
            r.lines.push(format!("{}. {line}", i + 1));
        }
    }
    r
}

/// 길드가 읽는 폴더 전부(길드 것 + 이 길드에서 쓰는 소스 것).
pub fn check_guild(guild_root: &Path) -> Vec<Report> {
    let mut dirs: Vec<PathBuf> = match std::fs::read_dir(super::plugins_dir(guild_root)) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join(MANIFEST).is_file() || p.join(OLD_MANIFEST).is_file())
            .collect(),
        Err(_) => Vec::new(),
    };
    dirs.sort();
    let (used, _) = super::sources::used_dirs(guild_root);
    dirs.extend(used.into_iter().map(|(_, p)| p));
    dirs.iter()
        .map(|d| check_dir(d, Some(guild_root)))
        .collect()
}

/// 스크립트 원문 전부를 한 덩이로. 아래 두 경고가 원문을 훑는 **어림짐작**이라 같이 쓴다.
fn script_text(dir: &Path, def: &PluginDef) -> String {
    let mut src = String::new();
    for rel in &def.scripts {
        if let Ok(s) = std::fs::read_to_string(dir.join(rel)) {
            src.push_str(&s);
            src.push('\n');
        }
    }
    src
}

/// 아무도 안 부르는 함수 — 이름을 잘못 적었거나 줄을 안 만든 것이다.
///
/// 줄이 안 부르는 것만으로는 부족하다. 다른 함수가 쓰는 **도우미**가 흔하기 때문에, 원문에
/// 부르는 자리가 있으면 조용히 넘어간다.
fn warn_unused_functions(
    def: &PluginDef,
    compiled: Option<&script::Script>,
    src: &str,
    r: &mut Report,
) {
    let Some(sc) = compiled else { return };
    for (name, arity) in sc.function_names() {
        // `with` 의 수가 다르면 다른 함수다 — 줄이 부르는 것과 정확히 맞아야 한다.
        let called = def
            .handlers
            .iter()
            .any(|h| h.call.as_deref() == Some(name.as_str()) && 1 + h.with.len() == arity);
        let used_in_script = call_sites(src, &name) > src.matches(&format!("fn {name}(")).count();
        if !called && !used_in_script {
            r.warnings.push(crate::tf!(
                "`{}` 를 부르는 줄이 없습니다 (인자 {}개) — `call` 을 확인하세요",
                "no line calls `{}` ({} argument(s)) — check `call`",
                name,
                arity
            ));
        }
    }
}

/// 권한 선언과 스크립트가 어긋나는 경우. 이것도 원문을 훑는 어림짐작이라, 없는 쪽만 말하고
/// 단정하지 않는다.
fn warn_permissions(def: &PluginDef, src: &str, r: &mut Report) {
    if src.is_empty() {
        return;
    }
    for perm in super::PERMISSIONS {
        let used = call_sites(src, perm) > 0;
        let declared = def.permissions.iter().any(|p| p == perm);
        if used && !declared {
            r.warnings.push(crate::tf!(
                "스크립트가 `{}()` 를 부르는데 `permissions` 에 없습니다 — 돌 때 그 줄이 실패합니다",
                "the script calls `{}()` but it is not in `permissions` — that line will fail at run time",
                perm
            ));
        }
        if declared && !used {
            r.warnings.push(crate::tf!(
                "`permissions` 의 `{}` 를 스크립트가 안 씁니다 — 안 쓰면 빼세요(허용 화면이 더 세 보입니다)",
                "`{}` is declared in `permissions` but the script never uses it — drop it (it makes the consent screen look scarier)",
                perm
            ));
        }
    }
}

/// 원문에서 `이름(` 을 세되, **앞뒤가 이름의 일부면 세지 않는다** — `copy_backup(` 이
/// `backup(` 으로 읽히면 "권한을 안 밝혔다" 는 헛경고가 난다.
fn call_sites(src: &str, name: &str) -> usize {
    let pat = format!("{name}(");
    let bytes = src.as_bytes();
    src.match_indices(&pat)
        .filter(|(i, _)| {
            *i == 0 || {
                let c = bytes[i - 1] as char;
                !(c.is_alphanumeric() || c == '_')
            }
        })
        .count()
}

/// 폴더에 `.rhai` 가 있는데 `scripts` 에 없다 — 적으면 안 도는 파일을 만들고 있는 것이다.
fn warn_stray_scripts(dir: &Path, def: &PluginDef, r: &mut Report) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut stray: Vec<String> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("rhai"))
        .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(str::to_string))
        .filter(|f| !def.scripts.iter().any(|s| s == f))
        // DEV-412: 시험 파일은 `scripts` 에 **안 적는 것이 맞다** — 적으면 진짜로 도는 코드가 된다.
        .filter(|f| !f.ends_with(super::testing::TEST_SUFFIX))
        .collect();
    stray.sort();
    for f in stray {
        r.warnings.push(crate::tf!(
            "{} 가 `scripts` 에 없습니다 — 불러오는 파일(`import`)이 아니면 안 돕니다",
            "{} is not in `scripts` — unless it is imported, it never runs",
            f
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(label: &str, manifest: &str, files: &[(&str, &str)]) -> PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-check-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        if !manifest.is_empty() {
            std::fs::write(dir.join(MANIFEST), manifest).unwrap();
        }
        for (name, body) in files {
            std::fs::write(dir.join(name), body).unwrap();
        }
        dir
    }

    const OK: &str = "name = \"p\"\nscope = [\"cli\"]\nscripts = [\"main.rhai\"]\npermissions = [\"notify\"]\n\n[[handlers]]\npost = [\"quest.created\"]\ncall = \"h\"\n";

    #[test]
    fn a_good_folder_passes_and_says_what_it_would_do() {
        let dir = folder("ok", OK, &[("main.rhai", "fn h(e) { notify(\"안녕\") }")]);
        let r = check_dir(&dir, None);
        assert!(r.ok(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert!(r.lines.iter().any(|l| l.contains("quest.created")), "{:?}", r.lines);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 적재가 거절하는 것은 여기서도 오류다 — 둘이 다르면 이 명령을 믿을 수 없다.
    #[test]
    fn a_missing_function_is_an_error_here_too() {
        let dir = folder("missing", OK, &[("main.rhai", "fn other(e) { }")]);
        let r = check_dir(&dir, None);
        assert!(!r.ok());
        assert!(r.errors[0].contains("h"), "{:?}", r.errors);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 이름의 일부가 겹치는 것을 권한 호출로 읽으면 안 된다 — `copy_backup()` 은 `backup()` 이
    /// 아니다.
    #[test]
    fn a_longer_name_is_not_a_permission_call() {
        let m = "name = \"p\"\nscope = [\"cli\"]\nscripts = [\"main.rhai\"]\n\n[[handlers]]\npost = [\"backup.created\"]\ncall = \"copy_backup\"\n";
        let dir = folder("wordy", m, &[("main.rhai", "fn copy_backup(e) { }")]);
        let r = check_dir(&dir, None);
        assert!(r.ok(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "헛경고가 났다: {:?}", r.warnings);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_without_a_manifest_says_so() {
        let dir = folder("bare", "", &[]);
        let r = check_dir(&dir, None);
        assert!(!r.ok());
        assert!(r.errors[0].contains(MANIFEST), "{:?}", r.errors);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 돌기는 하지만 거의 실수인 것들.
    #[test]
    fn warnings_point_at_the_usual_mistakes() {
        let m = "name = \"p\"\nscope = [\"cli\"]\nscripts = [\"main.rhai\"]\npermissions = [\"backup\"]\n\n[[handlers]]\npost = [\"quest.created\"]\ncall = \"h\"\n";
        let dir = folder(
            "warn",
            m,
            &[
                ("main.rhai", "fn h(e) { notify(\"안녕\") }\nfn never_called(e) { }"),
                ("옆.rhai", "fn x() { }"),
            ],
        );
        let r = check_dir(&dir, None);
        assert!(r.ok(), "{:?}", r.errors);
        let w = r.warnings.join("\n");
        assert!(w.contains("never_called"), "안 불리는 함수를 안 짚었다:\n{w}");
        assert!(w.contains("notify"), "선언 안 한 권한을 안 짚었다:\n{w}");
        assert!(w.contains("backup"), "안 쓰는 권한을 안 짚었다:\n{w}");
        assert!(w.contains("옆.rhai"), "빠진 파일을 안 짚었다:\n{w}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
