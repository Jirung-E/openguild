//! Campaign 도메인 서비스 (DEV-011). SQL CRUD + counter + 체크리스트 / link.
//!
//! 모든 함수는 `&SqlitePool` + plain Rust 인자 → `AppResult<T>`. HTTP / CLI 등
//! 호출자 인터페이스 무관.
//!
//! Counter: `campaign_counters` (단일 row, `id = 1`). 새 캠페인 생성 시
//! `last_number + 1` 로 다음 번호 할당, 한 트랜잭션 내 UPDATE.

use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::{
    AddChecklistRequest, CampaignChecklistItem, CampaignLinkedQuest, CampaignRow,
    CampaignStatus, CampaignSummary, CreateCampaignRequest, UpdateCampaignRequest,
    UpdateChecklistRequest,
};

const CAMPAIGN_SELECT: &str = r#"
    SELECT id, campaign_slug, title, description, status,
           started_at, ended_at, display_order,
           created_at, updated_at
      FROM campaigns
"#;

// ─────────────────────── 조회 ───────────────────────

pub async fn list_alive(pool: &SqlitePool) -> AppResult<Vec<CampaignRow>> {
    let sql = format!(
        "{CAMPAIGN_SELECT} WHERE deleted_at IS NULL \
         ORDER BY display_order ASC, datetime(created_at) DESC, id DESC"
    );
    let rows = sqlx::query_as::<_, CampaignRow>(&sql)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn list_by_status(pool: &SqlitePool, status: &str) -> AppResult<Vec<CampaignRow>> {
    let sql = format!(
        "{CAMPAIGN_SELECT} WHERE deleted_at IS NULL AND status = ? \
         ORDER BY display_order ASC, datetime(created_at) DESC, id DESC"
    );
    let rows = sqlx::query_as::<_, CampaignRow>(&sql)
        .bind(status)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn fetch_by_slug(pool: &SqlitePool, slug: &str) -> AppResult<CampaignRow> {
    let sql = format!("{CAMPAIGN_SELECT} WHERE campaign_slug = ? AND deleted_at IS NULL");
    let row = sqlx::query_as::<_, CampaignRow>(&sql)
        .bind(slug)
        .fetch_optional(pool)
        .await?;
    row.ok_or_else(|| AppError::NotFound(format!("campaign not found: {slug}")))
}

pub async fn fetch_by_id(pool: &SqlitePool, id: i64) -> AppResult<CampaignRow> {
    let sql = format!("{CAMPAIGN_SELECT} WHERE id = ? AND deleted_at IS NULL");
    let row = sqlx::query_as::<_, CampaignRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.ok_or_else(|| AppError::NotFound(format!("campaign id {id} not found")))
}

// ─────────────────────── 생성 / 수정 / 삭제 ───────────────────────

/// 새 캠페인 생성. counter 자동 증가 후 `C-NNN` slug 할당.
/// 트랜잭션 안에서: counter self-heal (max(last_number, current_max)) → +1 → INSERT.
pub async fn create(pool: &SqlitePool, body: CreateCampaignRequest) -> AppResult<CampaignRow> {
    let title = body.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("campaign title is required".into()));
    }

    let now = crate::time::now_local_iso8601();

    let mut tx = pool.begin().await?;

    // Counter self-heal: 외부에서 파일을 미리 만들었을 가능성 대비.
    sqlx::query(
        "UPDATE campaign_counters
            SET last_number = MAX(last_number,
                COALESCE((SELECT MAX(SUBSTR(campaign_slug, 3) + 0)
                          FROM campaigns), 0))
          WHERE id = 1",
    )
    .execute(&mut *tx)
    .await?;

    // 다음 번호
    sqlx::query("UPDATE campaign_counters SET last_number = last_number + 1 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let next: (i64,) =
        sqlx::query_as("SELECT last_number FROM campaign_counters WHERE id = 1")
            .fetch_one(&mut *tx)
            .await?;
    let slug = format!("C-{:03}", next.0);

    let inserted: (i64,) = sqlx::query_as(
        "INSERT INTO campaigns
            (campaign_slug, title, description, status,
             started_at, ended_at, display_order, created_at, updated_at)
         VALUES (?, ?, ?, 'active', ?, ?, 0, ?, ?)
         RETURNING id",
    )
    .bind(&slug)
    .bind(title)
    .bind(body.description.as_deref().unwrap_or(""))
    .bind(body.started_at.as_deref().unwrap_or(""))
    .bind(body.ended_at.as_deref().unwrap_or(""))
    .bind(&now)
    .bind(&now)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    fetch_by_id(pool, inserted.0).await
}

pub async fn update(
    pool: &SqlitePool,
    id: i64,
    body: UpdateCampaignRequest,
) -> AppResult<CampaignRow> {
    // status 값 검증
    if let Some(s) = &body.status
        && CampaignStatus::from_str(s).is_none()
    {
        return Err(AppError::BadRequest(format!(
            "invalid campaign status: '{s}' (expected 'active' | 'done')"
        )));
    }

    let now = crate::time::now_local_iso8601();
    let mut tx = pool.begin().await?;

    if let Some(t) = body.title {
        let t = t.trim();
        if t.is_empty() {
            return Err(AppError::BadRequest("campaign title cannot be empty".into()));
        }
        sqlx::query("UPDATE campaigns SET title = ? WHERE id = ?")
            .bind(t)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(d) = body.description {
        sqlx::query("UPDATE campaigns SET description = ? WHERE id = ?")
            .bind(d)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(s) = body.status {
        sqlx::query("UPDATE campaigns SET status = ? WHERE id = ?")
            .bind(s)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(s) = body.started_at {
        sqlx::query("UPDATE campaigns SET started_at = ? WHERE id = ?")
            .bind(s)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(e) = body.ended_at {
        sqlx::query("UPDATE campaigns SET ended_at = ? WHERE id = ?")
            .bind(e)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(o) = body.display_order {
        sqlx::query("UPDATE campaigns SET display_order = ? WHERE id = ?")
            .bind(o)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("UPDATE campaigns SET updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    fetch_by_id(pool, id).await
}

/// soft delete (deleted_at 설정). cascade FK 가 checklists / quest 연결 정리.
pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let now = crate::time::now_local_iso8601();
    sqlx::query("UPDATE campaigns SET deleted_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ─────────────────────── 체크리스트 ───────────────────────

pub async fn list_checklists(
    pool: &SqlitePool,
    campaign_id: i64,
) -> AppResult<Vec<CampaignChecklistItem>> {
    let items = sqlx::query_as::<_, CampaignChecklistItem>(
        "SELECT id, campaign_id, text, checked != 0 AS checked, order_idx
           FROM campaign_checklists
          WHERE campaign_id = ?
          ORDER BY order_idx ASC, id ASC",
    )
    .bind(campaign_id)
    .fetch_all(pool)
    .await?;
    Ok(items)
}

pub async fn add_checklist(
    pool: &SqlitePool,
    campaign_id: i64,
    body: AddChecklistRequest,
) -> AppResult<CampaignChecklistItem> {
    let text = body.text.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("checklist text is required".into()));
    }
    let next_order: (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(order_idx) + 1, 0) FROM campaign_checklists WHERE campaign_id = ?",
    )
    .bind(campaign_id)
    .fetch_one(pool)
    .await?;
    let id: (i64,) = sqlx::query_as(
        "INSERT INTO campaign_checklists (campaign_id, text, checked, order_idx)
         VALUES (?, ?, 0, ?) RETURNING id",
    )
    .bind(campaign_id)
    .bind(text)
    .bind(next_order.0)
    .fetch_one(pool)
    .await?;
    fetch_checklist(pool, id.0).await
}

pub async fn fetch_checklist(
    pool: &SqlitePool,
    id: i64,
) -> AppResult<CampaignChecklistItem> {
    let item = sqlx::query_as::<_, CampaignChecklistItem>(
        "SELECT id, campaign_id, text, checked != 0 AS checked, order_idx
           FROM campaign_checklists WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    item.ok_or_else(|| AppError::NotFound(format!("checklist id {id} not found")))
}

pub async fn update_checklist(
    pool: &SqlitePool,
    id: i64,
    body: UpdateChecklistRequest,
) -> AppResult<CampaignChecklistItem> {
    let mut tx = pool.begin().await?;
    if let Some(t) = body.text {
        let t = t.trim();
        if t.is_empty() {
            return Err(AppError::BadRequest("checklist text cannot be empty".into()));
        }
        sqlx::query("UPDATE campaign_checklists SET text = ? WHERE id = ?")
            .bind(t)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(c) = body.checked {
        sqlx::query("UPDATE campaign_checklists SET checked = ? WHERE id = ?")
            .bind(if c { 1 } else { 0 })
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(o) = body.order_idx {
        sqlx::query("UPDATE campaign_checklists SET order_idx = ? WHERE id = ?")
            .bind(o)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    fetch_checklist(pool, id).await
}

pub async fn delete_checklist(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM campaign_checklists WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 파일의 체크리스트 목록 (`ChecklistLine`) 으로 DB 를 fully 대체.
/// 파일 → DB 단방향 sync 의 핵심.
/// 이미 같으면 no-op (write 안 함). 다르면 DELETE + INSERT.
pub async fn replace_checklists_from_file(
    pool: &SqlitePool,
    campaign_id: i64,
    lines: &[crate::repo::ChecklistLine],
) -> AppResult<()> {
    let current = list_checklists(pool, campaign_id).await?;
    let same = current.len() == lines.len()
        && current
            .iter()
            .zip(lines.iter())
            .all(|(c, l)| c.text == l.text && c.checked == l.checked && c.order_idx == l.order_idx);
    if same {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM campaign_checklists WHERE campaign_id = ?")
        .bind(campaign_id)
        .execute(&mut *tx)
        .await?;
    for line in lines {
        sqlx::query(
            "INSERT INTO campaign_checklists (campaign_id, text, checked, order_idx)
             VALUES (?, ?, ?, ?)",
        )
        .bind(campaign_id)
        .bind(&line.text)
        .bind(if line.checked { 1 } else { 0 })
        .bind(line.order_idx)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

// ─────────────────────── Quest 연결 ───────────────────────

pub async fn list_linked_quests(
    pool: &SqlitePool,
    campaign_id: i64,
) -> AppResult<Vec<CampaignLinkedQuest>> {
    let rows = sqlx::query_as::<_, CampaignLinkedQuest>(
        r#"SELECT q.id,
                  qt.prefix || '-' || printf('%03d', q.number) AS quest_id,
                  q.title,
                  qt.prefix AS type_prefix,
                  qt.color  AS type_color,
                  qs.slug    AS status_slug,
                  qs.name_en AS status_name_en,
                  qs.color   AS status_color
             FROM campaign_quests cq
             JOIN quests q          ON cq.quest_id = q.id
             JOIN quest_types qt    ON q.quest_type_id = qt.id
             JOIN quest_statuses qs ON q.status_id    = qs.id
            WHERE cq.campaign_id = ? AND q.deleted_at IS NULL
            ORDER BY q.id ASC"#,
    )
    .bind(campaign_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn link_quest(pool: &SqlitePool, campaign_id: i64, quest_id: i64) -> AppResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO campaign_quests (campaign_id, quest_id) VALUES (?, ?)",
    )
    .bind(campaign_id)
    .bind(quest_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unlink_quest(pool: &SqlitePool, campaign_id: i64, quest_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM campaign_quests WHERE campaign_id = ? AND quest_id = ?")
        .bind(campaign_id)
        .bind(quest_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 특정 quest 에 연결된 모든 캠페인 (alive only). Quest Detail 의 Campaign
/// 섹션 표시용. 최근 연결 순서 보장은 안 함 — display_order / created_at.
pub async fn list_for_quest(
    pool: &SqlitePool,
    quest_id: i64,
) -> AppResult<Vec<CampaignRow>> {
    let sql = format!(
        "{CAMPAIGN_SELECT}
          JOIN campaign_quests cq ON c.id = cq.campaign_id
         WHERE cq.quest_id = ? AND c.deleted_at IS NULL
         ORDER BY c.display_order ASC, datetime(c.created_at) DESC, c.id DESC"
    );
    // CAMPAIGN_SELECT 의 from 절이 'FROM campaigns' 라 alias 가 필요 — 별도 query 사용.
    let _ = sql;
    let rows = sqlx::query_as::<_, CampaignRow>(
        "SELECT c.id, c.campaign_slug, c.title, c.description, c.status,
                c.started_at, c.ended_at, c.display_order,
                c.created_at, c.updated_at
           FROM campaigns c
           JOIN campaign_quests cq ON c.id = cq.campaign_id
          WHERE cq.quest_id = ? AND c.deleted_at IS NULL
          ORDER BY c.display_order ASC, datetime(c.created_at) DESC, c.id DESC",
    )
    .bind(quest_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// quest 의 slug 로 id resolve.
pub async fn resolve_quest_id(pool: &SqlitePool, quest_slug: &str) -> AppResult<i64> {
    let id: Option<(i64,)> = sqlx::query_as(
        "SELECT q.id FROM quests q
         JOIN quest_types qt ON q.quest_type_id = qt.id
         WHERE qt.prefix || '-' || printf('%03d', q.number) = ?
           AND q.deleted_at IS NULL",
    )
    .bind(quest_slug)
    .fetch_optional(pool)
    .await?;
    id.map(|(i,)| i)
        .ok_or_else(|| AppError::NotFound(format!("quest not found: {quest_slug}")))
}

// ─────────────────────── Home / Summary ───────────────────────

/// Home 카드용 — 현재 진행 중 캠페인 (status='active' + 현재 날짜가
/// `started_at` ~ `ended_at` 사이에 포함). BUG-023:
/// - status='active' 만으로는 부족 — 시작 전 / 종료 후도 active 인 경우 있음.
/// - 시작일이 비어있으면 "언제든 시작" (오늘 포함), 종료일이 비어있으면
///   "무기한 진행" (오늘 포함). 둘 다 명시되면 그 범위에 오늘이 포함되어야 함.
pub async fn list_active_summaries(pool: &SqlitePool) -> AppResult<Vec<CampaignSummary>> {
    let sql = format!(
        "{CAMPAIGN_SELECT}
          WHERE deleted_at IS NULL
            AND status = 'active'
            AND (started_at IS NULL OR started_at = ''
                 OR date(started_at) <= date('now', 'localtime'))
            AND (ended_at IS NULL OR ended_at = ''
                 OR date(ended_at) >= date('now', 'localtime'))
          ORDER BY display_order ASC, datetime(created_at) DESC, id DESC"
    );
    let rows: Vec<CampaignRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for c in rows {
        out.push(summarize(pool, c).await?);
    }
    Ok(out)
}

/// 곧 시작하는 캠페인 (started_at 이 향후 `days_ahead` 일 이내). 없으면
/// 가장 가까운 미래 시작일 1개 fallback. 모두 미래 시작일 없으면 빈 벡터.
pub async fn list_upcoming_summaries(
    pool: &SqlitePool,
    today: &str,
    days_ahead: i64,
) -> AppResult<Vec<CampaignSummary>> {
    // 1차: today < started_at <= today + N
    let sql_window = format!(
        "{CAMPAIGN_SELECT}
          WHERE deleted_at IS NULL
            AND status = 'active'
            AND started_at IS NOT NULL AND started_at != ''
            AND date(started_at) > date(?)
            AND date(started_at) <= date(?, '+' || ? || ' days')
          ORDER BY date(started_at) ASC, display_order ASC, id ASC"
    );
    let window: Vec<CampaignRow> = sqlx::query_as(&sql_window)
        .bind(today)
        .bind(today)
        .bind(days_ahead)
        .fetch_all(pool)
        .await?;

    let chosen = if !window.is_empty() {
        window
    } else {
        // fallback: 가장 가까운 미래 1개
        let sql_one = format!(
            "{CAMPAIGN_SELECT}
              WHERE deleted_at IS NULL
                AND status = 'active'
                AND started_at IS NOT NULL AND started_at != ''
                AND date(started_at) > date(?)
              ORDER BY date(started_at) ASC, display_order ASC, id ASC
              LIMIT 1"
        );
        sqlx::query_as::<_, CampaignRow>(&sql_one)
            .bind(today)
            .fetch_all(pool)
            .await?
    };

    let mut out = Vec::with_capacity(chosen.len());
    for c in chosen {
        out.push(summarize(pool, c).await?);
    }
    Ok(out)
}

async fn summarize(pool: &SqlitePool, c: CampaignRow) -> AppResult<CampaignSummary> {
    let stats: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),
                SUM(CASE WHEN checked != 0 THEN 1 ELSE 0 END)
           FROM campaign_checklists WHERE campaign_id = ?",
    )
    .bind(c.id)
    .fetch_one(pool)
    .await?;
    let total = stats.0;
    let checked = stats.1;
    let progress = if total > 0 {
        checked as f64 / total as f64
    } else {
        0.0
    };
    Ok(CampaignSummary {
        id: c.id,
        campaign_slug: c.campaign_slug,
        title: c.title,
        status: c.status,
        started_at: c.started_at,
        ended_at: c.ended_at,
        display_order: c.display_order,
        created_at: c.created_at,
        progress,
        checklist_total: total,
        checklist_checked: checked,
    })
}
