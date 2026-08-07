//! DEV-121 Phase 1: startup incremental sync.
//!
//! 풀 `reindex()` 대신 변경된 파일만 re-parse + UPSERT. `stat()` 만으로
//! file mtime 을 읽어 SQLite 의 `quests.cached_mtime` (Unix nanos) 와 비교 —
//! microsecond 비용 / 파일.
//!
//! ## Scope
//!
//! - `.guild/quests/*.md` (본문) — 수정 / 신규 / 삭제 (DEV-121, DEV-246).
//! - `{slug}.comments.md` / `{slug}.memo.md` (quest + campaign) — 수정 / 삭제
//!   (DEV-310).
//! - `campaigns/{slug}.md` (본문) — 수정 / 신규 / 삭제 (DEV-310 수정, DEV-311
//!   신규·삭제). 신규는 행 + 체크리스트 + `campaign_quests` 링크까지 한
//!   트랜잭션으로 INSERT. 삭제(파일 소멸 또는 외부에서 `deleted=true`)는 행
//!   DELETE — `campaign_checklists`/`campaign_quests`/`campaign_comments`/
//!   `campaign_memos` 는 FK `ON DELETE CASCADE`, `campaign_history` 는 quest
//!   와 동일하게 FK 없이 보존.
//! - `library/{BOOK-NNN}.md` (본문) — 수정 / 신규 / 삭제 (DEV-311). soft-delete
//!   된 문서도(reindex 와 동일하게) 행을 유지 — 번호 충돌 검증에 필요.
//!
//! 아직 full reindex 로 넘기는 것: statuses / types / tags 정의. 정의 파일이
//! 바뀌면 해석 자체가 달라지므로(상태 slug 등) 안전망을 유지하는 편이 맞다.
//!
//! DEV-311 조사 결과 **범위에서 뺀 것** (할 일이 아니라 애초에 캐시가 없음):
//! - **규칙 문서**(`rules/**`) — `repo::rules` 자체가 "파일 진리원, DB 캐시
//!   없음"(server/GUI 가 파일을 직접 읽음) 이라 동기화할 대상이 없다.
//! - **worklog 노트**(`worklog/*.md`) — `ops::worklog` 도 동일 패턴(파일 직독,
//!   `activities()`/`daily_summary()` 는 이미 동기화된 다른 테이블 위의 순수
//!   조회 쿼리). 그 테이블들이 최신이면 worklog 조회는 별도 작업 없이 최신.
//! - **doc_history 사이드카**(`.guild/history/*.jsonl`) **외부 편집** 반영 —
//!   `ops::doc_history` 가 앱 내부 mutation(rule/book create·update·delete)은
//!   이미 증분 반영한다(BUG-189). 하지만 사이드카 파일을 외부(git pull 등)에서
//!   직접 편집하는 경우는 quest_history/campaign_history 도 동일하게 전체
//!   재구축 전용 — 부분(추가된 줄만) 반영 메커니즘이 이 코드베이스 어디에도
//!   없다. 새로 만들기엔 범위가 크고 빈도가 낮아 이번엔 안전망(full reindex)
//!   유지.
//!
//! ## 시간 비교 안전성 (timezone)
//!
//! 비교 양쪽이 모두 Unix nanoseconds (절대 시각).
//! - File mtime: `SystemTime::duration_since(UNIX_EPOCH).as_nanos()`.
//! - DB: `INTEGER cached_mtime` (Unix nanos 저장).
//!
//! → local time / TZ / DST / 길드 이동에 무관.
//!
//! ## 알고리즘
//!
//! ```text
//! for each .md file in .guild/quests/ (body files only):
//!     file_mtime = stat(file).mtime_unix_nanos()
//!     db = SELECT slug, cached_mtime FROM quests WHERE slug = ?
//!     if db is None:                          # 신규 파일
//!         (reindex 가 처리 — Phase 1b 까지는 fallback)
//!     elif file_mtime > db.cached_mtime:      # 외부 편집
//!         parse + UPDATE (description, frontmatter 필드, cached_mtime)
//!     # else: skip
//!
//! for each db quest (alive):
//!     if file 사라짐:
//!         (reindex / drift 가 처리 — Phase 1b 까지는 fallback)
//! ```
//!
//! Phase 1 에선 **modified file 만** 처리 — 신규/삭제는 drift::auto_resync 가
//! 잡아 reindex 트리거. 이 조합으로 "외부 편집된 기존 파일이 mtime 비교
//! 실패로 안 잡히던" BUG-049 / BUG-059 의 핵심 시나리오를 해결.

use crate::error::AppResult;
use crate::repo::{QuestFile, fs as repo_fs};
use crate::store::Store;

/// BUG-080: 외부 편집된 quest 파일의 frontmatter `updated_at` 을 파일 mtime 으로
/// write-back (파일 = 진리원). frontmatter 의 `updated_at` 한 줄만 교체하고
/// 본문 / auto-block 은 건드리지 않는다. 반환: `(기록할 updated_at iso, write 후
/// mtime nanos)`. write 가 파일 mtime 을 다시 바꾸므로 반환된 새 mtime 을
/// cached_mtime 으로 저장해야 다음 sync 가 같은 편집을 재감지(churn)하지 않는다.
fn writeback_external_edit_ts(path: &std::path::Path) -> (String, i64) {
    let edit_iso = repo_fs::mtime_iso8601(path).unwrap_or_default();
    if !edit_iso.is_empty()
        && let Ok(src) = std::fs::read_to_string(path)
        && let Some(updated) = replace_frontmatter_updated_at(&src, &edit_iso)
    {
        let _ = repo_fs::write_atomic(path, &updated);
    }
    (edit_iso, repo_fs::mtime_unix_nanos(path))
}

/// frontmatter(맨 위 `+++ … +++`) 안의 `updated_at = "…"` 한 줄만 교체.
/// 본문은 그대로. updated_at 줄이 없으면 None(변경 안 함).
fn replace_frontmatter_updated_at(src: &str, new_ts: &str) -> Option<String> {
    if !src.trim_start().starts_with("+++") {
        return None;
    }
    let mut out = String::with_capacity(src.len() + 16);
    let mut opened = false;
    let mut in_fm = false;
    let mut replaced = false;
    for line in src.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        if body.trim() == "+++" {
            if !opened {
                opened = true;
                in_fm = true;
            } else if in_fm {
                in_fm = false;
            }
            out.push_str(line);
            continue;
        }
        if in_fm && !replaced && body.trim_start().starts_with("updated_at") {
            out.push_str(&format!("updated_at = \"{new_ts}\"\n"));
            replaced = true;
            continue;
        }
        out.push_str(line);
    }
    replaced.then_some(out)
}

/// BUG-103: 파싱된 quest 파일이 DB 캐시와 내용 동일한지.
///
/// git checkout/pull/restore 는 **내용이 같아도 mtime 을 바꾼다** — mtime 만으로
/// "외부 편집"을 판정해 updated_at 을 mtime 으로 write-back(BUG-080)하면, 브랜치
/// 전환 한 번에 전체 quest 의 updated_at 이 일괄 변조된다(2026-07-03 실사고,
/// 200+ 파일). write-back 전에 이 함수로 내용을 비교해 동일하면 updated_at 을
/// 건드리지 않고 cached_mtime 만 갱신한다.
///
/// 비교 대상: frontmatter 전 필드(title/status/urgency/parent/prereq/dates/
/// deleted/tags) + 본문(description). prereq/tags 는 나머지가 전부 같을 때만
/// 조회(2쿼리 — 드문 경로라 저렴).
async fn is_content_identical_to_db(
    pool: &sqlx::SqlitePool,
    id: i64,
    qf: &QuestFile,
) -> AppResult<bool> {
    #[derive(sqlx::FromRow)]
    struct Row {
        title: String,
        description: Option<String>,
        status_slug: String,
        urgency: i64,
        parent_slug: Option<String>,
        created_at: String,
        deleted: bool,
        desired_due: Option<String>,
        required_due: Option<String>,
    }
    let Some(row) = sqlx::query_as::<_, Row>(
        "SELECT q.title, q.description, s.slug AS status_slug, q.urgency,
                (SELECT pt.prefix || '-' || printf('%03d', p.number)
                   FROM quests p JOIN quest_types pt ON pt.id = p.quest_type_id
                  WHERE p.id = q.parent_quest_id) AS parent_slug,
                q.created_at,
                q.deleted_at IS NOT NULL AS deleted,
                q.desired_due, q.required_due
         FROM quests q JOIN quest_statuses s ON s.id = q.status_id
         WHERE q.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(false);
    };

    let fm = &qf.frontmatter;
    // due 는 빈 문자열 ↔ None 정규화 차이를 흡수.
    let norm_due = |v: &Option<String>| {
        v.as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(String::from)
    };
    // BUG-145: `updated_at` 은 의도적으로 비교 대상에서 제외 — 이 필드 자체가
    // write-back 이 매번 다시 쓰는 대상이라, 포함시키면 한 번이라도 오탐성
    // write-back 이 일어난 뒤부터는 DB 캐시의 updated_at 이 git 커밋된 파일의
    // 실제 값과 영원히 어긋나 매번 "내용이 다르다"고 오판 → 다시 write-back →
    // 다시 어긋남, 의 무한 재발 루프가 생긴다(브랜치 전환마다 무관한 quest 수십
    // 개가 계속 변조되던 원인). 진짜 콘텐츠(title/description/status/urgency/
    // parent/prereq/dates/deleted/tags)만 같으면 "동일"로 본다.
    if row.title != fm.title
        || row.description.as_deref().unwrap_or("") != qf.description
        || row.status_slug != fm.status
        || row.urgency != fm.urgency
        || row.parent_slug != fm.parent
        || row.created_at != crate::time::normalize_legacy_ts(&fm.created_at)
        || row.deleted != fm.deleted
        || norm_due(&row.desired_due) != norm_due(&fm.desired_due)
        || norm_due(&row.required_due) != norm_due(&fm.required_due)
    {
        return Ok(false);
    }

    // prereq / tags — 정렬 후 비교 (저장 순서 무관).
    let mut db_prereqs: Vec<String> = sqlx::query_scalar(
        "SELECT qt.prefix || '-' || printf('%03d', p.number)
         FROM quest_dependencies d
         JOIN quests p ON p.id = d.prerequisite_id
         JOIN quest_types qt ON qt.id = p.quest_type_id
         WHERE d.quest_id = ?",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut fm_prereqs = fm.prerequisites.clone();
    db_prereqs.sort();
    fm_prereqs.sort();
    if db_prereqs != fm_prereqs {
        return Ok(false);
    }

    let mut db_tags: Vec<String> =
        sqlx::query_scalar("SELECT tag FROM quest_tags WHERE quest_id = ?")
            .bind(id)
            .fetch_all(pool)
            .await?;
    let mut fm_tags = fm.tags.clone();
    db_tags.sort();
    fm_tags.sort();
    Ok(db_tags == fm_tags)
}

#[derive(Debug, Default, Clone)]
pub struct IncrementalReport {
    /// 외부 편집 감지되어 re-parse + UPDATE 한 quest slug 수.
    pub updated: usize,
    /// DEV-246: 새로 발견해 INSERT 한 quest 수.
    pub inserted: usize,
    /// DEV-246: 파일이 사라져 DELETE 한 quest 수.
    pub deleted: usize,
    /// 신규 / 삭제 등 본 모듈 범위 외 — 호출자가 drift::auto_resync 로
    /// 풀 reindex 트리거 권장.
    pub needs_full_reindex: bool,
    /// 파싱 실패 등으로 skip 한 항목.
    pub skipped: Vec<(String, String)>,
    /// DEV-310: 재적재한 sibling 파일 수 (`{slug}.comments.md` / `.memo.md`,
    /// quest + campaign). 삭제 반영도 포함.
    pub siblings_synced: usize,
    /// DEV-310: 외부 편집이 반영된 캠페인 본문 수.
    pub campaigns_synced: usize,
    /// DEV-311: 새로 발견해 INSERT 한 캠페인 수.
    pub campaigns_inserted: usize,
    /// DEV-311: 파일이 사라지거나 외부에서 soft-delete 되어 DELETE 한 캠페인 수.
    pub campaigns_deleted: usize,
    /// DEV-311: 외부 편집이 반영된 도서관 문서 수.
    pub library_synced: usize,
    /// DEV-311: 새로 발견해 INSERT 한 도서관 문서 수.
    pub library_inserted: usize,
    /// DEV-311: 파일이 사라져 DELETE 한 도서관 문서 수.
    pub library_deleted: usize,
}

/// DEV-310: sibling 파일의 종류 — 어느 캐시 테이블을 다시 채울지.
/// (quest/campaign 구분은 파일명이 아니라 어느 디렉토리에서 나열됐는지로 정한다.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SiblingKind {
    QuestComments,
    QuestMemo,
    CampaignComments,
    CampaignMemo,
}

