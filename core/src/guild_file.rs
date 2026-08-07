use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// DEV-064: 현재 실행파일이 지원하는 길드 파일 구조(schema) 버전.
/// 길드 파일 구조(frontmatter 필드, toml 형식 등)가 바뀌면 +1 하고 migration
/// 함수를 추가한다. 1 = 최초 baseline.
pub const CURRENT_SCHEMA_VERSION: i64 = 1;

fn default_schema_version() -> i64 {
    1
}

#[derive(Debug, Deserialize)]
pub struct GuildFile {
    pub name: String,
    pub version: String,
    pub created_at: String,
    /// DEV-064: 길드 파일 구조 버전. 필드 없는 구 길드는 1 로 간주.
    #[serde(default = "default_schema_version")]
    pub schema_version: i64,
}

/// 길드 마커(`{name}.guild`) 파일 내용 — 항상 현재 schema_version 으로 기록.
/// CLI(init) / GUI(create) 양쪽이 공유해 포맷 drift 방지.
pub fn marker_content(name: &str, created_at: &str) -> String {
    let esc = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "name = \"{esc}\"\nversion = \"1.0\"\ncreated_at = \"{created_at}\"\nschema_version = {CURRENT_SCHEMA_VERSION}\n"
    )
}

/// DEV-064: 길드 schema 버전 vs 실행파일 지원 버전 비교 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaCompat {
    /// 동일 — 정상.
    Current,
    /// 길드가 더 옛 버전 — migration 필요 (값 = 길드 버전).
    Older(i64),
    /// 길드가 더 새 버전 — 이 실행파일로는 못 엶. 앱 업데이트 필요 (값 = 길드 버전).
    Newer(i64),
}

/// schema_version 을 현재 지원 버전과 비교.
pub fn schema_compat(schema_version: i64) -> SchemaCompat {
    use std::cmp::Ordering;
    match schema_version.cmp(&CURRENT_SCHEMA_VERSION) {
        Ordering::Equal => SchemaCompat::Current,
        Ordering::Less => SchemaCompat::Older(schema_version),
        Ordering::Greater => SchemaCompat::Newer(schema_version),
    }
}

/// guild 디렉터리에서 `{name}.guild` 파일을 찾아 파싱한다.
pub fn load(guild_path: &str) -> Result<GuildFile> {
    let dir = Path::new(guild_path);

    let guild_file = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory: {guild_path}"))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|e| e.to_str()) == Some("guild"))
        .with_context(|| format!("no .guild file found in: {guild_path}"))?;

    let content = std::fs::read_to_string(&guild_file)
        .with_context(|| format!("failed to read: {}", guild_file.display()))?;

    toml::from_str(&content)
        .with_context(|| format!("failed to parse: {}", guild_file.display()))
}

