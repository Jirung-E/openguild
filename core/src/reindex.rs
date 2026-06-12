//! `.guild/quests/*.md` + `.guild/types/*.toml` + `.guild/statuses/*.toml` 파일들로부터
//! `.guild/index.db` 의 캐시 내용을 재구축.
//!
//! 사용 시나리오:
//! - 외부 편집 (사용자가 .md 파일 직접 수정) 후 캐시 동기화
//! - git pull 후 변경된 파일들 반영
//! - index.db 손상 / 삭제 후 복구
//!
//! 알고리즘:
//! 1. 현재 index.db 의 quests / dependencies / counters 비움 (DELETE).
//! 2. types/ 파일들을 quest_types 에 INSERT (id 는 prefix 알파벳 순서).
//! 3. statuses/ 파일들을 quest_statuses 에 INSERT (sort_order 보존).
//! 4. quests/ 파일들을 quests 에 INSERT (id 는 type + number 로 유추 — 충돌 없게).
//! 5. dependencies 는 quest frontmatter 의 prerequisites 에서 빌드.
//! 6. counters 는 types/{prefix}.toml 의 [counter].last_number 에서 가져옴.

use std::collections::HashMap;

use crate::error::AppResult;
use crate::repo::{auto, fs as repo_fs, CampaignFile, QuestFile, StatusFile, TypeFile};
use crate::store::Store;

#[derive(Debug, Default, Clone)]
pub struct ReindexReport {
    pub types_loaded: usize,
    pub statuses_loaded: usize,
    pub quests_loaded: usize,
    pub dependencies_loaded: usize,
    /// reindex 전후로 살아남은 quest 의 board 위치 복원 수.
    pub positions_restored: usize,
    /// DEV-011: campaign 파일 로드 수.
    pub campaigns_loaded: usize,
    /// DEV-102: sibling `{slug}.comments.md` 의 entry 수 (모든 quest 합산).
    pub comments_loaded: usize,
    /// DEV-102: sibling `{slug}.memo.md` 의 quest 수 (file 하나당 row 1개).
    pub memos_loaded: usize,
    /// DEV-068: frontmatter tags 에서 적재된 tag 수 (quest 전체 합산, 중복 dedupe 후).
    pub tags_loaded: usize,
    /// 파싱 / 무결성 실패로 skip 된 파일 (경로 + 사유).
    pub skipped: Vec<(String, String)>,
}

