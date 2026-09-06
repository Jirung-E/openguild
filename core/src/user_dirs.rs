//! DEV-247: 사용자 데이터 홈 `~/.openguild/`.
//!
//! 배경(admin 보고): 에이전트가 설치 폴더(%LOCALAPPDATA%\openguild\docs)의
//! 문서를 읽을 때 샌드박스/권한 문제가 발생하는 경우가 있음. 홈 도트폴더
//! (~/.cargo, ~/.ssh 관례)는 에이전트 도구들이 통상 허용하고, OS 불문 경로가
//! 동일(`~/.openguild/`)해 문서에 적기도 쉽다. 설치 디렉토리(exe)는 NSIS
//! 관례대로 유지 — **사용자 데이터만** 분리(언인스톨/업그레이드 시 데이터
//! 유실 위험도 해소).
//!
//! 이 모듈이 경로의 단일 진입점 — recents([`crate::recents`]), 번들 문서
//! 복사, (후속) 커스텀 테마/서버 설정 등이 같은 위치를 쓴다.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// `~/.openguild/` — 없으면 생성. 테스트는 `OPENGUILD_HOME` env 로 격리.
pub fn openguild_home() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("OPENGUILD_HOME") {
        let p = PathBuf::from(dir);
        std::fs::create_dir_all(&p)
            .with_context(|| format!("create test openguild home: {}", p.display()))?;
        return Ok(p);
    }
    let user = directories::UserDirs::new()
        .context("UserDirs::new failed — HOME / USERPROFILE 환경변수 미설정?")?;
    let p = user.home_dir().join(".openguild");
    std::fs::create_dir_all(&p)
        .with_context(|| format!("create openguild home: {}", p.display()))?;
    Ok(p)
}

