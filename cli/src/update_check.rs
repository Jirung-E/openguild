//! DEV-187: `openguild update --check` — 새 버전이 나왔는지 알려 준다.
//!
//! 앱은 Tauri updater 로 스스로 확인하지만, CLI · 서버만 쓰는 사람은 새 버전을 알 길이 없었다. 앱과 **같은**
//! `latest.json`(GitHub Releases 의 최신 릴리스)을 읽어 버전만 비교하고 안내한다. 설치는 하지 않는다 — 돌고
//! 있는 실행 파일을 바꾸는 것은 플랫폼마다 까다롭고, 설치는 설치 프로그램 · 앱의 몫이다.
//!
//! CLI 와 서버(`openguild-server`)는 늘 같은 버전으로 함께 나가므로 이 명령 하나로 둘 다 본다.
//!
//! 길드 점검(`check drift` 등)이 아니라 최상위 `update` 에 둔다 — 길드와 상관없고, 길드 옵션도 안 쓴다.
//! `--check` 없이 부르면 지금은 같은 확인에 "직접 바꾸기는 아직 없다" 는 말을 붙인다. 나중에 직접 바꾸게
//! 되면(DEV-351) 그 자리를 쓰고, `--check` 는 그때도 확인만 한다.

use anyhow::{Result, anyhow};
// `tf!` 가 부르는 언어 판정(main.rs).
use crate::current_locale_is_en;
use std::cmp::Ordering;

/// 앱의 updater 와 같은 주소 — `gui/tauri.conf.json` 의 `plugins.updater.endpoints[0]`. 시험이 둘이 같은지 본다.
pub const LATEST_JSON_URL: &str =
    "https://github.com/Jirung-E/openguild/releases/latest/download/latest.json";

/// 릴리스 페이지 — 태그는 `v{버전}`.
const RELEASE_PAGE: &str = "https://github.com/Jirung-E/openguild/releases/tag/v";

#[derive(Debug, serde::Deserialize)]
struct LatestJson {
    version: String,
    #[serde(default)]
    pub_date: Option<String>,
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    /// 받을 새 버전이 있다.
    pub update_available: bool,
    pub pub_date: Option<String>,
    pub release_url: String,
}

/// `0.6.0-beta` 같은 버전 둘을 semver 규칙으로 비교한다. 앞의 `v` 는 무시한다.
///
/// 숫자 셋을 먼저 보고, 같으면 **pre-release 가 없는 쪽이 더 새것**(`0.6.0` > `0.6.0-beta`), 둘 다 있으면 점으로
/// 나눈 조각을 차례로 — 숫자끼리는 수로, 글자끼리는 글자로, 숫자 조각은 글자 조각보다 작다(`beta.2` < `beta.10`).
/// 읽을 수 없는 숫자 조각은 0 으로 본다(깨진 latest.json 때문에 명령이 멈추지 않게).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> ([u64; 3], Option<&str>) {
        let v = v.trim().trim_start_matches('v');
        let v = v.split('+').next().unwrap_or(v); // 빌드 메타데이터는 순서와 상관없다
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (v, None),
        };
        let mut nums = [0u64; 3];
        for (slot, part) in nums.iter_mut().zip(core.split('.')) {
            *slot = part.parse().unwrap_or(0);
        }
        (nums, pre)
    }
    let (an, ap) = split(a);
    let (bn, bp) = split(b);
    an.cmp(&bn).then_with(|| match (ap, bp) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => {
            let mut xs = x.split('.');
            let mut ys = y.split('.');
            loop {
                match (xs.next(), ys.next()) {
                    (None, None) => return Ordering::Equal,
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(p), Some(q)) => {
                        let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                            (Ok(m), Ok(n)) => m.cmp(&n),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => p.cmp(q),
                        };
                        if o != Ordering::Equal {
                            return o;
                        }
                    }
                }
            }
        }
    })
}

/// 받은 `latest.json` 본문 → 상태. 네트워크 없이 시험할 수 있게 떼어 둔다.
pub fn status_from(current: &str, body: &str) -> Result<UpdateStatus> {
    let latest: LatestJson = serde_json::from_str(body).map_err(|e| {
        anyhow!(tf!(
            "latest.json 을 읽지 못했습니다 — 릴리스 파일이 깨졌거나 다른 것을 받았습니다: {}",
            "could not read latest.json — the release file is broken or something else was served: {}",
            e
        ))
    })?;
    let version = latest.version.trim().trim_start_matches('v').to_string();
    Ok(UpdateStatus {
        current: current.to_string(),
        update_available: compare_versions(&version, current) == Ordering::Greater,
        release_url: format!("{RELEASE_PAGE}{version}"),
        latest: version,
        pub_date: latest.pub_date,
    })
}

