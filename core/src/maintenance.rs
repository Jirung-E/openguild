//! DEV-162: 오프라인 정비 — CLI / server / HTTP admin 공용 로직.
//!
//! index.db `VACUUM` + journal.db(AOF) tail. 순수 데이터 작업이라 호출 측(CLI /
//! server / 향후 HTTP admin)이 출력/표현을 담당한다. (counter 검증은 `ops`,
//! snapshot/reindex 는 각 모듈에 있음.)

use anyhow::Result;

use crate::repo::GuildPaths;
use crate::store::Store;

#[derive(Debug, Clone, serde::Serialize)]
pub struct VacuumReport {
    pub before_bytes: u64,
    pub after_bytes: u64,
    /// 회수된 바이트 (HTTP/GUI 직렬화 편의 — saved() 와 동일).
    pub saved_bytes: u64,
}

impl VacuumReport {
    /// 회수된 바이트 (음수 없음).
    pub fn saved(&self) -> u64 {
        self.before_bytes.saturating_sub(self.after_bytes)
    }
}

/// index.db `VACUUM` + WAL checkpoint(TRUNCATE) — soft-delete 누적 후 dead row
/// 공간 회수 + 파일 크기 정리. VACUUM 은 트랜잭션 밖에서 실행.
pub async fn vacuum(store: &Store) -> Result<VacuumReport> {
    let index_db = store.paths.index_db();
    let before = std::fs::metadata(&index_db).map(|m| m.len()).unwrap_or(0);
    sqlx::query("VACUUM").execute(&store.index_pool).await?;
    // VACUUM 은 WAL 을 남길 수 있어 checkpoint 으로 사이즈 안정화.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&store.index_pool)
        .await
        .ok();
    let after = std::fs::metadata(&index_db).map(|m| m.len()).unwrap_or(0);
    Ok(VacuumReport {
        before_bytes: before,
        after_bytes: after,
        saved_bytes: before.saturating_sub(after),
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JournalOp {
    pub id: i64,
    pub ts: String,
    pub op: String,
    pub args: String,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct JournalTail {
    /// journal.db 의 전체 op 수.
    pub total: i64,
    /// 최근 N op — 오래된 → 최신 순(자연 read).
    pub rows: Vec<JournalOp>,
}

/// journal.db(AOF) 의 최근 `count` op 조회 (read-only). journal.db 가 없으면
/// `None` (아직 mutation 없거나 snapshot 직후).
pub async fn journal_tail(paths: &GuildPaths, count: i64) -> Result<Option<JournalTail>> {
    let jdb = paths.journal_db();
    if !jdb.exists() {
        return Ok(None);
    }
    let url = format!(
        "sqlite:{}?mode=ro",
        jdb.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('\\', "/")
    );
    let pool = crate::db::create_pool(&url).await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ops")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let mut rows: Vec<(i64, String, String, String, Option<String>)> =
        sqlx::query_as("SELECT id, ts, op, args, result FROM ops ORDER BY id DESC LIMIT ?")
            .bind(count)
            .fetch_all(&pool)
            .await?;
    // ID ascending 으로 되돌려 오래된→최신 순으로.
    rows.reverse();
    pool.close().await;
    Ok(Some(JournalTail {
        total,
        rows: rows
            .into_iter()
            .map(|(id, ts, op, args, result)| JournalOp {
                id,
                ts,
                op,
                args,
                result,
            })
            .collect(),
    }))
}
