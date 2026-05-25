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

use chrono::{DateTime, Local, SecondsFormat};

/// 현재 로컬 시각을 `YYYY-MM-DDTHH:MM:SS±HH:MM` 형식으로 반환.
///
/// 예: `2026-05-22T13:41:10+09:00`
///
/// SecondsFormat::Secs — 소수점 초 생략 (사람이 읽기 좋고, sqlite 정렬에도 OK).
pub fn now_local_iso8601() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// 오늘 날짜 (로컬) `YYYY-MM-DD` 형식. DEV-011 의 캠페인 기간 계산용.
pub fn today_local_iso_date() -> String {
    Local::now().format("%Y-%m-%d").to_string()
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

/// DEV-028 후속 버그 수정: `--created-after 2026-05-22T00:00:00` 같은 TZ 없는
/// 입력을 사용자의 로컬 TZ 기준으로 해석. 그래야 KST 사용자가 "오늘 00시 이후"
/// 라고 자연스럽게 입력했을 때 의도대로 동작.
///
/// 변환:
/// - 이미 TZ 마커 (`Z` / `±HH:MM` / `±HHMM`) 있으면 → 그대로.
/// - date-only (`YYYY-MM-DD`, 길이 10) → `YYYY-MM-DDT00:00:00<local_offset>`.
/// - datetime w/o TZ (`YYYY-MM-DDTHH:MM:SS`, 길이 19, T 포함) → `<input><local_offset>`.
/// - 그 외 (legacy 공백 구분 포함) → 그대로 (SQLite 가 UTC 로 해석).
pub fn normalize_filter_ts(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return s.to_string();
    }

    // TZ 마커 검출 — 끝에 Z 또는 +/-HH(:?)MM.
    if has_tz_marker(s) {
        return s.to_string();
    }

    let local_offset = Local::now().offset().to_string(); // 예: "+09:00"

    // date-only.
    if s.len() == 10 && s.as_bytes().get(4) == Some(&b'-') && s.as_bytes().get(7) == Some(&b'-') {
        return format!("{s}T00:00:00{local_offset}");
    }
    // datetime w/o TZ.
    if s.len() == 19 && s.contains('T') {
        return format!("{s}{local_offset}");
    }
    s.to_string()
}

fn has_tz_marker(s: &str) -> bool {
    if s.ends_with('Z') {
        return true;
    }
    // `+HH:MM` / `-HH:MM` / `+HHMM` / `-HHMM` — 마지막 5~6자 검사.
    let bytes = s.as_bytes();
    let n = bytes.len();
    if n < 5 {
        return false;
    }
    // 끝에서 5자: `±HHMM` 또는 `±HH:M` (M 의 짝). 6자: `±HH:MM`.
    let last5_sign_pos = n - 5;
    let last6_sign_pos = n.checked_sub(6);
    let is_sign = |b: u8| b == b'+' || b == b'-';
    if is_sign(bytes[last5_sign_pos])
        && bytes[last5_sign_pos + 1..].iter().all(|b| b.is_ascii_digit())
    {
        return true;
    }
    if let Some(p) = last6_sign_pos
        && is_sign(bytes[p])
        && bytes[p + 3] == b':'
        && bytes[p + 1..p + 3].iter().all(|b| b.is_ascii_digit())
        && bytes[p + 4..].iter().all(|b| b.is_ascii_digit())
    {
        return true;
    }
    false
}

