//! DEV-167: 작업 기록(Worklog) — 날짜/기간별 활동 조회 + 날짜별 노트.
//!
//! **활동**은 새 저장소 없이 index.db 의 기존 캐시(quest_history /
//! quest_comments / campaign_comments / quests.created_at)를 조회만 한다.
//! journal 은 snapshot 시 truncate 라 소스에서 제외 (퀘스트 본문 결정).
//!
//! **노트**는 `.guild/worklog/{YYYY-MM-DD}.md` plain markdown — rules 와 같은
//! "파일 진리원, DB 캐시 없음" 패턴. 전역 공유(git tracked) — quest memo
//! (개인, gitignored)와 구분해 note 로 명명 (admin 결정).

use anyhow::{anyhow, Context};
use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::store::{journal, Store};

/// `YYYY-MM-DD` 형식 검증 — 파일명이 되므로 traversal 방지 겸.
pub fn validate_date(date: &str) -> AppResult<()> {
    let ok = date.len() == 10
        && date.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    if !ok {
        return Err(AppError::BadRequest(format!(
            "잘못된 날짜 형식 (YYYY-MM-DD): {date:?}"
        )));
    }
    Ok(())
}

// ─── 활동 조회 ───

/// 활동 한 건 — 타임라인 행.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ActivityRow {
    /// 발생 시각 (로컬 ISO8601).
    pub ts: String,
    /// "status"(상태변경) | "type"(타입변경) | "comment" | "created" |
    /// "discussion" | DEV-288: "rule" | "book"(문서 변경).
    pub kind: String,
    /// 소속 슬러그 (DEV-001 / C-001 / rule slug / BOOK-001).
    pub slug: String,
    /// 한 줄 표시용: status = "old → new", comment = "author: 본문(앞부분)",
    /// created = 제목.
    pub summary: String,
    /// DEV-296: 대상 식별자 — 지금은 **댓글 번호**(kind="comment"/"discussion").
    /// 작업기록에서 그 항목을 클릭하면 해당 댓글로 바로 스크롤하기 위해 필요하다
    /// (slug 만으로는 문서까지만 갈 수 있다). 그 외 kind 는 None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<i64>,
}

/// 종류별 집계 (타임라인과 함께 반환).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ActivityCounts {
    pub status_changes: i64,
    pub comments: i64,
    pub created: i64,
    /// 상태변경 중 done 으로의 전환 수.
    pub done_transitions: i64,
    /// DEV-236: 토론 댓글 resolve/reopen 전환 수.
    pub discussion_events: i64,
    /// DEV-288: 규칙·도서관 문서 변경(create/update/delete/rename) 수.
    #[serde(default)]
    pub doc_changes: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorklogReport {
    pub from: String,
    pub to: String,
    pub activities: Vec<ActivityRow>,
    pub counts: ActivityCounts,
}

