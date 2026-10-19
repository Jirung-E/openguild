//! DEV-443: 개인 형광펜의 **진리원** — `.guild/highlights/highlights.json`.
//!
//! 읽는 화면에서 글을 골라 칠한 표시다. **본문은 건드리지 않는다**(admin) — 고친 기록에도,
//! git 에도 남지 않는다. 그래서 저장 클래스는 보드 위치(`positions.json`)와 같은 "로컬 UI
//! 상태" 다. 다만 위치와 달리 **백업에는 들어간다**(admin: 메모처럼) — 칠해 둔 것은 잃으면
//! 다시 만들 수 없다.
//!
//! # 대상 키
//!
//! `quest:DEV-001` · `campaign:C-001` · `rule:{slug}` · `library:BOOK-001`. 퀘스트 타입이
//! 바뀌거나 규칙 이름이 바뀌면 키를 옮긴다([`rename_targets`]). 도서관은 문서 번호로 잡아서
//! 폴더를 옮겨도 그대로다.
//!
//! # 자리 따라가기
//!
//! 글자 위치가 아니라 **칠한 글 + 앞뒤 몇 글자**를 적는다(W3C TextQuoteSelector 방식).
//! 본문이 바뀌어도 화면이 같은 글을 다시 찾아 칠한다. 찾는 일은 화면의 몫이다 — 여기서는
//! 적어 둘 뿐이다.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::GuildPaths;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Highlight {
    pub id: String,
    /// 대상 안의 어느 부분인가 — `body` · `memo` · `comment:{번호}`.
    pub part: String,
    /// 칠한 글 그대로.
    pub exact: String,
    /// 칠한 글 바로 앞 몇 글자 — 같은 글이 여러 번 나올 때 어느 것인지 가린다.
    #[serde(default)]
    pub prefix: String,
    /// 칠한 글 바로 뒤 몇 글자.
    #[serde(default)]
    pub suffix: String,
    pub created_at: String,
}

/// 대상 키 → 형광펜 목록. `BTreeMap` 이라 파일이 키 순으로 안정적으로 써진다.
pub type Highlights = BTreeMap<String, Vec<Highlight>>;

/// 파일을 읽는다. 없으면 빈 것.
///
/// 깨진 파일은 **오류**다. 빈 상태로 갈음하면 다음 쓰기가 그걸로 덮어써서 칠한 것이 통째로
/// 날아간다 — `positions.json` 과 같은 이유다.
pub fn read(paths: &GuildPaths) -> Result<Highlights> {
    let p = paths.highlights_json();
    let raw = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Highlights::new()),
        Err(e) => return Err(e).with_context(|| format!("failed to read {}", p.display())),
    };
    if raw.trim().is_empty() {
        return Ok(Highlights::new());
    }
    serde_json::from_str(&raw).with_context(|| {
        format!(
            "{} 를 읽을 수 없습니다 — 덮어쓰면 형광펜이 전부 사라지므로 멈춥니다. \
             파일을 고치거나 지운 뒤 다시 시도하세요",
            p.display()
        )
    })
}

/// 통째로 쓴다(원자적). 빈 대상은 지운다. 폴더와 그 안의 `.gitignore` 도 만든다.
pub fn write(paths: &GuildPaths, all: &Highlights) -> Result<()> {
    let dir = paths.highlights_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "*\n")
            .with_context(|| format!("failed to write {}", ignore.display()))?;
    }
    let kept: Highlights = all
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let body = serde_json::to_string_pretty(&kept).context("serialize highlights")?;
    super::fs::write_atomic(paths.highlights_json(), &body)
}

/// 키가 바뀐 대상의 형광펜을 옮긴다(퀘스트 타입 변경 · 규칙 이름 변경).
///
/// 두 이름이 서로 바뀌는 경우(`A→B`, `B→A`)도 안전하게 먼저 전부 떼어 낸 뒤 넣는다.
/// 옮겨 갈 자리에 이미 있으면 뒤에 붙인다 — 버리지 않는다.
pub fn rename_targets(paths: &GuildPaths, renames: &[(String, String)]) -> Result<()> {
    let renames: Vec<_> = renames.iter().filter(|(a, b)| a != b).collect();
    if renames.is_empty() || !paths.highlights_json().exists() {
        return Ok(());
    }
    let mut all = read(paths)?;
    let moved: Vec<(String, Vec<Highlight>)> = renames
        .iter()
        .filter_map(|(from, to)| all.remove(from).map(|v| (to.clone(), v)))
        .collect();
    if moved.is_empty() {
        return Ok(());
    }
    for (to, v) in moved {
        all.entry(to).or_default().extend(v);
    }
    write(paths, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> GuildPaths {
        let dir = std::env::temp_dir().join(format!(
            "og-hl-{}-{}",
            std::process::id(),
            crate::test_seq()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".guild")).unwrap();
        GuildPaths::new(&dir)
    }

    fn hl(id: &str) -> Highlight {
        Highlight {
            id: id.into(),
            part: "body".into(),
            exact: "글".into(),
            prefix: String::new(),
            suffix: String::new(),
            created_at: "2026-10-19T00:00:00+09:00".into(),
        }
    }

    #[test]
    fn the_folder_keeps_itself_out_of_git() {
        let p = fresh();
        let mut all = Highlights::new();
        all.insert("quest:DEV-001".into(), vec![hl("a")]);
        write(&p, &all).unwrap();
        let ig = std::fs::read_to_string(p.highlights_dir().join(".gitignore")).unwrap();
        assert_eq!(ig.trim(), "*");
        assert_eq!(read(&p).unwrap(), all);
    }

    #[test]
    fn a_missing_file_reads_as_empty_but_a_corrupt_one_is_an_error() {
        let p = fresh();
        assert!(read(&p).unwrap().is_empty());
        std::fs::create_dir_all(p.highlights_dir()).unwrap();
        std::fs::write(p.highlights_json(), "{ not json").unwrap();
        assert!(read(&p).is_err());
    }

    #[test]
    fn renaming_moves_and_swaps_safely() {
        let p = fresh();
        let mut all = Highlights::new();
        all.insert("quest:DEV-001".into(), vec![hl("a")]);
        all.insert("quest:BUG-001".into(), vec![hl("b")]);
        write(&p, &all).unwrap();
        rename_targets(
            &p,
            &[
                ("quest:DEV-001".into(), "quest:BUG-001".into()),
                ("quest:BUG-001".into(), "quest:DEV-001".into()),
            ],
        )
        .unwrap();
        let got = read(&p).unwrap();
        assert_eq!(got["quest:BUG-001"][0].id, "a");
        assert_eq!(got["quest:DEV-001"][0].id, "b");
    }
}