/// 주소에서 `latest.json` 을 받는다. 실패는 사람이 할 일을 알 수 있는 말로 — 앱의 `humanizeUpdaterError` 와 같은 갈래.
fn fetch(url: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(concat!("openguild-cli/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let resp = client.get(url).send().map_err(|e| {
        anyhow!(tf!(
            "새 버전을 확인하지 못했습니다 — 네트워크 연결을 확인하세요. ({})",
            "could not check for updates — check your network connection. ({})",
            e
        ))
    })?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(anyhow!(tf!(
            "아직 릴리스가 없거나 주소가 잘못됐습니다 ({}): {}",
            "no release yet, or the address is wrong ({}): {}",
            status,
            url
        )));
    }
    if !status.is_success() {
        return Err(anyhow!(tf!(
            "새 버전을 확인하지 못했습니다 — 서버가 {} 로 답했습니다: {}",
            "could not check for updates — the server answered {}: {}",
            status,
            url
        )));
    }
    Ok(resp.text()?)
}

/// `openguild update [--check]`. 주소는 `OPENGUILD_UPDATE_URL` 로 바꿀 수 있다(미러 · 시험).
pub fn run(json: bool, check_only: bool) -> Result<()> {
    let url = std::env::var("OPENGUILD_UPDATE_URL").unwrap_or_else(|_| LATEST_JSON_URL.to_string());
    let st = status_from(env!("CARGO_PKG_VERSION"), &fetch(&url)?)?;
    if json {
        println!("{}", crate::json_str(&st));
        return Ok(());
    }
    if !check_only {
        println!(
            "{}",
            tf!(
                "직접 바꾸기는 아직 없습니다 — 새 버전이 나왔는지만 봅니다. (확인만: openguild update --check)",
                "Installing by itself is not supported yet — this only checks for a new version. (check only: openguild update --check)"
            )
        );
    }
    if st.update_available {
        let date = st
            .pub_date
            .as_deref()
            .map(|d| d.get(..10).unwrap_or(d).to_string())
            .map(|d| format!(" ({d})"))
            .unwrap_or_default();
        println!(
            "{}",
            tf!(
                "새 버전이 있습니다: {}{} — 지금 {}\n받기: {}\n앱을 쓰면 앱이 업데이트를 안내합니다. CLI · 서버는 설치 프로그램이나 릴리스 페이지의 파일로 바꿉니다.",
                "A new version is available: {}{} — you have {}\nGet it: {}\nThe desktop app offers the update itself. Replace the CLI / server with the installer or the files on the release page.",
                st.latest,
                date,
                st.current,
                st.release_url
            )
        );
    } else {
        println!(
            "{}",
            tf!(
                "최신입니다 — 지금 {} (나온 것 중 최신 {})",
                "Up to date — you have {} (latest release {})",
                st.current,
                st.latest
            )
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_semver() {
        use Ordering::*;
        for (a, b, want) in [
            ("0.6.0-beta", "0.5.3-beta", Greater),
            ("0.5.3-beta", "0.6.0-beta", Less),
            ("0.6.0-beta", "0.6.0-beta", Equal),
            ("v0.6.0-beta", "0.6.0-beta", Equal),
            ("0.6.0", "0.6.0-beta", Greater),
            ("0.6.0-beta", "0.6.0", Less),
            ("0.6.0-beta.2", "0.6.0-beta.10", Less),
            ("0.6.0-rc", "0.6.0-beta", Greater),
            ("0.6.0-beta.1", "0.6.0-beta", Greater),
            ("0.10.0", "0.9.9", Greater),
            ("1.0.0", "0.99.99", Greater),
            ("0.6.0+build.7", "0.6.0", Equal),
        ] {
            assert_eq!(compare_versions(a, b), want, "{a} vs {b}");
        }
    }

    #[test]
    fn newer_release_is_offered_and_older_is_not() {
        let body = r#"{"version":"0.6.1-beta","notes":"x","pub_date":"2026-11-01T10:00:00Z","platforms":{}}"#;
        let st = status_from("0.6.0-beta", body).unwrap();
        assert!(st.update_available);
        assert_eq!(st.latest, "0.6.1-beta");
        assert_eq!(
            st.release_url,
            "https://github.com/Jirung-E/openguild/releases/tag/v0.6.1-beta"
        );
        assert_eq!(st.pub_date.as_deref(), Some("2026-11-01T10:00:00Z"));

        // 개발 빌드가 나온 것보다 새것이면 '최신'.
        let st = status_from("0.6.0-beta", r#"{"version":"0.5.3-beta"}"#).unwrap();
        assert!(!st.update_available);
        assert!(status_from("0.6.0-beta", "<html>not json</html>").is_err());
    }

    /// 앱의 updater 와 **같은** 곳을 봐야 한다 — 한쪽만 주소를 바꾸면 CLI 는 엉뚱한 버전을 안내한다.
    #[test]
    fn checks_the_same_latest_json_as_the_app() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../gui/tauri.conf.json")).unwrap();
        assert_eq!(conf["plugins"]["updater"]["endpoints"][0], LATEST_JSON_URL);
    }
}
