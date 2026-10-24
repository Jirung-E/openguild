//! DEV-446 (REQ-039): 목록을 거르는 짧은 문법 — `키:값`.
//!
//! ```text
//! status:open,in_progress campaign:C-003 -tag:wip urgency:<=2 due:overdue 로그인
//! ```
//!
//! - 같은 키의 쉼표 = 또는, 서로 다른 조건(띄어 씀) = 그리고, 앞에 `-` = 아닌 것.
//! - 키 없는 낱말은 목록 검색과 같다(제목 · 본문 · 번호). `"두 낱말"` 처럼 따옴표로 묶는다.
//! - 모르는 키는 오류다 — 오타(`stauts:`)를 조용히 검색어로 삼으면 "없음" 으로 보여 헷갈린다.
//!
//! 해석은 **여기 한 곳**에만 둔다. CLI · 서버 · 앱이 같은 [`crate::models::ListQuery`] 의 `q` 로
//! 넘기고, 목록 조회(`services::quests`)가 [`Query::to_sql`] 로 조건을 붙인다.
//!
//! DEV-451: 캠페인 · 도서관 목록도 같은 문법을 쓴다 — **키만 대상마다 다르다**([`Target`]).

use crate::error::{AppError, AppResult};

/// 퀘스트 목록에 쓸 수 있는 키. 순서는 오류 안내에 보이는 순서.
pub const KEYS: &[&str] = &[
    "status",
    "type",
    "tag",
    "campaign",
    "urgency",
    "due",
    "parent",
    "commented",
];

/// DEV-451: 캠페인 목록의 키.
pub const CAMPAIGN_KEYS: &[&str] = &["status", "start", "end", "quest"];

/// DEV-451: 도서관 목록의 키.
pub const LIBRARY_KEYS: &[&str] = &["tag", "folder", "updated"];

/// DEV-451: 쿼리를 거는 목록. 문법은 같고 쓸 수 있는 키와 SQL 만 다르다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Target {
    #[default]
    Quest,
    Campaign,
    Library,
}

impl Target {
    pub fn keys(self) -> &'static [&'static str] {
        match self {
            Target::Quest => KEYS,
            Target::Campaign => CAMPAIGN_KEYS,
            Target::Library => LIBRARY_KEYS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Status,
    Type,
    Tag,
    Campaign,
    Urgency,
    Due,
    Parent,
    /// DEV-448: 마지막 댓글의 날짜. 댓글은 `updated_at` 을 안 바꾼다(사이드카 — DEV-433).
    Commented,
    /// DEV-451: 캠페인의 시작 · 끝 날, 연결된 퀘스트.
    Start,
    End,
    Quest,
    /// DEV-451: 도서관 문서의 폴더 · 고친 날.
    Folder,
    Updated,
}

impl Key {
    /// 대상에 있는 키만 — 캠페인에 `urgency` 를 쓰면 모르는 키다.
    fn parse(target: Target, s: &str) -> Option<Key> {
        let s = s.to_ascii_lowercase();
        if !target.keys().contains(&s.as_str()) {
            return None;
        }
        Some(match s.as_str() {
            "status" => Key::Status,
            "type" => Key::Type,
            "tag" => Key::Tag,
            "campaign" => Key::Campaign,
            "urgency" => Key::Urgency,
            "due" => Key::Due,
            "parent" => Key::Parent,
            "commented" => Key::Commented,
            "start" => Key::Start,
            "end" => Key::End,
            "quest" => Key::Quest,
            "folder" => Key::Folder,
            "updated" => Key::Updated,
            _ => return None,
        })
    }
}

/// 조건 하나. `key` 가 없으면 검색 낱말이고 `values` 는 하나다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub negated: bool,
    pub key: Option<Key>,
    pub values: Vec<String>,
    /// 이 조건이 시작하는 글자 위치(0부터, 글자 단위) — 오류 안내용.
    pub pos: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub terms: Vec<Term>,
    pub target: Target,
}