/// 번들 문서(설치 폴더 `docs/`)를 `~/.openguild/docs/` 로 동기화 — 첫 실행
/// 복사 + 이후 업데이트 반영(원본이 더 새 것일 때만 덮어씀). GUI 시동이
/// best-effort 로 호출. src 가 없으면(개발 환경 등) 조용히 no-op.
pub fn sync_bundled_docs(src_dir: &Path) -> Result<usize> {
    if !src_dir.is_dir() {
        return Ok(0);
    }
    let dst_dir = openguild_home()?.join("docs");
    std::fs::create_dir_all(&dst_dir)
        .with_context(|| format!("create docs dir: {}", dst_dir.display()))?;
    let mut copied = 0usize;
    for entry in std::fs::read_dir(src_dir)
        .with_context(|| format!("read bundled docs: {}", src_dir.display()))?
        .flatten()
    {
        let src = entry.path();
        if !src.is_file() || src.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = src.file_name() else { continue };
        let dst = dst_dir.join(name);
        let src_mtime = std::fs::metadata(&src).and_then(|m| m.modified()).ok();
        let dst_mtime = std::fs::metadata(&dst).and_then(|m| m.modified()).ok();
        let stale = match (src_mtime, dst_mtime) {
            (_, None) => true,                       // 아직 없음 — 첫 복사.
            (Some(s), Some(d)) => s > d,             // 업데이트 반영.
            (None, Some(_)) => false,
        };
        if stale {
            std::fs::copy(&src, &dst)
                .with_context(|| format!("copy doc: {} → {}", src.display(), dst.display()))?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// DEV-264: 배포용 스킬 마켓플레이스(`skills/` — Claude Code plugin marketplace
/// 구조)를 `~/.openguild/skill-marketplace/` 로 동기화. `sync_bundled_docs` 와
/// 같은 정책(원본이 더 새 것일 때만 복사)이지만 이쪽은 중첩 디렉토리 전체를
/// 재귀 복사해야 해서(`.claude-plugin/`, `openguild-plugin/skills/openguild/`
/// 등) 별도 함수로 분리. src 가 없으면(개발 환경 등) 조용히 no-op.
pub fn sync_bundled_skill_marketplace(src_dir: &Path) -> Result<usize> {
    if !src_dir.is_dir() {
        return Ok(0);
    }
    let dst_dir = openguild_home()?.join("skill-marketplace");
    // BUG-267: **버전이 다르면 mtime 은 보지 않는다.**
    //
    // mtime 만으로 판단하면, 사본이 어떤 이유로든 더 새 시각을 갖게 된 순간
    // (저장소 CLI 실행, 백업 복원, 파일 복사, 편집) 그 뒤로 앱이 아무리 새
    // 스킬을 들고 와도 영영 무시된다. 실기에서 그 상태가 확인됐다 — 앱 번들은
    // 8/27 빌드(0.5.0)인데 사본은 9/4(0.5.2)라 앱을 실행해도 아무 일도 일어나지
    // 않았다. 실패가 조용해서 사용자는 옛 스킬을 계속 쓴다([[BUG-261]] 과 같은
    // 성질의 결함이다).
    //
    // 버전이 같을 때는 기존 mtime 규칙을 유지한다 — 저장소에서 같은 버전으로
    // 내용만 고치며 개발하는 흐름이 있다.
    let force = plugin_version(src_dir) != plugin_version(&dst_dir);
    copy_dir(src_dir, &dst_dir, force)
}

/// 마켓플레이스 트리의 `openguild-plugin/.claude-plugin/plugin.json` 버전.
/// 없거나 깨져 있으면 `None` — 그 경우 양쪽이 같은 `None` 이 아니면 복사한다.
fn plugin_version(root: &Path) -> Option<String> {
    let p = root
        .join("openguild-plugin")
        .join(".claude-plugin")
        .join("plugin.json");
    let raw = std::fs::read_to_string(p).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    json.get("version")?.as_str().map(|s| s.to_string())
}

/// `src` 트리를 `dst` 로 재귀 복사.
///
/// `force` 면 mtime 을 무시하고 전부 덮어쓴다(BUG-267). 아니면 파일별로 mtime
/// 을 비교해 원본이 더 새 것일 때만 덮어쓴다(`sync_bundled_docs` 와 동일 정책,
/// 디렉토리에 대해 재귀 적용).
fn copy_dir(src: &Path, dst: &Path, force: bool) -> Result<usize> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("create dir: {}", dst.display()))?;
    let mut copied = 0usize;
    for entry in std::fs::read_dir(src)
        .with_context(|| format!("read dir: {}", src.display()))?
        .flatten()
    {
        let src_path = entry.path();
        let Some(name) = src_path.file_name() else { continue };
        let dst_path = dst.join(name);
        if src_path.is_dir() {
            copied += copy_dir(&src_path, &dst_path, force)?;
            continue;
        }
        if !src_path.is_file() {
            continue;
        }
        let src_mtime = std::fs::metadata(&src_path).and_then(|m| m.modified()).ok();
        let dst_mtime = std::fs::metadata(&dst_path).and_then(|m| m.modified()).ok();
        let stale = force
            || match (src_mtime, dst_mtime) {
                (_, None) => true,
                (Some(s), Some(d)) => s > d,
                (None, Some(_)) => false,
            };
        if stale {
            std::fs::copy(&src_path, &dst_path).with_context(|| {
                format!("copy: {} → {}", src_path.display(), dst_path.display())
            })?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    // DEV-264: OPENGUILD_HOME 은 프로세스 전역 env — 병렬 테스트가 서로의 값을
    // 덮어쓰는 레이스가 있었다.
    // BUG-250: 그때는 이 파일 전용 mutex 였는데, 다른 파일(recents / locale /
    // snapshot)도 각자 env 를 건드려 **파일 사이에서** 겹쳤다. 전역 잠금 하나로.
    use crate::test_env::env_lock;

    fn fresh_tmp(label: &str) -> PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-home-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn sync_bundled_docs_copies_md_then_skips_when_fresh() {
        let _guard = env_lock();
        let src = fresh_tmp("docs-src");
        let home = fresh_tmp("docs-home");
        std::fs::write(src.join("USAGE.md"), "# usage").unwrap();
        std::fs::write(src.join("not-doc.txt"), "skip").unwrap();

        // OPENGUILD_HOME 격리 — 테스트 간 env 경합을 피하려 직접 dst 계산은
        // 안 하고, env 를 이 스코프에서만 설정.
        // (cargo test 는 스레드 병렬이지만 이 env 를 쓰는 테스트는 본 파일뿐.)
        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
        let copied = sync_bundled_docs(&src).unwrap();
        assert_eq!(copied, 1, "md 만 복사");
        assert!(home.join("docs/USAGE.md").is_file());
        assert!(!home.join("docs/not-doc.txt").exists());

        // 원본이 더 새 것이 아니면 재복사 안 함.
        let again = sync_bundled_docs(&src).unwrap();
        assert_eq!(again, 0);
        unsafe { std::env::remove_var("OPENGUILD_HOME") };

        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn sync_bundled_docs_missing_src_is_noop() {
        let missing = std::env::temp_dir().join("og-home-no-such-dir");
        assert_eq!(sync_bundled_docs(&missing).unwrap(), 0);
    }

    #[test]
    fn sync_bundled_skill_marketplace_copies_nested_tree_then_skips_when_fresh() {
        let _guard = env_lock();
        let src = fresh_tmp("skills-src");
        let home = fresh_tmp("skills-home");
        std::fs::create_dir_all(src.join(".claude-plugin")).unwrap();
        std::fs::write(src.join(".claude-plugin/marketplace.json"), "{}").unwrap();
        std::fs::create_dir_all(src.join("openguild-plugin/skills/openguild")).unwrap();
        std::fs::write(
            src.join("openguild-plugin/skills/openguild/SKILL.md"),
            "---\nname: openguild\n---\n",
        )
        .unwrap();

        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };
        let copied = sync_bundled_skill_marketplace(&src).unwrap();
        assert_eq!(copied, 2, "marketplace.json + SKILL.md");
        assert!(home.join("skill-marketplace/.claude-plugin/marketplace.json").is_file());
        assert!(
            home.join("skill-marketplace/openguild-plugin/skills/openguild/SKILL.md").is_file()
        );

        let again = sync_bundled_skill_marketplace(&src).unwrap();
        assert_eq!(again, 0, "원본이 더 새 것이 아니면 재복사 안 함");
        unsafe { std::env::remove_var("OPENGUILD_HOME") };

        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// BUG-267: **버전이 다르면 사본이 더 새것이어도 덮어쓴다.**
    ///
    /// mtime 만 보던 시절엔, 사본이 어떤 이유로든 최신 시각을 갖게 되는 순간
    /// 앱이 아무리 새 스킬을 들고 와도 영영 무시됐다. 실기에서 그 상태가
    /// 확인됐다(앱 번들 8/27·0.5.0 vs 사본 9/4·0.5.2). 실패가 조용해서
    /// 사용자는 옛 스킬을 계속 쓴다.
    ///
    /// "사본이 더 새것" 은 **사본을 나중에 덮어써서** 만든다 — 시각을 직접
    /// 세팅하는 크레이트를 테스트 때문에 들이지 않기 위해서다.
    fn write_marketplace(root: &std::path::Path, version: &str, body: &str) {
        std::fs::create_dir_all(root.join("openguild-plugin/.claude-plugin")).unwrap();
        std::fs::write(
            root.join("openguild-plugin/.claude-plugin/plugin.json"),
            format!("{{\n  \"name\": \"openguild-plugin\",\n  \"version\": \"{version}\"\n}}\n"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("openguild-plugin/skills/openguild")).unwrap();
        std::fs::write(
            root.join("openguild-plugin/skills/openguild/SKILL.md"),
            body,
        )
        .unwrap();
    }

    fn set_version(root: &std::path::Path, version: &str) {
        std::fs::write(
            root.join("openguild-plugin/.claude-plugin/plugin.json"),
            format!("{{\n  \"name\": \"openguild-plugin\",\n  \"version\": \"{version}\"\n}}\n"),
        )
        .unwrap();
    }

    fn skill_body(root: &std::path::Path) -> String {
        std::fs::read_to_string(root.join("openguild-plugin/skills/openguild/SKILL.md")).unwrap()
    }

    #[test]
    fn sync_skill_marketplace_version_differs_overwrites_even_when_copy_is_newer() {
        let _guard = env_lock();
        let src = fresh_tmp("skills-src-ver");
        let home = fresh_tmp("skills-home-ver");
        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };

        write_marketplace(&src, "0.5.0", "BUNDLED\n");
        assert!(sync_bundled_skill_marketplace(&src).unwrap() > 0);
        let dst = home.join("skill-marketplace");

        // 사본을 나중에 덮어써 **사본이 더 새것** 인 상태로 만든다.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            dst.join("openguild-plugin/skills/openguild/SKILL.md"),
            "STALE\n",
        )
        .unwrap();

        // 앱이 새 버전을 들고 왔다. 내용 파일의 mtime 은 여전히 사본보다 옛것이라
        // mtime 만 보면 여기서 아무 일도 일어나지 않는다 — 그게 이 버그였다.
        set_version(&src, "0.5.3");

        let copied = sync_bundled_skill_marketplace(&src).unwrap();
        assert!(copied > 0, "버전이 다르면 mtime 과 무관하게 복사해야 한다");
        assert_eq!(
            skill_body(&dst),
            "BUNDLED\n",
            "번들 내용으로 되돌아와야 한다"
        );

        unsafe { std::env::remove_var("OPENGUILD_HOME") };
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn sync_skill_marketplace_same_version_keeps_mtime_rule() {
        let _guard = env_lock();
        let src = fresh_tmp("skills-src-same");
        let home = fresh_tmp("skills-home-same");
        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };

        write_marketplace(&src, "0.5.3", "BUNDLED\n");
        assert!(sync_bundled_skill_marketplace(&src).unwrap() > 0);
        let dst = home.join("skill-marketplace");

        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            dst.join("openguild-plugin/skills/openguild/SKILL.md"),
            "LOCAL\n",
        )
        .unwrap();

        // 버전이 같으면 더 새 사본을 건드리지 않는다 — 저장소에서 같은 버전으로
        // 내용만 고치며 개발하는 흐름을 깨면 안 된다.
        let copied = sync_bundled_skill_marketplace(&src).unwrap();
        assert_eq!(copied, 0);
        assert_eq!(skill_body(&dst), "LOCAL\n");

        unsafe { std::env::remove_var("OPENGUILD_HOME") };
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn sync_skill_marketplace_broken_plugin_json_does_not_throw() {
        let _guard = env_lock();
        let src = fresh_tmp("skills-src-broken");
        let home = fresh_tmp("skills-home-broken");
        unsafe { std::env::set_var("OPENGUILD_HOME", &home) };

        std::fs::create_dir_all(src.join("openguild-plugin/.claude-plugin")).unwrap();
        std::fs::write(
            src.join("openguild-plugin/.claude-plugin/plugin.json"),
            "{ not json",
        )
        .unwrap();
        std::fs::create_dir_all(src.join("openguild-plugin/skills/openguild")).unwrap();
        std::fs::write(
            src.join("openguild-plugin/skills/openguild/SKILL.md"),
            "X\n",
        )
        .unwrap();

        assert!(sync_bundled_skill_marketplace(&src).is_ok());

        unsafe { std::env::remove_var("OPENGUILD_HOME") };
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn sync_bundled_skill_marketplace_missing_src_is_noop() {
        let missing = std::env::temp_dir().join("og-home-no-such-skills-dir");
        assert_eq!(sync_bundled_skill_marketplace(&missing).unwrap(), 0);
    }
}
