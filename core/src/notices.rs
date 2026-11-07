//! DEV-478: 알림함 — 잠깐 떴다 사라진 알림을 다시 본다.
//!
//! 플러그인의 `notify` 와 전달 실패를 `.guild/notices/notices.jsonl` 에 한 줄씩 적는다. 앱 · 서버 ·
//! CLI 어디서 났든 같은 파일이라, 앱을 껐다 켜도 · 같은 기계의 서버에서 난 것도 앱에서 보인다.
//!
//! - **개인 것이다.** 폴더 안 `.gitignore` 로 git 에서 빠지고(형광펜과 같다), 백업에도 안 담는다 —
//!   지나간 알림이라 잃어도 다시 만들 일이 없다.
//! - **최근 것만.** [`KEEP`] 개 · [`KEEP_DAYS`] 일. 넘치면 쓸 때 오래된 것부터 버린다.
//! - **기록이다.** 한 줄이 깨져도 나머지는 읽는다 — 진리원이 아니라 덮어쓸 위험이 없다.
//! - 읽은 데까지는 `seen` 파일에 시각 하나로 둔다.
//!
//! 앱 자체의 알림(저장 실패 등)은 여기 오지 않는다 — 그 순간의 일이라 앱이 켜진 동안만 화면이 들고 있다.

use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, Local, SecondsFormat};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::Mutex;

use crate::repo::GuildPaths;

/// 남기는 개수.
pub const KEEP: usize = 200;
/// 남기는 날수.
pub const KEEP_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// 플러그인이 `notify()` 로 보낸 글.
    Notify,
    /// 플러그인이 돌다 실패했다.
    Problem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    /// 적은 시각 — 밀리초까지(같은 초의 두 알림도 읽음 표시를 가른다).
    pub at: String,
    pub kind: Kind,
    /// 보낸 플러그인. 실패는 글 안에 이름이 있어 비어 있을 수 있다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    pub text: String,
    /// 누르면 열 문서 — `notify(글, #{ open, comment })` 그대로.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<i64>,
    /// 카드 앞 아이콘 — `notify(글, #{ icon })` 그대로.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl Notice {
    pub fn now(kind: Kind, plugin: Option<&str>, text: &str) -> Self {
        Self {
            at: Local::now().to_rfc3339_opts(SecondsFormat::Millis, false),
            kind,
            plugin: plugin.map(str::to_string),
            text: text.to_string(),
            open: None,
            comment: None,
            icon: None,
        }
    }
}

/// 알림함 — 최근 것이 먼저, 그리고 안 읽은 개수.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Inbox {
    pub notices: Vec<Notice>,
    pub unread: usize,
    /// 안 읽은 것 가운데 실패.
    pub unread_problems: usize,
}

fn file(paths: &GuildPaths) -> std::path::PathBuf {
    paths.notices_dir().join("notices.jsonl")
}

fn seen_file(paths: &GuildPaths) -> std::path::PathBuf {
    paths.notices_dir().join("seen")
}

fn parse(at: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(at.trim()).ok()
}

/// 한 프로세스 안에서 덧붙이기와 줄이기가 엇갈리지 않게. 다른 프로세스와는 줄이는 순간에만 겹칠 수
/// 있고, 그때 잃는 것은 그 순간의 알림 한 줄이다.
static WRITING: Mutex<()> = Mutex::new(());

/// 한 줄 적는다. 넘치면 오래된 것을 버린다.
pub fn record(paths: &GuildPaths, notice: &Notice) -> Result<()> {
    let _g = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let dir = paths.notices_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "*\n")
            .with_context(|| format!("failed to write {}", ignore.display()))?;
    }
    let p = file(paths);
    let line = serde_json::to_string(notice).context("serialize notice")?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .with_context(|| format!("failed to open {}", p.display()))?;
    writeln!(f, "{line}").with_context(|| format!("failed to write {}", p.display()))?;
    drop(f);
    // 줄이기는 넉넉히 넘쳤을 때만 — 매번 다시 쓰지 않게.
    let all = read_all(paths)?;
    if all.len() > KEEP + KEEP / 4 {
        let kept = recent(all);
        let mut body = String::new();
        for n in kept.iter().rev() {
            body.push_str(&serde_json::to_string(n).context("serialize notice")?);
            body.push('\n');
        }
        crate::repo::fs::write_atomic(&p, &body)?;
    }
    Ok(())
}