/// 해석 오류 — 몇 번째 글자에서 무엇이 틀렸나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    /// 0부터 센 글자 위치.
    pub pos: usize,
    pub message: String,
}

impl QueryError {
    fn at(pos: usize, message: String) -> Self {
        Self { pos, message }
    }
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.pos + 1;
        f.write_str(&crate::tf!(
            "쿼리 {n}번째 글자: {}",
            "Query, character {n}: {}",
            self.message
        ))
    }
}

impl From<QueryError> for AppError {
    fn from(e: QueryError) -> Self {
        AppError::BadRequest(e.to_string())
    }
}

/// 글 → 조건 목록 (퀘스트 목록).
pub fn parse(input: &str) -> Result<Query, QueryError> {
    parse_for(Target::Quest, input)
}

/// DEV-451: 글 → 조건 목록, 대상의 키로.
pub fn parse_for(target: Target, input: &str) -> Result<Query, QueryError> {
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let mut terms = Vec::new();
    let mut i = 0;
    while i < n {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let negated = chars[i] == '-';
        if negated {
            i += 1;
            if i >= n || chars[i].is_whitespace() {
                return Err(QueryError::at(
                    start,
                    crate::tf!("'-' 뒤에 조건이 없습니다", "nothing after '-'"),
                ));
            }
        }
        let body = i;

        // "두 낱말" — 따옴표로 묶은 검색어.
        if chars[i] == '"' {
            let (text, end) = read_quoted(&chars, i)?;
            if end < n && !chars[end].is_whitespace() {
                return Err(QueryError::at(
                    end,
                    crate::tf!(
                        "닫는 따옴표 뒤에는 띄어 쓰세요",
                        "put a space after the closing quote"
                    ),
                ));
            }
            if text.trim().is_empty() {
                return Err(QueryError::at(
                    start,
                    crate::tf!("빈 따옴표입니다", "empty quotes"),
                ));
            }
            terms.push(Term {
                negated,
                key: None,
                values: vec![text],
                pos: start,
            });
            i = end;
            continue;
        }

        // 키 — 글자로 시작하는 영문 · 숫자 · `_` 다음에 `:`. `10:30` 같은 글은 키가 아니다.
        let mut j = i;
        while j < n && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
            j += 1;
        }
        let is_key = j > i && chars[i].is_ascii_alphabetic() && j < n && chars[j] == ':';
        if !is_key {
            let mut end = i;
            while end < n && !chars[end].is_whitespace() {
                end += 1;
            }
            let text: String = chars[i..end].iter().collect();
            terms.push(Term {
                negated,
                key: None,
                values: vec![text],
                pos: start,
            });
            i = end;
            continue;
        }

        let name: String = chars[i..j].iter().collect();
        let Some(key) = Key::parse(target, &name) else {
            return Err(QueryError::at(
                body,
                crate::tf!(
                    "모르는 키 '{name}' — 쓸 수 있는 키: {}. 쌍점이 든 글을 찾으려면 따옴표로 묶으세요",
                    "unknown key '{name}' — available keys: {}. Quote text that contains a colon",
                    target.keys().join(", ")
                ),
            ));
        };
        // 값들 — 쉼표로 나누고, 따옴표 안의 쉼표 · 공백은 값의 일부.
        let mut values = Vec::new();
        let mut k = j + 1;
        loop {
            let vstart = k;
            let value = if k < n && chars[k] == '"' {
                let (text, end) = read_quoted(&chars, k)?;
                k = end;
                text
            } else {
                let mut end = k;
                while end < n && !chars[end].is_whitespace() && chars[end] != ',' {
                    end += 1;
                }
                let text: String = chars[k..end].iter().collect();
                k = end;
                text
            };
            if value.trim().is_empty() {
                return Err(QueryError::at(
                    vstart,
                    crate::tf!("'{name}:' 에 값이 없습니다", "'{name}:' has no value"),
                ));
            }
            values.push(value.trim().to_string());
            if k < n && chars[k] == ',' {
                k += 1;
                continue;
            }
            if k < n && !chars[k].is_whitespace() {
                return Err(QueryError::at(
                    k,
                    crate::tf!(
                        "닫는 따옴표 뒤에는 띄어 쓰거나 쉼표를 쓰세요",
                        "put a space or a comma after the closing quote"
                    ),
                ));
            }
            break;
        }
        let term = Term {
            negated,
            key: Some(key),
            values,
            pos: start,
        };
        check_values(target, &term, body)?;
        terms.push(term);
        i = k;
    }
    Ok(Query { terms, target })
}

