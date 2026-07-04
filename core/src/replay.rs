//! DEV-022: Journal replay — 시점 복원 (point-in-time recovery).
//!
//! snapshot(RDB)은 특정 시점의 `.guild/` 소스 파일 묶음이고, journal.db(AOF)는 그
//! snapshot **이후** mutation 들을 시간순으로 기록한다(snapshot 생성 시 truncate).
//! 따라서 "마지막 snapshot ~ 임의 시점 T" 로 복원하려면:
//!   1. 최신 snapshot 을 restore (파일 → snapshot 상태, reindex).
//!   2. journal ops 중 `ts <= T` 인 것을 시간순으로 재적용.
//!
//! ## id 안정성 (핵심)
//! journal 은 mutation 을 index.db `id`(autoincrement PK)로 기록하는데, 이 id 는
//! 파일에 저장되지 않아 reindex 마다 새로 부여된다. 그래서 journal 의 id 를 그대로
//! 쓰면 다른 엔티티를 건드린다. 해결: **restore 전** 현재 index.db 에서
//! `id → slug` 맵을 떠두고(이 시점엔 journal 의 id 가 유효), replay 때 각 op 의
//! id 를 `slug` 로 바꾼 뒤 복원된 db 에서 slug 로 현재 id 를 다시 찾는다.
//!
//! type 변경(`change_quest_type`)은 slug 자체가 바뀌어 안전 매핑이 깨지므로
//! **fail-loud**(replay 거부) — 그 경우 full snapshot restore 를 쓴다.
//!
//! 안전 우선: 처리할 수 없는 op 를 만나면 조용히 건너뛰지 않고 **에러로 중단**한다
//! (부분 복원으로 인한 무결성 손상 방지). 현재 dispatcher 는 quest op 을 다루고,
//! campaign/comment/rules/attachment op 은 후속 확장 전까지 fail-loud.

use std::collections::HashMap;

use anyhow::anyhow;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{
    AddPrerequisiteRequest, ChangeParentRequest, ChangeStatusRequest, CreateCampaignRequest,
    CreateQuestRequest, UpdateCampaignRequest, UpdateQuestRequest,
};
use crate::ops;
use crate::snapshot::{self, SnapshotInfo};
use crate::store::{Store, journal};

/// replay 결과 요약.
#[derive(Debug, Clone)]
pub struct ReplayReport {
    /// 적용된 op 수.
    pub applied: usize,
    /// 복원 목표 시점(포함). journal `ts` 와 동일 포맷(ISO8601 UTC).
    pub target_ts: String,
    /// DEV-212: restore 직전 현재 상태를 자동 백업한 스냅샷 timestamp
    /// (`YYYYMMDD-HHMMSS`). journal 이 비어 있었으면(잃을 것 없음) None.
    pub pre_backup: Option<String>,
}

/// restore 직전 현재 index.db 에서 떠두는 id → slug 맵.
struct IdMaps {
    /// index.db quest id → slug (예: "DEV-001"). 삭제된 quest 포함.
    quest: HashMap<i64, String>,
    /// index.db campaign id → campaign_slug (예: "C-001").
    campaign: HashMap<i64, String>,
}

async fn build_id_maps(store: &Store) -> AppResult<IdMaps> {
    let quest: Vec<(i64, String)> = sqlx::query_as(
        "SELECT q.id, qt.prefix || '-' || printf('%03d', q.number)
         FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id",
    )
    .fetch_all(&store.index_pool)
    .await?;
    let campaign: Vec<(i64, String)> = sqlx::query_as("SELECT id, campaign_slug FROM campaigns")
        .fetch_all(&store.index_pool)
        .await?;
    Ok(IdMaps {
        quest: quest.into_iter().collect(),
        campaign: campaign.into_iter().collect(),
    })
}

