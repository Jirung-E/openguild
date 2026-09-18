//! DEV-409: 스크립트가 쓰는 **길드 정보** — 상태·타입의 화면 이름, 길드 이름, 링크.
//!
//! # 왜 필요한가
//!
//! 이벤트에는 `status = "in_progress"` 처럼 **슬러그**가 실린다. 사람에게 보낼 글에 그대로
//! 쓰면 "in_progress 로 바뀜" 이 된다. 화면 이름("진행 중")은 `.guild/statuses/*.toml` 에
//! 있는데, 스크립트는 파일을 못 읽는다(샌드박스). 그래서 **코어가 읽어 주는 함수**로 준다.
//!
//! ```rhai
//! fn on_done(e) {
//!     notify(`${guild_name()} — ${e.quest.id} ${status_name(e.change.to)}`);
//! }
//! ```
//!
//! # 읽기 전용이고, 게으르다
//!
//! 여기서 하는 일은 `.guild/` 안의 작은 TOML 몇 개를 **읽는 것**뿐이다. 쓰지도, 밖으로 나가지도
//! 않는다. 적재 때 미리 읽으면 그 뒤에 상태를 고쳐도 옛 이름이 나오므로, 처음 부를 때 읽고
//! [`CACHE_TTL`] 동안만 재사용한다 — 반복문에서 백 번 불러도 디스크를 백 번 읽지 않는다.

use crate::repo::GuildPaths;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// 읽어 둔 이름을 이 시간 동안 재사용한다. 상태 이름이 바뀌는 일은 드물고, 바뀌어도 이만큼
/// 뒤에는 따라간다.
const CACHE_TTL: Duration = Duration::from_secs(5);

/// 링크의 앞부분. 웹으로 여는 길드면 사용자가 알려 준다 — 우리는 서버가 어떤 주소로 열려
/// 있는지 알 수 없다(터널·역프록시·포트 바꿔 열기).
pub const WEB_BASE_ENV: &str = "OPENGUILD_WEB_BASE";

/// 한 플러그인이 보는 길드. 스크립트 한 벌마다 하나씩 있고, 이벤트마다 재사용한다.
#[derive(Debug, Default)]
pub struct GuildInfo {
    root: Option<PathBuf>,
    cache: Option<Cache>,
    at: Option<Instant>,
}

#[derive(Debug, Clone, Default)]
struct Cache {
    name: String,
    /// 상태 슬러그 → (한국어, 영어).
    statuses: BTreeMap<String, (String, String)>,
    /// 타입 접두어 → 설명.
    types: BTreeMap<String, String>,
}

impl GuildInfo {
    /// 어느 길드를 보나. 적재 때 한 번 정한다.
    pub fn set_root(&mut self, root: &Path) {
        if self.root.as_deref() != Some(root) {
            self.root = Some(root.to_path_buf());
            self.cache = None;
            self.at = None;
        }
    }

    /// 길드 이름 — `{이름}.guild` 파일이 있으면 그 이름, 없으면 폴더 이름.
    pub fn guild_name(&mut self) -> String {
        self.fresh().name.clone()
    }

    /// 상태의 화면 이름. `lang` 은 `"ko"`/`"en"`, 없으면 지금 언어 설정. 모르는 슬러그는
    /// 슬러그 그대로 돌려준다 — 글 안에서 빈칸이 되는 것보다 낫다.
    pub fn status_name(&mut self, slug: &str, lang: Option<&str>) -> String {
        let en = match lang {
            Some(l) => l.eq_ignore_ascii_case("en"),
            None => crate::locale::current() == crate::locale::Locale::En,
        };
        match self.fresh().statuses.get(slug) {
            Some((ko, name_en)) => {
                let picked = if en { name_en } else { ko };
                if picked.is_empty() { slug.to_string() } else { picked.clone() }
            }
            None => slug.to_string(),
        }
    }

    /// 타입의 설명(`DEV` → "일반 개발 작업"). 설명이 없으면 접두어 그대로.
    pub fn type_name(&mut self, prefix: &str) -> String {
        match self.fresh().types.get(prefix) {
            Some(d) if !d.is_empty() => d.clone(),
            _ => prefix.to_string(),
        }
    }

    /// 문서로 가는 주소. `OPENGUILD_WEB_BASE` 가 있으면 그 앞부분을 붙이고, 없으면 앱 안의
    /// 경로만 돌려준다(`/quests/DEV-409`). 모르는 종류는 빈 글자.
    pub fn link(&mut self, kind: &str, id: &str) -> String {
        let path = match kind {
            "quest" => format!("/quests/{id}"),
            "campaign" => format!("/campaigns/{id}"),
            // 도서관·규칙은 전용 화면이 없다 — 목록에서 그 문서를 고른 주소로 보낸다.
            "book" => format!("/library?id={id}"),
            "rule" => format!("/rules?id={id}"),
            _ => return String::new(),
        };
        match std::env::var(WEB_BASE_ENV) {
            Ok(base) if !base.trim().is_empty() => {
                format!("{}{path}", base.trim().trim_end_matches('/'))
            }
            _ => path,
        }
    }

    /// 너무 오래됐으면 다시 읽는다.
    fn fresh(&mut self) -> &Cache {
        let stale = match self.at {
            Some(t) => t.elapsed() > CACHE_TTL,
            None => true,
        };
        if stale {
            self.cache = Some(read(self.root.as_deref()));
            self.at = Some(Instant::now());
        }
        self.cache.get_or_insert_with(Cache::default)
    }
}