/// `"…"` 를 읽는다 — 돌려주는 끝은 닫는 따옴표 다음 자리.
fn read_quoted(chars: &[char], open: usize) -> Result<(String, usize), QueryError> {
    let mut k = open + 1;
    while k < chars.len() && chars[k] != '"' {
        k += 1;
    }
    if k >= chars.len() {
        return Err(QueryError::at(
            open,
            crate::tf!("따옴표가 닫히지 않았습니다", "unclosed quote"),
        ));
    }
    Ok((chars[open + 1..k].iter().collect(), k + 1))
}

/// 값의 모양을 **해석할 때** 본다 — 조회까지 가서야 틀린 것을 알면 어디가 틀렸는지 못 짚는다.
fn check_values(target: Target, term: &Term, pos: usize) -> Result<(), QueryError> {
    let Some(key) = term.key else { return Ok(()) };
    for v in &term.values {
        let ok = match key {
            Key::Urgency => urgency_cond(v).is_some(),
            Key::Due => due_cond(v).is_some(),
            Key::Commented => commented_cond(v).is_some(),
            Key::Parent => v.eq_ignore_ascii_case("none") || split_slug(v).is_some(),
            Key::Campaign => v.eq_ignore_ascii_case("none") || split_slug(v).is_some(),
            // 캠페인 상태는 둘뿐이다 — 퀘스트처럼 길드가 정하지 않는다.
            Key::Status if target == Target::Campaign => {
                ["active", "done"].iter().any(|s| v.eq_ignore_ascii_case(s))
            }
            Key::Start | Key::Updated => date_cond("x", v).is_some(),
            Key::End => v.eq_ignore_ascii_case("overdue") || date_cond("x", v).is_some(),
            Key::Quest => split_slug(v).is_some(),
            Key::Status | Key::Type | Key::Tag | Key::Folder => true,
        };
        if !ok {
            let hint = match key {
                Key::Status => crate::tf!(
                    "캠페인의 status 는 active · done",
                    "a campaign's status is active or done"
                ),
                Key::Start => crate::tf!(
                    "start 는 none · 날짜(2026-11-01) · <=날짜 · >=날짜",
                    "start is none, a date (2026-11-01), <=date or >=date"
                ),
                Key::End => crate::tf!(
                    "end 는 overdue · none · 날짜(2026-11-01) · <=날짜 · >=날짜",
                    "end is overdue, none, a date (2026-11-01), <=date or >=date"
                ),
                Key::Updated => crate::tf!(
                    "updated 는 날짜(2026-11-01) · <=날짜 · >=날짜 — 고친 날과 비교",
                    "updated is a date (2026-11-01), <=date or >=date — compared with the day it was edited"
                ),
                Key::Quest => crate::tf!(
                    "quest 는 퀘스트 번호(DEV-012)",
                    "quest is a quest id (DEV-012)"
                ),
                Key::Urgency => crate::tf!(
                    "urgency 는 1~4 — 2 · 1-3 · <=2 · >=3 처럼",
                    "urgency is 1–4 — like 2, 1-3, <=2, >=3"
                ),
                Key::Due => crate::tf!(
                    "due 는 overdue · none · 날짜(2026-11-01) · <=날짜 · >=날짜",
                    "due is overdue, none, a date (2026-11-01), <=date or >=date"
                ),
                Key::Commented => crate::tf!(
                    "commented 는 none · 날짜(2026-11-01) · >=날짜 · <날짜 — 마지막 댓글의 날짜와 비교",
                    "commented is none, a date (2026-11-01), >=date or <date — compared with the last comment's date"
                ),
                Key::Parent => crate::tf!(
                    "parent 는 퀘스트 번호(DEV-012) 또는 none",
                    "parent is a quest id (DEV-012) or none"
                ),
                _ => crate::tf!(
                    "campaign 은 캠페인 번호(C-003) 또는 none",
                    "campaign is a campaign id (C-003) or none"
                ),
            };
            return Err(QueryError::at(
                pos,
                crate::tf!("'{v}' — {hint}", "'{v}' — {hint}"),
            ));
        }
    }
    Ok(())
}