/// 메인 진입점.
pub async fn reindex(store: &Store) -> AppResult<ReindexReport> {
    let mut report = ReindexReport::default();
    let pool = &store.index_pool;
    let paths = &store.paths;

    // 0. position 백업 — reindex 가 quest 정수 id 를 재배정하므로 slug 기준으로 보관.
    //    position 은 UI 상태라 파일에 저장 안 함 → reindex 가 wipe 하면 그대로 사라짐.
    //    quest 자체가 (slug 기준으로) 살아남으면 position 도 보존되어야 함.
    let position_backup: Vec<(String, f64, f64)> = sqlx::query_as(
        "SELECT t.prefix || '-' || printf('%03d', q.number) AS slug, p.x, p.y
           FROM quest_positions p
           JOIN quests q ON q.id = p.quest_id
           JOIN quest_types t ON t.id = q.quest_type_id",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // 1. 기존 내용 비움 (트랜잭션 안에서 — partial 실패 시 rollback).
    let mut tx = pool.begin().await?;

    // BUG-042: FK 위반 (787) 회피. reindex 의 quest INSERT 는 파일 정렬 순이라
    // parent_quest_id 가 자기보다 뒤에 들어갈 quest 를 가리키면 즉시 FK 검증에서
    // 실패. `PRAGMA defer_foreign_keys = 1` 로 transaction commit 시점에만 검증.
    // 이 PRAGMA 는 transaction 범위 — commit 후 자동 해제.
    sqlx::query("PRAGMA defer_foreign_keys = 1")
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM quest_dependencies").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quest_positions").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quests").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quest_counters").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quest_statuses").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quest_types").execute(&mut *tx).await?;
    // DEV-011: campaigns 관련 — 마이그레이션 0008 적용 후에만 존재.
    // 테이블이 아직 없는 환경 (구 DB) 대비 IF EXISTS 안 됨 → migration 으로
    // 보장된 후에만 reindex 실행되도록. 트랜잭션 안에서 안전.
    sqlx::query("DELETE FROM campaign_quests").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM campaign_checklists").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM campaigns").execute(&mut *tx).await?;
    sqlx::query("UPDATE campaign_counters SET last_number = 0 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    // DEV-102: 댓글 / 메모 캐시도 wipe — sibling 파일로부터 재구축.
    sqlx::query("DELETE FROM quest_comments").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM quest_memos").execute(&mut *tx).await?;
    // DEV-068: 태그 캐시도 wipe — frontmatter 의 tags 배열로부터 재구축.
    sqlx::query("DELETE FROM quest_tags").execute(&mut *tx).await?;
    // DEV-068 (tag defs): `.guild/tags/*.toml` 정의도 wipe + 재적재.
    sqlx::query("DELETE FROM quest_tag_defs").execute(&mut *tx).await?;
    // DEV-134: 캠페인 댓글 / 메모 캐시도 wipe — sibling 파일로부터 재구축.
    sqlx::query("DELETE FROM campaign_comments").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM campaign_memos").execute(&mut *tx).await?;

    // 2. types — id 는 파일 정렬 순.
    let type_paths = repo_fs::list_with_extension(paths.types_dir(), "toml")
        .map_err(crate::error::AppError::Internal)?;
    let mut prefix_to_id: HashMap<String, i64> = HashMap::new();
    for (i, path) in type_paths.iter().enumerate() {
        let id = (i + 1) as i64;
        match TypeFile::read(path) {
            Ok(t) => {
                sqlx::query(
                    "INSERT INTO quest_types (id, prefix, color, description) VALUES (?, ?, ?, ?)",
                )
                .bind(id)
                .bind(&t.prefix)
                .bind(&t.color)
                .bind(&t.description)
                .execute(&mut *tx)
                .await?;
                // counter
                sqlx::query(
                    "INSERT INTO quest_counters (quest_type_id, last_number) VALUES (?, ?)",
                )
                .bind(id)
                .bind(t.counter.last_number)
                .execute(&mut *tx)
                .await?;
                prefix_to_id.insert(t.prefix.clone(), id);
                report.types_loaded += 1;
            }
            Err(e) => {
                report.skipped.push((path.display().to_string(), format!("{e:#}")));
            }
        }
    }

    // 3. statuses — id 는 파일 정렬 순 (파일명 prefix 가 정렬 기준 = sort_order 동일).
    let status_paths = repo_fs::list_with_extension(paths.statuses_dir(), "toml")
        .map_err(crate::error::AppError::Internal)?;
    let mut slug_to_status_id: HashMap<String, i64> = HashMap::new();
    for (i, path) in status_paths.iter().enumerate() {
        let id = (i + 1) as i64;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let slug = StatusFile::slug_from_filename(filename).unwrap_or(filename);
        match StatusFile::read(path) {
            Ok(s) => {
                // DEV-042: slug 컬럼도 함께 INSERT — quest_history 가 slug 기반.
                // DEV-093: counts_as_done 도 file → DB sync.
                //
                // DEV-093 fix (사용자 보고: '연결 quest done 3 인데 진척도 0'):
                // 기존 길드의 status file 에 counts_as_done 키 자체가 없음 (옛 형식).
                // TOML 의 default false 가 들어가 migration 0012 의 backfill 가 reindex
                // 직후 사라짐. file 에 키 누락 + slug 가 done/cancelled 면 자동 true
                // (직관 일치 — '완료된 상태는 진행도 카운트'). file 에 명시 false 가
                // 들어와 있으면 그대로 false (사용자 의도 보존 — 단 TOML default
                // false 와 명시 false 를 구분 못해 best-effort).
                let counts_as_done = s.counts_as_done
                    || matches!(slug, "done" | "cancelled");
                sqlx::query(
                    "INSERT INTO quest_statuses (id, name_en, name_ko, color, sort_order, slug, counts_as_done)
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(id)
                .bind(&s.name_en)
                .bind(&s.name_ko)
                .bind(&s.color)
                .bind(s.sort_order)
                .bind(slug)
                .bind(counts_as_done as i64)
                .execute(&mut *tx)
                .await?;
                slug_to_status_id.insert(slug.to_string(), id);
                report.statuses_loaded += 1;
            }
            Err(e) => {
                report.skipped.push((path.display().to_string(), format!("{e:#}")));
            }
        }
    }

    // 4. quests — 파일 한 번 로드해서 모두 메모리에. id 는 파일 정렬 순.
    // BUG-047: sibling `.comments.md` / `.memo.md` 제외 — 이전엔 quest 본문으로
    // 오인해서 매 reindex 마다 "frontmatter 없음" skip 경고 발생.
    let quest_paths = repo_fs::list_quest_body_files(paths.quests_dir())
        .map_err(crate::error::AppError::Internal)?;
    let mut quest_files: Vec<(std::path::PathBuf, QuestFile)> = Vec::new();
    for path in &quest_paths {
        match QuestFile::read(path) {
            Ok(qf) => quest_files.push((path.clone(), qf)),
            Err(e) => {
                report.skipped.push((path.display().to_string(), format!("{e:#}")));
            }
        }
    }

    // slug → (id, parent_quest_id Option<i64>, prereq slugs)
    let mut slug_to_id: HashMap<String, i64> = HashMap::new();
    for (i, (_, qf)) in quest_files.iter().enumerate() {
        slug_to_id.insert(qf.frontmatter.quest_id.clone(), (i + 1) as i64);
    }

    for (i, (path, qf)) in quest_files.iter().enumerate() {
        let id = (i + 1) as i64;
        let prefix = qf.type_prefix().unwrap_or("").to_string();
        let Some(type_id) = prefix_to_id.get(&prefix).copied() else {
            report.skipped.push((
                path.display().to_string(),
                format!("unknown type prefix: {prefix}"),
            ));
            continue;
        };
        let number = match qf.number() {
            Ok(n) => n,
            Err(e) => {
                report.skipped.push((path.display().to_string(), format!("{e:#}")));
                continue;
            }
        };
        let Some(status_id) = slug_to_status_id.get(&qf.frontmatter.status).copied() else {
            report.skipped.push((
                path.display().to_string(),
                format!("unknown status slug: {}", qf.frontmatter.status),
            ));
            continue;
        };
        let parent_id = qf
            .frontmatter
            .parent
            .as_ref()
            .and_then(|s| slug_to_id.get(s).copied());

        // DEV-041: legacy ".md" 의 공백-구분 timestamp 는 migration 0005 와 일관되게
        // ISO 8601 UTC 로 normalize 한 뒤 db 에 기록. 새 format 은 그대로 통과.
        let created_at = crate::time::normalize_legacy_ts(&qf.frontmatter.created_at);
        let updated_at = crate::time::normalize_legacy_ts(&qf.frontmatter.updated_at);
        let deleted_at: Option<String> = qf
            .frontmatter
            .deleted
            .then(|| updated_at.clone());

        // BUG-060 후속: urgency 범위 (1..=4) 밖이면 4 (Low) 로 clamp + skipped
        // 에 경고. 이전엔 silent INSERT — invalid 값이 GUI 까지 흘러가 보드
        // 폭발의 원인이 됐음 (GUI 측 방어는 BUG-060 본 fix). quest 자체는
        // 잃지 않도록 skip 이 아닌 clamp.
        let urgency = if (1..=4).contains(&qf.frontmatter.urgency) {
            qf.frontmatter.urgency
        } else {
            report.skipped.push((
                path.display().to_string(),
                format!(
                    "urgency {} 가 유효 범위 (1..=4) 밖 — 4 (Low) 로 clamp 하여 적재. 파일 정정 권장.",
                    qf.frontmatter.urgency
                ),
            ));
            4
        };

        // DEV-076: desired_due / required_due 도 함께 적재 (file → DB sync).
        // DEV-121: cached_mtime 도 함께 — 이후 incremental sync 가 정확히 비교.
        let cached_mtime = crate::repo::fs::mtime_unix_nanos(path);
        sqlx::query(
            "INSERT INTO quests
             (id, quest_type_id, number, title, description, status_id, urgency, parent_quest_id,
              created_at, updated_at, deleted_at, desired_due, required_due, cached_mtime)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(type_id)
        .bind(number)
        .bind(&qf.frontmatter.title)
        .bind(&qf.description)
        .bind(status_id)
        .bind(urgency)
        .bind(parent_id)
        .bind(&created_at)
        .bind(&updated_at)
        .bind(deleted_at)
        .bind(qf.frontmatter.desired_due.as_deref())
        .bind(qf.frontmatter.required_due.as_deref())
        .bind(cached_mtime)
        .execute(&mut *tx)
        .await?;

        report.quests_loaded += 1;
    }

    // 5. dependencies — 각 quest 의 prerequisites 에서. DEV-068: 같은 loop 에서
    //    tags 도 적재 (per-quest 작업 묶음).
    for (_, qf) in &quest_files {
        let Some(qid) = slug_to_id.get(&qf.frontmatter.quest_id).copied() else {
            continue;
        };

        // DEV-068: tags — frontmatter 의 tags 배열 → quest_tags. 중복은
        // PRIMARY KEY (quest_id, tag) 가 막아주지만 정렬 안정성 위해
        // dedupe 후 INSERT OR IGNORE.
        for tag in &qf.frontmatter.tags {
            let normalized = tag.trim();
            if normalized.is_empty() {
                continue;
            }
            sqlx::query(
                "INSERT OR IGNORE INTO quest_tags (quest_id, tag) VALUES (?, ?)",
            )
            .bind(qid)
            .bind(normalized)
            .execute(&mut *tx)
            .await?;
            report.tags_loaded += 1;
        }

        for pslug in &qf.frontmatter.prerequisites {
            let Some(pid) = slug_to_id.get(pslug).copied() else {
                continue;
            };
            sqlx::query(
                "INSERT OR IGNORE INTO quest_dependencies (quest_id, prerequisite_id) VALUES (?, ?)",
            )
            .bind(qid)
            .bind(pid)
            .execute(&mut *tx)
            .await?;
            report.dependencies_loaded += 1;
        }
    }

    // 5b. position 복원 — 0 단계에서 백업한 slug → 새 quest id 로 재INSERT.
    //     slug 가 reindex 후에도 살아있는 quest 만 복원.
    //     DEV-049: quest_slug 도 함께 INSERT (stable identifier).
    let mut positions_restored = 0usize;
    for (slug, x, y) in &position_backup {
        if let Some(&qid) = slug_to_id.get(slug) {
            sqlx::query(
                "INSERT INTO quest_positions (quest_id, quest_slug, x, y) VALUES (?, ?, ?, ?)",
            )
            .bind(qid)
            .bind(slug)
            .bind(x)
            .bind(y)
            .execute(&mut *tx)
            .await?;
            positions_restored += 1;
        }
    }
    report.positions_restored = positions_restored;

    // 5b''. DEV-068 (tag defs): `.guild/tags/{slug}.toml` 파일들을 quest_tag_defs
    //       에 적재. file 진리원, DB 는 캐시.
    let tag_paths = repo_fs::list_with_extension(paths.tags_dir(), "toml")
        .map_err(crate::error::AppError::Internal)?;
    for path in &tag_paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        match crate::repo::TagFile::read(path) {
            Ok(tf) => {
                sqlx::query(
                    "INSERT INTO quest_tag_defs (slug, color, description) VALUES (?, ?, ?)
                     ON CONFLICT(slug) DO UPDATE SET color = excluded.color, description = excluded.description",
                )
                .bind(stem)
                .bind(&tf.color)
                .bind(&tf.description)
                .execute(&mut *tx)
                .await?;
            }
            Err(e) => {
                report.skipped.push((path.display().to_string(), format!("{e:#}")));
            }
        }
    }

    // 5b'. DEV-102: sibling 파일 (`{slug}.comments.md` / `{slug}.memo.md`) →
    //      `quest_comments` / `quest_memos` 캐시 sync. 파일이 진리원, 캐시는
    //      snapshot 백업 대상. quest 가 존재하지 않는 sibling 은 skip + 경고.
    let comment_paths = repo_fs::list_quest_comment_files(paths.quests_dir())
        .map_err(crate::error::AppError::Internal)?;
    for path in &comment_paths {
        let Some(slug) = repo_fs::quest_slug_from_sibling_path(path) else {
            continue;
        };
        let Some(&qid) = slug_to_id.get(&slug) else {
            report.skipped.push((
                path.display().to_string(),
                format!("no matching quest for comment sibling slug={slug}"),
            ));
            continue;
        };
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                report.skipped.push((path.display().to_string(), e.to_string()));
                continue;
            }
        };
        let entries = crate::repo::comments::parse_entries(&raw);
        for entry in entries {
            sqlx::query(
                "INSERT INTO quest_comments (quest_id, entry_id, ts, author, body, parent_id)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(qid)
            .bind(entry.id as i64)
            .bind(&entry.ts)
            .bind(&entry.author)
            .bind(&entry.body)
            .bind(entry.parent_id.map(|n| n as i64))
            .execute(&mut *tx)
            .await?;
            report.comments_loaded += 1;
        }
    }

    let memo_paths = repo_fs::list_quest_memo_files(paths.quests_dir())
        .map_err(crate::error::AppError::Internal)?;
    for path in &memo_paths {
        let Some(slug) = repo_fs::quest_slug_from_sibling_path(path) else {
            continue;
        };
        let Some(&qid) = slug_to_id.get(&slug) else {
            report.skipped.push((
                path.display().to_string(),
                format!("no matching quest for memo sibling slug={slug}"),
            ));
            continue;
        };
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                report.skipped.push((path.display().to_string(), e.to_string()));
                continue;
            }
        };
        // updated_at — file mtime 으로 근사. mutation 경로는 services 가
        // now_local_iso8601() 로 정확히 설정. single-user 단계라 user_id=0.
        let updated_at = repo_fs::mtime_iso8601(path).unwrap_or_default();
        sqlx::query(
            "INSERT INTO quest_memos (quest_id, user_id, content, updated_at)
             VALUES (?, 0, ?, ?)",
        )
        .bind(qid)
        .bind(&content)
        .bind(&updated_at)
        .execute(&mut *tx)
        .await?;
        report.memos_loaded += 1;
    }

    // 5c. quest_history 의 quest_id 재정렬 (DEV-049).
    //     이전 reindex 가 id 를 재할당했어도 quest_slug 컬럼이 stable identifier
    //     로 살아있음. quest_id 컬럼은 새 매핑으로 갱신.
    //
    // BUG-043: WHERE 절에 EXISTS 추가. 사용자가 quest 파일을 hard-delete 하면
    // 그 slug 에 해당하는 row 가 quests 에 없음 → subselect 가 NULL → NOT NULL
    // 컬럼에 NULL UPDATE → "NOT NULL constraint failed: quest_history.quest_id"
    // (sqlite 1299). 매칭 안 되는 history 행은 기존 quest_id 유지 (stale 가능,
    // 단 NULL 안 됨). 그 history 가 가리키던 quest 가 진짜로 사라졌다면 stale
    // FK 이지만 quest_history 에는 FK constraint 없으므로 dangling reference 로
    // 남아도 read-only 표시용 데이터라 무해.
    sqlx::query(
        "UPDATE quest_history
         SET quest_id = (
             SELECT q.id
             FROM quests q
             JOIN quest_types qt ON q.quest_type_id = qt.id
             WHERE qt.prefix || '-' || printf('%03d', q.number) = quest_history.quest_slug
         )
         WHERE quest_slug IS NOT NULL
           AND EXISTS (
             SELECT 1 FROM quests q
             JOIN quest_types qt ON q.quest_type_id = qt.id
             WHERE qt.prefix || '-' || printf('%03d', q.number) = quest_history.quest_slug
           )",
    )
    .execute(&mut *tx)
    .await?;

    // 6. DEV-011: campaigns — `.guild/campaigns/*.md` 파일 정렬 순.
    let campaigns_dir = paths.campaigns_dir();
    let mut max_camp_num: i64 = 0;
    // DEV-134: sibling (댓글/메모) 적재용 slug → id 맵.
    let mut camp_slug_to_id: HashMap<String, i64> = HashMap::new();
    if campaigns_dir.exists() {
        // DEV-134: BUG-047 의 캠페인판 — DEV-100 의 sibling `.comments.md` /
        // `.memo.md` 를 캠페인 본문으로 오인하지 않도록 body-file 필터 사용.
        // (헬퍼는 quest 명명이지만 디렉토리 무관 — stem 에 '.' 없는 .md 만.)
        let camp_paths = repo_fs::list_quest_body_files(&campaigns_dir)
            .map_err(crate::error::AppError::Internal)?;
        for (i, path) in camp_paths.iter().enumerate() {
            let id = (i + 1) as i64;
            let cf = match CampaignFile::read(path) {
                Ok(c) => c,
                Err(e) => {
                    report.skipped.push((path.display().to_string(), format!("{e:#}")));
                    continue;
                }
            };
            if cf.frontmatter.deleted {
                // soft-deleted 는 reindex 에서 스킵 (alive 만 캐싱).
                continue;
            }
            sqlx::query(
                "INSERT INTO campaigns
                    (id, campaign_slug, title, description, status,
                     started_at, ended_at, display_order, image_path, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(&cf.frontmatter.campaign_id)
            .bind(&cf.frontmatter.title)
            .bind(&cf.body)
            .bind(&cf.frontmatter.status)
            .bind(if cf.frontmatter.started_at.is_empty() {
                None
            } else {
                Some(&cf.frontmatter.started_at)
            })
            .bind(if cf.frontmatter.ended_at.is_empty() {
                None
            } else {
                Some(&cf.frontmatter.ended_at)
            })
            .bind(cf.frontmatter.display_order)
            // DEV-087: 배너 이미지 — frontmatter 가 진리원.
            .bind(cf.frontmatter.image.as_deref())
            .bind(&cf.frontmatter.created_at)
            .bind(&cf.frontmatter.updated_at)
            .execute(&mut *tx)
            .await?;

            // 체크리스트
            let items = crate::repo::extract_checklist_items(&cf.body);
            for line in &items {
                sqlx::query(
                    "INSERT INTO campaign_checklists (campaign_id, text, checked, order_idx)
                     VALUES (?, ?, ?, ?)",
                )
                .bind(id)
                .bind(&line.text)
                .bind(if line.checked { 1 } else { 0 })
                .bind(line.order_idx)
                .execute(&mut *tx)
                .await?;
            }

            // linked_quests: slug → quest id resolve.
            for qslug in &cf.frontmatter.linked_quests {
                if let Some(&qid) = slug_to_id.get(qslug) {
                    sqlx::query(
                        "INSERT OR IGNORE INTO campaign_quests (campaign_id, quest_id) VALUES (?, ?)",
                    )
                    .bind(id)
                    .bind(qid)
                    .execute(&mut *tx)
                    .await?;
                }
                // unresolved slug 는 silent skip — quest 가 삭제됐을 수도.
            }

            // campaign slug 의 숫자 부분 max 추적 (counter self-heal).
            if let Some(num_str) = cf.frontmatter.campaign_id.strip_prefix("C-")
                && let Ok(n) = num_str.parse::<i64>()
            {
                max_camp_num = max_camp_num.max(n);
            }

            camp_slug_to_id.insert(cf.frontmatter.campaign_id.clone(), id);
            report.campaigns_loaded += 1;
        }

        // 6b. DEV-134: 캠페인 sibling (`{slug}.comments.md` / `{slug}.memo.md`)
        //     → campaign_comments / campaign_memos 캐시. DEV-102 의 미러.
        let camp_comment_paths = repo_fs::list_quest_comment_files(&campaigns_dir)
            .map_err(crate::error::AppError::Internal)?;
        for path in &camp_comment_paths {
            let Some(slug) = repo_fs::quest_slug_from_sibling_path(path) else {
                continue;
            };
            let Some(&cid) = camp_slug_to_id.get(&slug) else {
                report.skipped.push((
                    path.display().to_string(),
                    format!("no matching campaign for comment sibling slug={slug}"),
                ));
                continue;
            };
            let raw = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    report.skipped.push((path.display().to_string(), e.to_string()));
                    continue;
                }
            };
            for entry in crate::repo::comments::parse_entries(&raw) {
                sqlx::query(
                    "INSERT INTO campaign_comments (campaign_id, entry_id, ts, author, body, parent_id)
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(cid)
                .bind(entry.id as i64)
                .bind(&entry.ts)
                .bind(&entry.author)
                .bind(&entry.body)
                .bind(entry.parent_id.map(|n| n as i64))
                .execute(&mut *tx)
                .await?;
                report.comments_loaded += 1;
            }
        }

        let camp_memo_paths = repo_fs::list_quest_memo_files(&campaigns_dir)
            .map_err(crate::error::AppError::Internal)?;
        for path in &camp_memo_paths {
            let Some(slug) = repo_fs::quest_slug_from_sibling_path(path) else {
                continue;
            };
            let Some(&cid) = camp_slug_to_id.get(&slug) else {
                report.skipped.push((
                    path.display().to_string(),
                    format!("no matching campaign for memo sibling slug={slug}"),
                ));
                continue;
            };
            let content = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    report.skipped.push((path.display().to_string(), e.to_string()));
                    continue;
                }
            };
            let updated_at = repo_fs::mtime_iso8601(path).unwrap_or_default();
            sqlx::query(
                "INSERT INTO campaign_memos (campaign_id, user_id, content, updated_at)
                 VALUES (?, 0, ?, ?)",
            )
            .bind(cid)
            .bind(&content)
            .bind(&updated_at)
            .execute(&mut *tx)
            .await?;
            report.memos_loaded += 1;
        }
    }
    // campaign_counters self-heal — alive campaign 의 최대 번호로.
    if max_camp_num > 0 {
        sqlx::query(
            "UPDATE campaign_counters
                SET last_number = MAX(last_number, ?)
              WHERE id = 1",
        )
        .bind(max_camp_num)
        .execute(&mut *tx)
        .await?;
    }

    // BUG-059: drift detection 의 신뢰 가능한 시간 기준 — reindex 종료 시점을
    // ISO 타임스탬프로 app_meta 에 기록. 다음 startup 의 detect_drift 가 이 값과
    // file mtime 을 비교 → SQLite WAL / Store::open 의 mtime 부작용에 영향 없음.
    let last_indexed_at = crate::time::now_local_iso8601();
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES ('last_indexed_at', ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(&last_indexed_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // 7. auto 블록을 SQL 기준으로 다시 그려서 파일에 쓰기 — 외부 편집 결과
    //    auto 블록이 stale 일 수 있음. write_consistent_auto_blocks 가 옵션.
    //    (현재 turn 에선 단순 reindex 만, auto 갱신은 호출자가 별도 호출 가능)
    let _ = auto::render; // keep import alive
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{seed_guild_dir, GuildPaths, QuestFile, QuestFrontmatter};

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-reindex-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup_store(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        Store::open(dir).await.unwrap()
    }

    /// BUG-047 regression: sibling `.comments.md` / `.memo.md` 파일이 있어도
    /// reindex 가 `skipped` 에 누적하지 않아야 함 (이전엔 frontmatter 없는 파일
    /// 로 인식해 매번 "missing opening +++ delimiter" 경고).
    #[tokio::test]
    async fn reindex_ignores_sibling_comment_and_memo_files() {
        let dir = fresh_tmp("siblings");
        let store = setup_store(&dir).await;

        // 정상 quest 파일 + sibling 두 종류 직접 작성.
        let paths = store.paths.clone();
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "with siblings".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path("DEV-001")).unwrap();

        // sibling — frontmatter 없는 plain markdown (DEV-012 / DEV-094 패턴).
        let comments = paths.quests_dir().join("DEV-001.comments.md");
        let memo = paths.quests_dir().join("DEV-001.memo.md");
        std::fs::write(&comments, "<!-- og-comment id=\"1\" ts=\"\" author=\"\" -->\ntest comment\n").unwrap();
        std::fs::write(&memo, "personal scratch").unwrap();

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.quests_loaded, 1, "정상 quest 1 개만 로드되어야");
        assert!(
            report.skipped.is_empty(),
            "sibling 파일들이 skipped 에 안 들어가야: {:?}",
            report.skipped
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-102: sibling 파일이 `quest_comments` / `quest_memos` 캐시로
    /// 적재되는지. snapshot 의 백업 대상이 되도록 (snapshot 은 index.db binary
    /// copy 라 cache 테이블만 보장).
    #[tokio::test]
    async fn reindex_loads_sibling_files_into_cache_tables() {
        let dir = fresh_tmp("sibling-cache");
        let store = setup_store(&dir).await;
        let paths = store.paths.clone();

        // 두 개 quest — 첫째는 댓글 2개 (top-level + reply) + 메모, 둘째는 메모만.
        for slug in ["DEV-001", "DEV-002"] {
            let qf = QuestFile {
                frontmatter: QuestFrontmatter {
                    quest_id: slug.into(),
                    title: "t".into(),
                    status: "open".into(),
                    urgency: 3,
                    parent: None,
                    prerequisites: vec![],
                    created_at: "2026-01-01T00:00:00Z".into(),
                    updated_at: "2026-01-01T00:00:00Z".into(),
                    deleted: false,
                    desired_due: None,
                    required_due: None,
                    tags: vec![],
                },
                description: String::new(),
                auto_block: String::new(),
            };
            qf.write(paths.quest_path(slug)).unwrap();
        }

        let comments_001 = paths.quests_dir().join("DEV-001.comments.md");
        std::fs::write(
            &comments_001,
            "<!-- og-comment id=\"1\" ts=\"2026-06-01T10:00:00+09:00\" author=\"alice\" -->\n\
             top-level\n\
             <!-- og-comment id=\"2\" ts=\"2026-06-01T11:00:00+09:00\" author=\"bob\" reply_to=\"1\" -->\n\
             answer\n",
        )
        .unwrap();
        let memo_001 = paths.quests_dir().join("DEV-001.memo.md");
        std::fs::write(&memo_001, "scratch one").unwrap();
        let memo_002 = paths.quests_dir().join("DEV-002.memo.md");
        std::fs::write(&memo_002, "scratch two").unwrap();

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.comments_loaded, 2, "DEV-001 의 댓글 2개");
        assert_eq!(report.memos_loaded, 2, "DEV-001 + DEV-002 메모 각 1");
        assert!(report.skipped.is_empty(), "skipped 비어야: {:?}", report.skipped);

        // DB 확인: 댓글 row 들 정확히 들어갔는지.
        let rows: Vec<(i64, i64, String, String, String, Option<i64>)> = sqlx::query_as(
            "SELECT quest_id, entry_id, ts, author, body, parent_id
             FROM quest_comments
             ORDER BY quest_id, entry_id",
        )
        .fetch_all(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, 1);
        assert_eq!(rows[0].3, "alice");
        assert_eq!(rows[0].4, "top-level");
        assert_eq!(rows[0].5, None);
        assert_eq!(rows[1].1, 2);
        assert_eq!(rows[1].3, "bob");
        assert_eq!(rows[1].5, Some(1));

        // memo row 들 — user_id 모두 0 (single-user sentinel).
        let memo_rows: Vec<(i64, i64, String)> = sqlx::query_as(
            "SELECT quest_id, user_id, content FROM quest_memos ORDER BY quest_id",
        )
        .fetch_all(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(memo_rows.len(), 2);
        assert!(memo_rows.iter().all(|r| r.1 == 0));
        assert_eq!(memo_rows[0].2, "scratch one");
        assert_eq!(memo_rows[1].2, "scratch two");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-102: sibling 파일이 가리키는 quest 가 없으면 skip + 경고 — DELETE 도
    /// 실패하지 않고 다른 sync 와 격리.
    #[tokio::test]
    async fn reindex_skips_orphan_sibling_files() {
        let dir = fresh_tmp("sibling-orphan");
        let store = setup_store(&dir).await;
        let paths = store.paths.clone();

        // quest 파일 0개. sibling 파일만 존재 — 정상 quest 없으니 orphan.
        std::fs::write(
            paths.quests_dir().join("ZZZ-999.comments.md"),
            "<!-- og-comment id=\"1\" ts=\"\" author=\"\" -->\norphan\n",
        )
        .unwrap();
        std::fs::write(
            paths.quests_dir().join("ZZZ-999.memo.md"),
            "orphan memo",
        )
        .unwrap();

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.comments_loaded, 0);
        assert_eq!(report.memos_loaded, 0);
        // 둘 다 skipped 에 경고로.
        assert_eq!(report.skipped.len(), 2, "orphan sibling 2 건: {:?}", report.skipped);
        assert!(report.skipped.iter().all(|(_, msg)| msg.contains("no matching quest")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-102: 두 번 reindex 해도 quest_comments / quest_memos 가 중복되지
    /// 않아야 (DELETE 가 정상 동작).
    #[tokio::test]
    async fn reindex_twice_is_idempotent_for_siblings() {
        let dir = fresh_tmp("sibling-idem");
        let store = setup_store(&dir).await;
        let paths = store.paths.clone();

        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "t".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path("DEV-001")).unwrap();
        std::fs::write(
            paths.quests_dir().join("DEV-001.comments.md"),
            "<!-- og-comment id=\"1\" ts=\"\" author=\"\" -->\nbody\n",
        )
        .unwrap();
        std::fs::write(paths.quests_dir().join("DEV-001.memo.md"), "memo").unwrap();

        let r1 = reindex(&store).await.unwrap();
        let r2 = reindex(&store).await.unwrap();
        assert_eq!(r1.comments_loaded, 1);
        assert_eq!(r2.comments_loaded, 1, "재실행도 1 (중복 없음)");
        let n_c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_comments")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        let n_m: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_memos")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n_c, 1);
        assert_eq!(n_m, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-134: 캠페인 sibling (`C-001.comments.md` / `.memo.md`) 이
    /// (a) 캠페인 본문으로 오인되지 않고 (BUG-047 의 캠페인판)
    /// (b) campaign_comments / campaign_memos 캐시로 적재되는지.
    #[tokio::test]
    async fn reindex_loads_campaign_siblings_without_misparse() {
        let dir = fresh_tmp("camp-siblings");
        let store = setup_store(&dir).await;
        let paths = store.paths.clone();
        std::fs::create_dir_all(paths.campaigns_dir()).unwrap();

        // 캠페인 본문 + sibling 2종.
        std::fs::write(
            paths.campaigns_dir().join("C-001.md"),
            "+++\ncampaign_id = \"C-001\"\ntitle = \"camp\"\nstatus = \"active\"\ncreated_at = \"x\"\nupdated_at = \"x\"\n+++\nbody\n",
        )
        .unwrap();
        std::fs::write(
            paths.campaign_comments_path("C-001"),
            "<!-- og-comment id=\"1\" ts=\"2026-06-12T10:00:00+09:00\" author=\"alice\" -->\ncamp comment\n",
        )
        .unwrap();
        std::fs::write(paths.campaign_memo_path("C-001"), "camp memo").unwrap();

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.campaigns_loaded, 1, "본문 1개만 캠페인으로");
        assert!(
            report.skipped.is_empty(),
            "sibling 이 본문 오인 / skip 경고를 내면 안 됨: {:?}",
            report.skipped
        );
        let n_c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_comments")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        let n_m: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_memos")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!((n_c, n_m), (1, 1));

        // idempotent — 재실행에도 중복 없음.
        reindex(&store).await.unwrap();
        let n_c2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaign_comments")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n_c2, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reindex_seeded_guild_no_quests() {
        let dir = fresh_tmp("empty");
        let store = setup_store(&dir).await;

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.types_loaded, 3);
        assert_eq!(report.statuses_loaded, 7);
        assert_eq!(report.quests_loaded, 0);
        assert!(report.skipped.is_empty());

        // index.db 에 types/statuses 들어가있음
        let n_types: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM quest_types").fetch_one(&store.index_pool).await.unwrap();
        assert_eq!(n_types, 3);
        let n_statuses: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_statuses").fetch_one(&store.index_pool).await.unwrap();
        assert_eq!(n_statuses, 7);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reindex_from_quest_files() {
        let dir = fresh_tmp("from-files");
        setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        // 파일 직접 작성 (외부 편집 시뮬레이션)
        let q1 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "first".into(),
                status: "open".into(),
                urgency: 2,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-05-16T15:00:00Z".into(),
                updated_at: "2026-05-16T15:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: "body".into(),
            auto_block: String::new(),
        };
        q1.write(paths.quest_path("DEV-001")).unwrap();

        let q2 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-002".into(),
                title: "child".into(),
                status: "in_progress".into(),
                urgency: 3,
                parent: Some("DEV-001".into()),
                prerequisites: vec![],
                created_at: "2026-05-16T15:01:00Z".into(),
                updated_at: "2026-05-16T15:01:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        q2.write(paths.quest_path("DEV-002")).unwrap();

        // counter 갱신 (사용자가 직접 — last_number 2)
        let mut dev = TypeFile::read(paths.type_path("DEV")).unwrap();
        dev.counter.last_number = 2;
        dev.write(paths.type_path("DEV")).unwrap();

        // 새 Store — index.db 빈 상태에서 reindex
        let store = Store::open(&dir).await.unwrap();
        let report = reindex(&store).await.unwrap();
        assert_eq!(report.quests_loaded, 2);

        // index.db 검증
        let titles: Vec<String> = sqlx::query_scalar("SELECT title FROM quests ORDER BY id")
            .fetch_all(&store.index_pool).await.unwrap();
        assert_eq!(titles, vec!["first".to_string(), "child".to_string()]);

        // parent 링크 보존
        let parent_id: Option<i64> = sqlx::query_scalar(
            "SELECT parent_quest_id FROM quests WHERE title = 'child'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert!(parent_id.is_some(), "child should have parent");

        // counter 보존 — DEV 의 type_id 를 prefix 로 조회.
        let counter: i64 = sqlx::query_scalar(
            "SELECT c.last_number FROM quest_counters c
             JOIN quest_types t ON c.quest_type_id = t.id
             WHERE t.prefix = 'DEV'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(counter, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-060 후속: 범위 밖 urgency 는 4 로 clamp + skipped 경고. quest 자체는
    /// 적재 (잃지 않음).
    #[tokio::test]
    async fn reindex_clamps_out_of_range_urgency() {
        let dir = fresh_tmp("urgency-clamp");
        let store = setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        let q = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "bad urgency".into(),
                status: "open".into(),
                urgency: 99,
                parent: None,
                prerequisites: vec![],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        q.write(paths.quest_path("DEV-001")).unwrap();

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.quests_loaded, 1, "clamp 일 뿐 quest 는 적재");
        assert!(
            report.skipped.iter().any(|(_, msg)| msg.contains("urgency 99")),
            "skipped 에 경고: {:?}",
            report.skipped
        );
        let u: i64 = sqlx::query_scalar("SELECT urgency FROM quests WHERE number = 1")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(u, 4, "4 (Low) 로 clamp");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reindex_skips_invalid_files() {
        let dir = fresh_tmp("invalid");
        let store = setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        // 정상 quest + 손상 quest
        let good = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "ok".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        good.write(paths.quest_path("DEV-001")).unwrap();
        std::fs::write(paths.quest_path("BROKEN"), "not a quest file").unwrap();

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.quests_loaded, 1);
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].1.contains("opening"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reindex_preserves_positions_across_rebuild() {
        // BUG-002 regression — reindex 는 quest_positions 를 wipe 한 뒤
        // 동일 slug 의 quest 가 살아남으면 위치를 복원해야 한다.
        let dir = fresh_tmp("positions");
        let store = setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        let q1 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "p1".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        q1.write(paths.quest_path("DEV-001")).unwrap();

        // 첫 reindex → DEV-001 이 SQL 에 들어옴
        let r1 = reindex(&store).await.unwrap();
        assert_eq!(r1.quests_loaded, 1);
        assert_eq!(r1.positions_restored, 0, "처음엔 보존할 position 없음");

        // position 수동 INSERT (실제 update_position 호출과 동등)
        let qid: i64 =
            sqlx::query_scalar("SELECT id FROM quests WHERE deleted_at IS NULL LIMIT 1")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO quest_positions (quest_id, quest_slug, x, y) VALUES (?, 'DEV-001', 162.0, 108.0)",
        )
        .bind(qid)
        .execute(&store.index_pool)
        .await
        .unwrap();

        // 두번째 reindex → position 이 살아남아야 함
        let r2 = reindex(&store).await.unwrap();
        assert_eq!(r2.quests_loaded, 1);
        assert_eq!(r2.positions_restored, 1, "DEV-001 의 position 1건 복원");

        let row: (f64, f64) = sqlx::query_as(
            "SELECT x, y FROM quest_positions
             JOIN quests ON quests.id = quest_positions.quest_id
             WHERE quests.deleted_at IS NULL",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert!((row.0 - 162.0).abs() < 1e-6);
        assert!((row.1 - 108.0).abs() < 1e-6);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-049 회귀: quest 추가 후 reindex 가 quests.id 를 시프트해도
    /// quest_history 와 quest_positions 가 정확한 quest 를 가리키는지.
    #[tokio::test]
    async fn reindex_preserves_history_and_position_across_id_shift() {
        let dir = fresh_tmp("id-shift");
        let store = setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        // 처음: DEV-001 만 존재 → id 알맞게 부여 (예: 1).
        let dev1 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "dev one".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        dev1.write(paths.quest_path("DEV-001")).unwrap();
        reindex(&store).await.unwrap();

        // DEV-001 의 id 와 position / history INSERT.
        let dev1_id: i64 = sqlx::query_scalar(
            "SELECT id FROM quests WHERE deleted_at IS NULL LIMIT 1",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO quest_positions (quest_id, quest_slug, x, y) VALUES (?, 'DEV-001', 11.0, 22.0)",
        )
        .bind(dev1_id)
        .execute(&store.index_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO quest_history (quest_id, quest_slug, ts, op, old_value, new_value)
             VALUES (?, 'DEV-001', '2026-05-22T10:00:00+09:00', 'change_status', 'open', 'in_progress')",
        )
        .bind(dev1_id)
        .execute(&store.index_pool)
        .await
        .unwrap();

        // 새 quest BUG-001 추가 — alphabetic 정렬 시 DEV-001 보다 앞이라
        // 다음 reindex 에서 DEV-001 의 id 가 시프트됨 (1 → 2).
        let bug1 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "BUG-001".into(),
                title: "bug one".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        bug1.write(paths.quest_path("BUG-001")).unwrap();
        reindex(&store).await.unwrap();

        // 새 id 확인 (DEV-001 의 id 가 바뀜).
        let dev1_new_id: i64 = sqlx::query_scalar(
            "SELECT q.id FROM quests q JOIN quest_types qt ON q.quest_type_id = qt.id
             WHERE qt.prefix = 'DEV' AND q.number = 1",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_ne!(dev1_new_id, dev1_id, "BUG-001 추가로 DEV-001 의 id 가 시프트되어야 함");

        // position: quest_id 와 quest_slug 둘 다 새 id 와 슬러그 가리켜야.
        let (p_qid, p_slug, p_x, p_y): (i64, String, f64, f64) = sqlx::query_as(
            "SELECT quest_id, quest_slug, x, y FROM quest_positions",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(p_qid, dev1_new_id, "position.quest_id 가 새 id 로 갱신");
        assert_eq!(p_slug, "DEV-001");
        assert!((p_x - 11.0).abs() < 1e-6);
        assert!((p_y - 22.0).abs() < 1e-6);

        // history: quest_id 도 새 id 로 갱신, slug 는 그대로.
        let (h_qid, h_slug): (i64, String) = sqlx::query_as(
            "SELECT quest_id, quest_slug FROM quest_history",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(h_qid, dev1_new_id, "history.quest_id 가 새 id 로 갱신");
        assert_eq!(h_slug, "DEV-001");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reindex_drops_position_for_deleted_quest() {
        // quest 가 파일에서 사라지면 (slug 일치 안 함) 그 position 도 자연스럽게 제거.
        let dir = fresh_tmp("position-drop");
        let store = setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        let q1 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "doomed".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        q1.write(paths.quest_path("DEV-001")).unwrap();
        reindex(&store).await.unwrap();
        let qid: i64 = sqlx::query_scalar("SELECT id FROM quests LIMIT 1")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO quest_positions (quest_id, x, y) VALUES (?, 50.0, 60.0)")
            .bind(qid)
            .execute(&store.index_pool)
            .await
            .unwrap();

        // 파일 제거 후 reindex
        std::fs::remove_file(paths.quest_path("DEV-001")).unwrap();
        let r = reindex(&store).await.unwrap();
        assert_eq!(r.quests_loaded, 0);
        assert_eq!(r.positions_restored, 0);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_positions")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reindex_preserves_dependencies() {
        let dir = fresh_tmp("deps");
        let store = setup_store(&dir).await;
        let paths = GuildPaths::new(&dir);

        let q1 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "a".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec![],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        q1.write(paths.quest_path("DEV-001")).unwrap();

        let q2 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-002".into(),
                title: "b".into(),
                status: "open".into(),
                urgency: 3,
                parent: None,
                prerequisites: vec!["DEV-001".into()],
                created_at: "x".into(),
                updated_at: "x".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: String::new(),
            auto_block: String::new(),
        };
        q2.write(paths.quest_path("DEV-002")).unwrap();

        let report = reindex(&store).await.unwrap();
        assert_eq!(report.dependencies_loaded, 1);

        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_dependencies")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