/// 파일 그대로(오래된 것 먼저). 깨진 줄은 건너뛴다.
fn read_all(paths: &GuildPaths) -> Result<Vec<Notice>> {
    let p = file(paths);
    let raw = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("failed to read {}", p.display())),
    };
    Ok(raw
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect())
}

/// 남길 것 — 최근 것이 먼저, [`KEEP_DAYS`] 일 안의 [`KEEP`] 개.
fn recent(all: Vec<Notice>) -> Vec<Notice> {
    let since = Local::now() - chrono::Duration::days(KEEP_DAYS);
    let mut out: Vec<Notice> = all
        .into_iter()
        .filter(|n| parse(&n.at).is_none_or(|t| t >= since))
        .collect();
    out.reverse();
    out.truncate(KEEP);
    out
}

/// 알림함을 읽는다.
pub fn inbox(paths: &GuildPaths) -> Result<Inbox> {
    let notices = recent(read_all(paths)?);
    let seen = std::fs::read_to_string(seen_file(paths))
        .ok()
        .and_then(|s| parse(&s));
    let new: Vec<&Notice> = notices
        .iter()
        .filter(|n| match (seen, parse(&n.at)) {
            (Some(s), Some(t)) => t > s,
            (None, _) => true,
            (Some(_), None) => false,
        })
        .collect();
    let unread = new.len();
    let unread_problems = new.iter().filter(|n| n.kind == Kind::Problem).count();
    Ok(Inbox {
        notices,
        unread,
        unread_problems,
    })
}

/// `at` 까지 읽었다고 적는다. 더 앞의 시각이면 그대로 둔다 — 다른 창이 더 읽었을 수 있다.
pub fn mark_seen(paths: &GuildPaths, at: &str) -> Result<()> {
    let Some(t) = parse(at) else {
        anyhow::bail!(crate::tf!("시각이 아닙니다: {at}", "not a time: {at}"));
    };
    let _g = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let p = seen_file(paths);
    if let Some(old) = std::fs::read_to_string(&p).ok().and_then(|s| parse(&s))
        && old >= t
    {
        return Ok(());
    }
    if !paths.notices_dir().exists() {
        return Ok(());
    }
    crate::repo::fs::write_atomic(&p, at.trim())
}

/// 하나를 지운다 — 시각과 글이 같은 줄. 지운 개수를 돌려준다(이미 없으면 0).
///
/// 알림에 따로 번호가 없어서 둘로 가른다. 같은 밀리초에 같은 글이 둘이면 함께 지워지는데, 그 둘은 화면에서도 가를 수 없다.
pub fn dismiss(paths: &GuildPaths, at: &str, text: &str) -> Result<usize> {
    let _g = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let all = read_all(paths)?;
    let before = all.len();
    let kept: Vec<Notice> = all
        .into_iter()
        .filter(|n| !(n.at == at && n.text == text))
        .collect();
    let gone = before - kept.len();
    if gone > 0 {
        let mut body = String::new();
        for n in &kept {
            body.push_str(&serde_json::to_string(n).context("serialize notice")?);
            body.push('\n');
        }
        crate::repo::fs::write_atomic(file(paths), &body)?;
    }
    Ok(gone)
}