/// 기간(`from`~`to`, 날짜 포함) 내 활동 타임라인 + 집계. 날짜 비교는 ts 의
/// 앞 10자(YYYY-MM-DD) — 저장 포맷이 로컬 ISO8601 로 일관되므로 안전.
pub async fn activities(store: &Store, from: &str, to: &str) -> AppResult<WorklogReport> {
    validate_date(from)?;
    validate_date(to)?;
    let rows = sqlx::query_as::<_, ActivityRow>(
        "SELECT * FROM (
           SELECT h.ts AS ts,
                  CASE h.op
                    WHEN 'change_status' THEN 'status'
                    WHEN 'discussion_resolved' THEN 'discussion'
                    WHEN 'discussion_reopened' THEN 'discussion'
                    ELSE 'type'
                  END AS kind,
                  h.quest_slug AS slug,
                  CASE h.op
                    WHEN 'discussion_resolved' THEN '댓글 #' || COALESCE(h.new_value,'?') || ' 토론 해결'
                    WHEN 'discussion_reopened' THEN '댓글 #' || COALESCE(h.new_value,'?') || ' 토론 재개(미해결)'
                    ELSE COALESCE(h.old_value,'?') || ' → ' || COALESCE(h.new_value,'?')
                  END AS summary,
                  -- DEV-296: 토론 해결/재개는 new_value 가 댓글 번호다.
                  CASE h.op
                    WHEN 'discussion_resolved' THEN CAST(h.new_value AS INTEGER)
                    WHEN 'discussion_reopened' THEN CAST(h.new_value AS INTEGER)
                    ELSE NULL
                  END AS ref_id
             FROM quest_history h
            WHERE h.quest_slug IS NOT NULL
              AND substr(h.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           SELECT c.ts, 'comment',
                  qt.prefix || '-' || printf('%03d', q.number),
                  c.author || ': ' || substr(c.body, 1, 200),
                  -- DEV-296: UI 앵커(`#comment-N`)는 **퀘스트별 번호**(entry_id)를
                  -- 쓴다. 전역 rowid(c.id)를 보내면 딥링크가 아무 데도 안 걸린다.
                  c.entry_id
             FROM quest_comments c
             JOIN quests q ON q.id = c.quest_id
             JOIN quest_types qt ON qt.id = q.quest_type_id
            WHERE substr(c.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           SELECT c.ts, 'comment', ca.campaign_slug,
                  c.author || ': ' || substr(c.body, 1, 200),
                  c.entry_id
             FROM campaign_comments c
             JOIN campaigns ca ON ca.id = c.campaign_id
            WHERE substr(c.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           SELECT q.created_at, 'created',
                  qt.prefix || '-' || printf('%03d', q.number), q.title, NULL
             FROM quests q
             JOIN quest_types qt ON qt.id = q.quest_type_id
            WHERE substr(q.created_at,1,10) BETWEEN ? AND ?
           UNION ALL
           -- DEV-226 후속(admin 보고): 캠페인 상태 변경도 활동 타임라인에.
           SELECT ch.ts, 'status', ch.campaign_slug,
                  COALESCE(ch.old_value,'?') || ' → ' || COALESCE(ch.new_value,'?'),
                  NULL
             FROM campaign_history ch
            WHERE substr(ch.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           -- DEV-288: 규칙/도서관 변경도 활동 타임라인에 (사이드카 → doc_history).
           SELECT d.ts,
                  CASE d.kind WHEN 'rule' THEN 'rule' ELSE 'book' END,
                  d.slug,
                  CASE d.op
                    WHEN 'rename' THEN COALESCE(d.old_value,'?') || ' → ' || COALESCE(d.new_value,'?')
                    ELSE d.op
                  END,
                  NULL
             FROM doc_history d
            WHERE substr(d.ts,1,10) BETWEEN ? AND ?
         )
         ORDER BY ts",
    )
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .fetch_all(&store.index_pool)
    .await?;

    let mut counts = ActivityCounts::default();
    for r in &rows {
        match r.kind.as_str() {
            "status" | "type" => {
                counts.status_changes += 1;
                if r.kind == "status" && r.summary.ends_with("→ done") {
                    counts.done_transitions += 1;
                }
            }
            "discussion" => counts.discussion_events += 1,
            "comment" => counts.comments += 1,
            "created" => counts.created += 1,
            // DEV-288: 규칙/도서관 변경 — 문서 편집 활동으로 따로 집계.
            "rule" | "book" => counts.doc_changes += 1,
            _ => {}
        }
    }

    Ok(WorklogReport {
        from: from.to_string(),
        to: to.to_string(),
        activities: rows,
        counts,
    })
}

/// 일별 활동 count (히트맵용 경량 집계). (date, count) 오름차순 —
/// 활동 없는 날짜는 행 자체가 없음 (클라이언트가 0 채움).
pub async fn daily_summary(
    store: &Store,
    from: &str,
    to: &str,
) -> AppResult<Vec<(String, i64)>> {
    validate_date(from)?;
    validate_date(to)?;
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT d, COUNT(*) FROM (
           SELECT substr(h.ts,1,10) AS d FROM quest_history h
            WHERE h.quest_slug IS NOT NULL AND substr(h.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           SELECT substr(c.ts,1,10) FROM quest_comments c
            WHERE substr(c.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           SELECT substr(c.ts,1,10) FROM campaign_comments c
            WHERE substr(c.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           SELECT substr(q.created_at,1,10) FROM quests q
            WHERE substr(q.created_at,1,10) BETWEEN ? AND ?
           UNION ALL
           -- DEV-226 후속: 캠페인 상태 변경도 히트맵 집계에.
           SELECT substr(ch.ts,1,10) FROM campaign_history ch
            WHERE substr(ch.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           -- DEV-288: 규칙/도서관 변경도 히트맵 집계에.
           SELECT substr(d2.ts,1,10) FROM doc_history d2
            WHERE substr(d2.ts,1,10) BETWEEN ? AND ?
         )
         GROUP BY d ORDER BY d",
    )
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .bind(from)
    .bind(to)
    .fetch_all(&store.index_pool)
    .await?;
    Ok(rows)
}

// ─── 노트 (.guild/worklog/{date}.md) ───

/// 노트 본문. 파일 없으면 None.
pub fn get_note(store: &Store, date: &str) -> AppResult<Option<String>> {
    validate_date(date)?;
    let p = store.paths.worklog_note_path(date);
    if !p.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(&p)
        .map(Some)
        .with_context(|| format!("failed to read: {}", p.display()))
        .map_err(AppError::Internal)
}

/// 노트 저장 — 빈(공백뿐인) 본문이면 파일 삭제 (clear 와 동일).
pub async fn set_note(store: &Store, date: &str, content: String) -> AppResult<()> {
    validate_date(date)?;
    let _ = journal::append(
        &store.journal_pool,
        "set_worklog_note",
        &json!({ "date": date, "len": content.len() }),
        None::<&serde_json::Value>,
    )
    .await
    .map_err(AppError::Internal)?;

    let p = store.paths.worklog_note_path(date);
    if content.trim().is_empty() {
        if p.exists() {
            std::fs::remove_file(&p)
                .with_context(|| format!("failed to remove: {}", p.display()))
                .map_err(AppError::Internal)?;
        }
        return Ok(());
    }
    std::fs::create_dir_all(store.paths.worklog_dir())
        .map_err(|e| AppError::Internal(anyhow!(e)))?;
    crate::repo::fs::write_atomic(&p, &content).map_err(AppError::Internal)
}

/// 기간 내 존재하는 노트 (date, content) 목록 — 주/월 뷰의 일별 노트 나열용.
pub fn list_notes(store: &Store, from: &str, to: &str) -> AppResult<Vec<(String, String)>> {
    validate_date(from)?;
    validate_date(to)?;
    let dir = store.paths.worklog_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|e| AppError::Internal(anyhow!(e)))? {
        let Ok(e) = e else { continue };
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }
        let Some(date) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if validate_date(date).is_err() || date < from || date > to {
            continue;
        }
        let content = std::fs::read_to_string(&p).map_err(|e| AppError::Internal(anyhow!(e)))?;
        out.push((date.to_string(), content));
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChangeStatusRequest, CreateQuestRequest};
    use crate::repo::seed_guild_dir;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-worklog-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        let store = Store::open(dir).await.unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        store
    }

    fn today() -> String {
        crate::time::now_local_iso8601()[..10].to_string()
    }

    #[tokio::test]
    async fn activities_collects_all_kinds_and_counts() {
        let dir = fresh_tmp("act");
        let store = setup(&dir).await;
        let tid: i64 = sqlx::query_scalar("SELECT id FROM quest_types WHERE prefix = 'DEV'")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();

        let q = crate::ops::quests::create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: tid,
                title: "작업기록 테스트".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        crate::ops::quests::change_status(
            &store,
            q.id,
            ChangeStatusRequest { status_slug: "done".into() },
        )
        .await
        .unwrap();
        crate::ops::comments::add_comment_entry(
            &store,
            &q.quest_id,
            "claude".into(),
            "진행 메모".into(),
            None,
            false,
        )
        .await
        .unwrap();

        let d = today();
        let report = activities(&store, &d, &d).await.unwrap();
        assert_eq!(report.counts.created, 1);
        assert_eq!(report.counts.status_changes, 1);
        assert_eq!(report.counts.comments, 1);
        assert_eq!(report.counts.done_transitions, 1, "open → done 전환");
        assert!(report.activities.iter().any(|a| a.kind == "created" && a.summary == "작업기록 테스트"));
        assert!(report
            .activities
            .iter()
            .any(|a| a.kind == "comment" && a.summary.starts_with("claude: 진행 메모")));

        // 기간 밖이면 빈 결과.
        let empty = activities(&store, "2000-01-01", "2000-01-02").await.unwrap();
        assert!(empty.activities.is_empty());

        // 히트맵 집계 — 오늘 4건 (created + status + 그 comment... created 1,
        // status 1, comment 1 = quest_history 는 status 1건만).
        let sum = daily_summary(&store, &d, &d).await.unwrap();
        assert_eq!(sum.len(), 1);
        assert_eq!(sum[0].0, d);
        assert_eq!(sum[0].1, report.activities.len() as i64);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-226 후속(admin 보고): 캠페인 상태 변경이 worklog 에 안 잡히던 회귀.
    #[tokio::test]
    async fn activities_includes_campaign_status_changes() {
        use crate::models::{CreateCampaignRequest, UpdateCampaignRequest};
        let dir = fresh_tmp("camp-status");
        let store = setup(&dir).await;

        let camp = crate::ops::campaigns::create_campaign(
            &store,
            CreateCampaignRequest {
                title: "월드컵".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        crate::ops::campaigns::update_campaign(
            &store,
            camp.id,
            UpdateCampaignRequest {
                status: Some("done".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let d = today();
        let report = activities(&store, &d, &d).await.unwrap();
        let row = report
            .activities
            .iter()
            .find(|a| a.slug == camp.campaign_slug)
            .expect("캠페인 상태 변경이 타임라인에 있어야 함");
        assert_eq!(row.kind, "status");
        assert_eq!(row.summary, "active → done");
        assert_eq!(report.counts.status_changes, 1);

        // 히트맵에도 반영.
        let sum = daily_summary(&store, &d, &d).await.unwrap();
        assert_eq!(sum[0].1, report.activities.len() as i64);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-236: 토론 resolve/reopen 전환이 'discussion' kind 로 타임라인에 표출.
    #[tokio::test]
    async fn activities_includes_discussion_resolve_events() {
        let dir = fresh_tmp("disc");
        let store = setup(&dir).await;
        let tid: i64 = sqlx::query_scalar("SELECT id FROM quest_types WHERE prefix = 'DEV'")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        let q = crate::ops::quests::create_quest(
            &store,
            CreateQuestRequest {
                quest_type_id: tid,
                title: "토론 테스트".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        let c = crate::ops::comments::add_comment_entry(
            &store,
            &q.quest_id,
            "claude".into(),
            "결정 필요".into(),
            None,
            false,
        )
        .await
        .unwrap();
        crate::ops::comments::toggle_comment_discussion(&store, &q.quest_id, c.id)
            .await
            .unwrap();
        crate::ops::comments::toggle_comment_resolved(&store, &q.quest_id, c.id)
            .await
            .unwrap();

        let d = today();
        let report = activities(&store, &d, &d).await.unwrap();
        assert_eq!(report.counts.discussion_events, 1);
        let ev = report
            .activities
            .iter()
            .find(|a| a.kind == "discussion")
            .expect("discussion 활동이 타임라인에 있어야");
        assert!(ev.summary.contains("해결"));
        assert!(ev.summary.contains(&c.id.to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn note_set_get_clear_and_list() {
        let dir = fresh_tmp("note");
        let store = setup(&dir).await;

        assert!(get_note(&store, "2026-07-05").unwrap().is_none());
        set_note(&store, "2026-07-05", "오늘 한 일 — 한글 노트".into())
            .await
            .unwrap();
        assert_eq!(
            get_note(&store, "2026-07-05").unwrap().as_deref(),
            Some("오늘 한 일 — 한글 노트")
        );
        set_note(&store, "2026-07-03", "이전 노트".into()).await.unwrap();

        let notes = list_notes(&store, "2026-07-01", "2026-07-31").unwrap();
        assert_eq!(
            notes.iter().map(|(d, _)| d.as_str()).collect::<Vec<_>>(),
            vec!["2026-07-03", "2026-07-05"]
        );
        let narrow = list_notes(&store, "2026-07-04", "2026-07-31").unwrap();
        assert_eq!(narrow.len(), 1);

        // 빈 본문 = 파일 삭제.
        set_note(&store, "2026-07-05", "  \n".into()).await.unwrap();
        assert!(get_note(&store, "2026-07-05").unwrap().is_none());
        assert!(!store.paths.worklog_note_path("2026-07-05").exists());

        // 날짜 검증.
        assert!(get_note(&store, "../evil").is_err());
        assert!(set_note(&store, "20260705", "x".into()).await.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-189: 규칙/BOOK 변경이 **reindex 없이** 바로 작업기록에 뜨는지.
    ///
    /// DEV-288 은 사이드카만 쓰고 캐시 투영을 reindex 에 맡겼는데, 평소 reindex
    /// 를 돌 일이 없어 사용자 눈엔 기록이 안 남는 것과 같았다.
    #[tokio::test]
    async fn rule_and_book_changes_appear_without_reindex() {
        let dir = fresh_tmp("doc-hist");
        let store = setup(&dir).await;
        let d = today();

        crate::ops::rules::create_rule(&store, "my-rule", "본문".into())
            .await
            .unwrap();
        crate::ops::library::create_book(&store, "문서 하나", "본문", "")
            .await
            .unwrap();
        crate::ops::rules::set_rule(&store, "my-rule", "고침".into())
            .await
            .unwrap();

        let r = activities(&store, &d, &d).await.unwrap();
        assert_eq!(r.counts.doc_changes, 3, "활동: {:?}", r.activities);
        let kinds: Vec<&str> = r.activities.iter().map(|a| a.kind.as_str()).collect();
        assert!(kinds.contains(&"rule") && kinds.contains(&"book"), "{kinds:?}");

        // reindex 는 캐시를 파일에서 재구축한다 — 같은 이벤트가 두 번 세어지면 안 된다.
        crate::reindex::reindex(&store).await.unwrap();
        let after = activities(&store, &d, &d).await.unwrap();
        assert_eq!(after.counts.doc_changes, 3, "reindex 후 중복: {:?}", after.activities);

        // rename 하면 지난 항목도 새 slug 를 가리켜야 (안 그러면 클릭해도 안 열린다).
        crate::ops::rules::rename_rule(&store, "my-rule", "renamed-rule")
            .await
            .unwrap();
        let renamed = activities(&store, &d, &d).await.unwrap();
        assert!(
            renamed
                .activities
                .iter()
                .filter(|a| a.kind == "rule")
                .all(|a| a.slug == "renamed-rule"),
            "옛 slug 가 남음: {:?}",
            renamed.activities
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