/// DEV-246: 신규 quest 파일 1개를 index.db 에 INSERT.
///
/// 반환:
/// - `Ok(Some((id, qf)))` — 삽입 성공(관계는 호출자가 2-pass 로 해석).
/// - `Ok(None)` — 타입/상태 정의가 없어 증분으로 표현 불가 → 호출자가 풀
///   reindex 로 넘긴다(정의 파일 변경은 이 함수 범위가 아니다).
///
/// 부모/선행은 여기서 세우지 않는다 — 같은 배치의 다른 신규 퀘스트를 가리킬 수
/// 있어서, 행이 모두 생긴 뒤 `resync_quest_relations` 가 해석한다.
async fn insert_new_quest(
    store: &Store,
    path: &std::path::Path,
) -> AppResult<Option<(i64, QuestFile)>> {
    let pool = &store.index_pool;
    let qf = QuestFile::read(path).map_err(crate::error::AppError::Internal)?;

    let Some(prefix) = qf.type_prefix() else {
        return Ok(None);
    };
    let type_id: Option<i64> = sqlx::query_scalar("SELECT id FROM quest_types WHERE prefix = ?")
        .bind(prefix)
        .fetch_optional(pool)
        .await?;
    let Some(type_id) = type_id else {
        return Ok(None); // 타입 정의가 아직 없음 — 풀 reindex 가 처리.
    };
    let number = qf.number().map_err(crate::error::AppError::Internal)?;
    let status_id: Option<i64> = sqlx::query_scalar("SELECT id FROM quest_statuses WHERE slug = ?")
        .bind(&qf.frontmatter.status)
        .fetch_optional(pool)
        .await?;
    let Some(status_id) = status_id else {
        return Ok(None); // 상태 정의가 아직 없음 — 풀 reindex 가 처리.
    };

    let created_at = crate::time::normalize_legacy_ts(&qf.frontmatter.created_at);
    let updated_at = crate::time::normalize_legacy_ts(&qf.frontmatter.updated_at);
    let deleted_at: Option<String> = qf.frontmatter.deleted.then(|| updated_at.clone());
    let cached_mtime = repo_fs::mtime_unix_nanos(path);

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO quests
           (quest_type_id, number, title, description, status_id, urgency,
            parent_quest_id, created_at, updated_at, deleted_at,
            desired_due, required_due, cached_mtime)
         VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(type_id)
    .bind(number)
    .bind(&qf.frontmatter.title)
    .bind(&qf.description)
    .bind(status_id)
    .bind(qf.frontmatter.urgency)
    .bind(&created_at)
    .bind(&updated_at)
    .bind(deleted_at)
    .bind(qf.frontmatter.desired_due.as_deref())
    .bind(qf.frontmatter.required_due.as_deref())
    .bind(cached_mtime)
    .fetch_one(pool)
    .await?;

    Ok(Some((id, qf)))
}

/// DEV-246: 한 quest 의 부모/선행/태그 캐시를 frontmatter 기준으로 재구성.
///
/// 예전에는 이 cascade 를 증분으로 못 해서 "파일이 하나라도 수정되면 풀
/// reindex" 였다. 관계는 **해당 퀘스트 것만** 지우고 다시 넣으므로 다른 퀘스트의
/// 캐시를 건드리지 않는다.
///
/// 아직 존재하지 않는 slug 를 가리키는 부모/선행은 조용히 건너뛴다 — 파일이
/// 진리원이고, 대상이 나중에 생기면 그때 그 파일의 sync 가 다시 해석한다.
async fn resync_quest_relations(store: &Store, id: i64, qf: &QuestFile) -> AppResult<()> {
    let pool = &store.index_pool;

    let resolve = |slug: String| async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT q.id FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id
              WHERE qt.prefix || '-' || printf('%03d', q.number) = ?",
        )
        .bind(slug)
        .fetch_optional(&store.index_pool)
        .await
    };

    // 부모.
    let parent_id: Option<i64> = match qf.frontmatter.parent.clone() {
        Some(s) => resolve(s).await?,
        None => None,
    };
    sqlx::query("UPDATE quests SET parent_quest_id = ? WHERE id = ?")
        .bind(parent_id)
        .bind(id)
        .execute(pool)
        .await?;

    // 선행 — 이 퀘스트의 행만 교체.
    sqlx::query("DELETE FROM quest_dependencies WHERE quest_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    for slug in &qf.frontmatter.prerequisites {
        if let Some(pid) = resolve(slug.clone()).await? {
            sqlx::query(
                "INSERT OR IGNORE INTO quest_dependencies (quest_id, prerequisite_id) VALUES (?, ?)",
            )
            .bind(id)
            .bind(pid)
            .execute(pool)
            .await?;
        }
    }

    // 태그 — 이 퀘스트의 행만 교체.
    // 정규화는 **reindex 와 동일하게 trim 만** 한다(reindex.rs:394). 소문자화를
    // 넣으면 같은 파일이 sync 경로냐 reindex 경로냐에 따라 다른 값이 들어가
    // drift 로 잡힌다.
    sqlx::query("DELETE FROM quest_tags WHERE quest_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    for tag in &qf.frontmatter.tags {
        let normalized = tag.trim();
        if normalized.is_empty() {
            continue;
        }
        sqlx::query("INSERT OR IGNORE INTO quest_tags (quest_id, tag) VALUES (?, ?)")
            .bind(id)
            .bind(normalized)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// 변경된 파일만 동기화. 신규 / 삭제는 본 함수가 안 하고 `needs_full_reindex`
/// flag 만 set — 호출자가 drift::auto_resync 로 풀 reindex.
pub async fn sync_changed_quest_files(store: &Store) -> AppResult<IncrementalReport> {
    let mut report = IncrementalReport::default();
    let paths = &store.paths;
    let pool = &store.index_pool;

    // 파일 목록.
    let quest_paths = repo_fs::list_quest_body_files(paths.quests_dir())
        .map_err(crate::error::AppError::Internal)?;

    // DB 의 slug → (id, cached_mtime) 맵.
    // DEV-246: soft-deleted(`deleted = true`) 행도 포함해야 한다.
    //
    // 예전엔 `WHERE q.deleted_at IS NULL` 로 걸렀는데, soft-delete 된 퀘스트는
    // **파일이 그대로 남아 있다**(진리원이므로). 그래서 그 파일이 매번 "신규"로
    // 취급돼 (a) 예전 코드에서는 needs_full_reindex 가 서서 **soft-delete 가
    // 하나만 있어도 열 때마다 전체 재구축**이 돌았고, (b) 증분 INSERT 를 붙인
    // 뒤에는 UNIQUE(quest_type_id, number) 충돌로 6건이 skip 됐다(실측).
    let db_rows: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT q.id, qt.prefix || '-' || printf('%03d', q.number) AS slug, q.cached_mtime
         FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id",
    )
    .fetch_all(pool)
    .await?;
    let db_map: std::collections::HashMap<String, (i64, i64)> = db_rows
        .into_iter()
        .map(|(id, slug, mtime)| (slug, (id, mtime)))
        .collect();

    // 파일 → DB row 매칭 + mtime 비교.
    // DEV-246: 신규 파일은 2-pass 로 처리(부모/선행이 같은 배치를 가리킬 수 있음),
    // 삽입·수정된 퀘스트는 관계 재구성 대상으로 모아둔다.
    let mut new_paths: Vec<std::path::PathBuf> = Vec::new();
    let mut touched: Vec<(i64, QuestFile)> = Vec::new();
    let mut file_slugs = std::collections::HashSet::new();
    for path in &quest_paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let slug = stem.to_string();
        file_slugs.insert(slug.clone());

        let file_mtime = repo_fs::mtime_unix_nanos(path);

        match db_map.get(&slug) {
            None => {
                // DEV-246: 신규 파일도 여기서 처리한다. 예전엔 파일 1개가 추가된
                // 것만으로 전체 reindex(이 길드 기준 warm 1.3s / cold 10.2s)로
                // 빠졌다 — git pull / 브랜치 전환 / 에이전트의 quest new 마다.
                // 부모·선행이 같은 배치의 신규 퀘스트를 가리킬 수 있으므로
                // **행 삽입을 먼저 모두 끝낸 뒤** 관계를 해석한다(2-pass).
                new_paths.push(path.clone());
            }
            Some(&(id, cached_mtime)) => {
                if file_mtime > cached_mtime {
                    // 외부 편집 후보. parse + UPDATE.
                    match QuestFile::read(path) {
                        Ok(qf) => {
                            // BUG-103: 내용이 DB 와 동일하면(git checkout 등 mtime 만
                            // 변경) updated_at write-back 하지 않고 cached_mtime 만
                            // 갱신 — 다음 sync 재감지 방지.
                            if is_content_identical_to_db(pool, id, &qf).await? {
                                sqlx::query("UPDATE quests SET cached_mtime = ? WHERE id = ?")
                                    .bind(file_mtime)
                                    .bind(id)
                                    .execute(pool)
                                    .await?;
                                continue;
                            }
                            // status_id 결정.
                            let status_id: Option<i64> =
                                sqlx::query_scalar("SELECT id FROM quest_statuses WHERE slug = ?")
                                    .bind(&qf.frontmatter.status)
                                    .fetch_optional(pool)
                                    .await?;
                            let Some(status_id) = status_id else {
                                report.needs_full_reindex = true; // status 가 없으면 풀 reindex 필요
                                report.skipped.push((
                                    path.display().to_string(),
                                    format!("unknown status slug: {}", qf.frontmatter.status),
                                ));
                                continue;
                            };

                            // parent / prereq 관계는 신규 컬럼 갱신만. 본격 cascade
                            // (quest_dependencies 재계산) 는 drift::auto_resync 가
                            // 풀 reindex 로 처리 — Phase 1 의 의도적 단순화.
                            let parent_slug = qf.frontmatter.parent.clone();
                            let parent_id: Option<i64> = match parent_slug {
                                Some(s) => {
                                    sqlx::query_scalar(
                                        "SELECT q.id FROM quests q
                                     JOIN quest_types qt ON qt.id = q.quest_type_id
                                     WHERE qt.prefix || '-' || printf('%03d', q.number) = ?",
                                    )
                                    .bind(&s)
                                    .fetch_optional(pool)
                                    .await?
                                }
                                None => None,
                            };

                            let created_at =
                                crate::time::normalize_legacy_ts(&qf.frontmatter.created_at);
                            // BUG-080: 외부 편집은 frontmatter updated_at 을 안 바꾸므로
                            // 파일 mtime 으로 보정 + frontmatter write-back (파일=진리원).
                            let (edit_iso, effective_mtime) = writeback_external_edit_ts(path);
                            let updated_at = if edit_iso.is_empty() {
                                crate::time::normalize_legacy_ts(&qf.frontmatter.updated_at)
                            } else {
                                edit_iso
                            };
                            let deleted_at: Option<String> =
                                qf.frontmatter.deleted.then(|| updated_at.clone());

                            sqlx::query(
                                "UPDATE quests SET
                                   title = ?, description = ?, status_id = ?, urgency = ?,
                                   parent_quest_id = ?, created_at = ?, updated_at = ?,
                                   deleted_at = ?, desired_due = ?, required_due = ?,
                                   cached_mtime = ?
                                 WHERE id = ?",
                            )
                            .bind(&qf.frontmatter.title)
                            .bind(&qf.description)
                            .bind(status_id)
                            .bind(qf.frontmatter.urgency)
                            .bind(parent_id)
                            .bind(&created_at)
                            .bind(&updated_at)
                            .bind(deleted_at)
                            .bind(qf.frontmatter.desired_due.as_deref())
                            .bind(qf.frontmatter.required_due.as_deref())
                            .bind(effective_mtime)
                            .bind(id)
                            .execute(pool)
                            .await?;

                            // DEV-246: prereq/tag cascade 도 증분으로. 예전엔 여기서
                            // 무조건 풀 reindex 를 요청해서, **파일이 하나라도 수정되면**
                            // 전체 재구축이 돌았다(실사용에서 거의 항상).
                            touched.push((id, qf.clone()));
                            report.updated += 1;
                        }
                        Err(e) => {
                            report
                                .skipped
                                .push((path.display().to_string(), format!("{e:#}")));
                        }
                    }
                }
                // else: file mtime <= cached → no-op.
            }
        }
    }

    // DEV-246: 파일이 사라진 행은 직접 삭제한다(예전엔 풀 reindex 트리거).
    // quest_tags / quest_dependencies / quest_positions 는 FK ON DELETE CASCADE.
    for (slug, &(id, _)) in &db_map {
        if !file_slugs.contains(slug) {
            sqlx::query("DELETE FROM quests WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await?;
            report.deleted += 1;
        }
    }

    // ── DEV-246 2-pass: 신규 행 삽입 → 관계(부모/선행/태그) 해석 ──
    for path in &new_paths {
        match insert_new_quest(store, path).await {
            Ok(Some((id, qf))) => {
                touched.push((id, qf));
                report.inserted += 1;
            }
            Ok(None) => {
                // 타입/상태 정의가 없는 등 증분으로 표현 불가 — 안전망으로 넘긴다.
                report.needs_full_reindex = true;
            }
            Err(e) => {
                report
                    .skipped
                    .push((path.display().to_string(), format!("{e:#}")));
                report.needs_full_reindex = true;
            }
        }
    }
    for (id, qf) in &touched {
        resync_quest_relations(store, *id, qf).await?;
    }

    Ok(report)
}