/// SQL 에 끼울 값.
#[derive(Debug, Clone, PartialEq)]
pub enum Bind {
    Text(String),
    Int(i64),
}

/// `DEV-12` / `c-003` → (`DEV`, 12). 번호 꼴이 아니면 `None`.
fn split_slug(s: &str) -> Option<(String, i64)> {
    let (prefix, num) = s.trim().rsplit_once('-')?;
    if prefix.is_empty()
        || !prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some((
        prefix.to_string(),
        num.parse::<i64>().ok().filter(|n| *n >= 0)?,
    ))
}

/// 비교 꼴 — `<=2` · `>=2026-11-01` 의 앞부분.
fn split_op(v: &str) -> (&'static str, &str) {
    for op in ["<=", ">=", "<", ">", "="] {
        if let Some(rest) = v.strip_prefix(op) {
            return (op, rest.trim());
        }
    }
    ("=", v)
}

fn urgency_num(s: &str) -> Option<i64> {
    s.trim().parse::<i64>().ok().filter(|n| (1..=4).contains(n))
}

/// urgency 값 하나 → 조건.
fn urgency_cond(v: &str) -> Option<(String, Vec<Bind>)> {
    if let Some((lo, hi)) = v.split_once('-')
        && !lo.is_empty()
    {
        let (lo, hi) = (urgency_num(lo)?, urgency_num(hi)?);
        return Some((
            "q.urgency BETWEEN ? AND ?".into(),
            vec![Bind::Int(lo.min(hi)), Bind::Int(lo.max(hi))],
        ));
    }
    let (op, rest) = split_op(v);
    Some((
        format!("q.urgency {op} ?"),
        vec![Bind::Int(urgency_num(rest)?)],
    ))
}

fn is_date(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// 유효 기한 — 필수 기한과, 연결된 진행 중 캠페인의 끝 날 중 이른 것(화면의 BUG-034 규칙과 같다).
const EFFECTIVE_DUE: &str = "(CASE
        WHEN NULLIF(substr(q.required_due, 1, 10), '') IS NULL THEN (SELECT MIN(substr(c.ended_at, 1, 10)) FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id WHERE cq.quest_id = q.id AND c.status = 'active' AND COALESCE(c.ended_at, '') != '')
        ELSE MIN(substr(q.required_due, 1, 10), COALESCE((SELECT MIN(substr(c.ended_at, 1, 10)) FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id WHERE cq.quest_id = q.id AND c.status = 'active' AND COALESCE(c.ended_at, '') != ''), '9999-12-31'))
    END)";

/// due 값 하나 → 조건. `overdue` 의 "오늘" 은 부르는 때 바인딩한다.
fn due_cond(v: &str) -> Option<(String, Vec<Bind>)> {
    match v.to_ascii_lowercase().as_str() {
        "overdue" => Some((
            format!("({EFFECTIVE_DUE} < ? AND qs.counts_as_done = 0)"),
            vec![Bind::Text(crate::time::today_local_iso_date())],
        )),
        "none" => Some((format!("{EFFECTIVE_DUE} IS NULL"), vec![])),
        _ => {
            let (op, rest) = split_op(v);
            is_date(rest).then(|| {
                (
                    format!("{EFFECTIVE_DUE} {op} ?"),
                    vec![Bind::Text(rest.to_string())],
                )
            })
        }
    }
}

