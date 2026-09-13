//! BUG-286: 보드 위치의 **진리원** — `.guild/positions.json`.
//!
//! # 왜 파일인가
//!
//! [[BOOK-001]] 은 저장 클래스를 셋으로 나누고 위치의 자리를 이미 정해 뒀다.
//!
//! - **tracked 진리** — `.guild/quests|…`, git 이 관리.
//! - **폐기가능 캐시** — `.guild/index.db`, 파일에서 언제든 재구축.
//! - **로컬 UI 상태** — `.guild/positions.json`, gitignored, 개인 로컬.
//!
//! 그런데 위치는 **캐시(`index.db`)에만** 있었고 이 파일은 경로만 선언돼 아무도 안
//! 읽고 안 썼다. 그래서 `rm .guild/index.db && openguild reindex` — BOOK-001 이
//! "회피책이 아니라 불변식의 일부" 라 하고 릴리스 규칙이 브랜치 전환마다 실행하라는
//! 명령 — 를 돌릴 때마다 **사용자의 보드 배치가 전부 사라졌다.**
//!
//! # 키는 slug
//!
//! 정수 id 는 reindex 가 재배정한다. slug(`DEV-001`)는 파일 이름과 같아 살아남는다.
//! 대신 퀘스트 타입이 바뀌어 slug 가 바뀌면 키를 옮겨야 한다([`rename_keys`]).
//!
//! # 커밋되지 않는다
//!
//! `.guild/.gitignore` 에 있다. 사람마다 보드를 다르게 배치하는 개인 UI 상태라, git
//! 으로 공유하면 한 사람이 옮길 때마다 남의 배치가 바뀐다.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::GuildPaths;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pos {
    pub x: f64,
    pub y: f64,
}

/// slug → 위치. `BTreeMap` 이라 파일이 slug 순으로 안정적으로 써진다 — 한 칸 옮겼는데
/// 파일 전체 순서가 뒤섞이면 사람이 열어 봤을 때 무엇이 바뀌었는지 못 본다.
pub type Positions = BTreeMap<String, Pos>;

/// 파일을 읽는다. **없으면 `None`** — "비어 있다" 와 "아직 만든 적 없다" 는 다르다.
/// 후자일 때만 옛 `index.db` 의 위치를 옮겨 온다([`crate::reindex`]).
///
/// 깨진 파일은 **오류**다. 빈 상태로 갈음하면 다음 쓰기가 그걸로 덮어써서 배치가
/// 통째로 날아간다 — [[DEV-385]] 가 동의 파일에서 겪은 것과 같다.
pub fn read(paths: &GuildPaths) -> Result<Option<Positions>> {
    let p = paths.positions_json();
    let raw = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("failed to read {}", p.display())),
    };
    if raw.trim().is_empty() {
        return Ok(Some(Positions::new()));
    }
    let parsed = serde_json::from_str(&raw).with_context(|| {
        format!(
            "{} 를 읽을 수 없습니다 — 덮어쓰면 보드 배치가 전부 사라지므로 멈춥니다. \
             파일을 고치거나 지운 뒤 다시 시도하세요",
            p.display()
        )
    })?;
    Ok(Some(parsed))
}

/// 통째로 쓴다(원자적 — 쓰다 죽어도 예전 파일이 남는다).
pub fn write(paths: &GuildPaths, positions: &Positions) -> Result<()> {
    let body = serde_json::to_string_pretty(positions).context("serialize positions")?;
    super::fs::write_atomic(paths.positions_json(), &body)
}

/// 여러 위치를 한 번에 넣는다 — **파일은 한 번만 쓴다.** 보드가 자동 배치 노드를
/// 고정할 때([[BUG-284]]) 수백 개가 오는데, 건마다 파일을 다시 쓰면 그만큼 디스크를
/// 친다.
pub fn upsert(paths: &GuildPaths, items: &[(String, Pos)]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let mut all = read(paths)?.unwrap_or_default();
    for (slug, pos) in items {
        all.insert(slug.clone(), *pos);
    }
    write(paths, &all)
}

/// slug 가 바뀐 퀘스트의 키를 옮긴다. 퀘스트 타입을 바꾸면(`DEV-001` → `BUG-005`)
/// 파일의 키도 따라가야 한다 — 안 그러면 다음 reindex 가 옛 slug 로 찾다가 못 찾아
/// 위치를 버린다.
///
/// 두 이름이 서로 바뀌는 경우(`A→B`, `B→A`)도 안전하게 먼저 전부 떼어 낸 뒤 넣는다.
pub fn rename_keys(paths: &GuildPaths, renames: &[(String, String)]) -> Result<()> {
    let renames: Vec<_> = renames.iter().filter(|(a, b)| a != b).collect();
    if renames.is_empty() {
        return Ok(());
    }
    let Some(mut all) = read(paths)? else {
        return Ok(()); // 파일이 없으면 옮길 것도 없다.
    };
    let moved: Vec<(String, Pos)> = renames
        .iter()
        .filter_map(|(from, to)| all.remove(from).map(|p| (to.clone(), p)))
        .collect();
    if moved.is_empty() {
        return Ok(());
    }
    for (to, p) in moved {
        all.insert(to, p);
    }
    write(paths, &all)
}