/// journal quest id → 복원된 db 의 현재 quest id (slug 경유). 해석 불가 시 Err(fail-loud).
async fn resolve_quest_id(store: &Store, maps: &IdMaps, journal_id: i64) -> AppResult<i64> {
    let slug = maps.quest.get(&journal_id).ok_or_else(|| {
        AppError::Internal(anyhow!(
            "replay: journal quest id {journal_id} 를 slug 로 해석할 수 없음 (restore 전 맵에 없음)"
        ))
    })?;
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT q.id FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id
         WHERE qt.prefix || '-' || printf('%03d', q.number) = ?",
    )
    .bind(slug)
    .fetch_optional(&store.index_pool)
    .await?;
    id.ok_or_else(|| {
        AppError::Internal(anyhow!(
            "replay: quest {slug} 가 복원된 상태에 없음 (replay 순서/무결성 문제)"
        ))
    })
}

/// journal campaign id → 복원된 db 의 현재 campaign id (slug 경유). 불가 시 Err.
async fn resolve_campaign_id(store: &Store, maps: &IdMaps, journal_id: i64) -> AppResult<i64> {
    let slug = maps.campaign.get(&journal_id).ok_or_else(|| {
        AppError::Internal(anyhow!(
            "replay: journal campaign id {journal_id} 를 slug 로 해석할 수 없음"
        ))
    })?;
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM campaigns WHERE campaign_slug = ?")
        .bind(slug)
        .fetch_optional(&store.index_pool)
        .await?;
    id.ok_or_else(|| AppError::Internal(anyhow!("replay: campaign {slug} 가 복원된 상태에 없음")))
}

/// 최신 snapshot 을 복원한 뒤 journal ops 를 `target_ts`(포함)까지 재적용.
///
/// `target_ts` 는 journal `ts` 와 같은 ISO8601 UTC 포맷(`YYYY-MM-DDThh:mm:ssZ`).
/// 문자열 사전순 비교 = 시간순 비교(동일 포맷이므로).
pub async fn replay_to(
    store: &Store,
    snapshot: &SnapshotInfo,
    target_ts: &str,
) -> AppResult<ReplayReport> {
    // 1. restore 전에 id→slug 맵 + 적용할 ops 를 메모리로 확보.
    //    (restore 는 journal 을 건드리지 않지만, replay 중 ops 가 재append 되므로
    //     미리 스냅샷.)
    let maps = build_id_maps(store).await?;
    let all = journal::list_all(&store.journal_pool)
        .await
        .map_err(AppError::Internal)?;
    let had_journal = !all.is_empty();
    let to_apply: Vec<journal::OpRow> = all
        .into_iter()
        .filter(|o| o.ts.as_str() <= target_ts)
        .collect();

    // 1.5 (DEV-212): 파괴적 replay 전 **현재 상태**를 정식 백업으로 자동 보존 —
    // journal truncate(4단계)로 비가역이 되는 걸 방지. 되돌리려면
    // `restore --to <pre_backup>`.
    //  - ops 는 이미 메모리에 확보돼 있어 create_snapshot 의 journal truncate 가
    //    replay 대상에 영향 없음.
    //  - 복원할 snapshot(인자)은 호출자가 진입 전에 선택 완료 — 새 스냅샷이
    //    "최신" 선택을 오염시키지 않음.
    //  - journal 이 비어 있었으면 현재 = 최신 스냅샷이라 스킵(멱등).
    let pre_backup = if had_journal {
        Some(
            snapshot::create_snapshot(store)
                .await
                .map_err(AppError::Internal)?
                .timestamp,
        )
    } else {
        None
    };

    // 2. 최신 snapshot 복원 (파일 → snapshot 상태, reindex).
    snapshot::restore_snapshot(store, snapshot)
        .await
        .map_err(AppError::Internal)?;

    // 3. replay 모드 — auto-snapshot 억제(replay 중 journal truncate 방지).
    store.set_replaying(true);
    let mut applied = 0usize;
    let mut result: AppResult<()> = Ok(());
    for op in &to_apply {
        if let Err(e) = apply_op(store, &maps, op).await {
            result = Err(e);
            break;
        }
        applied += 1;
    }
    store.set_replaying(false);
    result?;

    // 4. journal truncate — 복원된 시점이 새 baseline (T 이후 ops 는 폐기).
    journal::truncate(&store.journal_pool)
        .await
        .map_err(AppError::Internal)?;

    Ok(ReplayReport {
        applied,
        target_ts: target_ts.to_string(),
        pre_backup,
    })
}

