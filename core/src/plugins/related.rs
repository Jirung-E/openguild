//! DEV-405: 핸들러 줄의 `with` — 이벤트 대상과 **연결된 데이터**를 읽어 함수 인자로 넘긴다.
//!
//! ```toml
//! [[handlers]]
//!     post = ["comment.added"]
//!     with = ["subject", "parent"]
//!     call = "on_comment"          # fn on_comment(e, subject, parent)
//! ```
//!
//! # 외울 것을 줄인다
//!
//! 이벤트마다 받을 수 있는 것을 따로 정하면 55개를 외워야 한다. 대신 모든 이벤트에 대상
//! ([`crate::events::Subject`])이 있으니, 받을 수 있는 것은 **대상의 종류**가 정한다. 문서
//! 종류 넷의 표([`RELATIONS`])만 알면 된다.
//!
//! # 파일에서 읽는다
//!
//! 전달은 mutation 이 끝난 뒤 전용 스레드에서 돈다. 그 자리에서 캐시(index.db)를 비동기로
//! 부르기보다 **진리원인 `.guild/` 파일**을 바로 읽는다 — 동기이고, 캐시가 늦어도 틀리지 않는다.
//! 대상이 이미 없으면(지워진 캠페인 등) `null` 이다.

use crate::events::Subject;
use crate::repo::GuildPaths;
use serde_json::{Value, json};
use std::path::Path;

/// 대상 종류 → 받을 수 있는 것. 넷 모두 `subject`(대상 자체의 최신 모양)를 받는다.
pub const RELATIONS: &[(&str, &[&str])] = &[
    ("quest", &["subject", "parent", "children", "prereqs", "campaigns"]),
    ("campaign", &["subject", "quests"]),
    ("book", &["subject"]),
    ("rule", &["subject"]),
];

/// 그 종류가 받을 수 있는 것.
pub fn relations_of(kind: &str) -> &'static [&'static str] {
    RELATIONS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, r)| *r)
        .unwrap_or(&[])
}

/// 이벤트 패턴들이 걸리는 이벤트 전부에서 받을 수 있는 것(합집합, 표의 순서). 대상 종류가 여럿일
/// 수 있는 이벤트(댓글·첨부)는 그 종류 모두를 더한다 — 실제 대상에 없는 것은 `null` 로 온다.
pub fn available_for(patterns: &[String], phase: crate::events::Phase) -> Vec<&'static str> {
    use crate::events::{Phase, names};
    let candidates: &[&str] = match phase {
        Phase::Pre => names::PRE_CAPABLE,
        Phase::Post => names::ALL,
    };
    let mut kinds: Vec<&str> = Vec::new();
    for n in candidates {
        let hit = patterns.iter().any(|p| {
            let full = match phase {
                Phase::Pre => format!("pre:{p}"),
                Phase::Post => p.clone(),
            };
            names::matches(&full, n, phase)
        });
        if hit {
            for k in crate::events::subject_kinds_of(n) {
                if !kinds.contains(k) {
                    kinds.push(k);
                }
            }
        }
    }
    let mut out: Vec<&'static str> = Vec::new();
    for (k, rels) in RELATIONS {
        if kinds.contains(k) {
            for r in *rels {
                if !out.contains(r) {
                    out.push(r);
                }
            }
        }
    }
    out
}