/// DEV-137 (Phase 2): 단일 quest 의 lazy refresh — 상세 페이지 진입 시 호출.
///
/// BUG-089: 상세 진입 시 그 quest 파일 하나를 *항상* re-parse + UPDATE 한다
/// (파일 1개라 저렴). mtime 게이트로 건너뛰면 다른 openguild 프로세스(CLI/server)
/// 의 편집을 놓치기 때문. mtime 비교는 파일 write-back(updated_at 보정) 여부에만
/// 쓰고, 반환값 = '외부 편집 감지 여부'. 파일 없음 / DB 에 없음 → `false`.
///
/// Phase 1 과 동일한 의도적 한계: frontmatter 의 prereq / tags / parent
/// cascade 는 여기서 재계산하지 않음 — 그건 시동 sync 의 풀 reindex fallback
/// 영역. 본 함수는 제목 / 본문 / status / urgency / due 등 표시 필드 중심.
pub async fn refresh_quest_if_stale(store: &Store, slug: &str) -> AppResult<bool> {
    let path = store.paths.quest_path(slug);
    if !path.exists() {
        return Ok(false);
    }
    let file_mtime = repo_fs::mtime_unix_nanos(&path);
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT q.id, q.cached_mtime
         FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id
         WHERE qt.prefix || '-' || printf('%03d', q.number) = ?",
    )
    .bind(slug)
    .fetch_optional(&store.index_pool)
    .await?;
    let Some((id, cached_mtime)) = row else {
        return Ok(false); // 신규 파일 — 시동 sync / reindex 영역.
    };
    // BUG-089: 콘텐츠 re-read + UPDATE 는 상세 진입 시 *항상* 수행한다. mtime
    // 게이트로 re-read 를 건너뛰면, 다른 openguild 프로세스(CLI/server)가 파일 +
    // cached_mtime 을 함께 갱신한 경우 이 프로세스의 index.db 뷰가 stale 인 채
    // 남는다(게이트가 "이미 동기화됨"으로 오판). 파일 1개 re-parse 는 저렴.
    // 단, 파일 write-back(updated_at 보정)은 매 진입 churn 을 유발하므로 실제
    // 외부 편집(file_mtime > cached_mtime)일 때만.
    let externally_edited = file_mtime > cached_mtime;

    let qf = QuestFile::read(&path).map_err(crate::error::AppError::Internal)?;
    // BUG-103: mtime 이 앞서도 내용이 DB 와 동일하면(git checkout 등) 외부 편집
    // 아님 — write-back 하지 않는다. cached_mtime 은 아래 UPDATE 의
    // effective_mtime(max) 경로로 자연 갱신.
    let externally_edited =
        externally_edited && !is_content_identical_to_db(&store.index_pool, id, &qf).await?;
    let status_id: Option<i64> = sqlx::query_scalar("SELECT id FROM quest_statuses WHERE slug = ?")
        .bind(&qf.frontmatter.status)
        .fetch_optional(&store.index_pool)
        .await?;
    let Some(status_id) = status_id else {
        return Ok(false); // unknown status — 풀 reindex 가 처리할 영역.
    };
    let created_at = crate::time::normalize_legacy_ts(&qf.frontmatter.created_at);
    // BUG-080 + BUG-089: 외부 편집일 때만 파일 mtime 으로 updated_at 보정 +
    // frontmatter write-back (write-back 은 파일 재기록이라 churn 방지 위해 게이트).
    let (updated_at, effective_mtime) = if externally_edited {
        let (edit_iso, eff) = writeback_external_edit_ts(&path);
        if edit_iso.is_empty() {
            (
                crate::time::normalize_legacy_ts(&qf.frontmatter.updated_at),
                eff,
            )
        } else {
            (edit_iso, eff)
        }
    } else {
        (
            crate::time::normalize_legacy_ts(&qf.frontmatter.updated_at),
            // BUG-103: 내용 동일 + mtime 만 앞선 경우 file_mtime 으로 올려
            // 다음 진입마다 내용 비교를 반복하지 않게 한다.
            cached_mtime.max(file_mtime),
        )
    };
    let deleted_at: Option<String> = qf.frontmatter.deleted.then(|| updated_at.clone());

    sqlx::query(
        "UPDATE quests SET
           title = ?, description = ?, status_id = ?, urgency = ?,
           created_at = ?, updated_at = ?, deleted_at = ?,
           desired_due = ?, required_due = ?, cached_mtime = ?
         WHERE id = ?",
    )
    .bind(&qf.frontmatter.title)
    .bind(&qf.description)
    .bind(status_id)
    .bind(qf.frontmatter.urgency)
    .bind(&created_at)
    .bind(&updated_at)
    .bind(deleted_at)
    .bind(qf.frontmatter.desired_due.as_deref())
    .bind(qf.frontmatter.required_due.as_deref())
    .bind(effective_mtime)
    .bind(id)
    .execute(&store.index_pool)
    .await?;
    Ok(externally_edited)
}

/// DEV-178: 단일 campaign 의 lazy refresh — 상세 페이지 진입 시 호출
/// (`refresh_quest_if_stale` 의 campaign 판). BUG-089: 상세 진입 시 그 캠페인
/// 본문 파일 하나를 *항상* re-parse + UPDATE (체크리스트 / linked_quests 포함).
/// `file_mtime_cache` 비교는 파일 write-back / touch 여부에만 쓰고 반환값 =
/// '외부 편집 감지 여부'.
///
/// quest 본문은 per-row `cached_mtime` 를 쓰지만 campaigns 테이블엔 그 컬럼이 없어
/// sibling 과 동일한 범용 `file_mtime_cache`(BUG-068) 로 비교한다. 캐시에 아직
/// 없으면(첫 진입) 한 번 갱신 후 touch — 이후 churn 없음.
pub async fn refresh_campaign_if_stale(store: &Store, slug: &str) -> AppResult<bool> {
    refresh_campaign(store, slug, true).await
}

/// DEV-310: 위와 같은 재적재를 **파일을 건드리지 않고** 수행 — 길드 열기 경로용.
///
/// 열기 경로에서 write-back 을 하면 `git pull` 로 mtime 만 바뀐 캠페인 파일의
/// frontmatter `updated_at` 이 일괄 변조된다(quest 본문에서 실제로 터졌던
/// BUG-103 과 같은 사고 — 그쪽은 내용 비교 가드로 막았고, 캠페인엔 그 가드가
/// 아직 없다). 상세 진입은 사용자가 그 문서를 보는 시점이라 기존 동작을 유지.
async fn refresh_campaign_row_no_writeback(store: &Store, slug: &str) -> AppResult<bool> {
    refresh_campaign(store, slug, false).await
}

/// DEV-311: `campaign_checklists` 를 트랜잭션 안에서 통째로 교체.
///
/// `services::campaigns::replace_checklists_from_file` 과 같은 일을 하지만
/// **자기 트랜잭션을 열지 않는다** — 이미 연 트랜잭션 안에서 호출돼야 하는
/// `refresh_campaign`/`insert_new_campaign` 전용. (그 함수는 `&SqlitePool` 을
/// 받아 스스로 `pool.begin()` 하므로, 이미 트랜잭션이 열린 채로 호출하면 같은
/// pool 에서 커밋 안 된 쓰기 락을 다른 커넥션이 기다리게 되어 교착 위험이
/// 있다 — 그래서 별도로 트랜잭션-바인딩 버전을 둔다.)
async fn replace_checklists_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    campaign_id: i64,
    lines: &[crate::repo::ChecklistLine],
) -> AppResult<()> {
    sqlx::query("DELETE FROM campaign_checklists WHERE campaign_id = ?")
        .bind(campaign_id)
        .execute(&mut **tx)
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
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// DEV-311: `linked_quests`(frontmatter) → `campaign_quests` re-sync, 트랜잭션 안.
async fn resync_campaign_quests_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    campaign_id: i64,
    linked_quests: &[String],
) -> AppResult<()> {
    sqlx::query("DELETE FROM campaign_quests WHERE campaign_id = ?")
        .bind(campaign_id)
        .execute(&mut **tx)
        .await?;
    for qslug in linked_quests {
        let qid: Option<i64> = sqlx::query_scalar(
            "SELECT q.id FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id
             WHERE qt.prefix || '-' || printf('%03d', q.number) = ?",
        )
        .bind(qslug)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(qid) = qid {
            sqlx::query(
                "INSERT OR IGNORE INTO campaign_quests (campaign_id, quest_id) VALUES (?, ?)",
            )
            .bind(campaign_id)
            .bind(qid)
            .execute(&mut **tx)
            .await?;
        }
        // unresolved slug 는 조용히 skip — reindex.rs 의 같은 루프와 동일.
    }
    Ok(())
}

/// 캠페인 행 + 자식(체크리스트/링크/댓글/메모) 삭제. `campaign_history` 는
/// quest_history 와 동일하게 FK 없이 보존(audit 가치, DEV-226 결정).
async fn delete_campaign_row(store: &Store, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM campaigns WHERE id = ?")
        .bind(id)
        .execute(&store.index_pool)
        .await?;
    Ok(())
}

