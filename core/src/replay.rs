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
    AddPrerequisiteRequest, ChangeParentRequest, ChangeStatusRequest, CreateQuestRequest,
    UpdateQuestRequest,
};
use crate::ops;
use crate::snapshot::{self, SnapshotInfo};
use crate::store::{journal, Store};

/// replay 결과 요약.
#[derive(Debug, Clone)]
pub struct ReplayReport {
    /// 적용된 op 수.
    pub applied: usize,
    /// 복원 목표 시점(포함). journal `ts` 와 동일 포맷(ISO8601 UTC).
    pub target_ts: String,
}

/// restore 직전 현재 index.db 에서 떠두는 id → slug 맵.
struct IdMaps {
    /// index.db quest id → slug (예: "DEV-001"). 삭제된 quest 포함.
    quest: HashMap<i64, String>,
}

async fn build_id_maps(store: &Store) -> AppResult<IdMaps> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT q.id, qt.prefix || '-' || printf('%03d', q.number)
         FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id",
    )
    .fetch_all(&store.index_pool)
    .await?;
    Ok(IdMaps {
        quest: rows.into_iter().collect(),
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
    let to_apply: Vec<journal::OpRow> = all
        .into_iter()
        .filter(|o| o.ts.as_str() <= target_ts)
        .collect();

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
            let raw: Vec<i64> = serde_json::from_value(args["cascade_ids"].clone())
                .unwrap_or_default();
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
