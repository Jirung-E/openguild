//! DEV-443: 개인 형광펜 — 목록 · 칠하기 · 지우기.
//!
//! 진리원은 [`crate::repo::highlights`] 의 파일 하나다. DB 에는 비추지 않는다 — 한 화면이 대상
//! 하나의 목록을 통째로 읽을 뿐이라 색인할 것이 없다.
//!
//! 여기서 하는 일은 **받은 값을 따져 보고 적는 것**까지다. 칠할 자리를 찾는 일(글이 바뀌었을 때
//! 같은 글을 다시 찾기)은 화면이 한다.

use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::lock::DocKey;
use crate::repo::highlights::{self as file, Highlight};
use crate::store::Store;

/// 칠한 글의 최대 길이(글자). 문서 한 편을 통째로 칠하는 것은 형광펜이 아니다.
pub const MAX_EXACT: usize = 2000;
/// 앞뒤로 적어 두는 글자 수의 상한. 화면은 30자 안팎을 보낸다.
pub const MAX_CONTEXT: usize = 64;

#[derive(Debug, Clone, Deserialize)]
pub struct NewHighlight {
    pub part: String,
    pub exact: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
}

/// `quest:DEV-001` · `campaign:C-001` · `rule:{slug}` · `library:BOOK-001`.
fn check_target(target: &str) -> AppResult<()> {
    let ok = match target.split_once(':') {
        Some((kind, id)) => {
            matches!(kind, "quest" | "campaign" | "rule" | "library")
                && !id.trim().is_empty()
                && id.chars().count() <= 200
                && !id.contains(['\n', '\r'])
        }
        None => false,
    };
    if ok {
        Ok(())
    } else {
        Err(AppError::BadRequest(crate::tf!(
            "형광펜 대상이 올바르지 않습니다: {target}",
            "invalid highlight target: {target}"
        )))
    }
}

/// `body` · `memo` · `comment:{번호}`.
fn check_part(part: &str) -> AppResult<()> {
    let ok = match part {
        "body" | "memo" => true,
        _ => part
            .strip_prefix("comment:")
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())),
    };
    if ok {
        Ok(())
    } else {
        Err(AppError::BadRequest(crate::tf!(
            "형광펜 위치가 올바르지 않습니다: {part}",
            "invalid highlight part: {part}"
        )))
    }
}

/// 앞뒤 글자는 길면 **칠한 글 쪽**을 남기고 자른다 — 가까운 글자가 자리를 가린다.
fn clip_prefix(s: &str) -> String {
    let n = s.chars().count();
    s.chars().skip(n.saturating_sub(MAX_CONTEXT)).collect()
}
fn clip_suffix(s: &str) -> String {
    s.chars().take(MAX_CONTEXT).collect()
}

/// 겹치지 않는 새 id — 시각(나노초) + 같은 프로세스 안의 순번.
fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{ns:x}{:x}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// 대상 하나의 형광펜 — 칠한 순서대로.
pub async fn list_highlights(store: &Store, target: &str) -> AppResult<Vec<Highlight>> {
    check_target(target)?;
    let all = file::read(&store.paths).map_err(AppError::Internal)?;
    Ok(all.get(target).cloned().unwrap_or_default())
}

/// 칠한다.
pub async fn add_highlight(
    store: &Store,
    target: &str,
    body: NewHighlight,
) -> AppResult<Highlight> {
    check_target(target)?;
    check_part(&body.part)?;
    if body.exact.trim().is_empty() {
        return Err(AppError::BadRequest(crate::tf!(
            "칠할 글이 비어 있습니다",
            "nothing to highlight"
        )));
    }
    if body.exact.chars().count() > MAX_EXACT {
        return Err(AppError::BadRequest(crate::tf!(
            "한 번에 {MAX_EXACT}자까지 칠할 수 있습니다",
            "a highlight can be at most {MAX_EXACT} characters"
        )));
    }
    let _g = store.lock_docs(&[DocKey::single("highlights")]).await?;
    let mut all = file::read(&store.paths).map_err(AppError::Internal)?;
    let h = Highlight {
        id: new_id(),
        part: body.part,
        exact: body.exact,
        prefix: clip_prefix(&body.prefix),
        suffix: clip_suffix(&body.suffix),
        created_at: crate::time::now_local_iso8601(),
    };
    all.entry(target.to_string()).or_default().push(h.clone());
    file::write(&store.paths, &all).map_err(AppError::Internal)?;
    Ok(h)
}