/// Relative time label for a stored timestamp.
///
/// DEV-038 follow-up. Possible outputs: `"방금"`, `"X분 전"`, `"X시간 전"`, `"X일 전"`.
/// Returns `None` when the difference exceeds 7 days or the ts is in the future
/// (caller should fall back to an absolute representation).
///
/// Mirrors `formatRelative` in `gui/frontend/src/lib/utils/datetime.ts` so that
/// CLI and GUI history views stay in sync.
pub fn format_relative(ts: &str) -> Option<String> {
    let parsed: DateTime<chrono::FixedOffset> =
        DateTime::parse_from_rfc3339(&normalize_legacy_ts(ts)).ok()?;
    let now = Local::now();
    let diff = now.signed_duration_since(parsed);
    let sec = diff.num_seconds();
    if sec < 0 {
        return None; // 미래 — 호출자에서 절대값 폴백.
    }
    if sec < 60 {
        return Some("방금".into());
    }
    let min = sec / 60;
    if min < 60 {
        return Some(format!("{min}분 전"));
    }
    let hr = min / 60;
    if hr < 24 {
        return Some(format!("{hr}시간 전"));
    }
    let day = hr / 24;
    if day < 7 {
        return Some(format!("{day}일 전"));
    }
    None
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

    // --- normalize_filter_ts ---

    #[test]
    fn filter_ts_passes_through_z() {
        assert_eq!(normalize_filter_ts("2026-05-22T00:00:00Z"), "2026-05-22T00:00:00Z");
    }

    #[test]
    fn filter_ts_passes_through_offset() {
        let s = "2026-05-22T00:00:00+09:00";
        assert_eq!(normalize_filter_ts(s), s);
    }

    #[test]
    fn filter_ts_passes_through_compact_offset() {
        let s = "2026-05-22T00:00:00+0900";
        assert_eq!(normalize_filter_ts(s), s);
    }

    #[test]
    fn filter_ts_date_only_appends_midnight_local() {
        let out = normalize_filter_ts("2026-05-22");
        // 어느 환경이든 "2026-05-22T00:00:00<offset>" 형식.
        assert!(out.starts_with("2026-05-22T00:00:00"));
        assert!(out.len() >= 20);
    }

    #[test]
    fn filter_ts_naked_datetime_appends_local_offset() {
        let out = normalize_filter_ts("2026-05-22T00:00:00");
        assert!(out.starts_with("2026-05-22T00:00:00"));
        // append 후 25자 (with `+HH:MM`).
        assert_eq!(out.len(), 25);
    }

    #[test]
    fn filter_ts_empty_returns_empty() {
        assert_eq!(normalize_filter_ts(""), "");
        assert_eq!(normalize_filter_ts("   "), "");
    }

    #[test]
    fn has_tz_marker_detects_forms() {
        assert!(has_tz_marker("2026-05-22T00:00:00Z"));
        assert!(has_tz_marker("2026-05-22T00:00:00+09:00"));
        assert!(has_tz_marker("2026-05-22T00:00:00-05:30"));
        assert!(has_tz_marker("2026-05-22T00:00:00+0900"));
        assert!(!has_tz_marker("2026-05-22T00:00:00"));
        assert!(!has_tz_marker("2026-05-22"));
    }

    // --- format_relative ---

    #[test]
    fn format_relative_recent_is_some() {
        // 방금 인 ts.
        let now = Local::now().to_rfc3339_opts(SecondsFormat::Secs, false);
        let out = format_relative(&now).expect("recent should produce some label");
        assert!(out == "방금" || out.ends_with("분 전"),
            "recent label unexpected: {out:?}");
    }

    #[test]
    fn format_relative_old_returns_none() {
        // 1년 전 — 7일 초과.
        let out = format_relative("2024-01-01T00:00:00+09:00");
        assert!(out.is_none(), "오래된 ts → None 으로 absolute fallback 유도");
    }

    #[test]
    fn format_relative_future_returns_none() {
        // 미래 시각 — None (호출자가 absolute 폴백).
        let out = format_relative("2099-01-01T00:00:00+09:00");
        assert!(out.is_none());
    }

    #[test]
    fn format_relative_legacy_space_format() {
        // legacy "YYYY-MM-DD HH:MM:SS" 도 normalize_legacy_ts 통과해서 처리됨.
        let out = format_relative("2024-01-01 00:00:00");
        assert!(out.is_none(), "1년 이상 전 → None");
    }
}
