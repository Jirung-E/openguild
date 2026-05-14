//! Audit log — 모든 mutation HTTP 요청을 timestamped 로 파일에 기록.
//!
//! 위치: `<guild_path>/audit.log`
//! 형식: `<timestamp>\t<method>\t<path-and-query>\t<status>`
//!
//! Body 는 기록하지 않는다 — middleware 에서 body 를 소비하면 handler 가 못 읽기 때문.
//! 사고 추적엔 `(언제, 어떤 endpoint, 어떤 응답)` 으로 충분 — 자세한 데이터는 자동 백업으로 복구.

use axum::{
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::Response,
};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

#[derive(Clone)]
pub struct AuditState {
    /// 동시에 한 번에 한 줄만 쓰도록 lock.
    inner: Arc<Mutex<AuditInner>>,
}

struct AuditInner {
    log_path: std::path::PathBuf,
}

impl AuditState {
    pub fn new(guild_path: &str) -> Self {
        let log_path = std::path::Path::new(guild_path).join("audit.log");
        Self {
            inner: Arc::new(Mutex::new(AuditInner { log_path })),
        }
    }

    fn append(&self, line: &str) {
        if let Ok(g) = self.inner.lock() {
            // 실패해도 서버 동작에 영향 X — 경고만
            let res = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&g.log_path)
                .and_then(|mut f| writeln!(f, "{line}"));
            if let Err(e) = res {
                tracing::warn!("audit log write failed: {e}");
            }
        }
    }
}

/// axum middleware. mutation method (POST/PATCH/PUT/DELETE) 만 기록.
pub async fn audit_layer(
    State(state): State<AuditState>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path_q = req
        .uri()
        .path_and_query()
        .map(|p| p.to_string())
        .unwrap_or_else(|| req.uri().path().to_string());

    let response = next.run(req).await;

    let is_mutation = matches!(
        method,
        Method::POST | Method::PATCH | Method::PUT | Method::DELETE
    );
    if is_mutation {
        let ts = now_iso();
        let status = response.status().as_u16();
        let line = format!("{ts}\t{method}\t{path_q}\t{status}");
        state.append(&line);
    }
    response
}

/// ISO 8601 비슷한 timestamp (UTC) — chrono 의존 없이.
fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = epoch_to_ymdhms(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn epoch_to_ymdhms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (secs % 60) as u32;
    let mi = ((secs / 60) % 60) as u32;
    let h = ((secs / 3600) % 24) as u32;
    let mut days = (secs / 86400) as i64;
    let mut year: i64 = 1970;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days >= dy {
            days -= dy;
            year += 1;
        } else {
            break;
        }
    }
    let dim = days_in_months(year);
    let mut month: usize = 0;
    while month < 12 && days >= dim[month] as i64 {
        days -= dim[month] as i64;
        month += 1;
    }
    (year as u32, (month + 1) as u32, (days + 1) as u32, h, mi, s)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
fn days_in_months(y: i64) -> [u32; 12] {
    [
        31,
        if is_leap(y) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tmp(label: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("og-audit-{label}-{ns}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn now_iso_format_is_zulu_iso8601() {
        let s = now_iso();
        // "YYYY-MM-DDTHH:MM:SSZ" — 20 글자
        assert_eq!(s.len(), 20, "got {s:?}");
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert_eq!(&s[10..11], "T");
        assert_eq!(&s[13..14], ":");
        assert_eq!(&s[16..17], ":");
        assert_eq!(&s[19..20], "Z");
    }

    #[test]
    fn epoch_to_ymdhms_known_dates() {
        // 2024-02-29 12:34:56 UTC = 1709210096
        assert_eq!(epoch_to_ymdhms(1709210096), (2024, 2, 29, 12, 34, 56));
        // 1970-01-01 00:00:00 UTC = 0
        assert_eq!(epoch_to_ymdhms(0), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn append_creates_log_and_writes_line() {
        let dir = fresh_tmp("write");
        let guild_path = dir.to_str().unwrap();
        let state = AuditState::new(guild_path);

        state.append("hello");
        state.append("world");

        let log = std::fs::read_to_string(dir.join("audit.log")).unwrap();
        assert!(log.contains("hello"));
        assert!(log.contains("world"));
        // 두 줄
        assert_eq!(log.lines().count(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_is_concurrent_safe() {
        use std::sync::Arc;
        use std::thread;

        let dir = fresh_tmp("concurrent");
        let state = Arc::new(AuditState::new(dir.to_str().unwrap()));

        let mut handles = vec![];
        for i in 0..16 {
            let s = state.clone();
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    s.append(&format!("t{i}-{j}"));
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let log = std::fs::read_to_string(dir.join("audit.log")).unwrap();
        // 모든 라인이 손상 없이 기록되었는지 (160 lines)
        assert_eq!(log.lines().count(), 160);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
