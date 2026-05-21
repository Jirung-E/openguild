//! DEV-041: 타임스탬프 ISO 8601 + TZ offset 형식 (Git 식).
//!
//! 저장 형식: `2026-05-22T13:41:10+09:00` — 로컬 시각 + offset.
//! 어느 환경에서 봐도 절대 시각 + 작성자 TZ 둘 다 명확.
//!
//! ## 기존 데이터와의 호환
//!
//! 0001 ~ 0004 migration 시기에 SQLite `datetime('now')` 로 저장된 행들은
//! `2026-05-21 14:00:24` 형식 (UTC, TZ 마커 없음). frontend / CLI 의 display
//! 단계에서 "TZ 마커가 없으면 UTC 로 간주" 규칙으로 정상 해석.
//!
//! `migrations/0005` 가 이 legacy 행들을 `...T...Z` 로 정규화 (in-place data
//! upgrade) — 새 format 과 lexicographic 정렬 호환되도록.
//!
//! ## SQL 컬럼 default
//!
//! `DEFAULT (datetime('now'))` 는 유지 (safety net). 새 코드 경로는 항상
//! Rust 에서 명시적으로 bind 하여 새 format 으로 기록.

use chrono::{Local, SecondsFormat};

/// 현재 로컬 시각을 `YYYY-MM-DDTHH:MM:SS±HH:MM` 형식으로 반환.
///
/// 예: `2026-05-22T13:41:10+09:00`
///
/// SecondsFormat::Secs — 소수점 초 생략 (사람이 읽기 좋고, sqlite 정렬에도 OK).
pub fn now_local_iso8601() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// Legacy SQLite 형식 (`YYYY-MM-DD HH:MM:SS`, TZ 마커 없음) 을 ISO 8601 UTC
/// (`YYYY-MM-DDTHH:MM:SSZ`) 로 변환. 이미 변환된 형식이면 그대로 반환.
///
/// reindex 가 .md 파일 (legacy 형식 잔존) 을 다시 db 로 넣을 때 호출 — 한 번
/// migration 0005 가 정규화한 db 상태를 되돌리지 않도록.
pub fn normalize_legacy_ts(s: &str) -> String {
    // 19자 + 11번째가 공백 → legacy. 그 외엔 (T 가 있거나 빈 문자열) 그대로.
    if s.len() == 19 && s.as_bytes().get(10) == Some(&b' ') {
        let mut out = s.replace(' ', "T");
        out.push('Z');
        out
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_local_format_matches_pattern() {
        let s = now_local_iso8601();
        // YYYY-MM-DDTHH:MM:SS + (Z 또는 ±HH:MM)
        let re = regex_lite(&s);
        assert!(re, "unexpected format: {s}");
    }

    fn regex_lite(s: &str) -> bool {
        // chrono 의 use_z=false → 항상 ±HH:MM 형식 (UTC 라도 +00:00).
        // 길이 25 = "2026-05-22T13:41:10+09:00".
        if s.len() != 25 {
            return false;
        }
        let bytes = s.as_bytes();
        bytes[4] == b'-' && bytes[7] == b'-' && bytes[10] == b'T' && bytes[13] == b':'
            && bytes[16] == b':' && (bytes[19] == b'+' || bytes[19] == b'-') && bytes[22] == b':'
    }

    #[test]
    fn normalize_legacy_converts_space_separator() {
        assert_eq!(
            normalize_legacy_ts("2026-05-22 04:41:10"),
            "2026-05-22T04:41:10Z"
        );
    }

    #[test]
    fn normalize_legacy_passes_through_iso_z() {
        let s = "2026-05-22T04:41:10Z";
        assert_eq!(normalize_legacy_ts(s), s);
    }

    #[test]
    fn normalize_legacy_passes_through_iso_offset() {
        let s = "2026-05-22T13:41:10+09:00";
        assert_eq!(normalize_legacy_ts(s), s);
    }

    #[test]
    fn normalize_legacy_passes_through_empty() {
        assert_eq!(normalize_legacy_ts(""), "");
    }
}