/// 연결된 것 하나. 그 대상에 없는 것(캠페인의 `parent`)이거나 읽지 못하면 `null`.
pub fn load(guild_root: &Path, subject: &Subject, what: &str) -> Value {
    let paths = GuildPaths::new(guild_root);
    let id = subject.id.as_str();
    match (subject.kind, what) {
        ("quest", "subject") => quest(&paths, id),
        ("quest", "parent") => read_quest(&paths, id)
            .and_then(|q| q.parent)
            .map(|p| quest(&paths, &p))
            .unwrap_or(Value::Null),
        ("quest", "children") => Value::Array(
            quest_files(&paths)
                .into_iter()
                .filter(|q| !q.deleted && q.parent.as_deref() == Some(id))
                .map(|q| quest_json(&q))
                .collect(),
        ),
        ("quest", "prereqs") => Value::Array(
            read_quest(&paths, id)
                .map(|q| q.prerequisites)
                .unwrap_or_default()
                .iter()
                .map(|p| quest(&paths, p))
                .filter(|v| !v.is_null())
                .collect(),
        ),
        ("quest", "campaigns") => Value::Array(
            campaign_files(&paths)
                .into_iter()
                .filter(|c| !c.deleted && c.linked_quests.iter().any(|q| q == id))
                .map(|c| campaign_json(&c))
                .collect(),
        ),
        ("campaign", "subject") => read_campaign(&paths, id)
            .map(|c| campaign_json(&c))
            .unwrap_or(Value::Null),
        ("campaign", "quests") => Value::Array(
            read_campaign(&paths, id)
                .map(|c| c.linked_quests)
                .unwrap_or_default()
                .iter()
                .map(|q| quest(&paths, q))
                .filter(|v| !v.is_null())
                .collect(),
        ),
        ("book", "subject") => crate::repo::library::BookFile::read(paths.book_path(id))
            .map(|b| {
                let f = b.frontmatter;
                json!({
                    "id": f.book_id,
                    "title": f.title,
                    "path": f.path,
                    "tags": f.tags,
                    "deleted": f.deleted,
                    "created_at": f.created_at,
                    "updated_at": f.updated_at,
                })
            })
            .unwrap_or(Value::Null),
        ("rule", "subject") => crate::repo::rules::read_rule_entry(&paths, id)
            .ok()
            .flatten()
            .map(|r| crate::events::payload::rule(&r))
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn read_quest(paths: &GuildPaths, id: &str) -> Option<crate::repo::quest::QuestFrontmatter> {
    crate::repo::quest::QuestFile::read(paths.quest_path(id))
        .ok()
        .map(|q| q.frontmatter)
}

fn quest(paths: &GuildPaths, id: &str) -> Value {
    read_quest(paths, id)
        .map(|q| quest_json(&q))
        .unwrap_or(Value::Null)
}

/// 이벤트의 `quest` 와 같은 칸에, 관계·기한·삭제 여부를 더한다.
fn quest_json(q: &crate::repo::quest::QuestFrontmatter) -> Value {
    let prefix = q.quest_id.split('-').next().unwrap_or_default();
    json!({
        "id": q.quest_id,
        "title": q.title,
        "type": prefix,
        "status": q.status,
        "urgency": q.urgency,
        "tags": q.tags,
        "parent": q.parent,
        "prerequisites": q.prerequisites,
        "desired_due": q.desired_due,
        "required_due": q.required_due,
        "deleted": q.deleted,
        "created_at": q.created_at,
        "updated_at": q.updated_at,
    })
}

/// 퀘스트 파일 전부(댓글·메모 등 옆 파일 제외).
fn quest_files(paths: &GuildPaths) -> Vec<crate::repo::quest::QuestFrontmatter> {
    doc_files(&paths.quests_dir())
        .into_iter()
        .filter_map(|p| crate::repo::quest::QuestFile::read(p).ok())
        .map(|q| q.frontmatter)
        .collect()
}

fn read_campaign(paths: &GuildPaths, id: &str) -> Option<crate::repo::campaign::CampaignFrontmatter> {
    crate::repo::campaign::CampaignFile::read(paths.campaign_path(id))
        .ok()
        .map(|c| c.frontmatter)
}

fn campaign_files(paths: &GuildPaths) -> Vec<crate::repo::campaign::CampaignFrontmatter> {
    doc_files(&paths.campaigns_dir())
        .into_iter()
        .filter_map(|p| crate::repo::campaign::CampaignFile::read(p).ok())
        .map(|c| c.frontmatter)
        .collect()
}

fn campaign_json(c: &crate::repo::campaign::CampaignFrontmatter) -> Value {
    json!({
        "id": c.campaign_id,
        "title": c.title,
        "status": c.status,
        "started_at": c.started_at,
        "ended_at": c.ended_at,
        "linked_quests": c.linked_quests,
        "deleted": c.deleted,
        "created_at": c.created_at,
        "updated_at": c.updated_at,
    })
}

/// `{ID}.md` 문서 파일들 — `DEV-001.comments.md` 같은 옆 파일은 뺀다(이름에 점이 더 있다).
fn doc_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|x| x.to_str()) == Some("md")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| !s.contains('.'))
        })
        .collect();
    out.sort();
    out
}