/// DEV-448: 마지막 댓글의 날짜 — 댓글에 적힌 시각(`2026-10-23T11:16:24+09:00`)의 날짜 부분. 적은 기계의
/// 현지 날짜라 사람이 보는 날짜와 같다. 댓글이 없으면 NULL.
const LAST_COMMENT: &str = "(SELECT MAX(substr(c.ts, 1, 10)) FROM quest_comments c WHERE c.quest_id = q.id AND COALESCE(c.ts, '') != '')";

/// commented 값 하나 → 조건. `>=날짜` 는 "그날 이후 하나라도 달렸다" 와 같다(마지막 것이 그 뒤이므로).
fn commented_cond(v: &str) -> Option<(String, Vec<Bind>)> {
    if v.eq_ignore_ascii_case("none") {
        return Some((format!("{LAST_COMMENT} IS NULL"), vec![]));
    }
    let (op, rest) = split_op(v);
    is_date(rest).then(|| {
        (
            format!("{LAST_COMMENT} {op} ?"),
            vec![Bind::Text(rest.to_string())],
        )
    })
}

/// DEV-451: 날짜 칸 하나와 비교 — `none` · `2026-11-01` · `<=2026-11-01` 따위. `col` 은 날짜(앞 10글자)를 낸다.
fn date_cond(col: &str, v: &str) -> Option<(String, Vec<Bind>)> {
    if v.eq_ignore_ascii_case("none") {
        return Some((format!("{col} IS NULL"), vec![]));
    }
    let (op, rest) = split_op(v);
    is_date(rest).then(|| (format!("{col} {op} ?"), vec![Bind::Text(rest.to_string())]))
}

/// LIKE 의 와일드카드를 글자 그대로 — 폴더 이름에 `_` 가 흔하다.
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

const SLUG_LIKE: &str = "LOWER(qt.prefix || '-' || printf('%03d', q.number)) LIKE LOWER(?)";
const STATUS_NORM: &str = "REPLACE(REPLACE(LOWER(?), ' ', '_'), '-', '_')";

impl Query {
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// 조건들 → `AND (…)` 를 이어 붙인 SQL 과 그 값들(나온 순서 그대로).
    /// 퀘스트는 별칭 `q` · `qt` · `qs`([`crate::services::quests::QUEST_SELECT`] 의 것), 캠페인은
    /// `campaigns`, 도서관은 `library_docs` 테이블 이름을 그대로 쓴다.
    pub fn to_sql(&self) -> AppResult<(String, Vec<Bind>)> {
        let mut sql = String::new();
        let mut binds = Vec::new();
        for term in &self.terms {
            let mut alts = Vec::new();
            for v in &term.values {
                let cond = match self.target {
                    Target::Quest => value_cond(term.key, v),
                    Target::Campaign => campaign_cond(term.key, v),
                    Target::Library => library_cond(term.key, v),
                };
                let (cond, b) = cond
                    .ok_or_else(|| QueryError::at(term.pos, format!("'{v}'")))?;
                alts.push(cond);
                binds.extend(b);
            }
            let joined = if alts.len() == 1 {
                alts.remove(0)
            } else {
                format!("({})", alts.join(" OR "))
            };
            if term.negated {
                sql.push_str(&format!(" AND NOT {joined}"));
            } else {
                sql.push_str(&format!(" AND {joined}"));
            }
        }
        Ok((sql, binds))
    }
}

