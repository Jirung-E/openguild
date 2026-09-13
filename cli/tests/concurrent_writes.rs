//! BUG-287: **서로 다른 프로세스**가 같은 길드를 동시에 고쳐도 아무것도 안 사라진다.
//!
//! 프로세스 안 뮤텍스(REQ-003)는 이 경계를 못 넘는다 — 그래서 시험도 스레드가 아니라
//! 실제 `openguild` 바이너리를 여러 개 띄운다. 고치기 전에는 12개 중 1~4개만 남으면서
//! 12개 전부 종료 코드 0 이었다.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::io::Write;

const N: usize = 12;

struct Guild {
    root: PathBuf,
    home: PathBuf,
}

impl Guild {
    fn new(name: &str) -> Guild {
        let base = std::env::temp_dir().join(format!(
            "og-bug287-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = base.join("g");
        let home = base.join("home");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let g = Guild { root, home };
        g.ok(&["init", "--name", name], None);
        g
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_openguild"));
        c.current_dir(&self.root)
            .env("OPENGUILD_HOME", &self.home)
            .env_remove("OPENGUILD_REMOTE")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }

    fn ok(&self, args: &[&str], stdin: Option<&str>) -> String {
        let out = self.spawn(args, stdin).wait_with_output().unwrap();
        assert_ok(args, &out);
        String::from_utf8(out.stdout).unwrap()
    }

    /// 띄우고 stdin 을 **바로** 넣고 닫는다. 기다리는 쪽에서 넣으면 뒤 프로세스들이 stdin
    /// 에서 줄을 서서 동시가 아니게 된다 — 이 시험의 첫 판이 그래서 댓글 경합을 못 잡았다.
    fn spawn(&self, args: &[&str], stdin: Option<&str>) -> std::process::Child {
        let mut child = self.cmd(args).spawn().unwrap();
        let mut pipe = child.stdin.take().unwrap();
        if let Some(s) = stdin {
            pipe.write_all(s.as_bytes()).unwrap();
        }
        child
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        serde_json::from_str(&self.ok(&full, None)).unwrap()
    }

    /// N 개를 **먼저 전부 띄우고** 나서 기다린다 — 하나씩 기다리면 동시가 아니다.
    fn all_at_once(&self, make: impl Fn(usize) -> (Vec<String>, Option<String>)) {
        let jobs: Vec<_> = (0..N)
            .map(|i| {
                let (args, stdin) = make(i);
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let child = self.spawn(&refs, stdin.as_deref());
                (args, child)
            })
            .collect();
        for (args, child) in jobs {
            let out = child.wait_with_output().unwrap();
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            assert_ok(&refs, &out);
        }
    }
}

impl Drop for Guild {
    fn drop(&mut self) {
        if let Some(base) = self.root.parent() {
            let _ = std::fs::remove_dir_all(base);
        }
    }
}

fn assert_ok(args: &[&str], out: &Output) {
    assert!(
        out.status.success(),
        "{args:?} 실패\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn strings(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

fn new_quest(g: &Guild) -> String {
    g.json(&["quest", "new", "--type", "BUG", "--title", "target"])["quest_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn concurrent_tag_adds_all_survive() {
    let g = Guild::new("tags");
    let slug = new_quest(&g);

    g.all_at_once(|i| {
        let args = ["quest", "tag", "add", &slug, &format!("t{i:02}")];
        (args.iter().map(|s| s.to_string()).collect(), None)
    });

    let mut tags = strings(&g.json(&["quest", "tag", "list", &slug])["tags"]);
    tags.sort();
    let want: Vec<String> = (0..N).map(|i| format!("t{i:02}")).collect();
    assert_eq!(tags, want, "동시에 붙인 태그가 사라졌다");
}

#[test]
fn concurrent_comments_all_survive_with_distinct_ids() {
    let g = Guild::new("comments");
    let slug = new_quest(&g);
    // 댓글 파일이 이미 있을 때 잃는다 — 첫 댓글은 파일을 새로 만드는 다른 경로다.
    g.ok(&["quest", "comment", "add", &slug], Some("seed"));

    g.all_at_once(|i| {
        let args = ["quest", "comment", "add", slug.as_str()];
        (
            args.iter().map(|s| s.to_string()).collect(),
            Some(format!("body {i}")),
        )
    });

    let list = g.json(&["quest", "comment", "list", &slug]);
    let mut ids: Vec<i64> = list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect();
    ids.sort();
    let want: Vec<i64> = (1..=N as i64 + 1).collect();
    assert_eq!(ids, want, "동시에 쓴 댓글이 사라졌거나 번호가 겹쳤다");
}

#[test]
fn concurrent_quest_creation_gets_distinct_ids() {
    let g = Guild::new("create");
    new_quest(&g);

    g.all_at_once(|i| {
        let args = ["quest", "new", "--type", "BUG", "--title"];
        let mut v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        v.push(format!("q{i}"));
        (v, None)
    });

    let files = quest_files(&g.root);
    assert_eq!(files.len(), N + 1, "동시에 만든 퀘스트 파일 수: {files:?}");
}

#[test]
fn concurrent_mixed_writes_on_one_quest_all_survive() {
    // 서로 다른 필드를 고치는 쓰기라도 같은 파일을 통째로 다시 쓴다 — 태그를 붙이는
    // 동안 상태·긴급도를 바꾸면 태그가 지워지던 모양.
    let g = Guild::new("mixed");
    let slug = new_quest(&g);

    g.all_at_once(|i| {
        let tag = format!("m{i:02}");
        let args: Vec<String> = match i % 3 {
            0 => vec!["quest", "tag", "add", &slug, &tag],
            1 => vec!["quest", "tag", "add", &slug, &tag],
            _ => vec!["quest", "update", &slug, "--urgency", "2"],
        }
        .into_iter()
        .map(String::from)
        .collect();
        (args, None)
    });

    let mut tags = strings(&g.json(&["quest", "tag", "list", &slug])["tags"]);
    tags.sort();
    let want: Vec<String> = (0..N)
        .filter(|i| i % 3 != 2)
        .map(|i| format!("m{i:02}"))
        .collect();
    assert_eq!(tags, want, "다른 필드 수정과 겹친 태그가 사라졌다");
}

fn quest_files(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root.join(".guild/quests"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md"))
        .collect();
    v.sort();
    v
}