/// DEV-311: 캠페인 본문(`campaigns/{slug}.md`) 1개를 index.db 에 INSERT —
/// `sync_changed_campaign_files` 가 DB 에 없는 새 파일을 발견했을 때 호출.
/// soft-deleted(`deleted=true`) 로 갓 만들어진 파일은 reindex 와 동일하게
/// 삽입하지 않는다(`Ok(None)`) — 애초에 살아있던 적 없는 캠페인이라 행이
/// 필요 없다.
async fn insert_new_campaign(store: &Store, path: &std::path::Path) -> AppResult<Option<i64>> {
    let cf = crate::repo::CampaignFile::read(path).map_err(crate::error::AppError::Internal)?;
    if cf.frontmatter.deleted {
        return Ok(None);
    }
    let mut tx = store.index_pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO campaigns
           (campaign_slug, title, description, status,
            started_at, ended_at, display_order, image_path, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(&cf.frontmatter.campaign_id)
    .bind(&cf.frontmatter.title)
    .bind(&cf.body)
    .bind(&cf.frontmatter.status)
    .bind((!cf.frontmatter.started_at.is_empty()).then_some(&cf.frontmatter.started_at))
    .bind((!cf.frontmatter.ended_at.is_empty()).then_some(&cf.frontmatter.ended_at))
    .bind(cf.frontmatter.display_order)
    .bind(cf.frontmatter.image.as_deref())
    .bind(&cf.frontmatter.created_at)
    .bind(&cf.frontmatter.updated_at)
    .fetch_one(&mut *tx)
    .await?;

    let items = crate::repo::extract_checklist_items(&cf.body);
    replace_checklists_in_tx(&mut tx, id, &items).await?;
    resync_campaign_quests_in_tx(&mut tx, id, &cf.frontmatter.linked_quests).await?;
    tx.commit().await?;
    Ok(Some(id))
}

/// DEV-311(perf): 예전엔 UPDATE + 체크리스트 교체(자체 트랜잭션) + `campaign_quests`
/// DELETE + 링크당 SELECT/INSERT 가 **전부 별도(autocommit) 트랜잭션** 이었다 —
/// 링크 46개짜리 캠페인 재적재가 533ms(문장당 fsync ~95회). 지금은 한
/// 트랜잭션으로 묶는다(reindex() 의 전체 재구축과 동일한 패턴).
async fn refresh_campaign(store: &Store, slug: &str, writeback: bool) -> AppResult<bool> {
    let path = store.paths.campaign_path(slug);
    if !path.exists() {
        return Ok(false);
    }
    let rel = crate::file_mtime::rel_key(&store.paths, &path);
    let file_mtime = repo_fs::mtime_unix_nanos(&path);
    let cache = crate::file_mtime::load_all(store).await;
    // BUG-089: 콘텐츠 re-read + UPDATE(체크리스트/linked_quests 포함)는 상세 진입
    // 시 *항상* 수행 — mtime 게이트로 건너뛰면 다른 openguild 프로세스(CLI/server)
    // 의 편집을 놓친다. write-back/touch(파일 재기록·캐시 갱신)만 외부 편집 시로.
    let externally_edited = match cache.get(&rel) {
        Some(&cached) => file_mtime > cached,
        None => true, // 첫 진입(캐시 없음) — 갱신 + touch.
    };

    // campaigns 행 존재 확인 (신규는 insert_new_campaign 영역).
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM campaigns WHERE campaign_slug = ?")
        .bind(slug)
        .fetch_optional(&store.index_pool)
        .await?;
    let Some(id) = id else {
        return Ok(false);
    };
    let Ok(cf) = crate::repo::CampaignFile::read(&path) else {
        return Ok(false); // 파싱 실패 — reindex 가 skip 경고로 처리.
    };
    if cf.frontmatter.deleted {
        // DEV-311: 외부에서 soft-delete(파일에 deleted=true) — reindex 가 이런
        // 캠페인을 skip(=insert 안 함)하는 것과 동치이니 행을 직접 지운다.
        delete_campaign_row(store, id).await?;
        let _ = crate::file_mtime::touch(store, &path).await;
        return Ok(true);
    }

    // BUG-080 + BUG-089: 외부 편집일 때만 파일 mtime 으로 updated_at 보정 +
    // frontmatter write-back (write-back 은 파일 재기록 → churn 방지 위해 게이트).
    let updated_at = if externally_edited && writeback {
        let (edit_iso, _) = writeback_external_edit_ts(&path);
        if edit_iso.is_empty() {
            crate::time::normalize_legacy_ts(&cf.frontmatter.updated_at)
        } else {
            edit_iso
        }
    } else {
        crate::time::normalize_legacy_ts(&cf.frontmatter.updated_at)
    };

    let mut tx = store.index_pool.begin().await?;
    // 행 UPDATE — reindex 의 per-campaign INSERT 와 동일 필드.
    sqlx::query(
        "UPDATE campaigns SET
           title = ?, description = ?, status = ?,
           started_at = ?, ended_at = ?, display_order = ?, image_path = ?,
           created_at = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&cf.frontmatter.title)
    .bind(&cf.body)
    .bind(&cf.frontmatter.status)
    .bind((!cf.frontmatter.started_at.is_empty()).then_some(&cf.frontmatter.started_at))
    .bind((!cf.frontmatter.ended_at.is_empty()).then_some(&cf.frontmatter.ended_at))
    .bind(cf.frontmatter.display_order)
    .bind(cf.frontmatter.image.as_deref())
    .bind(&cf.frontmatter.created_at)
    .bind(&updated_at)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // 체크리스트 (본문) + linked_quests (frontmatter) re-sync — 같은 트랜잭션.
    let items = crate::repo::extract_checklist_items(&cf.body);
    replace_checklists_in_tx(&mut tx, id, &items).await?;
    resync_campaign_quests_in_tx(&mut tx, id, &cf.frontmatter.linked_quests).await?;
    tx.commit().await?;

    // 캐시 갱신 — 외부 편집 시에만 (다음 진입 churn 방지).
    if externally_edited {
        let _ = crate::file_mtime::touch(store, &path).await;
    }
    Ok(externally_edited)
}

/// DEV-310/DEV-311: 캠페인 본문(`campaigns/{slug}.md`) 의 수정/신규/삭제를 증분 반영.
///
/// 기존 캠페인의 편집은 상세 진입용 `refresh_campaign_if_stale` 과 같은 코어
/// (`refresh_campaign`) 를 쓴다(제목/본문/기간/체크리스트/linked_quests +
/// updated_at write-back 까지 동일 규칙). DEV-311 부터는 신규 파일도 여기서
/// `insert_new_campaign` 으로 바로 INSERT하고, 파일이 사라지거나 외부에서
/// soft-delete 된 캠페인도 행을 직접 DELETE한다 — 예전엔 둘 다 "행 생성·연쇄
/// 삭제가 얽혀 있다"는 이유로 full reindex 로 미뤘지만, 자식 테이블
/// (`campaign_checklists`/`campaign_quests`/`campaign_comments`/`campaign_memos`)
/// 이 전부 FK `ON DELETE CASCADE` 라 실제로는 quest 삭제와 같은 수준으로 단순하다.
async fn sync_changed_campaign_files(
    store: &Store,
    report: &mut IncrementalReport,
) -> AppResult<()> {
    let paths = &store.paths;
    let cache = crate::file_mtime::load_all(store).await;
    let Ok(files) = repo_fs::list_quest_body_files(paths.campaigns_dir()) else {
        return Ok(());
    };
    let mut alive: std::collections::HashSet<String> = std::collections::HashSet::new();
    for path in &files {
        let rel = crate::file_mtime::rel_key(paths, path);
        alive.insert(rel.clone());
        let fresh = match cache.get(&rel) {
            Some(&cached) => repo_fs::mtime_unix_nanos(path) > cached,
            // 캐시에 없음 = 아직 한 번도 반영 안 된 파일(신규일 수 있다).
            None => true,
        };
        if !fresh {
            continue;
        }
        let Some(slug) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM campaigns WHERE campaign_slug = ?")
                .bind(slug)
                .fetch_optional(&store.index_pool)
                .await?;
        if exists.is_none() {
            match insert_new_campaign(store, path).await {
                Ok(Some(_id)) => {
                    report.campaigns_inserted += 1;
                    let _ = crate::file_mtime::touch(store, path).await;
                }
                Ok(None) => {
                    // 신규 파일이 이미 deleted=true — 넣을 행이 없다. 다음 sync 에서
                    // 재검사하지 않도록만 캐시 갱신.
                    let _ = crate::file_mtime::touch(store, path).await;
                }
                Err(e) => {
                    report
                        .skipped
                        .push((path.display().to_string(), format!("{e:#}")));
                    report.needs_full_reindex = true;
                }
            }
            continue;
        }
        if refresh_campaign_row_no_writeback(store, slug).await? {
            // refresh_campaign 은 "외부 편집 반영" 과 "외부 soft-delete로 행 삭제"
            // 둘 다 true 를 반환한다 — 행이 아직 있는지로 구분해 카운터를 나눈다.
            let still_alive: Option<i64> =
                sqlx::query_scalar("SELECT id FROM campaigns WHERE campaign_slug = ?")
                    .bind(slug)
                    .fetch_optional(&store.index_pool)
                    .await?;
            if still_alive.is_some() {
                report.campaigns_synced += 1;
            } else {
                report.campaigns_deleted += 1;
            }
        }
    }
    // 캐시에는 있는데 파일이 사라진 캠페인 — 행 직접 삭제(FK cascade 로 자식도 정리).
    for rel in cache.keys() {
        if !is_campaign_body_rel(rel) || alive.contains(rel) {
            continue;
        }
        let Some(slug) = rel.strip_prefix("campaigns/").and_then(|s| s.strip_suffix(".md"))
        else {
            continue;
        };
        let id: Option<i64> = sqlx::query_scalar("SELECT id FROM campaigns WHERE campaign_slug = ?")
            .bind(slug)
            .fetch_optional(&store.index_pool)
            .await?;
        if let Some(id) = id {
            delete_campaign_row(store, id).await?;
            report.campaigns_deleted += 1;
        }
        let path = paths.campaign_path(slug);
        let _ = crate::file_mtime::touch(store, &path).await; // 파일 없음 → 캐시 row 제거.
    }
    Ok(())
}

/// `campaigns/C-001.md` 처럼 캠페인 **본문** rel 인지 (sibling `.comments.md` 제외).
fn is_campaign_body_rel(rel: &str) -> bool {
    let Some(name) = rel.strip_prefix("campaigns/") else {
        return false;
    };
    name.ends_with(".md") && name.matches('.').count() == 1
}

/// `library/BOOK-001.md` 처럼 도서관 문서 rel 인지.
fn is_library_body_rel(rel: &str) -> bool {
    let Some(name) = rel.strip_prefix("library/") else {
        return false;
    };
    let Some(stem) = name.strip_suffix(".md") else {
        return false;
    };
    crate::repo::library::parse_book_slug(stem).is_some()
}

/// DEV-311: 도서관 문서(`library/{BOOK-NNN}.md`) 1개를 index.db 에 INSERT —
/// `sync_changed_library_files` 가 DB 에 없는 새 파일을 발견했을 때 호출.
/// reindex.rs 와 동일하게 soft-deleted(`deleted=true`) 문서도 (deleted_at 채워)
/// 삽입한다 — 파일이 존재하는 한 번호 충돌 검증/복원을 위해 캐시에 실려야 한다
/// (campaigns 와 달리 skip 하지 않음).
async fn insert_new_library_doc(store: &Store, path: &std::path::Path) -> AppResult<i64> {
    let bf = crate::repo::library::BookFile::read(path).map_err(crate::error::AppError::Internal)?;
    let number = bf.number().map_err(crate::error::AppError::Internal)?;
    let deleted_at: Option<String> = bf.frontmatter.deleted.then(|| bf.frontmatter.updated_at.clone());

    let mut tx = store.index_pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO library_docs (number, title, body, path, created_at, updated_at, deleted_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(number)
    .bind(&bf.frontmatter.title)
    .bind(&bf.body)
    .bind(&bf.frontmatter.path)
    .bind(&bf.frontmatter.created_at)
    .bind(&bf.frontmatter.updated_at)
    .bind(deleted_at)
    .fetch_one(&mut *tx)
    .await?;
    for tag in &bf.frontmatter.tags {
        let normalized = tag.trim();
        if normalized.is_empty() {
            continue;
        }
        sqlx::query("INSERT OR IGNORE INTO library_tags (book_id, tag) VALUES (?, ?)")
            .bind(id)
            .bind(normalized)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// DEV-311: 도서관 문서(`library/{BOOK-NNN}.md`) 의 수정/신규/삭제를 증분 반영.
///
/// 기존 문서의 "수정"은 상세 조회용 `ops::library::get_book` 이 이미 하는 일
/// (파일 재-read + UPDATE + tags sync, BUG-134) 을 그대로 재사용한다 — 문서
/// 상세를 안 열어도 열기 시점에 한 번 훑어 최신화해둔다. 신규/삭제는 이
/// 함수가 직접 처리한다(`get_book` 은 상세 진입 케이스라 신규 INSERT 를
/// 의도적으로 안 함 — 그 함수 주석 참고).
async fn sync_changed_library_files(store: &Store, report: &mut IncrementalReport) -> AppResult<()> {
    let paths = &store.paths;
    let cache = crate::file_mtime::load_all(store).await;
    let Ok(files) = crate::repo::library::list_book_files(paths) else {
        return Ok(());
    };
    let mut alive: std::collections::HashSet<String> = std::collections::HashSet::new();
    for path in &files {
        let rel = crate::file_mtime::rel_key(paths, path);
        alive.insert(rel.clone());
        let fresh = match cache.get(&rel) {
            Some(&cached) => repo_fs::mtime_unix_nanos(path) > cached,
            None => true, // 캐시에 없음 = 아직 한 번도 반영 안 된 파일(신규일 수 있다).
        };
        if !fresh {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(number) = crate::repo::library::parse_book_slug(stem) else {
            continue;
        };
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM library_docs WHERE number = ?")
            .bind(number)
            .fetch_optional(&store.index_pool)
            .await?;
        if exists.is_none() {
            match insert_new_library_doc(store, path).await {
                Ok(_id) => report.library_inserted += 1,
                Err(e) => {
                    report
                        .skipped
                        .push((path.display().to_string(), format!("{e:#}")));
                    report.needs_full_reindex = true;
                    continue;
                }
            }
        } else {
            match crate::ops::library::get_book(store, stem).await {
                Ok(_) => report.library_synced += 1,
                Err(e) => {
                    report
                        .skipped
                        .push((path.display().to_string(), format!("{e:#}")));
                    continue;
                }
            }
        }
        let _ = crate::file_mtime::touch(store, path).await;
    }
    // 캐시에는 있는데 파일이 사라진 문서 — 행 hard delete(library_tags 는 FK cascade).
    for rel in cache.keys() {
        if !is_library_body_rel(rel) || alive.contains(rel) {
            continue;
        }
        let Some(book_id) = rel.strip_prefix("library/").and_then(|s| s.strip_suffix(".md")) else {
            continue;
        };
        if let Some(number) = crate::repo::library::parse_book_slug(book_id) {
            let result = sqlx::query("DELETE FROM library_docs WHERE number = ?")
                .bind(number)
                .execute(&store.index_pool)
                .await?;
            if result.rows_affected() > 0 {
                report.library_deleted += 1;
            }
        }
        let path = paths.book_path(book_id);
        let _ = crate::file_mtime::touch(store, &path).await; // 파일 없음 → 캐시 row 제거.
    }
    Ok(())
}

/// DEV-310: sibling 파일(`{slug}.comments.md` / `{slug}.memo.md`) 변경을 증분
/// 반영한다. quest 본문(DEV-246)에 이어 **빈도가 가장 높은** 부류 — 댓글 하나를
/// 외부에서 고치거나 git pull 로 들어오면 예전엔 전체 재색인이 돌았다.
///
/// 판정은 `detect_drift` 와 같은 기준(per-file `file_mtime_cache`):
/// - 캐시에 없거나 파일이 더 새것 → 그 파일만 다시 파싱해 해당 행을 교체
/// - 캐시에 있는데 파일이 사라짐 → 그 행 삭제 + 캐시 row 제거
///
/// 대상 quest/campaign 이 index 에 없으면 증분으로 표현할 수 없으니
/// `needs_full_reindex` 로 넘긴다(본문 파일 신규 적재는 quest 쪽 경로의 일).
async fn sync_changed_sibling_files(store: &Store, report: &mut IncrementalReport) -> AppResult<()> {
    use crate::repo::fs as rfs;
    let paths = &store.paths;
    let pool = &store.index_pool;
    let cache = crate::file_mtime::load_all(store).await;

    // (디렉토리, 종류) 별로 현존 파일을 나열 — 파일명만으로는 quest/campaign 을
    // 구분할 수 없다(둘 다 `{slug}.comments.md`).
    let mut targets: Vec<(std::path::PathBuf, SiblingKind)> = Vec::new();
    for (dir, comments, memo) in [
        (
            paths.quests_dir(),
            SiblingKind::QuestComments,
            SiblingKind::QuestMemo,
        ),
        (
            paths.campaigns_dir(),
            SiblingKind::CampaignComments,
            SiblingKind::CampaignMemo,
        ),
    ] {
        if let Ok(files) = rfs::list_quest_comment_files(&dir) {
            targets.extend(files.into_iter().map(|p| (p, comments)));
        }
        if let Ok(files) = rfs::list_quest_memo_files(&dir) {
            targets.extend(files.into_iter().map(|p| (p, memo)));
        }
    }

    let mut alive: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (path, kind) in &targets {
        let rel = crate::file_mtime::rel_key(paths, path);
        alive.insert(rel.clone());
        let file_mtime = rfs::mtime_unix_nanos(path);
        let fresh = match cache.get(&rel) {
            Some(&cached) => file_mtime > cached,
            None => true,
        };
        if !fresh {
            continue;
        }
        let Some(slug) = rfs::quest_slug_from_sibling_path(path) else {
            continue;
        };
        let owner = owner_id(pool, *kind, &slug).await?;
        let Some(owner) = owner else {
            // 본문이 아직 index 에 없다 — 이번 열기의 quest 경로가 방금 넣었을
            // 수도 있고(그럼 다음 열기에 잡힌다), 캠페인처럼 아직 증분 대상이
            // 아닐 수도 있다. 안전하게 풀 reindex 로.
            report.needs_full_reindex = true;
            continue;
        };
        match replace_sibling_rows(store, *kind, owner, path).await {
            Ok(()) => {
                let _ = crate::file_mtime::touch(store, path).await;
                report.siblings_synced += 1;
            }
            Err(e) => report.skipped.push((path.display().to_string(), e.to_string())),
        }
    }

    // 캐시에 남아 있는데 파일은 사라진 sibling — 그 행을 지운다.
    // (primary cached 파일(캠페인 본문/정의 toml)은 이 모듈 범위가 아니므로
    //  sibling 이름 규칙에 맞는 rel 만 본다.)
    for rel in cache.keys() {
        if alive.contains(rel) {
            continue;
        }
        let Some(kind) = sibling_kind_from_rel(rel) else {
            continue;
        };
        let slug = rel
            .rsplit('/')
            .next()
            .and_then(|n| n.split('.').next())
            .unwrap_or_default()
            .to_string();
        if slug.is_empty() {
            continue;
        }
        if let Some(owner) = owner_id(pool, kind, &slug).await? {
            delete_sibling_rows(pool, kind, owner).await?;
        }
        sqlx::query("DELETE FROM file_mtime_cache WHERE rel_path = ?")
            .bind(rel)
            .execute(pool)
            .await?;
        report.siblings_synced += 1;
    }
    Ok(())
}

/// rel_path(`quests/DEV-001.comments.md`) → sibling 종류. sibling 이 아니면 None.
fn sibling_kind_from_rel(rel: &str) -> Option<SiblingKind> {
    let (dir, name) = rel.split_once('/')?;
    let comments = name.ends_with(".comments.md");
    let memo = name.ends_with(".memo.md");
    match (dir, comments, memo) {
        ("quests", true, _) => Some(SiblingKind::QuestComments),
        ("quests", _, true) => Some(SiblingKind::QuestMemo),
        ("campaigns", true, _) => Some(SiblingKind::CampaignComments),
        ("campaigns", _, true) => Some(SiblingKind::CampaignMemo),
        _ => None,
    }
}

/// slug → 소유 행 id (quest 는 삭제된 것도 포함 — 파일이 진리원이라 남아 있다).
async fn owner_id(
    pool: &sqlx::SqlitePool,
    kind: SiblingKind,
    slug: &str,
) -> AppResult<Option<i64>> {
    let id = match kind {
        SiblingKind::QuestComments | SiblingKind::QuestMemo => {
            // quest 는 (prefix, number) 복합키 — slug 를 쪼개서 찾는다.
            let Some((prefix, number)) = slug
                .rsplit_once('-')
                .and_then(|(p, n)| n.parse::<i64>().ok().map(|n| (p, n)))
            else {
                return Ok(None);
            };
            sqlx::query_scalar::<_, i64>(
                "SELECT q.id FROM quests q JOIN quest_types qt ON qt.id = q.quest_type_id
                 WHERE qt.prefix = ? AND q.number = ?",
            )
            .bind(prefix)
            .bind(number)
            .fetch_optional(pool)
            .await?
        }
        SiblingKind::CampaignComments | SiblingKind::CampaignMemo => {
            sqlx::query_scalar::<_, i64>("SELECT id FROM campaigns WHERE campaign_slug = ?")
                .bind(slug)
                .fetch_optional(pool)
                .await?
        }
    };
    Ok(id)
}

async fn delete_sibling_rows(
    pool: &sqlx::SqlitePool,
    kind: SiblingKind,
    owner: i64,
) -> AppResult<()> {
    let sql = match kind {
        SiblingKind::QuestComments => "DELETE FROM quest_comments WHERE quest_id = ?",
        SiblingKind::QuestMemo => "DELETE FROM quest_memos WHERE quest_id = ?",
        SiblingKind::CampaignComments => "DELETE FROM campaign_comments WHERE campaign_id = ?",
        SiblingKind::CampaignMemo => "DELETE FROM campaign_memos WHERE campaign_id = ?",
    };
    sqlx::query(sql).bind(owner).execute(pool).await?;
    Ok(())
}

/// 파일 1개 → 그 소유자의 캐시 행 전체 교체 (delete + insert). reindex 의
/// 같은 부분(5b' / 6b)과 동일한 컬럼·의미를 유지해야 한다.
async fn replace_sibling_rows(
    store: &Store,
    kind: SiblingKind,
    owner: i64,
    path: &std::path::Path,
) -> AppResult<()> {
    let pool = &store.index_pool;
    let raw = std::fs::read_to_string(path)
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?;
    let mut tx = pool.begin().await?;
    match kind {
        SiblingKind::QuestComments => {
            sqlx::query("DELETE FROM quest_comments WHERE quest_id = ?")
                .bind(owner)
                .execute(&mut *tx)
                .await?;
            for entry in crate::repo::comments::parse_entries(&raw) {
                sqlx::query(
                    "INSERT INTO quest_comments
                        (quest_id, entry_id, ts, author, body, parent_id, discussion, resolved, pinned, edited_at, reactions)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(owner)
                .bind(entry.id as i64)
                .bind(&entry.ts)
                .bind(&entry.author)
                .bind(&entry.body)
                .bind(entry.parent_id.map(|n| n as i64))
                .bind(entry.discussion as i64)
                .bind(entry.resolved as i64)
                .bind(entry.pinned as i64)
                .bind(&entry.edited_at)
                .bind(entry.reactions.join(","))
                .execute(&mut *tx)
                .await?;
            }
        }
        SiblingKind::CampaignComments => {
            sqlx::query("DELETE FROM campaign_comments WHERE campaign_id = ?")
                .bind(owner)
                .execute(&mut *tx)
                .await?;
            for entry in crate::repo::comments::parse_entries(&raw) {
                sqlx::query(
                    "INSERT INTO campaign_comments
                        (campaign_id, entry_id, ts, author, body, parent_id, pinned, edited_at, reactions)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(owner)
                .bind(entry.id as i64)
                .bind(&entry.ts)
                .bind(&entry.author)
                .bind(&entry.body)
                .bind(entry.parent_id.map(|n| n as i64))
                .bind(entry.pinned as i64)
                .bind(&entry.edited_at)
                .bind(entry.reactions.join(","))
                .execute(&mut *tx)
                .await?;
            }
        }
        SiblingKind::QuestMemo | SiblingKind::CampaignMemo => {
            // 메모는 파일 하나당 행 하나. updated_at 은 reindex 와 같이 mtime 근사.
            let updated_at = crate::repo::fs::mtime_iso8601(path).unwrap_or_default();
            let (del, ins) = if kind == SiblingKind::QuestMemo {
                (
                    "DELETE FROM quest_memos WHERE quest_id = ?",
                    "INSERT INTO quest_memos (quest_id, user_id, content, updated_at) VALUES (?, 0, ?, ?)",
                )
            } else {
                (
                    "DELETE FROM campaign_memos WHERE campaign_id = ?",
                    "INSERT INTO campaign_memos (campaign_id, user_id, content, updated_at) VALUES (?, 0, ?, ?)",
                )
            };
            sqlx::query(del).bind(owner).execute(&mut *tx).await?;
            sqlx::query(ins)
                .bind(owner)
                .bind(&raw)
                .bind(&updated_at)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Store::open 후 통합 sync — 증분 처리 + 필요할 때만 fallback reindex.
///
/// 흐름:
/// 1. `sync_changed_quest_files` — quest 본문의 수정/신규/삭제 (DEV-246).
/// 2. `sync_changed_campaign_files` — 캠페인 본문의 수정/신규/삭제 (DEV-310/311).
/// 3. `sync_changed_sibling_files` — 댓글/메모 파일 (DEV-310).
/// 4. `sync_changed_library_files` — 도서관 문서의 수정/신규/삭제 (DEV-311).
///    quest/campaign 과 독립(참조 관계 없음) — 순서 무관.
/// 5. 위 중 하나라도 증분으로 표현 못 하는 변화를 만났으면 `drift::auto_resync`.
///
/// 통합 호출자는 `Store::open_with_sync` (store.rs).
pub async fn sync_on_open(
    store: &Store,
) -> AppResult<(IncrementalReport, Option<crate::reindex::ReindexReport>)> {
    let mut inc = sync_changed_quest_files(store).await?;
    // DEV-310: 캠페인 본문 → sibling 순서. 캠페인 행이 먼저 최신이어야 그 캠페인의
    // 댓글도 같은 열기에서 반영된다(순서가 반대면 소유자가 없어 full 로 넘어간다).
    sync_changed_campaign_files(store, &mut inc).await?;
    sync_changed_sibling_files(store, &mut inc).await?;
    sync_changed_library_files(store, &mut inc).await?;
    let inc = inc;
    let reindex_report = if inc.needs_full_reindex {
        crate::drift::auto_resync(store)
            .await
            .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?
    } else {
        None
    };
    Ok((inc, reindex_report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{QuestFile, QuestFrontmatter, seed_guild_dir};

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-inc-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    async fn setup(dir: &std::path::Path) -> Store {
        seed_guild_dir(dir).unwrap();
        let store = Store::open(dir).await.unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        store
    }

    #[test]
    fn replace_frontmatter_updated_at_swaps_only_that_line() {
        let src = "+++\nquest_id = \"DEV-001\"\nupdated_at = \"2020-01-01T00:00:00Z\"\ndeleted = false\n+++\n\nbody updated_at = stays\n";
        let out = replace_frontmatter_updated_at(src, "2026-06-19T12:00:00+09:00").unwrap();
        assert!(out.contains("updated_at = \"2026-06-19T12:00:00+09:00\""));
        assert!(!out.contains("2020-01-01T00:00:00Z"));
        // 본문의 'updated_at' 텍스트는 그대로.
        assert!(out.contains("body updated_at = stays"));
        assert!(out.contains("quest_id = \"DEV-001\""));
    }

    #[test]
    fn replace_frontmatter_updated_at_none_without_frontmatter() {
        assert!(replace_frontmatter_updated_at("no frontmatter here\n", "x").is_none());
    }

    fn mk_quest(id: &str, title: &str, parent: Option<&str>, prereqs: &[&str], tags: &[&str]) -> QuestFile {
        QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: id.into(),
                title: title.into(),
                status: "open".into(),
                urgency: 3,
                parent: parent.map(str::to_string),
                prerequisites: prereqs.iter().map(|s| s.to_string()).collect(),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: tags.iter().map(|s| s.to_string()).collect(),
            },
            description: format!("body of {id}"),
            auto_block: String::new(),
        }
    }

    /// DEV-246: 신규 파일이 **풀 reindex 없이** 적재된다.
    ///
    /// 예전엔 파일 1개 추가만으로 needs_full_reindex 가 서서 전체 재구축(이 길드
    /// 기준 warm 1.3s / cold 10.2s)이 돌았다 — git pull/브랜치 전환/CLI 생성마다.
    #[tokio::test]
    async fn new_file_inserted_without_full_reindex() {
        let dir = fresh_tmp("new-file");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        mk_quest("DEV-001", "brand new", None, &[], &["alpha"])
            .write(paths.quest_path("DEV-001"))
            .unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report.inserted, 1, "신규 파일이 INSERT 돼야");
        assert!(
            !report.needs_full_reindex,
            "신규 파일 하나로 풀 reindex 를 요청하면 DEV-246 실패"
        );

        let title: String = sqlx::query_scalar("SELECT title FROM quests WHERE title = 'brand new'")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(title, "brand new");
        // 태그 캐시도 함께 (증분 cascade).
        let tag: String = sqlx::query_scalar("SELECT tag FROM quest_tags LIMIT 1")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(tag, "alpha");
    }

    /// DEV-246: 파일이 사라지면 해당 행만 DELETE — 역시 풀 reindex 없이.
    #[tokio::test]
    async fn deleted_file_removes_row_without_full_reindex() {
        let dir = fresh_tmp("del-file");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        mk_quest("DEV-001", "keep", None, &[], &[])
            .write(paths.quest_path("DEV-001"))
            .unwrap();
        mk_quest("DEV-002", "will vanish", None, &[], &[])
            .write(paths.quest_path("DEV-002"))
            .unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        std::fs::remove_file(paths.quest_path("DEV-002")).unwrap();
        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report.deleted, 1);
        assert!(!report.needs_full_reindex, "삭제 하나로 풀 reindex 금지");

        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM quests WHERE title = 'will vanish'")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
        let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM quests WHERE title = 'keep'")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(kept, 1, "남은 퀘스트는 그대로여야");
    }

    /// DEV-246: frontmatter 의 선행/태그만 바꿔도 증분으로 반영된다.
    /// (예전엔 이 경우가 풀 reindex 트리거였다 — 사실상 매번.)
    #[tokio::test]
    async fn prereq_and_tag_change_syncs_incrementally() {
        let dir = fresh_tmp("cascade");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        mk_quest("DEV-001", "base", None, &[], &[])
            .write(paths.quest_path("DEV-001"))
            .unwrap();
        mk_quest("DEV-002", "dependent", None, &[], &["old"])
            .write(paths.quest_path("DEV-002"))
            .unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        // 외부 편집: DEV-002 에 선행(DEV-001) + 태그 교체.
        std::thread::sleep(std::time::Duration::from_millis(20));
        mk_quest("DEV-002", "dependent", None, &["DEV-001"], &["fresh"])
            .write(paths.quest_path("DEV-002"))
            .unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert!(
            !report.needs_full_reindex,
            "선행/태그 변경으로 풀 reindex 를 요청하면 DEV-246 실패"
        );

        let deps: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM quest_dependencies d
               JOIN quests q ON q.id = d.quest_id
              WHERE q.title = 'dependent'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(deps, 1, "선행 관계가 증분으로 들어가야");

        let tags: Vec<String> = sqlx::query_scalar(
            "SELECT tag FROM quest_tags t JOIN quests q ON q.id = t.quest_id
              WHERE q.title = 'dependent'",
        )
        .fetch_all(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(tags, vec!["fresh".to_string()], "옛 태그는 사라지고 새 태그만");
    }

    /// DEV-246: 신규 퀘스트가 **같은 배치의 다른 신규 퀘스트**를 부모/선행으로
    /// 가리켜도 해석된다(2-pass). git pull 로 여러 파일이 함께 들어오는 경우.
    #[tokio::test]
    async fn new_files_referencing_each_other_resolve() {
        let dir = fresh_tmp("batch");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        // 부모가 목록 뒤쪽에 오도록 일부러 자식을 먼저 쓴다.
        mk_quest("DEV-002", "child", Some("DEV-001"), &["DEV-001"], &[])
            .write(paths.quest_path("DEV-002"))
            .unwrap();
        mk_quest("DEV-001", "parent", None, &[], &[])
            .write(paths.quest_path("DEV-001"))
            .unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report.inserted, 2);
        assert!(!report.needs_full_reindex);

        let parent_title: String = sqlx::query_scalar(
            "SELECT p.title FROM quests c JOIN quests p ON p.id = c.parent_quest_id
              WHERE c.title = 'child'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(parent_title, "parent", "같은 배치의 부모도 연결돼야");

        let deps: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM quest_dependencies d JOIN quests q ON q.id = d.quest_id
              WHERE q.title = 'child'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(deps, 1, "같은 배치의 선행도 연결돼야");
    }

    /// DEV-246: 증분으로 표현 불가능한 경우(정의에 없는 상태)는 안전망으로
    /// 여전히 풀 reindex 를 요청한다 — 조용히 틀린 상태로 두지 않는다.
    #[tokio::test]
    async fn unknown_status_still_falls_back() {
        let dir = fresh_tmp("unknown-status");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        let mut qf = mk_quest("DEV-001", "weird", None, &[], &[]);
        qf.frontmatter.status = "no-such-status".into();
        qf.write(paths.quest_path("DEV-001")).unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert!(
            report.needs_full_reindex,
            "정의에 없는 상태는 안전망(풀 reindex)으로 넘겨야"
        );
    }

    /// 외부 편집된 파일이 정확히 UPDATE 되고 cached_mtime 이 갱신.
    #[tokio::test]
    async fn modified_file_detected_and_updated() {
        let dir = fresh_tmp("modify");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        // 시드 + reindex 후 quest 하나 추가 + 풀 reindex.
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "original".into(),
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
            description: "body v1".into(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path("DEV-001")).unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        // 외부 편집 시뮬레이션 — 살짝 기다린 후 새 mtime 으로 덮어씀.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let qf2 = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "edited externally".into(),
                status: "open".into(),
                urgency: 2,
                parent: None,
                prerequisites: vec![],
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-02T00:00:00Z".into(),
                deleted: false,
                desired_due: None,
                required_due: None,
                tags: vec![],
            },
            description: "body v2".into(),
            auto_block: String::new(),
        };
        qf2.write(paths.quest_path("DEV-001")).unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report.updated, 1, "modified file 1건 UPDATE 되어야");

        // DB 확인 — 새 title / urgency 가 반영.
        let row: (String, i64, String) =
            sqlx::query_as("SELECT title, urgency, updated_at FROM quests WHERE id = 1")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(row.0, "edited externally");
        assert_eq!(row.1, 2);
        // BUG-080: updated_at 은 stale frontmatter(2026-01-02)가 아니라 파일 mtime 보정.
        assert_ne!(
            row.2, "2026-01-02T00:00:00Z",
            "updated_at 이 파일 mtime 으로 갱신되어야"
        );
        // 파일 frontmatter 도 write-back — DB 와 동일 값, 본문은 보존.
        let on_disk = std::fs::read_to_string(paths.quest_path("DEV-001")).unwrap();
        assert!(
            !on_disk.contains("updated_at = \"2026-01-02T00:00:00Z\""),
            "frontmatter updated_at 이 write-back 되어야"
        );
        assert!(on_disk.contains(&format!("updated_at = \"{}\"", row.2)));
        assert!(on_disk.contains("body v2"), "본문은 보존되어야");

        // 두 번째 호출 — 변경 없음 (write-back 후 cached_mtime 갱신 → churn 없음).
        let report2 = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report2.updated, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-103: git checkout/pull 시뮬레이션 — **내용은 동일한데 mtime 만 갱신**된
    /// 파일은 외부 편집이 아님. updated_at write-back 없이 cached_mtime 만 갱신.
    #[tokio::test]
    async fn same_content_new_mtime_does_not_rewrite_updated_at() {
        let dir = fresh_tmp("bug103");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "original".into(),
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
            description: "body v1".into(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path("DEV-001")).unwrap();
        crate::reindex::reindex(&store).await.unwrap();
        let before: String = sqlx::query_scalar("SELECT updated_at FROM quests WHERE id = 1")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();

        // git checkout 시뮬레이션: 동일 내용 재작성 → mtime 만 전진.
        std::thread::sleep(std::time::Duration::from_millis(20));
        qf.write(paths.quest_path("DEV-001")).unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(
            report.updated, 0,
            "내용 동일 — 외부 편집으로 세면 안 됨 (BUG-103)"
        );
        assert!(!report.needs_full_reindex, "내용 동일 — 풀 reindex 불필요");

        // updated_at 은 DB / 파일 모두 원래 값 그대로.
        let after: String = sqlx::query_scalar("SELECT updated_at FROM quests WHERE id = 1")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(before, after, "updated_at 변조 금지 (BUG-103)");
        let on_disk = std::fs::read_to_string(paths.quest_path("DEV-001")).unwrap();
        assert!(
            on_disk.contains("updated_at = \"2026-01-01T00:00:00Z\""),
            "파일 frontmatter updated_at 도 원본 유지"
        );

        // cached_mtime 은 갱신되어 재감지 churn 없음.
        let report2 = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report2.updated, 0);

        // refresh_quest_if_stale 도 동일 게이트 — 재작성(mtime 전진) 후 진입해도
        // 외부 편집 아님으로 판정.
        std::thread::sleep(std::time::Duration::from_millis(20));
        qf.write(paths.quest_path("DEV-001")).unwrap();
        let edited = refresh_quest_if_stale(&store, "DEV-001").await.unwrap();
        assert!(!edited, "내용 동일 — refresh 도 외부 편집 아님 (BUG-103)");
        let after2: String = sqlx::query_scalar("SELECT updated_at FROM quests WHERE id = 1")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(before, after2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-145: BUG-103 재발 — DB 의 cached `updated_at` 이 (과거의 오탐성
    /// write-back 등으로) 파일의 실제 committed 값과 이미 어긋나 있는 상태에서,
    /// 진짜 콘텐츠(title/description/...)는 파일과 DB 가 동일하면 — mtime 만
    /// 앞서 있어도(git checkout 시뮬레이션) 여전히 "동일"로 보고 파일을
    /// write-back 하면 안 된다. `updated_at` 을 비교에 포함시키면 이 케이스가
    /// 매번 "다르다"로 오판되어 checkout 할 때마다 무관한 quest 파일이 계속
    /// 변조되는 무한 재발 루프가 생긴다.
    #[tokio::test]
    async fn bug145_stale_cached_updated_at_does_not_perpetually_rewrite() {
        let dir = fresh_tmp("bug145");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "original".into(),
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
            description: "body v1".into(),
            auto_block: String::new(),
        };
        qf.write(paths.quest_path("DEV-001")).unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        // DB 의 cached updated_at 을 파일과 어긋나게 직접 오염 — 과거의 오탐성
        // write-back 이 이미 한 번 일어난 상태를 시뮬레이션.
        sqlx::query("UPDATE quests SET updated_at = ? WHERE id = 1")
            .bind("2026-07-19T09:00:00+09:00")
            .execute(&store.index_pool)
            .await
            .unwrap();

        // git checkout 시뮬레이션: 파일은 원래(committed) 내용 그대로 재작성 →
        // mtime 만 전진. 내용(title/description/...)은 DB 와 여전히 동일 —
        // 어긋난 건 오직 DB 의 updated_at 뿐.
        std::thread::sleep(std::time::Duration::from_millis(20));
        qf.write(paths.quest_path("DEV-001")).unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(
            report.updated, 0,
            "updated_at 만 DB 와 어긋난 상태 — 진짜 콘텐츠가 같으면 외부 편집으로 세면 안 됨 (BUG-145)"
        );

        // 파일이 write-back 되지 않아야 — 원본 그대로.
        let on_disk = std::fs::read_to_string(paths.quest_path("DEV-001")).unwrap();
        assert!(
            on_disk.contains("updated_at = \"2026-01-01T00:00:00Z\""),
            "파일 frontmatter 가 재기록되면 안 됨 (BUG-145 재발 방지)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 변경 없으면 cached_mtime 비교만 — UPDATE 0건.
    #[tokio::test]
    async fn no_change_no_update() {
        let dir = fresh_tmp("noop");
        let store = setup(&dir).await;

        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "stable".into(),
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
            description: "stable body".into(),
            auto_block: String::new(),
        };
        qf.write(store.paths.quest_path("DEV-001")).unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report.updated, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-137: 단일 quest lazy refresh — 변경 시 true + DB 갱신, 무변경 false.
    #[tokio::test]
    async fn refresh_quest_if_stale_single_file() {
        let dir = fresh_tmp("lazy");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "v1".into(),
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
        crate::reindex::reindex(&store).await.unwrap();

        // 변경 없음 → false.
        assert!(!refresh_quest_if_stale(&store, "DEV-001").await.unwrap());

        // 외부 편집 → true + title 갱신.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut qf2 = qf.clone();
        qf2.frontmatter.title = "v2 external".into();
        qf2.write(paths.quest_path("DEV-001")).unwrap();
        assert!(refresh_quest_if_stale(&store, "DEV-001").await.unwrap());
        let title: String = sqlx::query_scalar("SELECT title FROM quests WHERE id = 1")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(title, "v2 external");
        // 두 번째 호출 — cached_mtime 갱신됐으니 false.
        assert!(!refresh_quest_if_stale(&store, "DEV-001").await.unwrap());

        // 없는 slug → false (에러 아님).
        assert!(!refresh_quest_if_stale(&store, "DEV-999").await.unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-246 이전에는 "신규 파일 → needs_full_reindex" 를 고정하던 테스트였다.
    /// 이제 신규 파일도 증분으로 INSERT 하므로 **반대**를 검증한다 — updated 는
    /// 0(수정이 아니므로)이고 inserted 가 1, 풀 reindex 는 요청하지 않는다.
    #[tokio::test]
    async fn new_file_inserted_not_full_reindex() {
        let dir = fresh_tmp("new");
        let store = setup(&dir).await;

        // 시드만 — quest 0건.
        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "new quest".into(),
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
        qf.write(store.paths.quest_path("DEV-001")).unwrap();

        let report = sync_changed_quest_files(&store).await.unwrap();
        assert_eq!(report.updated, 0, "신규는 updated 가 아니라 inserted");
        assert_eq!(report.inserted, 1);
        assert!(
            !report.needs_full_reindex,
            "DEV-246: 신규 파일 하나로 전체 재구축을 요청하지 않는다"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-178: 캠페인 상세 lazy refresh — 외부 편집 감지 시 그 행만 갱신,
    /// 무변경엔 no-op (touch 로 churn 없음).
    #[tokio::test]
    async fn refresh_campaign_if_stale_detects_external_edit() {
        let dir = fresh_tmp("camp-lazy");
        let store = setup(&dir).await;
        let paths = store.paths.clone();
        std::fs::create_dir_all(paths.campaigns_dir()).unwrap();

        let cf = crate::repo::CampaignFile {
            frontmatter: crate::repo::CampaignFrontmatter {
                campaign_id: "C-001".into(),
                title: "orig".into(),
                status: "active".into(),
                started_at: String::new(),
                ended_at: String::new(),
                linked_quests: vec![],
                display_order: 0,
                image: None,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
            },
            body: "body v1".into(),
        };
        cf.write(paths.campaign_path("C-001")).unwrap();
        // reindex 가 campaigns 행 + file_mtime 캐시(sync_all) 채움.
        crate::reindex::reindex(&store).await.unwrap();

        // 변경 없음 → false.
        assert!(!refresh_campaign_if_stale(&store, "C-001").await.unwrap());

        // 외부 편집 — 파일 직접 수정 (새 mtime).
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut cf2 = cf.clone();
        cf2.frontmatter.title = "edited externally".into();
        cf2.body = "body v2".into();
        cf2.write(paths.campaign_path("C-001")).unwrap();

        // 감지 → true, DB 갱신.
        assert!(refresh_campaign_if_stale(&store, "C-001").await.unwrap());
        let (title, desc): (String, Option<String>) = sqlx::query_as(
            "SELECT title, description FROM campaigns WHERE campaign_slug = 'C-001'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(title, "edited externally");
        assert_eq!(desc.as_deref(), Some("body v2"));

        // BUG-080: updated_at 이 stale frontmatter(2026-01-01)가 아니라 파일 mtime 보정.
        let db_updated: String =
            sqlx::query_scalar("SELECT updated_at FROM campaigns WHERE campaign_slug = 'C-001'")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_ne!(
            db_updated, "2026-01-01T00:00:00Z",
            "updated_at 이 파일 mtime 으로 보정되어야"
        );
        // 파일 frontmatter 도 write-back, 본문 보존.
        let on_disk = std::fs::read_to_string(paths.campaign_path("C-001")).unwrap();
        assert!(
            !on_disk.contains("updated_at = \"2026-01-01T00:00:00Z\""),
            "frontmatter updated_at 이 write-back 되어야"
        );
        assert!(on_disk.contains("body v2"), "본문은 보존되어야");

        // 두 번째 호출 — touch 로 cache 갱신됐으니 false (churn 없음).
        assert!(!refresh_campaign_if_stale(&store, "C-001").await.unwrap());

        // 없는 slug → false (에러 아님).
        assert!(!refresh_campaign_if_stale(&store, "C-999").await.unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-089: mtime 게이트가 닫혀 있어도(다른 openguild 프로세스가 파일 +
    /// cached_mtime 을 함께 갱신한 상황) 상세 refresh 는 콘텐츠를 *항상* re-read 해
    /// DB 에 반영한다. 파일 write-back(churn)은 없음(false 반환).
    #[tokio::test]
    async fn refresh_quest_resyncs_content_even_when_gate_closed() {
        let dir = fresh_tmp("lazy-poison");
        let store = setup(&dir).await;
        let paths = store.paths.clone();

        let qf = QuestFile {
            frontmatter: QuestFrontmatter {
                quest_id: "DEV-001".into(),
                title: "v1".into(),
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
        crate::reindex::reindex(&store).await.unwrap();

        // 다른 프로세스(CLI)가 파일을 v2 로 고치고 cached_mtime 도 새 mtime 으로
        // 갱신한 상황 시뮬레이션 → file_mtime <= cached → 게이트 닫힘.
        let mut qf2 = qf.clone();
        qf2.frontmatter.title = "v2 by other process".into();
        qf2.write(paths.quest_path("DEV-001")).unwrap();
        let m = repo_fs::mtime_unix_nanos(paths.quest_path("DEV-001"));
        sqlx::query("UPDATE quests SET cached_mtime = ? WHERE id = 1")
            .bind(m)
            .execute(&store.index_pool)
            .await
            .unwrap();

        // 게이트상 외부편집 아님(false) — 그러나 콘텐츠는 re-sync 되어야.
        let edited = refresh_quest_if_stale(&store, "DEV-001").await.unwrap();
        assert!(!edited, "file_mtime <= cached → write-back 안 함(false)");
        let title: String = sqlx::query_scalar("SELECT title FROM quests WHERE id = 1")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(
            title, "v2 by other process",
            "게이트가 닫혀도 콘텐츠는 항상 re-read 되어야 (BUG-089)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// BUG-089: 캠페인도 동일 — 게이트가 닫혀도 콘텐츠/체크리스트/링크 re-sync.
    #[tokio::test]
    async fn refresh_campaign_resyncs_content_even_when_gate_closed() {
        let dir = fresh_tmp("camp-lazy-poison");
        let store = setup(&dir).await;
        let paths = store.paths.clone();
        std::fs::create_dir_all(paths.campaigns_dir()).unwrap();

        let cf = crate::repo::CampaignFile {
            frontmatter: crate::repo::CampaignFrontmatter {
                campaign_id: "C-001".into(),
                title: "orig".into(),
                status: "active".into(),
                started_at: String::new(),
                ended_at: String::new(),
                linked_quests: vec![],
                display_order: 0,
                image: None,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                deleted: false,
            },
            body: "body v1".into(),
        };
        cf.write(paths.campaign_path("C-001")).unwrap();
        crate::reindex::reindex(&store).await.unwrap();

        // 다른 프로세스가 파일 수정 + cache touch (= CLI 동작) → 게이트 닫힘.
        let mut cf2 = cf.clone();
        cf2.frontmatter.title = "edited by other process".into();
        cf2.body = "body v2".into();
        cf2.write(paths.campaign_path("C-001")).unwrap();
        let _ = crate::file_mtime::touch(&store, &paths.campaign_path("C-001")).await;

        // 게이트상 외부편집 아님(false) — 그러나 콘텐츠 re-sync 되어야.
        let edited = refresh_campaign_if_stale(&store, "C-001").await.unwrap();
        assert!(
            !edited,
            "file_mtime <= cached(touch) → write-back 안 함(false)"
        );
        let (title, desc): (String, Option<String>) = sqlx::query_as(
            "SELECT title, description FROM campaigns WHERE campaign_slug = 'C-001'",
        )
        .fetch_one(&store.index_pool)
        .await
        .unwrap();
        assert_eq!(
            title, "edited by other process",
            "게이트가 닫혀도 콘텐츠는 항상 re-read 되어야 (BUG-089)"
        );
        assert_eq!(desc.as_deref(), Some("body v2"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── DEV-310: sibling(.comments.md / .memo.md) 증분 ───

    /// 외부 편집 시뮬 — ops 밖에서 파일을 쓰고 mtime 이 캐시보다 새것이 되게 한다.
    /// (마커 없는 댓글 파일은 전체가 entry 1개로 파싱된다 — repo::comments)
    fn write_external(path: &std::path::Path, body: &str) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(path, body).unwrap();
    }

    async fn mk_one_quest(store: &Store) -> String {
        crate::ops::create_quest(
            store,
            crate::models::CreateQuestRequest {
                quest_type_id: 1,
                title: "sibling 대상".into(),
                description: None,
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap()
        .quest_id
    }

    #[tokio::test]
    async fn external_comment_edit_syncs_without_full_reindex() {
        let dir = fresh_tmp("sib-comment");
        let store = setup(&dir).await;
        let slug = mk_one_quest(&store).await;
        let path = store.paths.dot_guild().join(format!("quests/{slug}.comments.md"));

        write_external(&path, "외부에서 쓴 댓글\n");
        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(
            !inc.needs_full_reindex && full.is_none(),
            "댓글 파일 하나 때문에 풀 reindex 가 돌면 DEV-310 실패: {inc:?}"
        );
        assert!(inc.siblings_synced >= 1);
        let bodies: Vec<String> =
            sqlx::query_scalar("SELECT body FROM quest_comments ORDER BY entry_id")
                .fetch_all(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(bodies.len(), 1);
        assert!(bodies[0].contains("외부에서 쓴 댓글"), "실제 내용: {bodies:?}");

        // 두 번째 편집도 반영되고(내용 교체, 중복 누적 X), 아무 변화 없으면 skip.
        write_external(&path, "고친 댓글\n");
        let (inc2, _) = sync_on_open(&store).await.unwrap();
        assert!(!inc2.needs_full_reindex);
        let bodies: Vec<String> = sqlx::query_scalar("SELECT body FROM quest_comments")
            .fetch_all(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(bodies.len(), 1, "행이 누적되면 안 됨: {bodies:?}");
        assert!(bodies[0].contains("고친 댓글"));

        let (inc3, _) = sync_on_open(&store).await.unwrap();
        assert_eq!(inc3.siblings_synced, 0, "변화 없으면 다시 읽지 않아야");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn deleted_comment_file_clears_rows_without_full_reindex() {
        let dir = fresh_tmp("sib-delete");
        let store = setup(&dir).await;
        let slug = mk_one_quest(&store).await;
        let path = store.paths.dot_guild().join(format!("quests/{slug}.comments.md"));

        write_external(&path, "지워질 댓글\n");
        sync_on_open(&store).await.unwrap();

        std::fs::remove_file(&path).unwrap();
        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(
            !inc.needs_full_reindex && full.is_none(),
            "sibling 삭제로 풀 reindex 를 타면 안 됨: {inc:?}"
        );
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quest_comments")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "파일이 사라졌으면 캐시 행도 사라져야");
        // 캐시 row 도 정리 — 안 지우면 매번 '사라진 파일' 로 다시 잡힌다.
        let (inc2, _) = sync_on_open(&store).await.unwrap();
        assert_eq!(inc2.siblings_synced, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn external_memo_edit_syncs_incrementally() {
        let dir = fresh_tmp("sib-memo");
        let store = setup(&dir).await;
        let slug = mk_one_quest(&store).await;
        let path = store.paths.dot_guild().join(format!("quests/{slug}.memo.md"));

        write_external(&path, "메모 v1\n");
        let (inc, _) = sync_on_open(&store).await.unwrap();
        assert!(!inc.needs_full_reindex);
        let content: String = sqlx::query_scalar("SELECT content FROM quest_memos")
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert!(content.contains("메모 v1"));

        write_external(&path, "메모 v2\n");
        sync_on_open(&store).await.unwrap();
        let rows: Vec<String> = sqlx::query_scalar("SELECT content FROM quest_memos")
            .fetch_all(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "메모는 파일 하나당 행 하나여야: {rows:?}");
        assert!(rows[0].contains("메모 v2"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn external_campaign_comment_edit_syncs_incrementally() {
        let dir = fresh_tmp("sib-camp");
        let store = setup(&dir).await;
        let camp = crate::ops::campaigns::create_campaign(
            &store,
            crate::models::CreateCampaignRequest {
                title: "캠페인".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        let path = store
            .paths
            .dot_guild()
            .join(format!("campaigns/{}.comments.md", camp.campaign_slug));

        write_external(&path, "캠페인 댓글\n");
        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(
            !inc.needs_full_reindex && full.is_none(),
            "캠페인 sibling 도 증분이어야: {inc:?}"
        );
        let bodies: Vec<String> = sqlx::query_scalar("SELECT body FROM campaign_comments")
            .fetch_all(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(bodies.len(), 1);
        assert!(bodies[0].contains("캠페인 댓글"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn external_campaign_body_edit_syncs_without_full_reindex() {
        let dir = fresh_tmp("camp-body");
        let store = setup(&dir).await;
        let camp = crate::ops::campaigns::create_campaign(
            &store,
            crate::models::CreateCampaignRequest {
                title: "원래 제목".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        let path = store.paths.campaign_path(&camp.campaign_slug);

        // 외부 편집 — 제목만 바꿔 다시 쓴다.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let src = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, src.replace("원래 제목", "고친 제목")).unwrap();

        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(
            !inc.needs_full_reindex && full.is_none(),
            "캠페인 본문 편집으로 풀 reindex 를 타면 안 됨: {inc:?}"
        );
        assert_eq!(inc.campaigns_synced, 1);
        let title: String = sqlx::query_scalar("SELECT title FROM campaigns WHERE id = ?")
            .bind(camp.id)
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(title, "고친 제목");

        // 변화 없으면 다시 UPDATE 하지 않는다.
        let (inc2, _) = sync_on_open(&store).await.unwrap();
        assert_eq!(inc2.campaigns_synced, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn open_path_does_not_rewrite_campaign_file() {
        // git pull 처럼 mtime 만 바뀐 경우에도 열기 경로가 파일을 다시 쓰면
        // frontmatter updated_at 이 일괄 변조된다(BUG-103 의 캠페인판).
        let dir = fresh_tmp("camp-nowrite");
        let store = setup(&dir).await;
        let camp = crate::ops::campaigns::create_campaign(
            &store,
            crate::models::CreateCampaignRequest {
                title: "제목".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        let path = store.paths.campaign_path(&camp.campaign_slug);

        // 내용 그대로 재기록 = mtime 만 갱신 (git checkout 시뮬).
        std::thread::sleep(std::time::Duration::from_millis(20));
        let before = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, &before).unwrap();

        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(!inc.needs_full_reindex && full.is_none());
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, before, "열기 경로가 캠페인 파일을 다시 쓰면 안 됨");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-311: 손으로 만든(index 에 없는) 캠페인 파일이 **풀 reindex 없이** INSERT.
    #[tokio::test]
    async fn new_campaign_file_inserted_without_full_reindex() {
        let dir = fresh_tmp("camp-new");
        let store = setup(&dir).await;
        let quest_slug = mk_one_quest(&store).await;
        let path = store.paths.campaign_path("C-042");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!(
                "+++\ncampaign_id = \"C-042\"\ntitle = \"손으로 넣은 캠페인\"\nstatus = \"active\"\ncreated_at = \"2026-01-01T00:00:00Z\"\nupdated_at = \"2026-01-01T00:00:00Z\"\nlinked_quests = [\"{quest_slug}\"]\n+++\n\n- [ ] 할 일 1\n- [x] 할 일 2\n"
            ),
        )
        .unwrap();

        let mut report = IncrementalReport::default();
        sync_changed_campaign_files(&store, &mut report).await.unwrap();
        assert!(!report.needs_full_reindex, "신규 캠페인 파일도 증분 INSERT 여야: {report:?}");
        assert_eq!(report.campaigns_inserted, 1);

        let row: (String, i64) =
            sqlx::query_as("SELECT title, id FROM campaigns WHERE campaign_slug = 'C-042'")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(row.0, "손으로 넣은 캠페인");
        let checklist_n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM campaign_checklists WHERE campaign_id = ?")
                .bind(row.1)
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(checklist_n, 2, "체크리스트도 같이 들어가야");
        let linked_n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM campaign_quests WHERE campaign_id = ?")
                .bind(row.1)
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(linked_n, 1, "linked_quests 도 같이 연결돼야");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-311: 캠페인 파일이 사라지면 행 + 체크리스트/링크가 풀 reindex 없이 DELETE.
    #[tokio::test]
    async fn deleted_campaign_file_removes_row_without_full_reindex() {
        let dir = fresh_tmp("camp-del");
        let store = setup(&dir).await;
        let camp = crate::ops::campaigns::create_campaign(
            &store,
            crate::models::CreateCampaignRequest {
                title: "지워질 캠페인".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        let path = store.paths.campaign_path(&camp.campaign_slug);
        crate::ops::campaigns::add_checklist_line(&store, camp.id, "체크")
            .await
            .unwrap();
        // sibling 테스트와 동일 패턴 — 삭제 감지는 file_mtime_cache 에 이전 항목이
        // 있어야 하니, 먼저 한 번 sync 로 캐시를 "정착"시킨다(재기동 시나리오).
        sync_on_open(&store).await.unwrap();

        std::fs::remove_file(&path).unwrap();
        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(
            !inc.needs_full_reindex && full.is_none(),
            "캠페인 파일 삭제로 풀 reindex 를 타면 안 됨: {inc:?}"
        );
        assert_eq!(inc.campaigns_deleted, 1);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaigns WHERE id = ?")
            .bind(camp.id)
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
        let checklist_n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM campaign_checklists WHERE campaign_id = ?")
                .bind(camp.id)
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(checklist_n, 0, "FK cascade 로 체크리스트도 같이 지워져야");

        // 캐시 row 도 정리 — 안 지우면 매번 '사라진 파일' 로 다시 잡힘.
        let (inc2, _) = sync_on_open(&store).await.unwrap();
        assert_eq!(inc2.campaigns_deleted, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-311: 외부에서 캠페인 파일에 `deleted = true` 를 써넣으면 행이 DELETE.
    #[tokio::test]
    async fn externally_soft_deleted_campaign_removes_row() {
        let dir = fresh_tmp("camp-extdel");
        let store = setup(&dir).await;
        let camp = crate::ops::campaigns::create_campaign(
            &store,
            crate::models::CreateCampaignRequest {
                title: "제목".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        let path = store.paths.campaign_path(&camp.campaign_slug);

        std::thread::sleep(std::time::Duration::from_millis(20));
        let src = std::fs::read_to_string(&path).unwrap();
        // frontmatter 는 이미 기본값 `deleted = false` 를 직렬화해 담고 있다
        // (serde default 는 역직렬화만 관여, 직렬화는 항상 씀) — 새 줄을 더하면
        // TOML 중복 키로 파싱이 깨지니 기존 줄을 바꿔치기한다.
        let edited = src.replace("deleted = false", "deleted = true");
        assert_ne!(edited, src, "deleted = false 줄을 못 찾음: {src}");
        std::fs::write(&path, edited).unwrap();

        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(!inc.needs_full_reindex && full.is_none(), "{inc:?}");
        assert_eq!(inc.campaigns_deleted, 1);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM campaigns WHERE id = ?")
            .bind(camp.id)
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEV-311(perf): 트랜잭션 리팩터 후에도 링크 여러 개짜리 캠페인 재적재가
    /// 정확한지 — UPDATE + 체크리스트 교체 + campaign_quests 재연결이 전부
    /// 한 커밋 안에서 일관되게 반영되는지 확인(부분 반영/롤백 없음).
    #[tokio::test]
    async fn campaign_refresh_with_many_links_is_transactionally_consistent() {
        let dir = fresh_tmp("camp-perf");
        let store = setup(&dir).await;
        let mut slugs = Vec::new();
        for i in 0..10 {
            slugs.push(mk_one_quest(&store).await);
            let _ = i;
        }
        let camp = crate::ops::campaigns::create_campaign(
            &store,
            crate::models::CreateCampaignRequest {
                title: "링크 많은 캠페인".into(),
                description: None,
                started_at: None,
                ended_at: None,
            },
        )
        .await
        .unwrap();
        let path = store.paths.campaign_path(&camp.campaign_slug);

        std::thread::sleep(std::time::Duration::from_millis(20));
        let linked = slugs
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let src = std::fs::read_to_string(&path).unwrap();
        // frontmatter 는 이미 기본값 `linked_quests = []` 을 직렬화해 담고 있다 —
        // 새 줄을 더하면 TOML 중복 키로 파싱이 깨지니 기존 줄을 바꿔치기한다.
        let edited = src.replace("linked_quests = []", &format!("linked_quests = [{linked}]"));
        assert_ne!(edited, src, "linked_quests = [] 줄을 못 찾음: {src}");
        std::fs::write(&path, edited).unwrap();

        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(!inc.needs_full_reindex && full.is_none(), "{inc:?}");
        assert_eq!(inc.campaigns_synced, 1);
        let linked_n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM campaign_quests WHERE campaign_id = ?")
                .bind(camp.id)
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(linked_n, slugs.len() as i64, "링크 10개가 전부 반영돼야");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── DEV-311: 도서관(library) 문서 증분 ───

    /// 손으로 만든(index 에 없는) 도서관 문서 파일이 **풀 reindex 없이** INSERT.
    #[tokio::test]
    async fn new_library_file_inserted_without_full_reindex() {
        let dir = fresh_tmp("lib-new");
        let store = setup(&dir).await;
        let path = store.paths.book_path("BOOK-042");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "+++\nbook_id = \"BOOK-042\"\ntitle = \"손으로 넣은 문서\"\npath = \"\"\ncreated_at = \"2026-01-01T00:00:00Z\"\nupdated_at = \"2026-01-01T00:00:00Z\"\ntags = [\"architecture\"]\n+++\n\n본문 내용\n",
        )
        .unwrap();

        let mut report = IncrementalReport::default();
        sync_changed_library_files(&store, &mut report).await.unwrap();
        assert!(!report.needs_full_reindex, "신규 문서 파일도 증분 INSERT 여야: {report:?}");
        assert_eq!(report.library_inserted, 1);

        let row: (String, i64) =
            sqlx::query_as("SELECT title, id FROM library_docs WHERE number = 42")
                .fetch_one(&store.index_pool)
                .await
                .unwrap();
        assert_eq!(row.0, "손으로 넣은 문서");
        let tags: Vec<String> = sqlx::query_scalar("SELECT tag FROM library_tags WHERE book_id = ?")
            .bind(row.1)
            .fetch_all(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(tags, vec!["architecture".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 외부 편집(제목 변경)이 열기 경로에서 증분 반영.
    #[tokio::test]
    async fn external_library_edit_syncs_without_full_reindex() {
        let dir = fresh_tmp("lib-edit");
        let store = setup(&dir).await;
        let book = crate::ops::library::create_book(&store, "원래 제목", "본문", "").await.unwrap();
        let path = store.paths.book_path(&book.book_id());

        std::thread::sleep(std::time::Duration::from_millis(20));
        let src = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, src.replace("원래 제목", "고친 제목")).unwrap();

        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(
            !inc.needs_full_reindex && full.is_none(),
            "도서관 문서 편집으로 풀 reindex 를 타면 안 됨: {inc:?}"
        );
        assert_eq!(inc.library_synced, 1);
        let title: String = sqlx::query_scalar("SELECT title FROM library_docs WHERE id = ?")
            .bind(book.id)
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(title, "고친 제목");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 도서관 문서 파일이 사라지면 행 + 태그가 풀 reindex 없이 DELETE.
    #[tokio::test]
    async fn deleted_library_file_removes_row_without_full_reindex() {
        let dir = fresh_tmp("lib-del");
        let store = setup(&dir).await;
        let book = crate::ops::library::create_book(&store, "지워질 문서", "본문", "").await.unwrap();
        crate::ops::library::set_book_tags(&store, &book.book_id(), vec!["temp".into()])
            .await
            .unwrap();
        let path = store.paths.book_path(&book.book_id());
        // 삭제 감지는 file_mtime_cache 에 이전 항목이 있어야 하니, 먼저 한 번
        // sync 로 캐시를 "정착"시킨다(재기동 시나리오 — ops::library 는 생성
        // 시점에 이 캐시를 안 건드림, get_book 의 lazy refresh 만 씀).
        sync_on_open(&store).await.unwrap();

        std::fs::remove_file(&path).unwrap();
        let (inc, full) = sync_on_open(&store).await.unwrap();
        assert!(
            !inc.needs_full_reindex && full.is_none(),
            "도서관 문서 파일 삭제로 풀 reindex 를 타면 안 됨: {inc:?}"
        );
        assert_eq!(inc.library_deleted, 1);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM library_docs WHERE id = ?")
            .bind(book.id)
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
        let tag_n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM library_tags WHERE book_id = ?")
            .bind(book.id)
            .fetch_one(&store.index_pool)
            .await
            .unwrap();
        assert_eq!(tag_n, 0, "FK cascade 로 태그도 같이 지워져야");

        // 캐시 row 도 정리 — 안 지우면 매번 '사라진 파일' 로 다시 잡힘.
        let (inc2, _) = sync_on_open(&store).await.unwrap();
        assert_eq!(inc2.library_deleted, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn orphan_sibling_falls_back_to_full_reindex() {
        // 소유 quest 가 index 에 없는 sibling — 증분으로 표현 불가 → 안전망.
        let dir = fresh_tmp("sib-orphan");
        let store = setup(&dir).await;
        let path = store.paths.dot_guild().join("quests/DEV-999.comments.md");
        write_external(&path, "주인 없는 댓글\n");

        let mut report = IncrementalReport::default();
        sync_changed_sibling_files(&store, &mut report).await.unwrap();
        assert!(
            report.needs_full_reindex,
            "본문 없는 sibling 은 풀 reindex 로 넘겨야"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