/// 값 하나 → 괄호로 감싼 조건과 값들.
fn value_cond(key: Option<Key>, v: &str) -> Option<(String, Vec<Bind>)> {
    let t = |s: &str| Bind::Text(s.to_string());
    let (cond, binds) = match key {
        None => {
            let pat = format!("%{v}%");
            (
                format!(
                    "LOWER(q.title) LIKE LOWER(?) OR LOWER(COALESCE(q.description, '')) LIKE LOWER(?) OR {SLUG_LIKE}"
                ),
                vec![t(&pat), t(&pat), t(&pat)],
            )
        }
        // 지금 `--status` 와 같은 비교(영어 이름 · slug, 대소문자 · 공백 · `-` 무시) + 한국어 이름.
        Some(Key::Status) => (
            format!(
                "REPLACE(REPLACE(LOWER(qs.name_en), ' ', '_'), '-', '_') = {STATUS_NORM}
                 OR REPLACE(REPLACE(LOWER(COALESCE(qs.slug, '')), ' ', '_'), '-', '_') = {STATUS_NORM}
                 OR qs.name_ko = ?"
            ),
            vec![t(v), t(v), t(v)],
        ),
        Some(Key::Type) => ("UPPER(qt.prefix) = UPPER(?)".into(), vec![t(v)]),
        Some(Key::Tag) => (
            "EXISTS (SELECT 1 FROM quest_tags tg WHERE tg.quest_id = q.id AND tg.tag = ?)".into(),
            vec![t(v)],
        ),
        Some(Key::Campaign) if v.eq_ignore_ascii_case("none") => (
            "NOT EXISTS (SELECT 1 FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id
              WHERE cq.quest_id = q.id AND c.deleted_at IS NULL)"
                .into(),
            vec![],
        ),
        Some(Key::Campaign) => {
            let (prefix, num) = split_slug(v)?;
            (
                "EXISTS (SELECT 1 FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id
                  WHERE cq.quest_id = q.id AND c.deleted_at IS NULL AND UPPER(c.campaign_slug) = UPPER(?))"
                    .into(),
                vec![Bind::Text(format!("{prefix}-{num:03}"))],
            )
        }
        Some(Key::Urgency) => urgency_cond(v)?,
        Some(Key::Due) => due_cond(v)?,
        Some(Key::Commented) => commented_cond(v)?,
        Some(Key::Parent) if v.eq_ignore_ascii_case("none") => ("q.parent_quest_id IS NULL".into(), vec![]),
        Some(Key::Parent) => {
            let (prefix, num) = split_slug(v)?;
            (
                "q.parent_quest_id = (SELECT p.id FROM quests p JOIN quest_types pt ON p.quest_type_id = pt.id
                  WHERE UPPER(pt.prefix) = UPPER(?) AND p.number = ? AND p.deleted_at IS NULL)"
                    .into(),
                vec![Bind::Text(prefix), Bind::Int(num)],
            )
        }
        // 다른 대상의 키 — 해석에서 이미 막는다.
        Some(Key::Start | Key::End | Key::Quest | Key::Folder | Key::Updated) => return None,
    };
    Some((format!("({cond})"), binds))
}

/// DEV-451: 캠페인 목록의 값 하나 → 조건(테이블 `campaigns`).
fn campaign_cond(key: Option<Key>, v: &str) -> Option<(String, Vec<Bind>)> {
    const START: &str = "NULLIF(substr(campaigns.started_at, 1, 10), '')";
    const END: &str = "NULLIF(substr(campaigns.ended_at, 1, 10), '')";
    let t = |s: &str| Bind::Text(s.to_string());
    let (cond, binds) = match key {
        None => {
            let pat = format!("%{v}%");
            (
                "LOWER(campaigns.title) LIKE LOWER(?) OR LOWER(COALESCE(campaigns.description, '')) LIKE LOWER(?)
                 OR LOWER(campaigns.campaign_slug) LIKE LOWER(?)"
                    .into(),
                vec![t(&pat), t(&pat), t(&pat)],
            )
        }
        Some(Key::Status) => ("LOWER(campaigns.status) = LOWER(?)".into(), vec![t(v)]),
        Some(Key::Start) => date_cond(START, v)?,
        // 퀘스트의 `due:overdue` 와 같은 뜻 — 날이 지났는데 아직 진행 중.
        Some(Key::End) if v.eq_ignore_ascii_case("overdue") => (
            format!("{END} < ? AND campaigns.status = 'active'"),
            vec![Bind::Text(crate::time::today_local_iso_date())],
        ),
        Some(Key::End) => date_cond(END, v)?,
        Some(Key::Quest) => {
            let (prefix, num) = split_slug(v)?;
            (
                "EXISTS (SELECT 1 FROM campaign_quests cq JOIN quests q ON q.id = cq.quest_id
                  JOIN quest_types qt ON qt.id = q.quest_type_id
                  WHERE cq.campaign_id = campaigns.id AND q.deleted_at IS NULL
                    AND UPPER(qt.prefix) = UPPER(?) AND q.number = ?)"
                    .into(),
                vec![Bind::Text(prefix), Bind::Int(num)],
            )
        }
        _ => return None,
    };
    Some((format!("({cond})"), binds))
}