/// 길드에서 이름들을 읽는다. 길드가 없거나 파일이 깨졌으면 빈 값 — 이름을 못 읽었다고 이벤트
/// 전달이 실패하면 안 된다.
fn read(root: Option<&Path>) -> Cache {
    let Some(root) = root else {
        return Cache::default();
    };
    let paths = GuildPaths::new(root);
    let mut c = Cache {
        name: crate::recents::guess_name(root),
        ..Cache::default()
    };
    if let Ok(entries) = std::fs::read_dir(paths.statuses_dir()) {
        for e in entries.flatten() {
            let path = e.path();
            let Some(file) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(slug) = crate::repo::status_def::StatusFile::slug_from_filename(file) else {
                continue;
            };
            if let Ok(s) = crate::repo::status_def::StatusFile::read(&path) {
                c.statuses.insert(slug.to_string(), (s.name_ko, s.name_en));
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(paths.types_dir()) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            if let Ok(t) = crate::repo::type_def::TypeFile::read(&path) {
                c.types.insert(t.prefix, t.description.unwrap_or_default());
            }
        }
    }
    c
}

/// 글을 `n` 글자로 자른다(넘치면 `…`). 글자 수는 **문자 단위**다 — 한글 한 자가 3바이트라고
/// 잘라 버리면 깨진 글자가 나간다.
pub fn truncate(s: &str, n: i64) -> String {
    let n = n.max(0) as usize;
    if s.chars().count() <= n {
        return s.to_string();
    }
    if n == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// 마크다운에서 표시용 글만 남긴다 — 코드블록·링크·제목 기호를 덜어낸다. 완전한 파서가 아니라
/// **알림 한 줄에 쓸 만큼**만 한다(마크다운을 못 읽는 곳에 보낼 때).
pub fn plain_text(md: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in md.lines() {
        let t = line.trim_end();
        if t.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let mut s = t.trim_start();
        // 제목·인용·목록 기호.
        s = s.trim_start_matches('#').trim_start_matches('>').trim_start();
        if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
            s = rest;
        }
        let line = strip_inline(s);
        if line.trim().is_empty() {
            if !out.ends_with('\n') && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        out.push_str(line.trim());
        out.push('\n');
    }
    out.trim().to_string()
}

/// 줄 안의 표시 기호 — `[글](주소)` → `글`, `` `코드` `` → `코드`, `**굵게**` → `굵게`.
fn strip_inline(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        let (before, after) = rest.split_at(i);
        out.push_str(before);
        // `[글](주소)` 모양일 때만 링크로 본다.
        let Some(close) = after.find(']') else {
            out.push_str(after);
            return clean_marks(&out);
        };
        let text = &after[1..close];
        let tail = &after[close + 1..];
        if let Some(paren) = tail.strip_prefix('(')
            && let Some(end) = paren.find(')')
        {
            out.push_str(text);
            rest = &paren[end + 1..];
        } else {
            out.push_str(&after[..=close]);
            rest = tail;
        }
    }
    out.push_str(rest);
    clean_marks(&out)
}

fn clean_marks(s: &str) -> String {
    s.replace("**", "").replace('`', "").replace("~~", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guild(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("og-info-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".guild/statuses")).unwrap();
        std::fs::create_dir_all(dir.join(".guild/types")).unwrap();
        std::fs::write(
            dir.join(".guild/statuses/2-in_progress.toml"),
            "sort_order = 2\nname_en = \"In Progress\"\nname_ko = \"진행 중\"\ncolor = \"#fff\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(".guild/types/DEV.toml"),
            "prefix = \"DEV\"\ncolor = \"#fff\"\ndescription = \"일반 개발 작업\"\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn reads_display_names_from_guild_files() {
        let dir = guild("names");
        let mut info = GuildInfo::default();
        info.set_root(&dir);
        assert_eq!(info.status_name("in_progress", Some("ko")), "진행 중");
        assert_eq!(info.status_name("in_progress", Some("en")), "In Progress");
        assert_eq!(info.type_name("DEV"), "일반 개발 작업");
        assert_eq!(info.guild_name(), dir.file_name().unwrap().to_str().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_slug_comes_back_as_is() {
        let dir = guild("unknown");
        let mut info = GuildInfo::default();
        info.set_root(&dir);
        // 모르는 것이 빈칸이 되면 글이 "상태:  로 바뀜" 이 된다.
        assert_eq!(info.status_name("no_such", None), "no_such");
        assert_eq!(info.type_name("ZZZ"), "ZZZ");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_guild_is_not_an_error() {
        let mut info = GuildInfo::default();
        assert_eq!(info.status_name("done", Some("ko")), "done");
        assert_eq!(info.guild_name(), "");
    }

    #[test]
    fn link_is_a_path_without_a_web_base() {
        let mut info = GuildInfo::default();
        assert_eq!(info.link("quest", "DEV-409"), "/quests/DEV-409");
        assert_eq!(info.link("campaign", "spring"), "/campaigns/spring");
        assert_eq!(info.link("tag", "x"), "");
    }

    #[test]
    fn truncate_counts_characters() {
        // 바이트로 자르면 한글이 깨진다.
        assert_eq!(truncate("가나다라마", 3), "가나…");
        assert_eq!(truncate("가나", 5), "가나");
        assert_eq!(truncate("abc", 0), "");
    }

    #[test]
    fn plain_text_drops_markup() {
        let md = "# 제목\n\n- [링크](http://x) 와 `코드` 그리고 **굵게**\n\n```\nfn x() {}\n```\n끝";
        assert_eq!(plain_text(md), "제목\n링크 와 코드 그리고 굵게\n끝");
    }
}