/// 단일 op 재적용. journal id 를 현재 id 로 변환 후 해당 ops:: 함수 호출.
async fn apply_op(store: &Store, maps: &IdMaps, op: &journal::OpRow) -> AppResult<()> {
    let args: Value = serde_json::from_str(&op.args)
        .map_err(|e| AppError::Internal(anyhow!("replay: op {} args 파싱 실패: {e}", op.op)))?;

    match op.op.as_str() {
        "create_quest" => {
            let mut body: CreateQuestRequest = serde_json::from_value(args)
                .map_err(|e| AppError::Internal(anyhow!("create_quest args: {e}")))?;
            // parent_quest_id 는 quest id → 변환.
            if let Some(pid) = body.parent_quest_id {
                body.parent_quest_id = Some(resolve_quest_id(store, maps, pid).await?);
            }
            ops::quests::create_quest(store, body).await?;
        }
        "update_quest" => {
            let id = resolve_quest_id(store, maps, arg_i64(&args, "id")?).await?;
            let body: UpdateQuestRequest = serde_json::from_value(args["body"].clone())
                .map_err(|e| AppError::Internal(anyhow!("update_quest body: {e}")))?;
            ops::quests::update_quest(store, id, body).await?;
        }
        "change_status" => {
            let id = resolve_quest_id(store, maps, arg_i64(&args, "id")?).await?;
            let status_slug = arg_str(&args, "status_slug")?;
            ops::quests::change_status(store, id, ChangeStatusRequest { status_slug }).await?;
        }
        "set_due_dates" => {
            let id = resolve_quest_id(store, maps, arg_i64(&args, "id")?).await?;
            // Option<Option<String>>: 없음 / 해제(null) / 설정 구분.
            let desired = opt_opt_string(&args, "desired_due");
            let required = opt_opt_string(&args, "required_due");
            ops::quests::set_due_dates(store, id, desired, required).await?;
        }
        "set_quest_tags" => {
            let id = resolve_quest_id(store, maps, arg_i64(&args, "id")?).await?;
            let tags: Vec<String> = serde_json::from_value(args["tags"].clone())
                .map_err(|e| AppError::Internal(anyhow!("set_quest_tags tags: {e}")))?;
            ops::quests::set_quest_tags(store, id, tags).await?;
        }
        "change_parent" => {
            let id = resolve_quest_id(store, maps, arg_i64(&args, "id")?).await?;
            let parent_quest_id = match args.get("parent_quest_id").and_then(Value::as_i64) {
                Some(pid) => Some(resolve_quest_id(store, maps, pid).await?),
                None => None,
            };
            ops::quests::change_parent(store, id, ChangeParentRequest { parent_quest_id }).await?;
        }
        "add_prerequisite" => {
            let id = resolve_quest_id(store, maps, arg_i64(&args, "id")?).await?;
            let prerequisite_id =
                resolve_quest_id(store, maps, arg_i64(&args, "prerequisite_id")?).await?;
            ops::quests::add_prerequisite(store, id, AddPrerequisiteRequest { prerequisite_id })
                .await?;
        }
        "remove_prerequisite" => {
            let id = resolve_quest_id(store, maps, arg_i64(&args, "id")?).await?;
            let prereq_id =
                resolve_quest_id(store, maps, arg_i64(&args, "prerequisite_id")?).await?;
            ops::quests::remove_prerequisite(store, id, prereq_id).await?;
        }
        "delete_quest" => {
            let id = resolve_quest_id(store, maps, arg_i64(&args, "id")?).await?;
            let raw: Vec<i64> =
                serde_json::from_value(args["cascade_ids"].clone()).unwrap_or_default();
            let mut cascade = Vec::with_capacity(raw.len());
            for c in raw {
                cascade.push(resolve_quest_id(store, maps, c).await?);
            }
            ops::quests::delete_quest(store, id, &cascade).await?;
        }
        "restore_quest" => {
            let id = resolve_quest_id(store, maps, arg_i64(&args, "id")?).await?;
            ops::quests::restore_quest(store, id).await?;
        }
        // ── 캠페인 구조 op: campaign_id 는 변환, slug/text/index 는 그대로. ──
        "create_campaign" => {
            let body: CreateCampaignRequest = serde_json::from_value(args)
                .map_err(|e| AppError::Internal(anyhow!("create_campaign args: {e}")))?;
            ops::campaigns::create_campaign(store, body).await?;
        }
        "update_campaign" => {
            let id = resolve_campaign_id(store, maps, arg_i64(&args, "id")?).await?;
            let body: UpdateCampaignRequest = serde_json::from_value(args["body"].clone())
                .map_err(|e| AppError::Internal(anyhow!("update_campaign body: {e}")))?;
            ops::campaigns::update_campaign(store, id, body).await?;
        }
        "clear_campaign_banner" => {
            ops::campaigns::clear_banner_image(store, &arg_str(&args, "slug")?).await?;
        }
        "delete_campaign" => {
            let id = resolve_campaign_id(store, maps, arg_i64(&args, "id")?).await?;
            ops::campaigns::delete_campaign(store, id).await?;
        }
        "campaign_link_quest" => {
            let cid = resolve_campaign_id(store, maps, arg_i64(&args, "campaign_id")?).await?;
            ops::campaigns::link_quest_by_slug(store, cid, &arg_str(&args, "quest_slug")?).await?;
        }
        "campaign_unlink_quest" => {
            let cid = resolve_campaign_id(store, maps, arg_i64(&args, "campaign_id")?).await?;
            ops::campaigns::unlink_quest_by_slug(store, cid, &arg_str(&args, "quest_slug")?)
                .await?;
        }
        "campaign_checklist_add" => {
            let cid = resolve_campaign_id(store, maps, arg_i64(&args, "campaign_id")?).await?;
            ops::campaigns::add_checklist_line(store, cid, &arg_str(&args, "text")?).await?;
        }
        "campaign_checklist_set" => {
            let cid = resolve_campaign_id(store, maps, arg_i64(&args, "campaign_id")?).await?;
            let index = arg_u64(&args, "index")? as usize;
            let checked = args
                .get("checked")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            ops::campaigns::set_checklist_checked_by_index(store, cid, index, checked).await?;
        }
        "campaign_checklist_rm" => {
            let cid = resolve_campaign_id(store, maps, arg_i64(&args, "campaign_id")?).await?;
            let index = arg_u64(&args, "index")? as usize;
            ops::campaigns::remove_checklist_by_index(store, cid, index).await?;
        }
        // 배너는 외부 원본 이미지 경로에 의존 — replay 시 그 파일이 없을 수 있어 fail-loud.
        "set_campaign_banner" => {
            return Err(AppError::Internal(anyhow!(
                "replay: 'set_campaign_banner' 는 외부 원본 이미지 경로에 의존해 시점 \
                 replay 로 재현할 수 없습니다. full snapshot restore 를 사용하세요."
            )));
        }

        // ── 댓글 토글/삭제: slug + entry id 기반. 둘 다 파일 내장값이라 안정적. ──
        "toggle_comment_reaction" => {
            let slug = arg_str(&args, "slug")?;
            let id = arg_u64(&args, "id")?;
            let emoji = arg_str(&args, "emoji")?;
            let author = arg_str(&args, "author")?;
            ops::comments::toggle_comment_reaction(store, &slug, id, &emoji, &author).await?;
        }
        "toggle_comment_discussion" => {
            let slug = arg_str(&args, "slug")?;
            ops::comments::toggle_comment_discussion(store, &slug, arg_u64(&args, "id")?).await?;
        }
        "toggle_comment_resolved" => {
            let slug = arg_str(&args, "slug")?;
            ops::comments::toggle_comment_resolved(store, &slug, arg_u64(&args, "id")?).await?;
        }
        "delete_comment_entry" => {
            let slug = arg_str(&args, "slug")?;
            ops::comments::delete_comment_entry(store, &slug, arg_u64(&args, "id")?).await?;
        }
        "toggle_campaign_comment_reaction" => {
            let slug = arg_str(&args, "slug")?;
            ops::campaign_comments::toggle_reaction(
                store,
                &slug,
                arg_u64(&args, "id")?,
                &arg_str(&args, "emoji")?,
                &arg_str(&args, "author")?,
            )
            .await?;
        }
        "delete_campaign_comment" => {
            let slug = arg_str(&args, "slug")?;
            ops::campaign_comments::delete_entry(store, &slug, arg_u64(&args, "id")?).await?;
        }

        // ── 길드 규칙: slug 기반. 삭제/이름변경은 내용 불필요 → replayable. ──
        "delete_rule" => {
            ops::rules::delete_rule(store, &arg_str(&args, "slug")?).await?;
        }
        "rename_rule" => {
            ops::rules::rename_rule(
                store,
                &arg_str(&args, "old_slug")?,
                &arg_str(&args, "new_slug")?,
            )
            .await?;
        }

        // ── 내용(body)을 journal 에 기록하지 않는 op (audit 로그라 len 만 저장) ──
        // → replay 로 내용을 복원할 수 없음. fail-loud.
        "set_comments"
        | "add_comment_entry"
        | "update_comment_entry"
        | "set_memo"
        | "add_campaign_comment"
        | "update_campaign_comment"
        | "set_campaign_memo"
        | "create_rule"
        | "set_rule"
        | "set_rules" => {
            return Err(AppError::Internal(anyhow!(
                "replay: '{}' 는 내용(body)이 journal 에 기록되지 않아(감사 로그) \
                 replay 로 복원할 수 없습니다. 이 시점 범위는 full snapshot restore 를 \
                 사용하세요.",
                op.op
            )));
        }

        // ── 첨부: 바이너리/외부 파일 경로에 의존 — 시점 replay 불가. ──
        "save_attachment" | "add_attachment" | "remove_attachment" => {
            return Err(AppError::Internal(anyhow!(
                "replay: '{}' 는 첨부 바이너리/외부 파일에 의존해 시점 replay 로 \
                 재현할 수 없습니다. full snapshot restore 를 사용하세요.",
                op.op
            )));
        }

        // slug 가 바뀌어 안전 매핑 불가 — full snapshot restore 권장.
        "change_quest_type" => {
            return Err(AppError::Internal(anyhow!(
                "replay: 'change_quest_type' 는 slug 가 바뀌어 시점 replay 로 안전하게 \
                 재적용할 수 없습니다. 이 시점 범위는 full snapshot restore 를 사용하세요."
            )));
        }
        // 후속 확장 전까지 fail-loud (조용한 부분 복원 금지).
        other => {
            return Err(AppError::Internal(anyhow!(
                "replay: 아직 지원하지 않는 op '{other}'. (현재 quest op 만 replay 가능 — \
                 이 시점 범위는 full snapshot restore 를 사용하세요.)"
            )));
        }
    }
    Ok(())
}

