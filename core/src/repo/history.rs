//! DEV-180: quest_history 사이드카 — `.guild/history/{slug}.jsonl`.
//!
//! quest_history 는 그동안 index.db **전용** 권위 데이터였다 — "index.db 는
//! 재생성 가능한 캐시" 원칙을 깨는 유일한 예외라, index.db 를 지우면(새 clone /
//! 캐시 초기화) 변경 이력이 영구 소실됐다. 본 모듈이 파일을 진리원으로 승격:
//!
//! - **포맷**: 한 줄 = 한 이벤트 JSON (`{"ts","op","old","new"}`), append-only.
//!   slug 는 파일명이 담당.
//! - **위치**: `.guild/quests/` 가 아닌 최상위 `.guild/history/` 전용 디렉토리.
//!   quests/ 는 이미 `.comments.md`/`.memo.md`/`.attachments.json` 사이드카로
//!   퀘스트당 파일이 4개인데, 여기에 history 까지 더하면 디렉토리가 난잡해짐
//!   (실사고: git diff 에 미커밋 사이드카 278개가 한꺼번에 쏟아짐) — backups/
//!   처럼 최상위에 분리해 quests/ 는 그대로 둔다.
//! - **쓰기**: history 를 남기는 mutation(change_status / change_type)이
//!   DB INSERT 와 동시에 append. change_type 은 `.md` 처럼 사이드카도 rename.
//! - **읽기/복구**: reindex 가 사이드카 → quest_history 재구축. 사이드카가
//!   없는데 DB 에 행이 있으면(DEV-180 이전 데이터) DB → 사이드카로 일회성
//!   export (자가 치유 마이그레이션 — 기존 이력 보존).
//! - git **tracked** (감사 기록 — 개인 메모가 아님).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::GuildPaths;

/// 사이드카 한 줄 = 한 이벤트.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    /// 발생 시각 (로컬 ISO8601 — quest_history.ts 와 동일 포맷).
    pub ts: String,
    /// 이벤트 종류 (`change_status` / `change_type` ...).
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
}

/// `.guild/history/{slug}.jsonl` 경로.
pub fn history_path(paths: &GuildPaths, slug: &str) -> PathBuf {
    paths.quest_history_sidecar_path(slug)
}

/// DEV-288: 편의 함수 — 현재 시각으로 이벤트 1건 기록. best-effort(실패해도
/// mutation 은 진행 — 활동 기록은 부가 정보이지 진리원이 아님). 규칙/도서관
/// 등 quest/campaign 외 엔티티의 활동 기록에 사용.
pub fn record(paths: &GuildPaths, slug: &str, op: &str, old: Option<String>, new: Option<String>) {
    let entry = HistoryEntry {
        ts: crate::time::now_local_iso8601(),
        op: op.to_string(),
        old,
        new,
    };
    if let Err(e) = append(paths, slug, &entry) {
        tracing::warn!("history 기록 실패 ({slug}, {op}): {e:#}");
    }
}

/// 이벤트 1건 append (파일 없으면 생성; 디렉토리도 최초 1회 생성).
pub fn append(paths: &GuildPaths, slug: &str, entry: &HistoryEntry) -> Result<()> {
    let path = history_path(paths, slug);
    std::fs::create_dir_all(paths.history_dir())
        .with_context(|| format!("history 디렉토리 생성 실패: {}", paths.history_dir().display()))?;
    let line = serde_json::to_string(entry).context("history entry 직렬화")?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("history 사이드카 열기 실패: {}", path.display()))?;
    writeln!(f, "{line}").with_context(|| format!("history append 실패: {}", path.display()))?;
    Ok(())
}

/// 사이드카 전체 읽기 (파일 없으면 빈 vec). 깨진 줄은 skip (fail-soft —
/// 감사 데이터라 일부 손상이 전체 이력을 막으면 안 됨).
pub fn read_all(path: &Path) -> Result<Vec<HistoryEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("history 사이드카 읽기 실패: {}", path.display()))?;
    Ok(content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect())
}

/// change_type 의 slug rename cascade — `.md` 와 동일하게 사이드카도 이동.
/// 원본이 없으면 no-op.
pub fn rename(paths: &GuildPaths, old_slug: &str, new_slug: &str) -> Result<()> {
    let old = history_path(paths, old_slug);
    if !old.exists() {
        return Ok(());
    }
    let new = history_path(paths, new_slug);
    std::fs::rename(&old, &new).with_context(|| {
        format!(
            "history 사이드카 rename 실패: {} → {}",
            old.display(),
            new.display()
        )
    })?;
    Ok(())
}

/// `.guild/history/` 안의 모든 사이드카 → (slug, path) 목록.
pub fn list_sidecars(paths: &GuildPaths) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(paths.history_dir()) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?.to_string();
            let slug = name.strip_suffix(".jsonl")?.to_string();
            Some((slug, p))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_read_roundtrip_and_rename() {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-hist-{ns}"));
        std::fs::create_dir_all(dir.join(".guild/history")).unwrap();
        let paths = GuildPaths::new(&dir);

        let e1 = HistoryEntry {
            ts: "2026-07-05T00:00:00+09:00".into(),
            op: "change_status".into(),
            old: Some("open".into()),
            new: Some("testing".into()),
        };
        append(&paths, "DEV-001", &e1).unwrap();
        let e2 = HistoryEntry {
            ts: "2026-07-05T00:01:00+09:00".into(),
            op: "change_type".into(),
            old: Some("DEV-001".into()),
            new: Some("BUG-001".into()),
        };
        append(&paths, "DEV-001", &e2).unwrap();

        let got = read_all(&history_path(&paths, "DEV-001")).unwrap();
        assert_eq!(got, vec![e1.clone(), e2.clone()]);

        // rename cascade.
        rename(&paths, "DEV-001", "BUG-001").unwrap();
        assert!(!history_path(&paths, "DEV-001").exists());
        assert_eq!(read_all(&history_path(&paths, "BUG-001")).unwrap().len(), 2);

        // 깨진 줄 fail-soft.
        std::fs::write(
            history_path(&paths, "DEV-002"),
            "not json\n{\"ts\":\"t\",\"op\":\"change_status\"}\n",
        )
        .unwrap();
        assert_eq!(read_all(&history_path(&paths, "DEV-002")).unwrap().len(), 1);

        // list_sidecars.
        let mut slugs: Vec<String> = list_sidecars(&paths).into_iter().map(|(s, _)| s).collect();
        slugs.sort();
        assert_eq!(slugs, vec!["BUG-001", "DEV-002"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