/// `start` 에서 시작해 부모 방향으로 거슬러 올라가며 `.guild` 가 있는 첫 디렉토리를 반환.
/// git 의 `.git` 탐색과 동일 패턴. 못 찾으면 None.
pub fn find_from(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    loop {
        if has_guild_file(&current) {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// cwd 부터 부모 방향 탐색. 못 찾으면 None.
pub fn find_from_cwd() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    find_from(&cwd)
}

/// DEV-319: `--guild` 값을 경로 또는 길드 **이름**으로 해석.
///
/// 1. 그대로 경로로 봤을 때 `find_from(&pb) == Some(pb)` (그 디렉토리 자체에
///    `.guild` 마커) 면 그 경로 사용 — 기존 동작 그대로.
/// 2. 아니면 최근 접속 길드 목록(`recents`)에서 `name` 이 정확히 일치하는
///    항목을 찾는다.
///    - 정확히 하나 → 그 경로(단, 지금도 유효한 `.guild` 인지 재확인 —
///      이동/삭제됐을 수 있어서).
///    - 둘 이상 → 에러(모호함 — 경로로 지정하라고 안내).
///    - 없음 → 기존 "경로에 .guild 없음" 에러.
///
/// `recents` 는 **읽기만** 한다 — CLI 실행이 recents 를 갱신하지 않는다는
/// 기존 정책(DEV-117, `cli/src/main.rs` 의 `Backend::new` 주석 참고)은 그대로
/// 유지한다. 즉 GUI 로 한 번도 안 연 길드는 이름으로 못 찾는다 — 알려진 한계.
/// 경로로 지정하면 항상 동작하니 폴백은 있다.
pub fn resolve_guild_ref(raw: &str) -> Result<PathBuf> {
    let pb = PathBuf::from(raw);
    if find_from(&pb).is_some_and(|f| f == pb) {
        return Ok(pb);
    }

    let recents = crate::recents::list().unwrap_or_default();
    let matches: Vec<&crate::recents::Recent> = recents.iter().filter(|r| r.name == raw).collect();
    match matches.len() {
        0 => Err(anyhow!(
            "no .guild file at {raw} (use `openguild init` first, or check the name with `openguild guild list`)"
        )),
        1 => {
            let candidate = PathBuf::from(&matches[0].path);
            if find_from(&candidate).is_some_and(|f| f == candidate) {
                Ok(candidate)
            } else {
                Err(anyhow!(
                    "guild {raw:?} was found in recents but no longer has a valid .guild marker at {} \
                     (moved or deleted? try the path directly, or re-open it once via the GUI to refresh recents)",
                    candidate.display()
                ))
            }
        }
        _ => {
            let paths: Vec<String> = matches.iter().map(|r| r.path.clone()).collect();
            Err(anyhow!(
                "guild name {raw:?} matches {} guilds — specify the path instead:\n{}",
                matches.len(),
                paths.join("\n")
            ))
        }
    }
}

fn has_guild_file(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("guild"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = base.join(format!("openguild-test-{id}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn load_parses_valid_guild_file() {
        let dir = tmp_dir();
        fs::write(
            dir.join("monitor.guild"),
            "name = \"모니터\"\nversion = \"1.0\"\ncreated_at = \"2026-05-12\"\n",
        )
        .unwrap();

        let g = load(dir.to_str().unwrap()).unwrap();
        assert_eq!(g.name, "모니터");
        assert_eq!(g.version, "1.0");
        assert_eq!(g.created_at, "2026-05-12");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_fails_when_no_guild_file() {
        let dir = tmp_dir();
        // 빈 디렉토리
        let err = load(dir.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("no .guild file"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_fails_when_directory_missing() {
        let err = load("/nonexistent/path/that/should/not/exist").unwrap_err();
        assert!(err.to_string().contains("failed to read directory"));
    }

    #[test]
    fn load_finds_guild_file_regardless_of_name() {
        let dir = tmp_dir();
        // 파일명이 "monitor.guild" 가 아니라 "anything.guild" 여도 OK
        fs::write(
            dir.join("anything.guild"),
            "name = \"X\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();
        let g = load(dir.to_str().unwrap()).unwrap();
        assert_eq!(g.name, "X");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_fails_on_malformed_toml() {
        let dir = tmp_dir();
        fs::write(dir.join("broken.guild"), "this is not toml === ").unwrap();
        let err = load(dir.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("failed to parse"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_from_finds_in_same_dir() {
        let dir = tmp_dir();
        fs::write(
            dir.join("monitor.guild"),
            "name = \"M\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();
        let found = find_from(&dir).expect("should find guild");
        assert_eq!(found, dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_from_walks_up_to_parent() {
        let root = tmp_dir();
        fs::write(
            root.join("monitor.guild"),
            "name = \"M\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();
        let nested = root.join("a/b/c");
        fs::create_dir_all(&nested).unwrap();

        let found = find_from(&nested).expect("should walk up");
        assert_eq!(found, root);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_from_returns_none_when_no_guild_anywhere() {
        // 임시 디렉토리 하위에서만 검사 — 부모(시스템 temp)에 .guild 가 없다고 가정
        let dir = tmp_dir();
        let nested = dir.join("x/y");
        fs::create_dir_all(&nested).unwrap();
        // 결과: 부모로 거슬러 올라가며 시스템 root 까지 갈 수 있음. 매우 드물게 시스템 어딘가에
        // .guild 가 있을 수 있으나 일반 환경에선 None. 발견 시엔 우리 dir 위가 아닌 곳이어야 함.
        let found = find_from(&nested);
        if let Some(p) = &found {
            assert!(
                !p.starts_with(&dir),
                "false positive — our tmp tree had no .guild"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    // ─── DEV-319: resolve_guild_ref (경로 또는 이름) ───

    /// BUG-048: `recents::list/add` 가 읽는 `OPENGUILD_RECENTS_DIR` 는
    /// process-global env — `recents::tests` 의 **같은** Mutex 를 공유해야
    /// 직렬화가 실제로 걸린다(별개 Mutex 를 두면 서로를 못 막아 두 모듈
    /// 테스트가 동시에 env 를 건드리는 형태로 재발 — 실사고로 확인).
    use crate::recents::tests::with_env as with_recents_env;

    #[test]
    fn resolve_guild_ref_accepts_direct_path() {
        let dir = tmp_dir();
        fs::write(
            dir.join("m.guild"),
            "name = \"M\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();
        let resolved = resolve_guild_ref(dir.to_str().unwrap()).unwrap();
        assert_eq!(resolved, dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_guild_ref_resolves_by_name_from_recents() {
        let recents_dir = tmp_dir();
        let guild_dir = tmp_dir();
        fs::write(
            guild_dir.join("my-project.guild"),
            "name = \"my-project\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();
        with_recents_env(&recents_dir, || {
            crate::recents::add(&guild_dir).unwrap();
            let resolved = resolve_guild_ref("my-project").unwrap();
            // recents 는 canonicalize 된 경로를 저장(macOS 는 /var → /private/var
            // symlink 라 raw guild_dir 과 바이트가 다를 수 있음) — 같은 정규화로 비교.
            assert_eq!(resolved.to_str().unwrap(), normalize_abs_for_test(&guild_dir));
        });
        let _ = fs::remove_dir_all(&recents_dir);
        let _ = fs::remove_dir_all(&guild_dir);
    }

    #[test]
    fn resolve_guild_ref_ambiguous_name_errors_with_paths() {
        let recents_dir = tmp_dir();
        let a = tmp_dir();
        let b = tmp_dir();
        for d in [&a, &b] {
            fs::write(
                d.join("dup.guild"),
                "name = \"dup\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
            )
            .unwrap();
        }
        with_recents_env(&recents_dir, || {
            crate::recents::add(&a).unwrap();
            crate::recents::add(&b).unwrap();
            let err = resolve_guild_ref("dup").unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("matches 2 guilds"), "{msg}");
            assert!(msg.contains(a.to_str().unwrap()) || msg.contains(&normalize_abs_for_test(&a)));
        });
        let _ = fs::remove_dir_all(&recents_dir);
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn resolve_guild_ref_unknown_name_errors() {
        let recents_dir = tmp_dir();
        with_recents_env(&recents_dir, || {
            let err = resolve_guild_ref("no-such-guild").unwrap_err();
            assert!(err.to_string().contains("no .guild file"));
        });
        let _ = fs::remove_dir_all(&recents_dir);
    }

    #[test]
    fn resolve_guild_ref_stale_recents_entry_errors() {
        // recents 엔 있지만 실제로 그 경로에 .guild 마커가 이제 없는 경우
        // (디렉토리를 지웠거나 옮김) — 조용히 잘못된 경로로 넘기면 안 된다.
        let recents_dir = tmp_dir();
        let guild_dir = tmp_dir();
        fs::write(
            guild_dir.join("gone.guild"),
            "name = \"gone\"\nversion = \"1.0\"\ncreated_at = \"2026-01-01\"\n",
        )
        .unwrap();
        with_recents_env(&recents_dir, || {
            crate::recents::add(&guild_dir).unwrap();
        });
        fs::remove_dir_all(&guild_dir).unwrap(); // 마커가 사라짐.
        with_recents_env(&recents_dir, || {
            let err = resolve_guild_ref("gone").unwrap_err();
            assert!(err.to_string().contains("no longer has a valid"), "{}", err);
        });
        let _ = fs::remove_dir_all(&recents_dir);
    }

    /// `recents::add` 는 canonicalize 된 절대경로를 저장하므로, tmp_dir()(이미
    /// 절대경로) 를 대조할 땐 symlink 해소(macOS `/tmp` → `/private/tmp`) 차이가
    /// 있을 수 있어 같은 정규화를 거쳐 비교한다.
    fn normalize_abs_for_test(p: &Path) -> String {
        crate::recents::normalize_abs(p)
    }
}