// ── args 추출 헬퍼 ──

fn arg_i64(args: &Value, key: &str) -> AppResult<i64> {
    args.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::Internal(anyhow!("replay: 인자 '{key}'(i64) 누락")))
}

fn arg_u64(args: &Value, key: &str) -> AppResult<u64> {
    args.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::Internal(anyhow!("replay: 인자 '{key}'(u64) 누락")))
}

fn arg_str(args: &Value, key: &str) -> AppResult<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AppError::Internal(anyhow!("replay: 인자 '{key}'(str) 누락")))
}

/// `Option<Option<String>>` 의미: 키 없음/`null` 자체가 아니라 — set_due_dates 는
/// `Some(Some(date))`=설정 / `Some(None)`=해제 / `None`=변경없음. journal 에는
/// 이 3-상태가 JSON 으로 그대로 직렬화돼 있으므로 from_value 로 복원.
fn opt_opt_string(args: &Value, key: &str) -> Option<Option<String>> {
    match args.get(key) {
        None | Some(Value::Null) => None,
        Some(v) => serde_json::from_value(v.clone()).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChangeStatusRequest, CreateCampaignRequest, CreateQuestRequest};
    use crate::repo::seed_guild_dir;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-replay-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        let store = Store::open(dir).await.unwrap();
        // 시드된 types/statuses 파일을 index.db 로 — create_quest 가 type/status 조회.
        crate::reindex::reindex(&store).await.unwrap();
        store
    }

    async fn dev_type_id(store: &Store) -> i64 {
        sqlx::query_scalar("SELECT id FROM quest_types WHERE prefix = 'DEV'")
            .fetch_one(&store.index_pool)
            .await
            .unwrap()
    }

    fn new_dev(tid: i64, title: &str) -> CreateQuestRequest {
        CreateQuestRequest {
            quest_type_id: tid,
            title: title.into(),
            description: None,
            status_slug: "open".into(),
            urgency: Some(3),
            parent_quest_id: None,
        }
    }

    async fn status_of(store: &Store, slug: &str) -> Option<String> {
        sqlx::query_scalar(
            "SELECT s.slug FROM quests q
             JOIN quest_statuses s ON s.id = q.status_id
             JOIN quest_types qt ON qt.id = q.quest_type_id
             WHERE qt.prefix || '-' || printf('%03d', q.number) = ?",
        )
        .bind(slug)
        .fetch_optional(&store.index_pool)
        .await
        .unwrap()
    }

    async fn exists(store: &Store, slug: &str) -> bool {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id
             WHERE qt.prefix || '-' || printf('%03d', q.number) = ?",
        )
        .bind(slug)
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        n > 0
    }

    /// snapshot 이후의 quest mutation(상태 변경 + 신규 생성)을 replay 가 재적용.
    #[tokio::test]
    async fn replay_reapplies_quest_ops_on_top_of_snapshot() {
        let dir = fresh_tmp("reapply");
        let store = setup(&dir).await;
        let tid = dev_type_id(&store).await;

        // base: DEV-001 생성 후 snapshot (journal truncate).
        let q1 = ops::quests::create_quest(&store, new_dev(tid, "first"))
            .await
            .unwrap();
        assert_eq!(q1.quest_id, "DEV-001");
        let snap = snapshot::create_snapshot(&store).await.unwrap();

        // snapshot 이후 mutation: DEV-001 → testing, DEV-002 신규.
        ops::quests::change_status(
            &store,
            q1.id,
            ChangeStatusRequest {
                status_slug: "testing".into(),
            },
        )
        .await
        .unwrap();
        ops::quests::create_quest(&store, new_dev(tid, "second"))
            .await
            .unwrap();
        assert_eq!(
            status_of(&store, "DEV-001").await.as_deref(),
            Some("testing")
        );
        assert!(exists(&store, "DEV-002").await);

        // 먼 미래로 replay → snapshot(DEV-001 open, DEV-002 없음) 복원 후 ops 재적용.
        let report = replay_to(&store, &snap, "9999-12-31T23:59:59Z")
            .await
            .unwrap();
        assert_eq!(report.applied, 2, "change_status + create_quest 2개");
        assert_eq!(
            status_of(&store, "DEV-001").await.as_deref(),
            Some("testing"),
            "replay 가 status 재적용 (id→slug→id 변환 포함)"
        );
        assert!(exists(&store, "DEV-002").await, "replay 가 create 재적용");
        assert_eq!(
            journal::count(&store.journal_pool).await.unwrap(),
            0,
            "replay 후 journal 은 새 baseline 으로 truncate"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// snapshot 직전 시점(=ops 이전)으로 replay 하면 어떤 op 도 적용 안 됨 → snapshot 상태.
    #[tokio::test]
    async fn replay_before_any_op_yields_snapshot_state() {
        let dir = fresh_tmp("point");
        let store = setup(&dir).await;
        let tid = dev_type_id(&store).await;

        ops::quests::create_quest(&store, new_dev(tid, "first"))
            .await
            .unwrap();
        let snap = snapshot::create_snapshot(&store).await.unwrap();
        ops::quests::create_quest(&store, new_dev(tid, "second"))
            .await
            .unwrap();
        assert!(exists(&store, "DEV-002").await);

        // target = 1970 (모든 journal op 이전) → 0개 적용 → snapshot 상태.
        let report = replay_to(&store, &snap, "1970-01-01T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(report.applied, 0);
        assert!(exists(&store, "DEV-001").await);
        assert!(
            !exists(&store, "DEV-002").await,
            "snapshot 이후 생성된 DEV-002 는 미적용"
        );

        // DEV-212: 파괴적 replay(과거 시점) 전 현재 상태가 자동 백업됨 —
        // 그 스냅샷으로 restore 하면 폐기됐던 DEV-002 가 되살아난다(가역화).
        let pre = report
            .pre_backup
            .expect("journal 이 있었으므로 자동 백업 생성");
        let pre_snap = snapshot::list_snapshots(&store.paths)
            .unwrap()
            .into_iter()
            .find(|s| s.timestamp == pre)
            .expect("자동 백업이 backup 목록에 존재해야");
        snapshot::restore_snapshot(&store, &pre_snap).await.unwrap();
        assert!(
            exists(&store, "DEV-002").await,
            "pre_backup 복원으로 폐기 전 상태 복귀 (DEV-212)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-212: journal 이 비어 있으면 자동 백업 스킵 (잃을 것 없음 — 멱등).
    #[tokio::test]
    async fn replay_skips_pre_backup_when_journal_empty() {
        let dir = fresh_tmp("nopre");
        let store = setup(&dir).await;
        let tid = dev_type_id(&store).await;

        ops::quests::create_quest(&store, new_dev(tid, "only"))
            .await
            .unwrap();
        // snapshot 생성 = journal truncate → 빈 journal 상태.
        let snap = snapshot::create_snapshot(&store).await.unwrap();
        let before = snapshot::list_snapshots(&store.paths).unwrap().len();

        let report = replay_to(&store, &snap, "9999-12-31T23:59:59Z")
            .await
            .unwrap();
        assert!(report.pre_backup.is_none(), "빈 journal — 자동 백업 불필요");
        assert_eq!(
            snapshot::list_snapshots(&store.paths).unwrap().len(),
            before,
            "스냅샷 수 불변"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 캠페인 구조 op(링크 + 체크리스트)을 replay 가 재적용 — campaign_id 변환 검증.
    #[tokio::test]
    async fn replay_reapplies_campaign_structural_ops() {
        let dir = fresh_tmp("camp");
        let store = setup(&dir).await;
        let tid = dev_type_id(&store).await;

        // base: 캠페인 C-001 + 퀘스트 DEV-001, snapshot.
        let camp = ops::campaigns::create_campaign(
            &store,
            CreateCampaignRequest {
                title: "camp".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(camp.campaign_slug, "C-001");
        ops::quests::create_quest(&store, new_dev(tid, "q1"))
            .await
            .unwrap();
        let snap = snapshot::create_snapshot(&store).await.unwrap();

        // snapshot 이후: 링크 + 체크리스트.
        ops::campaigns::link_quest_by_slug(&store, camp.id, "DEV-001")
            .await
            .unwrap();
        ops::campaigns::add_checklist_line(&store, camp.id, "item one")
            .await
            .unwrap();

        let report = replay_to(&store, &snap, "9999-12-31T23:59:59Z")
            .await
            .unwrap();
        assert_eq!(report.applied, 2, "link + checklist_add 2개");

        let linked: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id
             WHERE c.campaign_slug = 'C-001'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(linked, 1, "replay 가 campaign_id 변환 후 링크 재적용");

        let items: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM campaign_checklists cc JOIN campaigns c ON c.id = cc.campaign_id
             WHERE c.campaign_slug = 'C-001'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(items, 1, "replay 가 체크리스트 재적용");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