/// 모두 지운다(읽음 표시는 둔다). 지운 개수를 돌려준다.
pub fn clear(paths: &GuildPaths) -> Result<usize> {
    let _g = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let n = read_all(paths)?.len();
    if n > 0 {
        crate::repo::fs::write_atomic(file(paths), "")?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> GuildPaths {
        let dir = std::env::temp_dir().join(format!(
            "og-notice-{}-{}",
            std::process::id(),
            crate::test_seq()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".guild")).unwrap();
        GuildPaths::new(&dir)
    }

    fn at(n: &Notice, s: &str) -> Notice {
        Notice {
            at: s.into(),
            ..n.clone()
        }
    }

    #[test]
    fn recorded_notices_come_back_newest_first_and_stay_out_of_git() {
        let p = fresh();
        let mut a = Notice::now(Kind::Notify, Some("p"), "하나");
        a.open = Some("DEV-1".into());
        record(&p, &a).unwrap();
        record(&p, &Notice::now(Kind::Problem, None, "둘")).unwrap();
        let ib = inbox(&p).unwrap();
        assert_eq!(
            ib.notices
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>(),
            ["둘", "하나"]
        );
        assert_eq!(ib.notices[1].open.as_deref(), Some("DEV-1"));
        assert_eq!((ib.unread, ib.unread_problems), (2, 1));
        assert_eq!(
            std::fs::read_to_string(p.notices_dir().join(".gitignore")).unwrap(),
            "*\n"
        );
    }

    #[test]
    fn seen_counts_only_what_came_after_and_never_goes_back() {
        let p = fresh();
        let base = Notice::now(Kind::Notify, Some("p"), "x");
        record(&p, &at(&base, "2026-11-06T10:00:00.000+09:00")).unwrap();
        record(&p, &at(&base, "2026-11-06T10:00:00.500+09:00")).unwrap();
        mark_seen(&p, "2026-11-06T10:00:00.000+09:00").unwrap();
        assert_eq!(
            inbox(&p).unwrap().unread,
            1,
            "같은 초 안의 뒤엣것은 안 읽었다"
        );
        mark_seen(&p, "2026-11-06T10:00:00.500+09:00").unwrap();
        mark_seen(&p, "2026-11-06T09:00:00.000+09:00").unwrap();
        assert_eq!(inbox(&p).unwrap().unread, 0, "앞 시각으로 되돌아갔다");
        assert!(mark_seen(&p, "어제").is_err());
    }

    #[test]
    fn dismiss_takes_out_only_that_one() {
        let p = fresh();
        let base = Notice::now(Kind::Notify, Some("p"), "x");
        record(&p, &at(&base, "2026-11-06T10:00:00.000+09:00")).unwrap();
        record(&p, &at(&base, "2026-11-06T10:00:01.000+09:00")).unwrap();
        let other = Notice {
            text: "y".into(),
            ..at(&base, "2026-11-06T10:00:01.000+09:00")
        };
        record(&p, &other).unwrap();
        assert_eq!(
            dismiss(&p, "2026-11-06T10:00:01.000+09:00", "x").unwrap(),
            1
        );
        let left: Vec<_> = inbox(&p)
            .unwrap()
            .notices
            .into_iter()
            .map(|n| (n.at, n.text))
            .collect();
        assert_eq!(
            left,
            [
                ("2026-11-06T10:00:01.000+09:00".to_string(), "y".to_string()),
                ("2026-11-06T10:00:00.000+09:00".to_string(), "x".to_string()),
            ]
        );
        assert_eq!(
            dismiss(&p, "2026-11-06T10:00:01.000+09:00", "x").unwrap(),
            0,
            "이미 없다"
        );
        assert_eq!(clear(&p).unwrap(), 2);
        assert!(inbox(&p).unwrap().notices.is_empty());
        assert_eq!(clear(&p).unwrap(), 0);
    }

    #[test]
    fn old_and_extra_notices_are_dropped_and_a_broken_line_is_skipped() {
        let p = fresh();
        let base = Notice::now(Kind::Notify, Some("p"), "x");
        record(&p, &at(&base, "2020-01-01T00:00:00.000+09:00")).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(p.notices_dir().join("notices.jsonl"))
            .unwrap()
            .write_all(b"{broken\n")
            .unwrap();
        for i in 0..KEEP + KEEP / 4 {
            record(&p, &Notice::now(Kind::Notify, Some("p"), &i.to_string())).unwrap();
        }
        let ib = inbox(&p).unwrap();
        assert_eq!(ib.notices.len(), KEEP);
        assert_eq!(ib.notices[0].text, (KEEP + KEEP / 4 - 1).to_string());
        let lines = std::fs::read_to_string(p.notices_dir().join("notices.jsonl"))
            .unwrap()
            .lines()
            .count();
        assert!(lines <= KEEP + KEEP / 4, "줄이지 않았다: {lines}");
    }
}
