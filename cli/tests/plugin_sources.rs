//! DEV-399: 길드 밖 플러그인 — 실제 `openguild` 바이너리로 끝까지 돌린다.
//!
//! 코어 시험은 적재 규칙을 본다. 여기서는 사람이 치는 명령 순서 그대로
//! `source add → available → add → allow → 이벤트 → remove` 를 밟고, **훅이 실제로
//! 돌았는지**를 파일로 확인한다. 이름으로 찾는 것(폴더 이름과 다른 `name`)을 처음에
//! 실환경에서 밟았기 때문에 일부러 폴더 이름과 정의 이름을 다르게 둔다.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Lab {
    base: PathBuf,
}

impl Lab {
    fn new(label: &str) -> Lab {
        let base = std::env::temp_dir().join(format!(
            "og-dev399-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for d in ["g", "home", "mine/hello-folder"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        // 폴더 이름(hello-folder) ≠ 정의 이름(hello-log).
        std::fs::write(
            base.join("mine/hello-folder/plugin.json"),
            r#"{ "name": "hello-log", "description": "퀘스트가 생기면 한 줄 적는다.",
                 "on": ["quest.created"], "scope": ["cli"],
                 "action": { "run": { "command": "sh",
                   "args": ["-c", "cat >> hello.log; echo >> hello.log"] } } }"#,
        )
        .unwrap();
        let lab = Lab { base };
        lab.ok(&["init", "--name", "srclab"]);
        lab
    }

    fn guild(&self) -> PathBuf {
        self.base.join("g")
    }

    fn home(&self) -> PathBuf {
        self.base.join("home")
    }

    fn source(&self) -> PathBuf {
        self.base.join("mine")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_openguild"))
            .current_dir(self.guild())
            .env("OPENGUILD_HOME", self.home())
            .env_remove("OPENGUILD_REMOTE")
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?} 실패\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        serde_json::from_str(&self.ok(&full)).unwrap()
    }

    fn hook_log(&self) -> Option<String> {
        find_file(&self.home().join("plugin-data"), "hello.log")
            .and_then(|p| std::fs::read_to_string(p).ok())
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(found) = find_file(&p, name) {
                return Some(found);
            }
        } else if p.file_name().is_some_and(|n| n == name) {
            return Some(p);
        }
    }
    None
}

fn names(list: &serde_json::Value, key: &str) -> Vec<String> {
    list[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_plugin_from_a_source_runs_in_place_and_leaves_cleanly() {
    let lab = Lab::new("flow");
    let src = lab.source().display().to_string();

    let added = lab.json(&["plugin", "source", "add", &src]);
    assert_eq!(added["plugins"], serde_json::json!(1));
    let source = added["source"].as_str().unwrap().to_string();

    // 사람이 부르는 이름은 정의의 `name` 이다 — 폴더 이름이 아니라.
    let avail = lab.json(&["plugin", "available"]);
    assert_eq!(names(&avail, "available"), vec!["hello-log"]);
    assert_eq!(avail["available"][0]["used"], serde_json::json!(false));

    // 등록만으로는 안 붙는다.
    assert!(names(&lab.json(&["plugin", "list"]), "needs_consent").is_empty());

    lab.ok(&["plugin", "add", "hello-log"]);
    let listed = lab.json(&["plugin", "list"]);
    assert_eq!(names(&listed, "needs_consent"), vec!["hello-log"]);

    // 허용하면 **실제로 돈다** — 이벤트 하나에 한 줄.
    lab.ok(&["plugin", "allow", "hello-log", "--yes"]);
    let active = lab.json(&["plugin", "list"]);
    assert_eq!(active["active"][0]["source"], serde_json::json!(source));
    lab.ok(&["quest", "new", "--type", "DEV", "--title", "훅 확인"]);
    let log = lab.hook_log().expect("훅이 돌지 않았다");
    assert!(log.contains("quest.created"), "{log}");

    // 안 쓰기로 하면 빠진다 — 원본은 그대로.
    lab.ok(&["plugin", "remove", "hello-log"]);
    let after = lab.json(&["plugin", "list"]);
    assert!(names(&after, "active").is_empty());
    assert!(lab.source().join("hello-folder/plugin.json").is_file(), "원본을 지웠다");
}

#[test]
fn a_folder_path_registers_and_uses_in_one_step() {
    let lab = Lab::new("path");
    let one = lab.source().join("hello-folder").display().to_string();
    lab.ok(&["plugin", "add", &one]);
    let listed = lab.json(&["plugin", "list"]);
    assert_eq!(names(&listed, "needs_consent"), vec!["hello-log"]);
}

/// 폴더 하나짜리 소스가 둘일 때, 하나를 `remove` 해도 다른 하나는 남는다.
#[test]
fn removing_one_single_folder_source_keeps_the_other() {
    let lab = Lab::new("two");
    let other = lab.base.join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(
        other.join("plugin.json"),
        r#"{ "name": "other-log", "on": ["quest.created"], "scope": ["cli"],
             "action": { "run": { "command": "sh", "args": ["-c", "true"] } } }"#,
    )
    .unwrap();
    let one = lab.source().join("hello-folder").display().to_string();
    lab.ok(&["plugin", "add", &one]);
    lab.ok(&["plugin", "add", &other.display().to_string()]);
    let mut both = names(&lab.json(&["plugin", "list"]), "needs_consent");
    both.sort();
    assert_eq!(both, vec!["hello-log", "other-log"]);

    lab.ok(&["plugin", "remove", "hello-log"]);
    assert_eq!(names(&lab.json(&["plugin", "list"]), "needs_consent"), vec!["other-log"]);
}

#[test]
fn a_vanished_source_is_reported_not_dropped() {
    let lab = Lab::new("gone");
    let src = lab.source().display().to_string();
    lab.ok(&["plugin", "source", "add", &src]);
    lab.ok(&["plugin", "add", "hello-log"]);

    let moved = lab.base.join("moved");
    std::fs::rename(lab.source(), &moved).unwrap();
    let list = lab.json(&["plugin", "list"]);
    let errors = list["errors"].as_array().unwrap();
    assert!(
        errors.iter().any(|e| e["error"].as_str().unwrap().contains("mine")),
        "사라진 소스가 조용히 빠졌다: {errors:?}"
    );
    std::fs::rename(&moved, lab.source()).unwrap();
    assert_eq!(names(&lab.json(&["plugin", "list"]), "needs_consent"), vec!["hello-log"]);
}

#[test]
fn a_name_that_exists_in_the_guild_is_refused_from_a_source() {
    let lab = Lab::new("clash");
    let dir = lab.guild().join(".guild/plugins/hello-log");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(lab.source().join("hello-folder/plugin.json"), dir.join("plugin.json")).unwrap();
    let src = lab.source().display().to_string();
    lab.ok(&["plugin", "source", "add", &src]);
    lab.ok(&["plugin", "add", "hello-log@mine"]);

    let list = lab.json(&["plugin", "list"]);
    assert_eq!(names(&list, "needs_consent"), vec!["hello-log"], "둘 다 실렸다");
    assert!(
        list["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["error"].as_str().unwrap().contains("겹칩니다")),
        "{list}"
    );
}
