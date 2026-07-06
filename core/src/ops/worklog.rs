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
    /// "status"(상태변경) | "type"(타입변경) | "comment" | "created".
    pub kind: String,
    /// 소속 슬러그 (DEV-001 / C-001).
    pub slug: String,
    /// 한 줄 표시용: status = "old → new", comment = "author: 본문(앞부분)",
    /// created = 제목.
    pub summary: String,
}

/// 종류별 집계 (타임라인과 함께 반환).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ActivityCounts {
    pub status_changes: i64,
    pub comments: i64,
    pub created: i64,
    /// 상태변경 중 done 으로의 전환 수.
    pub done_transitions: i64,
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
                  CASE h.op WHEN 'change_status' THEN 'status' ELSE 'type' END AS kind,
                  h.quest_slug AS slug,
                  COALESCE(h.old_value,'?') || ' → ' || COALESCE(h.new_value,'?') AS summary
             FROM quest_history h
            WHERE h.quest_slug IS NOT NULL
              AND substr(h.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           SELECT c.ts, 'comment',
                  qt.prefix || '-' || printf('%03d', q.number),
                  c.author || ': ' || substr(c.body, 1, 200)
             FROM quest_comments c
             JOIN quests q ON q.id = c.quest_id
             JOIN quest_types qt ON qt.id = q.quest_type_id
            WHERE substr(c.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           SELECT c.ts, 'comment', ca.campaign_slug,
                  c.author || ': ' || substr(c.body, 1, 200)
             FROM campaign_comments c
             JOIN campaigns ca ON ca.id = c.campaign_id
            WHERE substr(c.ts,1,10) BETWEEN ? AND ?
           UNION ALL
           SELECT q.created_at, 'created',
                  qt.prefix || '-' || printf('%03d', q.number), q.title
             FROM quests q
             JOIN quest_types qt ON qt.id = q.quest_type_id
            WHERE substr(q.created_at,1,10) BETWEEN ? AND ?
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
            "comment" => counts.comments += 1,
            "created" => counts.created += 1,
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
}