/// DEV-451: 도서관 목록의 값 하나 → 조건(테이블 `library_docs`).
fn library_cond(key: Option<Key>, v: &str) -> Option<(String, Vec<Bind>)> {
    let t = |s: &str| Bind::Text(s.to_string());
    let (cond, binds) = match key {
        None => {
            let pat = format!("%{v}%");
            (
                "LOWER(library_docs.title) LIKE LOWER(?) OR LOWER(library_docs.body) LIKE LOWER(?)
                 OR LOWER('BOOK-' || printf('%03d', library_docs.number)) LIKE LOWER(?)"
                    .into(),
                vec![t(&pat), t(&pat), t(&pat)],
            )
        }
        Some(Key::Tag) => (
            "EXISTS (SELECT 1 FROM library_tags lt WHERE lt.book_id = library_docs.id AND lt.tag = ?)".into(),
            vec![t(v)],
        ),
        // `--folder` 와 같다 — 그 폴더와 하위 폴더. `none` 은 최상위만.
        Some(Key::Folder) if v.eq_ignore_ascii_case("none") => ("library_docs.path = ''".into(), vec![]),
        Some(Key::Folder) => {
            let f = v.trim_end_matches('/');
            (
                "library_docs.path = ? OR library_docs.path LIKE ? ESCAPE '\\'".into(),
                vec![t(f), Bind::Text(format!("{}/%", like_escape(f)))],
            )
        }
        Some(Key::Updated) => date_cond("substr(library_docs.updated_at, 1, 10)", v)?,
        _ => return None,
    };
    Some((format!("({cond})"), binds))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(q: &Query) -> Vec<(bool, Option<Key>, Vec<&str>)> {
        q.terms
            .iter()
            .map(|t| {
                (
                    t.negated,
                    t.key,
                    t.values.iter().map(String::as_str).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn keys_commas_minus_and_words() {
        let q = parse(r#"status:open,in_progress -tag:wip 로그인 urgency:<=2"#).unwrap();
        assert_eq!(
            kv(&q),
            vec![
                (false, Some(Key::Status), vec!["open", "in_progress"]),
                (true, Some(Key::Tag), vec!["wip"]),
                (false, None, vec!["로그인"]),
                (false, Some(Key::Urgency), vec!["<=2"]),
            ]
        );
        assert_eq!(q.terms[2].pos, 33);
    }

    #[test]
    fn quotes_keep_spaces_and_commas() {
        let q = parse(r#"tag:"두 낱말",x "a:b c" -"빼고""#).unwrap();
        assert_eq!(
            kv(&q),
            vec![
                (false, Some(Key::Tag), vec!["두 낱말", "x"]),
                (false, None, vec!["a:b c"]),
                (true, None, vec!["빼고"]),
            ]
        );
    }

    #[test]
    fn keys_are_case_insensitive_and_times_are_not_keys() {
        let q = parse("STATUS:open 10:30").unwrap();
        assert_eq!(
            kv(&q),
            vec![
                (false, Some(Key::Status), vec!["open"]),
                (false, None, vec!["10:30"])
            ]
        );
    }

    #[test]
    fn errors_point_at_the_place() {
        let at = |s: &str| parse(s).unwrap_err().pos;
        assert_eq!(at("open stauts:open"), 5); // 모르는 키
        assert_eq!(at("status:"), 7); // 빈 값
        assert_eq!(at("status:open,"), 12); // 쉼표 뒤가 빔
        assert_eq!(at(r#"x "abc"#), 2); // 안 닫힌 따옴표
        assert_eq!(at("a -"), 2); // '-' 뒤가 빔
        assert_eq!(at("urgency:5"), 0);
        assert_eq!(at("urgency:<=x"), 0);
        assert_eq!(at("due:soon"), 0);
        assert_eq!(at("due:2026-13-01"), 0);
        assert_eq!(at("commented:yesterday"), 0);
        assert_eq!(at("parent:abc"), 0);
        assert_eq!(at("-campaign:C3"), 1);
        assert!(
            parse("stauts:open")
                .unwrap_err()
                .message
                .contains("status, type, tag")
        );
    }

    #[test]
    fn values_turn_into_conditions() {
        assert_eq!(
            urgency_cond("3-1").unwrap().1,
            vec![Bind::Int(1), Bind::Int(3)]
        );
        assert_eq!(urgency_cond(">=3").unwrap().0, "q.urgency >= ?");
        assert_eq!(urgency_cond("2").unwrap().0, "q.urgency = ?");
        assert!(due_cond("<=2026-11-01").unwrap().0.ends_with("<= ?"));
        assert_eq!(split_slug("c-3"), Some(("c".into(), 3)));
        let (sql, binds) = parse("-status:done,cancelled campaign:c-3")
            .unwrap()
            .to_sql()
            .unwrap();
        assert!(sql.starts_with(" AND NOT (("));
        assert_eq!(binds.len(), 7); // status 3개씩 두 값 + 캠페인 하나
        assert_eq!(binds[6], Bind::Text("c-003".into()));
    }

    #[test]
    fn empty_input_is_no_condition() {
        assert!(parse("   ").unwrap().is_empty());
        assert_eq!(parse("").unwrap().to_sql().unwrap().0, "");
    }

    /// DEV-451: 키는 대상마다 — 다른 대상의 키는 모르는 키이고, 안내에 그 대상의 키만 나온다.
    #[test]
    fn each_target_has_its_own_keys() {
        let err = |t: Target, s: &str| parse_for(t, s).unwrap_err().message;
        assert!(err(Target::Campaign, "urgency:2").contains("status, start, end, quest"));
        assert!(err(Target::Library, "status:open").contains("tag, folder, updated"));
        assert!(err(Target::Quest, "folder:a").contains("status, type, tag"));
        assert!(parse_for(Target::Campaign, "status:active,DONE end:overdue quest:dev-3").is_ok());
        assert!(err(Target::Campaign, "status:open").contains("active"));
        assert!(err(Target::Campaign, "start:soon").contains("start"));
        assert!(parse_for(Target::Library, "tag:a folder:none updated:>=2026-01-01 낱말").is_ok());
        // 퀘스트의 `status` 는 길드가 정하므로 아무 값이나 받는다.
        assert!(parse("status:anything").is_ok());

        let (sql, binds) = parse_for(Target::Library, "folder:a_b")
            .unwrap()
            .to_sql()
            .unwrap();
        assert!(sql.contains("library_docs.path LIKE ? ESCAPE"), "{sql}");
        assert_eq!(binds, vec![Bind::Text("a_b".into()), Bind::Text("a\\_b/%".into())]);
        let (sql, binds) = parse_for(Target::Campaign, "-quest:DEV-12")
            .unwrap()
            .to_sql()
            .unwrap();
        assert!(sql.starts_with(" AND NOT (EXISTS"), "{sql}");
        assert_eq!(binds, vec![Bind::Text("DEV".into()), Bind::Int(12)]);
    }
}