/// 지운다. 없던 것이면 `false` — 다른 창에서 먼저 지웠을 수 있으니 오류로 보지 않는다.
pub async fn remove_highlight(store: &Store, target: &str, id: &str) -> AppResult<bool> {
    check_target(target)?;
    let _g = store.lock_docs(&[DocKey::single("highlights")]).await?;
    let mut all = file::read(&store.paths).map_err(AppError::Internal)?;
    let Some(list) = all.get_mut(target) else {
        return Ok(false);
    };
    let before = list.len();
    list.retain(|h| h.id != id);
    if list.len() == before {
        return Ok(false);
    }
    file::write(&store.paths, &all).map_err(AppError::Internal)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChangeTypeRequest, CreateQuestRequest};

    fn fresh_tmp() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "og-hlops-{}-{}",
            std::process::id(),
            crate::test_seq()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn open(dir: &std::path::Path) -> Store {
        let s = Store::open(dir).await.unwrap();
        crate::reindex::reindex(&s).await.unwrap();
        s
    }

    fn mark(part: &str, exact: &str) -> NewHighlight {
        NewHighlight {
            part: part.into(),
            exact: exact.into(),
            prefix: "앞".into(),
            suffix: "뒤".into(),
        }
    }

    #[tokio::test]
    async fn add_list_remove() {
        let dir = fresh_tmp();
        let store = open(&dir).await;
        let a = add_highlight(&store, "quest:DEV-001", mark("body", "중요한 글"))
            .await
            .unwrap();
        let b = add_highlight(&store, "quest:DEV-001", mark("comment:3", "답"))
            .await
            .unwrap();
        add_highlight(&store, "rule:코딩 규칙", mark("body", "규칙"))
            .await
            .unwrap();
        let got = list_highlights(&store, "quest:DEV-001").await.unwrap();
        assert_eq!(
            got.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            [a.id.as_str(), b.id.as_str()]
        );
        assert_eq!(got[0].prefix, "앞");
        assert!(
            remove_highlight(&store, "quest:DEV-001", &a.id)
                .await
                .unwrap()
        );
        assert!(
            !remove_highlight(&store, "quest:DEV-001", &a.id)
                .await
                .unwrap(),
            "두 번째는 없던 것"
        );
        assert_eq!(
            list_highlights(&store, "quest:DEV-001")
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            list_highlights(&store, "rule:코딩 규칙")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn bad_input_is_refused() {
        let dir = fresh_tmp();
        let store = open(&dir).await;
        for t in ["", "quest:", "board:x", "DEV-001"] {
            assert!(list_highlights(&store, t).await.is_err(), "{t}");
        }
        for p in ["", "head", "comment:", "comment:x"] {
            assert!(
                add_highlight(&store, "quest:DEV-001", mark(p, "글"))
                    .await
                    .is_err(),
                "{p}"
            );
        }
        assert!(
            add_highlight(&store, "quest:DEV-001", mark("body", "  "))
                .await
                .is_err()
        );
        let long = "가".repeat(MAX_EXACT + 1);
        assert!(
            add_highlight(&store, "quest:DEV-001", mark("body", &long))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn long_context_keeps_the_side_next_to_the_mark() {
        let dir = fresh_tmp();
        let store = open(&dir).await;
        let h = add_highlight(
            &store,
            "quest:DEV-001",
            NewHighlight {
                part: "body".into(),
                exact: "X".into(),
                prefix: format!("{}끝", "a".repeat(100)),
                suffix: format!("시작{}", "b".repeat(100)),
            },
        )
        .await
        .unwrap();
        assert_eq!(h.prefix.chars().count(), MAX_CONTEXT);
        assert!(h.prefix.ends_with('끝'));
        assert!(h.suffix.starts_with("시작"));
    }

    /// 타입을 바꿔 번호가 바뀌어도 형광펜이 따라간다.
    #[tokio::test]
    async fn highlights_follow_a_quest_type_change() {
        let dir = fresh_tmp();
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = open(&dir).await;
        let q = crate::ops::quests::create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: "t".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let old = format!("quest:{}", q.quest_id);
        add_highlight(&store, &old, mark("body", "글"))
            .await
            .unwrap();
        let other = crate::services::meta::list_quest_types(&store.index_pool)
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.prefix != q.type_prefix)
            .unwrap();
        let moved = crate::ops::quests::change_quest_type(
            &store,
            q.id,
            ChangeTypeRequest {
                new_type_prefix: other.prefix.clone(),
            },
        )
        .await
        .unwrap();
        assert!(list_highlights(&store, &old).await.unwrap().is_empty());
        let new = format!("quest:{}", moved.quest_id);
        assert_eq!(list_highlights(&store, &new).await.unwrap().len(), 1);
    }
}
