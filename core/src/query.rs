//! DEV-446 (REQ-039): 목록을 거르는 짧은 문법 — `키:값`.
//!
//! ```text
//! status:[open, in_progress] (tag:urgent | due:overdue) -tag:wip urgency:1..=2 title:로그인*
//! ```
//!
//! - 띄어 쓴 조건 = 그리고(`&` · `AND` 도 된다), `|` · `OR` = 또는, 앞에 `-` = 아닌 것, `( )` 로 묶는다.
//!   그리고가 또는보다 먼저 묶인다 — `a b | c` 는 `(a b) | c`. 키워드는 대문자만(`or` 는 낱말이다).
//! - 여러 값 중 하나는 `[a, b]` 로 묶는다(REQ-040 — 쉼표만으로는 읽기 어렵다).
//! - 범위는 `..`(끝 제외) · `..=`(끝 포함) — `urgency:1..=2` · `due:2026-11-01..2026-12-01` · `updated:2026-10-01..`.
//!   비교(`<=2`)도 된다.
//! - 글 값은 키 있는 것이 완전일치, `*` 를 쓰면 그 자리에 아무 글이나(`tag:ui-*`), `/…/` 는 정규식(대소문자 무시).
//!   `title:` · `body:` 는 부분일치가 기본이고 `=` 를 붙이면 완전일치(`title:="로그인 화면"`).
//! - 키 없는 낱말은 목록 검색과 같다(제목 · 본문 · 번호, 부분일치). `"두 낱말"` 처럼 따옴표로 묶는다.
//! - 모르는 키는 오류다 — 오타(`stauts:`)를 조용히 검색어로 삼으면 "없음" 으로 보여 헷갈린다.
//!
//! 해석은 **여기 한 곳**에만 둔다. CLI · 서버 · 앱이 같은 [`crate::models::ListQuery`] 의 `q` 로
//! 넘기고, 목록 조회(`services::quests`)가 [`Query::to_sql`] 로 조건을 붙인다. 정규식은 SQLite 의 `REGEXP`
//! 로 거른다(`db` 가 연결마다 Rust `regex` 로 등록한다 — 입력 길이에 비례해 끝나서 서버에 넣어도 안전하다).
//!
//! DEV-451: 캠페인 · 도서관 목록도 같은 문법을 쓴다 — **키만 대상마다 다르다**([`Target`]).

use crate::error::{AppError, AppResult};

/// 퀘스트 목록에 쓸 수 있는 키. 순서는 오류 안내에 보이는 순서.
pub const KEYS: &[&str] = &[
    "id",
    "status",
    "type",
    "tag",
    "campaign",
    "urgency",
    "due",
    "required_due",
    "desired_due",
    "campaign_end",
    "parent",
    "child",
    "prereq",
    "successor",
    "discussion",
    "created",
    "updated",
    "commented",
    "title",
    "body",
];

/// DEV-451: 캠페인 목록의 키.
pub const CAMPAIGN_KEYS: &[&str] = &[
    "id",
    "status",
    "start",
    "end",
    "quest",
    "checklist",
    "created",
    "updated",
    "title",
    "body",
];

/// DEV-451: 도서관 목록의 키.
pub const LIBRARY_KEYS: &[&str] = &["id", "tag", "folder", "created", "updated", "title", "body"];
/// 댓글 — 제목이 없고, `quest` · `campaign` 은 그 댓글이 달린 문서다.
pub const COMMENT_KEYS: &[&str] = &[
    "id",
    "quest",
    "campaign",
    "author",
    "parent",
    "discussion",
    "created",
    "updated",
    "body",
];

/// DEV-451: 쿼리를 거는 목록. 문법은 같고 쓸 수 있는 키와 SQL 만 다르다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Target {
    #[default]
    Quest,
    Campaign,
    Library,
    Comment,
}

impl Target {
    pub fn keys(self) -> &'static [&'static str] {
        match self {
            Target::Quest => KEYS,
            Target::Campaign => CAMPAIGN_KEYS,
            Target::Library => LIBRARY_KEYS,
            Target::Comment => COMMENT_KEYS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// 문서 번호 — 세 대상 모두.
    Id,
    Status,
    Type,
    Tag,
    Campaign,
    Urgency,
    Due,
    /// 필수 기한만 · 희망 기한만 · 연결된 진행 중 캠페인의 끝 날만 — `due` 는 앞의 것과 마지막 것 중 이른 날.
    RequiredDue,
    DesiredDue,
    CampaignEnd,
    Parent,
    Child,
    Prereq,
    Successor,
    Discussion,
    Checklist,
    Created,
    /// DEV-448: 마지막 댓글의 날짜. 댓글은 `updated_at` 을 안 바꾼다(사이드카 — DEV-433).
    Commented,
    /// DEV-451: 캠페인의 시작 · 끝 날, 연결된 퀘스트.
    Start,
    End,
    Quest,
    /// 댓글을 쓴 사람.
    Author,
    /// DEV-451: 도서관 문서의 폴더 · 고친 날.
    Folder,
    Updated,
    /// REQ-040: 제목만 · 본문만 — 키 없는 낱말은 둘 다(와 번호)를 본다.
    Title,
    Body,
}

impl Key {
    /// 대상에 있는 키만 — 캠페인에 `urgency` 를 쓰면 모르는 키다.
    fn parse(target: Target, s: &str) -> Option<Key> {
        let s = s.to_ascii_lowercase();
        if !target.keys().contains(&s.as_str()) {
            return None;
        }
        Some(match s.as_str() {
            "id" => Key::Id,
            "status" => Key::Status,
            "type" => Key::Type,
            "tag" => Key::Tag,
            "campaign" => Key::Campaign,
            "urgency" => Key::Urgency,
            "due" => Key::Due,
            "required_due" => Key::RequiredDue,
            "desired_due" => Key::DesiredDue,
            "campaign_end" => Key::CampaignEnd,
            "parent" => Key::Parent,
            "child" => Key::Child,
            "prereq" => Key::Prereq,
            "successor" => Key::Successor,
            "discussion" => Key::Discussion,
            "checklist" => Key::Checklist,
            "created" => Key::Created,
            "commented" => Key::Commented,
            "start" => Key::Start,
            "end" => Key::End,
            "quest" => Key::Quest,
            "author" => Key::Author,
            "folder" => Key::Folder,
            "updated" => Key::Updated,
            "title" => Key::Title,
            "body" => Key::Body,
            _ => return None,
        })
    }

    /// 글을 맞추는 키 — `*` 와 `/정규식/` 을 받는다.
    fn is_text(self) -> bool {
        matches!(
            self,
            Key::Id | Key::Tag | Key::Type | Key::Folder | Key::Author | Key::Title | Key::Body
        )
    }
}

/// 값 하나의 모양.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Val {
    /// 따옴표 없는 글 — `*` · 비교(`<=`) · 범위(`..`) 가 뜻을 갖는다.
    Bare(String),
    /// `"…"` — 글자 그대로.
    Quoted(String),
    /// `=…` — 완전일치(`title:` · `body:` 의 기본은 부분일치라).
    Exact(String),
    /// `/…/` — 정규식, 대소문자 무시.
    Regex(String),
}

impl Val {
    /// 오류 안내 · 비교에 쓰는 글.
    pub fn text(&self) -> &str {
        match self {
            Val::Bare(s) | Val::Quoted(s) | Val::Exact(s) | Val::Regex(s) => s,
        }
    }
}

/// 조건 하나. `key` 가 없으면 검색 낱말이고 `values` 는 하나다. 값이 여럿이면 그중 하나(`[a, b]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub key: Option<Key>,
    pub values: Vec<Val>,
    /// 이 조건이 시작하는 글자 위치(0부터, 글자 단위) — 오류 안내용.
    pub pos: usize,
}

/// 조건을 묶은 식.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Term(Term),
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// 빈 쿼리면 `None`.
    pub expr: Option<Expr>,
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

/// 글 → 조건 (퀘스트 목록).
pub fn parse(input: &str) -> Result<Query, QueryError> {
    parse_for(Target::Quest, input)
}

/// REQ-040: 이 쿼리의 결과에 칠할 말 — 화면이 묻는다(해석은 여기 한 곳). `of` 는 `quest` · `campaign` ·
/// `library` · `comment`.
pub fn highlights_for(of: &str, input: &str) -> Result<Vec<HitTerm>, QueryError> {
    let target = match of {
        "campaign" => Target::Campaign,
        "library" | "book" => Target::Library,
        "comment" => Target::Comment,
        _ => Target::Quest,
    };
    Ok(parse_for(target, input)?.highlights())
}

/// DEV-451: 글 → 조건, 대상의 키로.
pub fn parse_for(target: Target, input: &str) -> Result<Query, QueryError> {
    let chars: Vec<char> = input.chars().collect();
    let tokens = lex(target, &chars)?;
    let mut p = Parser {
        tokens: &tokens,
        i: 0,
        end: chars.len(),
    };
    if tokens.is_empty() {
        return Ok(Query { expr: None, target });
    }
    let expr = p.or()?;
    if let Some(t) = p.peek() {
        // 남은 것이 있으면 `)` 가 열린 적 없이 닫힌 것이다(다른 것은 `and` 가 다 먹는다).
        return Err(QueryError::at(
            t.pos,
            crate::tf!(
                "여는 괄호 없이 ')' 가 있습니다",
                "')' without a matching '('"
            ),
        ));
    }
    Ok(Query {
        expr: Some(expr),
        target,
    })
}

// ── 낱말 나누기 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Open,
    Close,
    Or,
    And,
    Not,
    Term(Term),
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    pos: usize,
}

/// 낱말을 끝내는 글자 — 띄어쓰기, 그리고 `|` · `&`. `)` 는 괄호가 열려 있을 때만(`f(x)` 같은 낱말을 깨지 않게).
fn ends_word(c: char, depth: usize) -> bool {
    c.is_whitespace() || c == '|' || c == '&' || (c == ')' && depth > 0)
}

fn lex(target: Target, chars: &[char]) -> Result<Vec<Token>, QueryError> {
    let n = chars.len();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let simple = match c {
            '(' => Some(Tok::Open),
            ')' if depth > 0 => Some(Tok::Close),
            '|' => Some(Tok::Or),
            '&' => Some(Tok::And),
            _ => None,
        };
        if let Some(tok) = simple {
            match tok {
                Tok::Open => depth += 1,
                Tok::Close => depth -= 1,
                _ => {}
            }
            out.push(Token { tok, pos: start });
            i += 1;
            continue;
        }
        // 대문자 `OR` · `AND` — 낱말 하나로 서 있을 때만.
        for (word, tok) in [("OR", Tok::Or), ("AND", Tok::And)] {
            let w: Vec<char> = word.chars().collect();
            if chars[i..].starts_with(&w)
                && (i + w.len() == n
                    || ends_word(chars[i + w.len()], depth)
                    || chars[i + w.len()] == '(')
            {
                out.push(Token {
                    tok: tok.clone(),
                    pos: start,
                });
                i += w.len();
                break;
            }
        }
        if out.last().is_some_and(|t| t.pos == start) {
            continue;
        }
        if c == '-' {
            if i + 1 >= n || chars[i + 1].is_whitespace() {
                return Err(QueryError::at(
                    start,
                    crate::tf!("'-' 뒤에 조건이 없습니다", "nothing after '-'"),
                ));
            }
            out.push(Token {
                tok: Tok::Not,
                pos: start,
            });
            i += 1;
            continue;
        }
        let (term, end) = read_term(target, chars, i, depth)?;
        out.push(Token {
            tok: Tok::Term(term),
            pos: start,
        });
        i = end;
    }
    Ok(out)
}

/// 조건 하나 — 키 있는 것(`tag:[a, b]`) 또는 낱말(`로그인` · `"두 낱말"` · `/정규식/`).
fn read_term(
    target: Target,
    chars: &[char],
    i: usize,
    depth: usize,
) -> Result<(Term, usize), QueryError> {
    let n = chars.len();
    // 키 — 글자로 시작하는 영문 · 숫자 · `_` 다음에 `:`. `10:30` 같은 글은 키가 아니다.
    let mut j = i;
    while j < n && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
        j += 1;
    }
    let is_key = j > i && chars[i].is_ascii_alphabetic() && j < n && chars[j] == ':';
    if !is_key {
        let (val, end) = read_value(chars, i, depth, false, false)?;
        if let Val::Quoted(s) = &val
            && s.trim().is_empty()
        {
            return Err(QueryError::at(
                i,
                crate::tf!("빈 따옴표입니다", "empty quotes"),
            ));
        }
        let term = Term {
            key: None,
            values: vec![val],
            pos: i,
        };
        check_values(target, &term, i)?;
        return Ok((term, end));
    }
    let name: String = chars[i..j].iter().collect();
    let Some(key) = Key::parse(target, &name) else {
        return Err(QueryError::at(
            i,
            crate::tf!(
                "모르는 키 '{name}' — 쓸 수 있는 키: {}. 쌍점이 든 글을 찾으려면 따옴표로 묶으세요",
                "unknown key '{name}' — available keys: {}. Quote text that contains a colon",
                target.keys().join(", ")
            ),
        ));
    };
    let mut k = j + 1;
    let mut values = Vec::new();
    // `=` 는 title · body 에서만 완전일치 — 다른 키에서는 비교(`urgency:=2`)다.
    let eq = matches!(key, Key::Title | Key::Body);
    if k < n && chars[k] == '[' {
        // `[a, b]` — 그중 하나.
        k += 1;
        loop {
            while k < n && chars[k].is_whitespace() {
                k += 1;
            }
            if k < n && chars[k] == ']' && values.is_empty() {
                return Err(QueryError::at(
                    k,
                    crate::tf!("'{name}:[ ]' 이 비어 있습니다", "'{name}:[ ]' is empty"),
                ));
            }
            let vstart = k;
            let (val, end) = read_value(chars, k, depth, true, eq)?;
            if val.text().trim().is_empty() {
                return Err(QueryError::at(
                    vstart,
                    crate::tf!(
                        "'{name}:' 의 목록에 빈 값이 있습니다",
                        "'{name}:' has an empty value in its list"
                    ),
                ));
            }
            values.push(val);
            k = end;
            while k < n && chars[k].is_whitespace() {
                k += 1;
            }
            match chars.get(k) {
                Some(',') => k += 1,
                Some(']') => {
                    k += 1;
                    break;
                }
                _ => {
                    return Err(QueryError::at(
                        k.min(n),
                        crate::tf!("']' 로 목록을 닫으세요", "close the list with ']'"),
                    ));
                }
            }
        }
    } else {
        let vstart = k;
        let (val, end) = read_value(chars, k, depth, false, eq)?;
        if val.text().trim().is_empty() {
            return Err(QueryError::at(
                vstart,
                crate::tf!("'{name}:' 에 값이 없습니다", "'{name}:' has no value"),
            ));
        }
        if let Val::Bare(s) = &val
            && s.contains(',')
        {
            return Err(QueryError::at(
                vstart,
                crate::tf!(
                    "여러 값은 [ ] 로 묶으세요 — {name}:[{}]",
                    "wrap several values in [ ] — {name}:[{}]",
                    s.split(',').map(str::trim).collect::<Vec<_>>().join(", ")
                ),
            ));
        }
        values.push(val);
        k = end;
    }
    let term = Term {
        key: Some(key),
        values,
        pos: i,
    };
    check_values(target, &term, i)?;
    Ok((term, k))
}

/// 값 하나 — `"…"`, `/…/`, (`eq` 면) `=…`, 또는 띄어쓰기 · 연산자까지의 글. `in_list` 면 `,` · `]` 에서도 끝난다.
fn read_value(
    chars: &[char],
    i: usize,
    depth: usize,
    in_list: bool,
    eq: bool,
) -> Result<(Val, usize), QueryError> {
    let n = chars.len();
    let stop = |c: char| ends_word(c, depth) || (in_list && (c == ',' || c == ']'));
    let after = |end: usize| -> Result<(), QueryError> {
        if end < n && !stop(chars[end]) && !(chars[end] == ')') {
            return Err(QueryError::at(
                end,
                crate::tf!(
                    "닫는 따옴표 · 빗금 뒤에는 띄어 쓰세요",
                    "put a space after the closing quote or slash"
                ),
            ));
        }
        Ok(())
    };
    if eq && i < n && chars[i] == '=' {
        let (inner, end) = read_value(chars, i + 1, depth, in_list, false)?;
        return Ok((Val::Exact(inner.text().to_string()), end));
    }
    if i < n && chars[i] == '"' {
        let mut k = i + 1;
        while k < n && chars[k] != '"' {
            k += 1;
        }
        if k >= n {
            return Err(QueryError::at(
                i,
                crate::tf!("따옴표가 닫히지 않았습니다", "unclosed quote"),
            ));
        }
        after(k + 1)?;
        return Ok((Val::Quoted(chars[i + 1..k].iter().collect()), k + 1));
    }
    if i < n && chars[i] == '/' {
        // `\/` 는 빗금 글자 — 정규식 안에 그대로 넘긴다(`regex` 도 `\/` 를 `/` 로 읽는다).
        let mut k = i + 1;
        while k < n && chars[k] != '/' {
            if chars[k] == '\\' {
                k += 1;
            }
            k += 1;
        }
        if k >= n {
            return Err(QueryError::at(
                i,
                crate::tf!(
                    "정규식이 '/' 로 닫히지 않았습니다",
                    "the regex is not closed with '/'"
                ),
            ));
        }
        after(k + 1)?;
        let pat: String = chars[i + 1..k].iter().collect();
        if pat.is_empty() {
            return Err(QueryError::at(
                i,
                crate::tf!("빈 정규식입니다", "empty regex"),
            ));
        }
        return Ok((Val::Regex(pat), k + 1));
    }
    let mut end = i;
    while end < n && !stop(chars[end]) {
        end += 1;
    }
    Ok((Val::Bare(chars[i..end].iter().collect()), end))
}

// ── 묶기 ─────────────────────────────────────────────────────────────────────

struct Parser<'a> {
    tokens: &'a [Token],
    i: usize,
    /// 입력 끝 — "뒤에 조건이 없다" 를 가리킬 자리.
    end: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.i)
    }

    fn or(&mut self) -> Result<Expr, QueryError> {
        let mut parts = vec![self.and()?];
        while let Some(t) = self.peek()
            && t.tok == Tok::Or
        {
            self.i += 1;
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Expr::Or(parts)
        })
    }

    fn and(&mut self) -> Result<Expr, QueryError> {
        let mut parts = vec![self.unary()?];
        loop {
            match self.peek().map(|t| &t.tok) {
                Some(Tok::And) => {
                    self.i += 1;
                    parts.push(self.unary()?);
                }
                Some(Tok::Not | Tok::Open | Tok::Term(_)) => parts.push(self.unary()?),
                _ => break,
            }
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Expr::And(parts)
        })
    }

    fn unary(&mut self) -> Result<Expr, QueryError> {
        let Some(t) = self.peek().cloned() else {
            return Err(QueryError::at(
                self.end,
                crate::tf!("연산자 뒤에 조건이 없습니다", "nothing after the operator"),
            ));
        };
        self.i += 1;
        match t.tok {
            Tok::Not => Ok(Expr::Not(Box::new(self.unary()?))),
            Tok::Term(term) => Ok(Expr::Term(term)),
            Tok::Open => {
                let inner = self.or()?;
                match self.peek() {
                    Some(c) if c.tok == Tok::Close => {
                        self.i += 1;
                        Ok(inner)
                    }
                    _ => Err(QueryError::at(
                        t.pos,
                        crate::tf!("괄호가 닫히지 않았습니다", "unclosed '('"),
                    )),
                }
            }
            Tok::Close => Err(QueryError::at(
                t.pos,
                crate::tf!(
                    "괄호 안에 조건이 없습니다",
                    "nothing inside the parentheses"
                ),
            )),
            Tok::Or | Tok::And => Err(QueryError::at(
                t.pos,
                crate::tf!(
                    "앞에 조건이 없는 연산자입니다",
                    "an operator with nothing before it"
                ),
            )),
        }
    }
}

// ── 값 검사 ──────────────────────────────────────────────────────────────────

/// 정규식을 대소문자 무시로 — SQL(`REGEXP`) · 강화 검색이 같은 것을 쓴다.
pub fn regex_ci(pat: &str) -> String {
    format!("(?i){pat}")
}

/// 값의 모양을 **해석할 때** 본다 — 조회까지 가서야 틀린 것을 알면 어디가 틀렸는지 못 짚는다.
fn check_values(target: Target, term: &Term, pos: usize) -> Result<(), QueryError> {
    for v in &term.values {
        if let Val::Regex(p) = v {
            if term.key.is_some_and(|k| !k.is_text()) {
                return Err(QueryError::at(
                    pos,
                    crate::tf!(
                        "'/{p}/' — 정규식은 낱말과 id · tag · type · folder · author · title · body 에만 씁니다",
                        "'/{p}/' — regexes work for words and id, tag, type, folder, author, title, body"
                    ),
                ));
            }
            if let Err(e) = regex::RegexBuilder::new(&regex_ci(p))
                .size_limit(1 << 20)
                .build()
            {
                return Err(QueryError::at(
                    pos,
                    crate::tf!(
                        "'/{p}/' — 정규식이 틀렸습니다: {e}",
                        "'/{p}/' — invalid regex: {e}"
                    ),
                ));
            }
            continue;
        }
        if let Val::Exact(_) = v
            && !matches!(term.key, Some(Key::Title | Key::Body))
        {
            return Err(QueryError::at(
                pos,
                crate::tf!(
                    "'={}' — '=' 완전일치는 title · body 에만 씁니다(다른 키는 원래 완전일치)",
                    "'={}' — '=' (exact) is for title and body (other keys already match exactly)",
                    v.text()
                ),
            ));
        }
        let Some(key) = term.key else { continue };
        let s = v.text();
        let ok = match (key, v) {
            // 댓글의 번호는 그 문서 안의 번호고, `quest` · `campaign` 은 달린 문서다.
            (Key::Id, _) if target == Target::Comment => entry_cond("x", v).is_some(),
            (Key::Parent, _) if target == Target::Comment => {
                s.eq_ignore_ascii_case("none") || entry_cond("x", v).is_some()
            }
            (Key::Quest | Key::Campaign, _) if target == Target::Comment => {
                owner_cond("quest", v).is_some()
            }
            (Key::Author, _) => true,
            (Key::Urgency, _) => urgency_cond(s).is_some(),
            (Key::Id, _) => id_cond("x", "x", "x", v).is_some(),
            (Key::Due, _) => due_cond(s).is_some(),
            (Key::RequiredDue | Key::DesiredDue | Key::CampaignEnd, _) => {
                due_like_cond("x", s).is_some()
            }
            (Key::Commented, _) => commented_cond(s).is_some(),
            (Key::Parent | Key::Child | Key::Prereq | Key::Successor | Key::Campaign, _) => {
                s.eq_ignore_ascii_case("none") || split_slug(s).is_some()
            }
            // 캠페인 상태는 둘뿐이다 — 퀘스트처럼 길드가 정하지 않는다.
            (Key::Status, _) if target == Target::Campaign => {
                ["active", "done"].iter().any(|x| s.eq_ignore_ascii_case(x))
            }
            (Key::Discussion, _) => ["unresolved", "resolved", "none"]
                .iter()
                .any(|x| s.eq_ignore_ascii_case(x)),
            (Key::Checklist, _) => ["open", "done", "none"]
                .iter()
                .any(|x| s.eq_ignore_ascii_case(x)),
            (Key::Start, _) => date_cond("x", s).is_some(),
            (Key::Created | Key::Updated, _) => moment_cond("x", s).is_some(),
            (Key::End, _) => s.eq_ignore_ascii_case("overdue") || date_cond("x", s).is_some(),
            (Key::Quest, _) => split_slug(s).is_some(),
            (Key::Status | Key::Type | Key::Tag | Key::Folder | Key::Title | Key::Body, _) => true,
        };
        if !ok {
            let hint = match key {
                Key::Id if target == Target::Comment => crate::tf!(
                    "댓글의 id 는 문서 안의 번호 — 3 · 1..=3 · >=3 · * · /정규식/",
                    "a comment's id is its number in the document — 3, 1..=3, >=3, * or /regex/"
                ),
                Key::Parent if target == Target::Comment => crate::tf!(
                    "댓글의 parent 는 답글을 받은 댓글의 번호(3) 또는 none(답글이 아닌 것)",
                    "a comment's parent is the number of the comment it replies to (3) or none (not a reply)"
                ),
                Key::Quest | Key::Campaign if target == Target::Comment => crate::tf!(
                    "댓글이 달린 문서 — 번호(DEV-012) · 범위(DEV-1..=DEV-9) · * · none(그 종류가 아닌 것)",
                    "the document it is on — an id (DEV-012), a range (DEV-1..=DEV-9), * or none (on the other kind)"
                ),
                Key::Discussion if target == Target::Comment => crate::tf!(
                    "댓글의 discussion 은 unresolved(미해결) · resolved(해결됨) · none(토론이 아닌 것)",
                    "a comment's discussion is unresolved, resolved or none (not a discussion)"
                ),
                Key::Id => crate::tf!(
                    "id 는 번호(DEV-012) · 범위(DEV-1..=DEV-9) · * · /정규식/",
                    "id is an id (DEV-012), a range (DEV-1..=DEV-9), * or /regex/"
                ),
                Key::RequiredDue | Key::DesiredDue | Key::CampaignEnd => crate::tf!(
                    "기한 키는 overdue · none · 날짜(2026-11-01) · <=날짜 · 범위(2026-11-01..2026-12-01)",
                    "a due key is overdue, none, a date (2026-11-01), <=date or a range (2026-11-01..2026-12-01)"
                ),
                Key::Child | Key::Prereq | Key::Successor => crate::tf!(
                    "child · prereq · successor 는 퀘스트 번호(DEV-012) 또는 none",
                    "child, prereq and successor are a quest id (DEV-012) or none"
                ),
                Key::Discussion => crate::tf!(
                    "discussion 은 unresolved(미해결이 있음) · resolved(모두 해결) · none(토론 없음)",
                    "discussion is unresolved (some open), resolved (all resolved) or none (no discussion)"
                ),
                Key::Checklist => crate::tf!(
                    "checklist 는 open(남은 것이 있음) · done(모두 체크) · none(항목 없음)",
                    "checklist is open (items left), done (all checked) or none (no items)"
                ),
                Key::Created => crate::tf!(
                    "created 는 날짜(2026-11-01) · <=날짜 · >=시각(2026-11-01T10:00) · 범위(2026-10-01..) — 만든 때와 비교",
                    "created is a date (2026-11-01), <=date, >=time (2026-11-01T10:00) or a range (2026-10-01..) — compared with when it was created"
                ),
                Key::Status => crate::tf!(
                    "캠페인의 status 는 active · done",
                    "a campaign's status is active or done"
                ),
                Key::Start => crate::tf!(
                    "start 는 none · 날짜(2026-11-01) · <=날짜 · 범위(2026-11-01..2026-12-01)",
                    "start is none, a date (2026-11-01), <=date or a range (2026-11-01..2026-12-01)"
                ),
                Key::End => crate::tf!(
                    "end 는 overdue · none · 날짜(2026-11-01) · <=날짜 · 범위(2026-11-01..)",
                    "end is overdue, none, a date (2026-11-01), <=date or a range (2026-11-01..)"
                ),
                Key::Updated => crate::tf!(
                    "updated 는 날짜(2026-11-01) · <=날짜 · >=시각(2026-11-01T10:00) · 범위(2026-10-01..) — 고친 때와 비교",
                    "updated is a date (2026-11-01), <=date, >=time (2026-11-01T10:00) or a range (2026-10-01..) — compared with when it was edited"
                ),
                Key::Quest => crate::tf!(
                    "quest 는 퀘스트 번호(DEV-012)",
                    "quest is a quest id (DEV-012)"
                ),
                Key::Urgency => crate::tf!(
                    "urgency 는 1~4 — 2 · 1..=3 · <=2 · >=3 처럼",
                    "urgency is 1–4 — like 2, 1..=3, <=2, >=3"
                ),
                Key::Due => crate::tf!(
                    "due 는 overdue · none · 날짜(2026-11-01) · <=날짜 · 범위(2026-11-01..2026-12-01)",
                    "due is overdue, none, a date (2026-11-01), <=date or a range (2026-11-01..2026-12-01)"
                ),
                Key::Commented => crate::tf!(
                    "commented 는 none · 날짜(2026-11-01) · >=날짜 · >=시각(2026-11-01T10:00) · 범위 — 마지막 댓글과 비교",
                    "commented is none, a date (2026-11-01), >=date, >=time (2026-11-01T10:00) or a range — compared with the last comment"
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
                crate::tf!("'{s}' — {hint}", "'{s}' — {hint}"),
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

/// REQ-040: 범위 — `a..b`(끝 제외) · `a..=b`(끝 포함) · `a..` · `..b` · `..=b`. 범위가 아니면 `None`,
/// 양 끝이 다 비면 `Some(None)`(틀린 범위).
#[allow(clippy::type_complexity)]
fn split_range(v: &str) -> Option<Option<(Option<&str>, Option<&str>, bool)>> {
    let (lo, rest) = v.split_once("..")?;
    let (hi, inclusive) = match rest.strip_prefix('=') {
        Some(h) => (h, true),
        None => (rest, false),
    };
    let lo = Some(lo.trim()).filter(|s| !s.is_empty());
    let hi = Some(hi.trim()).filter(|s| !s.is_empty());
    if lo.is_none() && hi.is_none() {
        return Some(None);
    }
    Some(Some((lo, hi, inclusive)))
}

/// 범위 → `col >= ? AND col < ?` 따위. 끝값은 `ok` 로 검사한다.
fn range_cond(
    col: &str,
    v: &str,
    ok: impl Fn(&str) -> Option<Bind>,
) -> Option<Option<(String, Vec<Bind>)>> {
    let r = split_range(v)?;
    let Some((lo, hi, inclusive)) = r else {
        return Some(None);
    };
    let mut conds = Vec::new();
    let mut binds = Vec::new();
    if let Some(lo) = lo {
        let Some(b) = ok(lo) else { return Some(None) };
        conds.push(format!("{col} >= ?"));
        binds.push(b);
    }
    if let Some(hi) = hi {
        let Some(b) = ok(hi) else { return Some(None) };
        conds.push(format!("{col} {} ?", if inclusive { "<=" } else { "<" }));
        binds.push(b);
    }
    Some(Some((conds.join(" AND "), binds)))
}

fn urgency_num(s: &str) -> Option<i64> {
    s.trim().parse::<i64>().ok().filter(|n| (1..=4).contains(n))
}

/// urgency 값 하나 → 조건.
fn urgency_cond(v: &str) -> Option<(String, Vec<Bind>)> {
    if let Some(r) = range_cond("q.urgency", v, |s| urgency_num(s).map(Bind::Int)) {
        return r;
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

fn date_bind(s: &str) -> Option<Bind> {
    is_date(s).then(|| Bind::Text(s.to_string()))
}

/// 연결된 진행 중 캠페인의 끝 날 중 가장 이른 것.
macro_rules! campaign_end_sql {
    () => {
        "(SELECT MIN(substr(c.ended_at, 1, 10)) FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id WHERE cq.quest_id = q.id AND c.status = 'active' AND COALESCE(c.ended_at, '') != '')"
    };
}
const CAMPAIGN_END: &str = campaign_end_sql!();
const REQUIRED_DUE: &str = "NULLIF(substr(q.required_due, 1, 10), '')";
const DESIRED_DUE: &str = "NULLIF(substr(q.desired_due, 1, 10), '')";

/// 유효 기한 — 필수 기한과, 연결된 진행 중 캠페인의 끝 날 중 이른 것(화면의 BUG-034 규칙과 같다).
const EFFECTIVE_DUE: &str = concat!(
    "(CASE WHEN NULLIF(substr(q.required_due, 1, 10), '') IS NULL THEN ",
    campaign_end_sql!(),
    " ELSE MIN(substr(q.required_due, 1, 10), COALESCE(",
    campaign_end_sql!(),
    ", '9999-12-31')) END)"
);

/// 기한 칸 하나와 비교. `overdue` 의 "오늘" 은 부르는 때 바인딩한다.
fn due_like_cond(col: &str, v: &str) -> Option<(String, Vec<Bind>)> {
    match v.to_ascii_lowercase().as_str() {
        "overdue" => Some((
            format!("({col} < ? AND qs.counts_as_done = 0)"),
            vec![Bind::Text(crate::time::today_local_iso_date())],
        )),
        _ => date_cond(col, v),
    }
}

/// due 값 하나 → 조건.
fn due_cond(v: &str) -> Option<(String, Vec<Bind>)> {
    due_like_cond(EFFECTIVE_DUE, v)
}

/// 번호 값 하나 → 조건. `DEV-1` 은 `DEV-001` 과 같고, `DEV-1..=DEV-9` 는 같은 타입 안의 범위다. `*` · 정규식은
/// 적힌 그대로의 번호(`DEV-001`)에 맞춘다.
fn id_cond(slug: &str, prefix: &str, number: &str, v: &Val) -> Option<(String, Vec<Bind>)> {
    let s = v.text();
    if matches!(v, Val::Regex(_)) || (s.contains('*') && matches!(v, Val::Bare(_))) {
        return Some(text_cond(&[slug], v, Match::Exact));
    }
    if matches!(v, Val::Bare(_))
        && let Some(r) = split_range(s)
    {
        let (lo, hi, inclusive) = r?;
        let lo = match lo {
            Some(x) => Some(split_slug(x)?),
            None => None,
        };
        let hi = match hi {
            Some(x) => Some(split_slug(x)?),
            None => None,
        };
        let p = match (&lo, &hi) {
            (Some((a, _)), Some((b, _))) if !a.eq_ignore_ascii_case(b) => return None,
            (Some((a, _)), _) | (_, Some((a, _))) => a.clone(),
            _ => return None,
        };
        let mut cond = format!("UPPER({prefix}) = UPPER(?)");
        let mut binds = vec![Bind::Text(p)];
        if let Some((_, n)) = lo {
            cond += &format!(" AND {number} >= ?");
            binds.push(Bind::Int(n));
        }
        if let Some((_, n)) = hi {
            cond += &format!(" AND {number} {} ?", if inclusive { "<=" } else { "<" });
            binds.push(Bind::Int(n));
        }
        return Some((cond, binds));
    }
    let (p, n) = split_slug(s)?;
    Some((
        format!("UPPER({prefix}) = UPPER(?) AND {number} = ?"),
        vec![Bind::Text(p), Bind::Int(n)],
    ))
}

/// 이어진 퀘스트(`o`) — `none` 이면 없는 것, 번호면 그 퀘스트가 이어진 것. `from` 은 `q` 와 `o` 를 잇는
/// `FROM … WHERE …`.
fn related_cond(from: &str, v: &str) -> Option<(String, Vec<Bind>)> {
    if v.eq_ignore_ascii_case("none") {
        return Some((
            format!("NOT EXISTS (SELECT 1 {from} AND o.deleted_at IS NULL)"),
            vec![],
        ));
    }
    let (prefix, num) = split_slug(v)?;
    Some((
        format!(
            "EXISTS (SELECT 1 {from} AND o.deleted_at IS NULL AND o.number = ?
              AND o.quest_type_id = (SELECT ot.id FROM quest_types ot WHERE UPPER(ot.prefix) = UPPER(?)))"
        ),
        vec![Bind::Int(num), Bind::Text(prefix)],
    ))
}

/// DEV-448: 마지막 댓글의 날짜 — 댓글에 적힌 시각(`2026-10-23T11:16:24+09:00`)의 날짜 부분. 적은 기계의
/// 적힌 시간대가 섞여 있어 UTC 로 맞춰 고른다. 댓글이 없으면 NULL.
const LAST_COMMENT: &str = "(SELECT MAX(datetime(c.ts)) FROM quest_comments c WHERE c.quest_id = q.id AND COALESCE(c.ts, '') != '')";

/// commented 값 하나 → 조건. `>=날짜` 는 "그날 이후 하나라도 달렸다" 와 같다(마지막 것이 그 뒤이므로).
fn commented_cond(v: &str) -> Option<(String, Vec<Bind>)> {
    moment_cond(LAST_COMMENT, v)
}

fn is_moment(s: &str) -> bool {
    use chrono::{DateTime, NaiveDateTime};
    let naive = s.strip_suffix('Z').unwrap_or(s);
    ["%Y-%m-%dT%H:%M", "%Y-%m-%dT%H:%M:%S"]
        .iter()
        .any(|f| NaiveDateTime::parse_from_str(naive, f).is_ok())
        || ["%Y-%m-%dT%H:%M%:z", "%Y-%m-%dT%H:%M:%S%:z"]
            .iter()
            .any(|f| DateTime::parse_from_str(s, f).is_ok())
}

/// 시각까지 적힌 칸(`created` · `updated` · `commented`)과 비교. 날짜면 이 기계 시간대의 날짜로, 시각이면 그
/// 순간으로 잰다 — 저장된 값은 `Z` 와 `+09:00` 이 섞여 글자로는 못 잰다. 시각은 딱 맞을 일이 없어 비교 · 범위로만.
fn moment_cond(col: &str, v: &str) -> Option<(String, Vec<Bind>)> {
    if v.eq_ignore_ascii_case("none") {
        return Some((format!("{col} IS NULL"), vec![]));
    }
    let one = |op: &str, s: &str| -> Option<(String, Bind)> {
        let s = s.trim();
        if is_date(s) {
            Some((
                format!("date({col}, 'localtime') {op} ?"),
                Bind::Text(s.to_string()),
            ))
        } else if is_moment(s) {
            Some((
                format!("datetime({col}) {op} datetime(?)"),
                Bind::Text(crate::time::normalize_filter_ts(s)),
            ))
        } else {
            None
        }
    };
    if let Some(r) = split_range(v) {
        let (lo, hi, inclusive) = r?;
        let mut conds = Vec::new();
        let mut binds = Vec::new();
        if let Some(lo) = lo {
            let (c, b) = one(">=", lo)?;
            conds.push(c);
            binds.push(b);
        }
        if let Some(hi) = hi {
            let (c, b) = one(if inclusive { "<=" } else { "<" }, hi)?;
            conds.push(c);
            binds.push(b);
        }
        return Some((conds.join(" AND "), binds));
    }
    let (op, rest) = split_op(v);
    if op == "=" && !is_date(rest.trim()) {
        return None;
    }
    let (c, b) = one(op, rest)?;
    Some((c, vec![b]))
}

/// DEV-451: 날짜 칸 하나와 비교 — `none` · `2026-11-01` · `<=2026-11-01` · `2026-11-01..2026-12-01` 따위.
/// `col` 은 날짜(앞 10글자)를 낸다.
fn date_cond(col: &str, v: &str) -> Option<(String, Vec<Bind>)> {
    if v.eq_ignore_ascii_case("none") {
        return Some((format!("{col} IS NULL"), vec![]));
    }
    if let Some(r) = range_cond(col, v, date_bind) {
        return r;
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

/// 글 칸 맞추기의 기본.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Match {
    /// 그 글이 들어 있으면(키 없는 낱말 · `title:` · `body:`).
    Partial,
    /// 칸 전체가 그 글이면(대소문자 무시).
    Exact,
}

/// REQ-040: 글 칸(`cols` 중 하나라도) 맞추기 — 값의 모양이 기본을 바꾼다. `*` 는 아무 글(그 칸 전체에 대해),
/// `/…/` 는 정규식, `=…` 는 완전일치, 따옴표는 글자 그대로(`*` 도 글자).
fn text_cond(cols: &[&str], v: &Val, default: Match) -> (String, Vec<Bind>) {
    let mut conds = Vec::new();
    let mut binds = Vec::new();
    for col in cols {
        let (c, b) = match v {
            Val::Regex(p) => (format!("{col} REGEXP ?"), Bind::Text(regex_ci(p))),
            Val::Bare(s) if s.contains('*') => (
                format!("LOWER({col}) LIKE LOWER(?) ESCAPE '\\'"),
                Bind::Text(like_escape(s).replace('*', "%")),
            ),
            Val::Exact(s) => (format!("LOWER({col}) = LOWER(?)"), Bind::Text(s.clone())),
            Val::Bare(s) | Val::Quoted(s) => match default {
                Match::Exact => (format!("LOWER({col}) = LOWER(?)"), Bind::Text(s.clone())),
                Match::Partial => (
                    format!("LOWER({col}) LIKE LOWER(?) ESCAPE '\\'"),
                    Bind::Text(format!("%{}%", like_escape(s))),
                ),
            },
        };
        conds.push(c);
        binds.push(b);
    }
    (conds.join(" OR "), binds)
}

const SLUG_EXPR: &str = "(qt.prefix || '-' || printf('%03d', q.number))";
const STATUS_NORM: &str = "REPLACE(REPLACE(LOWER(?), ' ', '_'), '-', '_')";

impl Query {
    pub fn is_empty(&self) -> bool {
        self.expr.is_none()
    }

    /// 식 → ` AND (…)` SQL 과 그 값들(나온 순서 그대로). 빈 쿼리면 빈 글.
    /// 퀘스트는 별칭 `q` · `qt` · `qs`([`crate::services::quests::QUEST_SELECT`] 의 것), 캠페인은
    /// `campaigns`, 도서관은 `library_docs` 테이블 이름을 그대로 쓴다. 댓글은
    /// [`crate::services::comments::search_global`] 의 바깥 별칭 `comments` 다.
    pub fn to_sql(&self) -> AppResult<(String, Vec<Bind>)> {
        let Some(expr) = &self.expr else {
            return Ok((String::new(), Vec::new()));
        };
        let mut binds = Vec::new();
        let sql = self.expr_sql(expr, &mut binds)?;
        Ok((format!(" AND {sql}"), binds))
    }

    fn expr_sql(&self, e: &Expr, binds: &mut Vec<Bind>) -> AppResult<String> {
        Ok(match e {
            Expr::Term(t) => {
                let mut alts = Vec::new();
                for v in &t.values {
                    let cond = match self.target {
                        Target::Quest => value_cond(t.key, v),
                        Target::Campaign => campaign_cond(t.key, v),
                        Target::Library => library_cond(t.key, v),
                        Target::Comment => comment_cond(t.key, v),
                    };
                    let (cond, b) =
                        cond.ok_or_else(|| QueryError::at(t.pos, format!("'{}'", v.text())))?;
                    alts.push(cond);
                    binds.extend(b);
                }
                if alts.len() == 1 {
                    alts.remove(0)
                } else {
                    format!("({})", alts.join(" OR "))
                }
            }
            Expr::Not(inner) => format!("NOT {}", self.expr_sql(inner, binds)?),
            Expr::And(parts) | Expr::Or(parts) => {
                let op = if matches!(e, Expr::And(_)) {
                    " AND "
                } else {
                    " OR "
                };
                let mut out = Vec::new();
                for p in parts {
                    out.push(self.expr_sql(p, binds)?);
                }
                format!("({})", out.join(op))
            }
        })
    }

    /// REQ-040: 결과에 칠할 말 — 키 없는 낱말 · `title:` · `body:` 의 값(아닌 것 `-` 아래는 빼고). `*` 는 칠할
    /// 조각으로 나눈다(`로그인*` → `로그인`).
    pub fn highlights(&self) -> Vec<HitTerm> {
        fn walk(e: &Expr, out: &mut Vec<HitTerm>) {
            match e {
                Expr::Term(t) if matches!(t.key, None | Some(Key::Title | Key::Body)) => {
                    for v in &t.values {
                        let h = match v {
                            Val::Regex(p) => vec![HitTerm::Regex(p.clone())],
                            Val::Bare(s) => s
                                .split('*')
                                .filter(|x| !x.trim().is_empty())
                                .map(|x| HitTerm::Text(x.to_string()))
                                .collect(),
                            Val::Quoted(s) | Val::Exact(s) => vec![HitTerm::Text(s.clone())],
                        };
                        for x in h {
                            if !out.contains(&x) {
                                out.push(x);
                            }
                        }
                    }
                }
                Expr::Term(_) | Expr::Not(_) => {}
                Expr::And(parts) | Expr::Or(parts) => parts.iter().for_each(|p| walk(p, out)),
            }
        }
        let mut out = Vec::new();
        if let Some(e) = &self.expr {
            walk(e, &mut out);
        }
        out
    }
}

/// REQ-040: 칠할 말 하나 — 글(대소문자 무시 부분일치) 또는 정규식.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HitTerm {
    Text(String),
    Regex(String),
}

/// 값 하나 → 괄호로 감싼 조건과 값들.
fn value_cond(key: Option<Key>, v: &Val) -> Option<(String, Vec<Bind>)> {
    let s = v.text();
    let t = |s: &str| Bind::Text(s.to_string());
    let (cond, binds) = match key {
        None => text_cond(
            &["q.title", "COALESCE(q.description, '')", SLUG_EXPR],
            v,
            Match::Partial,
        ),
        Some(Key::Id) => id_cond(SLUG_EXPR, "qt.prefix", "q.number", v)?,
        Some(Key::Title) => text_cond(&["q.title"], v, Match::Partial),
        Some(Key::Body) => text_cond(&["COALESCE(q.description, '')"], v, Match::Partial),
        // 지금 `--status` 와 같은 비교(영어 이름 · slug, 대소문자 · 공백 · `-` 무시) + 한국어 이름.
        Some(Key::Status) => (
            format!(
                "REPLACE(REPLACE(LOWER(qs.name_en), ' ', '_'), '-', '_') = {STATUS_NORM}
                 OR REPLACE(REPLACE(LOWER(COALESCE(qs.slug, '')), ' ', '_'), '-', '_') = {STATUS_NORM}
                 OR qs.name_ko = ?"
            ),
            vec![t(s), t(s), t(s)],
        ),
        Some(Key::Type) => text_cond(&["qt.prefix"], v, Match::Exact),
        Some(Key::Tag) => {
            let (c, b) = text_cond(&["tg.tag"], v, Match::Exact);
            (
                format!("EXISTS (SELECT 1 FROM quest_tags tg WHERE tg.quest_id = q.id AND ({c}))"),
                b,
            )
        }
        Some(Key::Campaign) if s.eq_ignore_ascii_case("none") => (
            "NOT EXISTS (SELECT 1 FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id
              WHERE cq.quest_id = q.id AND c.deleted_at IS NULL)"
                .into(),
            vec![],
        ),
        Some(Key::Campaign) => {
            let (prefix, num) = split_slug(s)?;
            (
                "EXISTS (SELECT 1 FROM campaign_quests cq JOIN campaigns c ON c.id = cq.campaign_id
                  WHERE cq.quest_id = q.id AND c.deleted_at IS NULL AND UPPER(c.campaign_slug) = UPPER(?))"
                    .into(),
                vec![Bind::Text(format!("{prefix}-{num:03}"))],
            )
        }
        Some(Key::Urgency) => urgency_cond(s)?,
        Some(Key::Due) => due_cond(s)?,
        Some(Key::RequiredDue) => due_like_cond(REQUIRED_DUE, s)?,
        Some(Key::DesiredDue) => due_like_cond(DESIRED_DUE, s)?,
        Some(Key::CampaignEnd) => due_like_cond(CAMPAIGN_END, s)?,
        Some(Key::Created) => moment_cond("q.created_at", s)?,
        Some(Key::Updated) => moment_cond("q.updated_at", s)?,
        Some(Key::Commented) => commented_cond(s)?,
        Some(Key::Parent) if s.eq_ignore_ascii_case("none") => ("q.parent_quest_id IS NULL".into(), vec![]),
        Some(Key::Parent) => {
            let (prefix, num) = split_slug(s)?;
            (
                "q.parent_quest_id = (SELECT p.id FROM quests p JOIN quest_types pt ON p.quest_type_id = pt.id
                  WHERE UPPER(pt.prefix) = UPPER(?) AND p.number = ? AND p.deleted_at IS NULL)"
                    .into(),
                vec![Bind::Text(prefix), Bind::Int(num)],
            )
        }
        Some(Key::Child) => related_cond("FROM quests o WHERE o.parent_quest_id = q.id", s)?,
        Some(Key::Prereq) => related_cond(
            "FROM quest_dependencies d JOIN quests o ON o.id = d.prerequisite_id WHERE d.quest_id = q.id",
            s,
        )?,
        Some(Key::Successor) => related_cond(
            "FROM quest_dependencies d JOIN quests o ON o.id = d.quest_id WHERE d.prerequisite_id = q.id",
            s,
        )?,
        // 목록의 토론 아이콘과 같다 — 미해결이 하나라도 있으면 unresolved, 모두 풀렸으면 resolved.
        Some(Key::Discussion) => {
            const ANY: &str = "SELECT 1 FROM quest_comments dc WHERE dc.quest_id = q.id AND dc.discussion = 1";
            let cond = match s.to_ascii_lowercase().as_str() {
                "unresolved" => format!("EXISTS ({ANY} AND dc.resolved = 0)"),
                "resolved" => format!("EXISTS ({ANY}) AND NOT EXISTS ({ANY} AND dc.resolved = 0)"),
                "none" => format!("NOT EXISTS ({ANY})"),
                _ => return None,
            };
            (cond, vec![])
        }
        // 다른 대상의 키 — 해석에서 이미 막는다.
        Some(Key::Start | Key::End | Key::Quest | Key::Author | Key::Folder | Key::Checklist) => {
            return None;
        }
    };
    Some((format!("({cond})"), binds))
}

/// DEV-451: 캠페인 목록의 값 하나 → 조건(테이블 `campaigns`).
fn campaign_cond(key: Option<Key>, v: &Val) -> Option<(String, Vec<Bind>)> {
    const START: &str = "NULLIF(substr(campaigns.started_at, 1, 10), '')";
    const END: &str = "NULLIF(substr(campaigns.ended_at, 1, 10), '')";
    const DESC: &str = "COALESCE(campaigns.description, '')";
    let s = v.text();
    let (cond, binds) = match key {
        None => text_cond(
            &["campaigns.title", DESC, "campaigns.campaign_slug"],
            v,
            Match::Partial,
        ),
        Some(Key::Id) => id_cond(
            "campaigns.campaign_slug",
            "substr(campaigns.campaign_slug, 1, instr(campaigns.campaign_slug, '-') - 1)",
            "CAST(substr(campaigns.campaign_slug, instr(campaigns.campaign_slug, '-') + 1) AS INTEGER)",
            v,
        )?,
        Some(Key::Title) => text_cond(&["campaigns.title"], v, Match::Partial),
        Some(Key::Body) => text_cond(&[DESC], v, Match::Partial),
        Some(Key::Status) => (
            "LOWER(campaigns.status) = LOWER(?)".into(),
            vec![Bind::Text(s.to_string())],
        ),
        Some(Key::Start) => date_cond(START, s)?,
        // 퀘스트의 `due:overdue` 와 같은 뜻 — 날이 지났는데 아직 진행 중.
        Some(Key::End) if s.eq_ignore_ascii_case("overdue") => (
            format!("{END} < ? AND campaigns.status = 'active'"),
            vec![Bind::Text(crate::time::today_local_iso_date())],
        ),
        Some(Key::End) => date_cond(END, s)?,
        Some(Key::Created) => moment_cond("campaigns.created_at", s)?,
        Some(Key::Updated) => moment_cond("campaigns.updated_at", s)?,
        Some(Key::Checklist) => {
            const ANY: &str =
                "SELECT 1 FROM campaign_checklists cl WHERE cl.campaign_id = campaigns.id";
            let cond = match s.to_ascii_lowercase().as_str() {
                "open" => format!("EXISTS ({ANY} AND cl.checked = 0)"),
                "done" => format!("EXISTS ({ANY}) AND NOT EXISTS ({ANY} AND cl.checked = 0)"),
                "none" => format!("NOT EXISTS ({ANY})"),
                _ => return None,
            };
            (cond, vec![])
        }
        Some(Key::Quest) => {
            let (prefix, num) = split_slug(s)?;
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
fn library_cond(key: Option<Key>, v: &Val) -> Option<(String, Vec<Bind>)> {
    const SLUG: &str = "('BOOK-' || printf('%03d', library_docs.number))";
    let s = v.text();
    let t = |s: &str| Bind::Text(s.to_string());
    let (cond, binds) = match key {
        None => text_cond(
            &["library_docs.title", "library_docs.body", SLUG],
            v,
            Match::Partial,
        ),
        Some(Key::Id) => id_cond(SLUG, "'BOOK'", "library_docs.number", v)?,
        Some(Key::Title) => text_cond(&["library_docs.title"], v, Match::Partial),
        Some(Key::Body) => text_cond(&["library_docs.body"], v, Match::Partial),
        Some(Key::Tag) => {
            let (c, b) = text_cond(&["lt.tag"], v, Match::Exact);
            (
                format!(
                    "EXISTS (SELECT 1 FROM library_tags lt WHERE lt.book_id = library_docs.id AND ({c}))"
                ),
                b,
            )
        }
        // `--folder` 와 같다 — 그 폴더와 하위 폴더. `none` 은 최상위만. `*` · 정규식은 폴더 경로 전체에.
        Some(Key::Folder)
            if matches!(v, Val::Regex(_)) || (s.contains('*') && matches!(v, Val::Bare(_))) =>
        {
            text_cond(&["library_docs.path"], v, Match::Exact)
        }
        Some(Key::Folder) if s.eq_ignore_ascii_case("none") => {
            ("library_docs.path = ''".into(), vec![])
        }
        Some(Key::Folder) => {
            let f = s.trim_end_matches('/');
            (
                "library_docs.path = ? OR library_docs.path LIKE ? ESCAPE '\\'".into(),
                vec![t(f), Bind::Text(format!("{}/%", like_escape(f)))],
            )
        }
        Some(Key::Created) => moment_cond("library_docs.created_at", s)?,
        Some(Key::Updated) => moment_cond("library_docs.updated_at", s)?,
        _ => return None,
    };
    Some((format!("({cond})"), binds))
}

/// 댓글 번호 값 하나 — 번호 · 범위 · 비교. `*` · 정규식은 번호를 글로 맞춘다.
fn entry_cond(col: &str, v: &Val) -> Option<(String, Vec<Bind>)> {
    let s = v.text();
    if matches!(v, Val::Regex(_)) || (s.contains('*') && matches!(v, Val::Bare(_))) {
        return Some(text_cond(
            &[&format!("CAST({col} AS TEXT)")],
            v,
            Match::Exact,
        ));
    }
    let num = |s: &str| {
        s.trim()
            .parse::<i64>()
            .ok()
            .filter(|n| *n >= 0)
            .map(Bind::Int)
    };
    if matches!(v, Val::Bare(_))
        && let Some(r) = range_cond(col, s, num)
    {
        return r;
    }
    let (op, rest) = split_op(s);
    Some((format!("{col} {op} ?"), vec![num(rest)?]))
}

/// 댓글이 달린 문서 — `scope` 는 `quest` · `campaign`. `none` 이면 그 종류의 문서에 달리지 않은 것.
fn owner_cond(scope: &str, v: &Val) -> Option<(String, Vec<Bind>)> {
    if v.text().eq_ignore_ascii_case("none") {
        return Some((format!("comments.scope != '{scope}'"), vec![]));
    }
    let (c, b) = id_cond(
        "comments.slug",
        "substr(comments.slug, 1, instr(comments.slug, '-') - 1)",
        "CAST(substr(comments.slug, instr(comments.slug, '-') + 1) AS INTEGER)",
        v,
    )?;
    Some((format!("comments.scope = '{scope}' AND ({c})"), b))
}

/// 댓글 목록의 값 하나 → 조건. 고친 적 없는 댓글의 `updated` 는 도서관처럼 쓴 날이다.
fn comment_cond(key: Option<Key>, v: &Val) -> Option<(String, Vec<Bind>)> {
    let s = v.text();
    let (cond, binds) = match key {
        None => text_cond(&["comments.body", "comments.slug"], v, Match::Partial),
        Some(Key::Id) => entry_cond("comments.entry_id", v)?,
        Some(Key::Quest) => owner_cond("quest", v)?,
        Some(Key::Campaign) => owner_cond("campaign", v)?,
        Some(Key::Author) => text_cond(&["comments.author"], v, Match::Exact),
        Some(Key::Parent) if s.eq_ignore_ascii_case("none") => {
            ("comments.parent_id IS NULL".into(), vec![])
        }
        Some(Key::Parent) => entry_cond("comments.parent_id", v)?,
        Some(Key::Discussion) => {
            let cond = match s.to_ascii_lowercase().as_str() {
                "unresolved" => "comments.discussion = 1 AND comments.resolved = 0",
                "resolved" => "comments.discussion = 1 AND comments.resolved = 1",
                "none" => "comments.discussion = 0",
                _ => return None,
            };
            (cond.into(), vec![])
        }
        Some(Key::Created) => moment_cond("comments.ts", s)?,
        Some(Key::Updated) => {
            moment_cond("COALESCE(NULLIF(comments.edited_at, ''), comments.ts)", s)?
        }
        Some(Key::Body) => text_cond(&["comments.body"], v, Match::Partial),
        _ => return None,
    };
    Some((format!("({cond})"), binds))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 식을 짧은 글로 — 시험에서 모양을 한눈에 본다.
    fn show(q: &Query) -> String {
        fn val(v: &Val) -> String {
            match v {
                Val::Bare(s) => s.clone(),
                Val::Quoted(s) => format!("\"{s}\""),
                Val::Exact(s) => format!("={s}"),
                Val::Regex(s) => format!("/{s}/"),
            }
        }
        fn go(e: &Expr) -> String {
            match e {
                Expr::Term(t) => {
                    let k = t
                        .key
                        .map(|k| format!("{k:?}:").to_lowercase())
                        .unwrap_or_default();
                    let vs: Vec<String> = t.values.iter().map(val).collect();
                    if vs.len() == 1 {
                        format!("{k}{}", vs[0])
                    } else {
                        format!("{k}[{}]", vs.join(","))
                    }
                }
                Expr::Not(i) => format!("-{}", go(i)),
                Expr::And(p) => format!("({})", p.iter().map(go).collect::<Vec<_>>().join(" & ")),
                Expr::Or(p) => format!("({})", p.iter().map(go).collect::<Vec<_>>().join(" | ")),
            }
        }
        q.expr.as_ref().map(go).unwrap_or_default()
    }

    #[test]
    fn keys_lists_minus_and_words() {
        let q = parse(r#"status:[open, in_progress] -tag:wip 로그인 urgency:<=2"#).unwrap();
        assert_eq!(
            show(&q),
            "(status:[open,in_progress] & -tag:wip & 로그인 & urgency:<=2)"
        );
    }

    #[test]
    fn or_and_and_parentheses() {
        // 그리고가 또는보다 먼저 묶인다.
        assert_eq!(show(&parse("a b | c").unwrap()), "((a & b) | c)");
        assert_eq!(show(&parse("a OR b AND c").unwrap()), "(a | (b & c))");
        assert_eq!(show(&parse("a & b").unwrap()), "(a & b)");
        assert_eq!(
            show(&parse("(tag:x | due:overdue) -status:done").unwrap()),
            "((tag:x | due:overdue) & -status:done)"
        );
        assert_eq!(show(&parse("-(a|b)").unwrap()), "-(a | b)");
        // 소문자 or · and 는 낱말이다.
        assert_eq!(
            show(&parse("cats and dogs").unwrap()),
            "(cats & and & dogs)"
        );
        // 괄호가 열려 있지 않으면 `)` 는 낱말의 일부.
        assert_eq!(show(&parse("f(x)").unwrap()), "f(x)");
    }

    #[test]
    fn quotes_regex_exact_and_wildcards() {
        let q =
            parse(r#"tag:["두 낱말", x] "a:b c" -"빼고" /로그인|인증/ title:="정확히" tag:ui-*"#)
                .unwrap();
        assert_eq!(
            show(&q),
            r#"(tag:["두 낱말",x] & "a:b c" & -"빼고" & /로그인|인증/ & title:=정확히 & tag:ui-*)"#
        );
        // 정규식 안의 `\/` 와 띄어쓰기.
        assert_eq!(show(&parse(r"/a\/b c/").unwrap()), r"/a\/b c/");
    }

    #[test]
    fn keys_are_case_insensitive_and_times_are_not_keys() {
        assert_eq!(
            show(&parse("STATUS:open 10:30").unwrap()),
            "(status:open & 10:30)"
        );
    }

    #[test]
    fn errors_point_at_the_place() {
        let at = |s: &str| parse(s).unwrap_err().pos;
        assert_eq!(at("open stauts:open"), 5); // 모르는 키
        assert_eq!(at("status:"), 7); // 빈 값
        assert_eq!(at(r#"x "abc"#), 2); // 안 닫힌 따옴표
        assert_eq!(at("a -"), 2); // '-' 뒤가 빔
        assert_eq!(at("urgency:5"), 0);
        assert_eq!(at("urgency:<=x"), 0);
        assert_eq!(at("due:soon"), 0);
        assert_eq!(at("due:2026-13-01"), 0);
        assert_eq!(at("commented:yesterday"), 0);
        assert_eq!(at("parent:abc"), 0);
        assert_eq!(at("-campaign:C3"), 1);
        assert_eq!(at("(a | b"), 0); // 안 닫힌 괄호
        assert_eq!(at("| a"), 0); // 앞이 빈 연산자
        assert_eq!(at("a |"), 3); // 뒤가 빈 연산자
        assert_eq!(at("tag:[a, b"), 9); // 안 닫힌 목록
        assert_eq!(at("tag:[]"), 5);
        assert_eq!(at("/abc"), 0); // 안 닫힌 정규식
        assert_eq!(at("/(/"), 0); // 틀린 정규식
        assert_eq!(at("urgency:/1/"), 0); // 정규식을 못 쓰는 키
        assert!(
            parse("stauts:open")
                .unwrap_err()
                .message
                .contains("status, type, tag")
        );
        // 쉼표만 쓴 목록은 [ ] 를 알려 준다.
        let e = parse("status:open,done").unwrap_err();
        assert_eq!(e.pos, 7);
        assert!(e.message.contains("status:[open, done]"), "{}", e.message);
    }

    #[test]
    fn ranges() {
        assert!(parse("urgency:2..").is_ok());
        assert!(parse("urgency:=2 campaign:\"C-001\"").is_ok());
        let (sql, b) = urgency_cond("1..=3").unwrap();
        assert_eq!(sql, "q.urgency >= ? AND q.urgency <= ?");
        assert_eq!(b, vec![Bind::Int(1), Bind::Int(3)]);
        let (sql, _) = urgency_cond("2..4").unwrap();
        assert_eq!(sql, "q.urgency >= ? AND q.urgency < ?");
        let (sql, b) = date_cond("d", "2026-11-01..").unwrap();
        assert_eq!((sql.as_str(), b.len()), ("d >= ?", 1));
        let (sql, _) = date_cond("d", "..=2026-11-30").unwrap();
        assert_eq!(sql, "d <= ?");
        assert!(date_cond("d", "..").is_none());
        assert!(date_cond("d", "2026-11-01..soon").is_none());
        assert!(parse("due:2026-11-01..2026-12-01 updated:x").is_err());
        assert!(parse_for(Target::Library, "updated:2026-10-01..").is_ok());
        // 예전 `1-3` 은 이제 범위가 아니다.
        assert!(parse("urgency:1-3").is_err());
    }

    #[test]
    fn values_turn_into_conditions() {
        assert_eq!(urgency_cond(">=3").unwrap().0, "q.urgency >= ?");
        assert_eq!(urgency_cond("2").unwrap().0, "q.urgency = ?");
        assert!(due_cond("<=2026-11-01").unwrap().0.ends_with("<= ?"));
        assert_eq!(split_slug("c-3"), Some(("c".into(), 3)));
        let (sql, binds) = parse("-status:[done, cancelled] campaign:c-3")
            .unwrap()
            .to_sql()
            .unwrap();
        assert!(sql.starts_with(" AND (NOT (("), "{sql}");
        assert_eq!(binds.len(), 7); // status 3개씩 두 값 + 캠페인 하나
        assert_eq!(binds[6], Bind::Text("c-003".into()));
        let (sql, _) = parse("a | b").unwrap().to_sql().unwrap();
        assert!(sql.contains(") OR ("), "{sql}");
        // 글 값의 모양.
        let (sql, b) = text_cond(&["c"], &Val::Bare("ui-*".into()), Match::Exact);
        assert_eq!(
            (sql.as_str(), b),
            (
                "LOWER(c) LIKE LOWER(?) ESCAPE '\\'",
                vec![Bind::Text("ui-%".into())]
            )
        );
        let (sql, b) = text_cond(&["c"], &Val::Regex("^a".into()), Match::Exact);
        assert_eq!(
            (sql.as_str(), b),
            ("c REGEXP ?", vec![Bind::Text("(?i)^a".into())])
        );
        let (_, b) = text_cond(&["c"], &Val::Bare("50%_x".into()), Match::Partial);
        assert_eq!(b, vec![Bind::Text("%50\\%\\_x%".into())]);
        let (_, b) = text_cond(&["c"], &Val::Quoted("a*b".into()), Match::Partial);
        assert_eq!(b, vec![Bind::Text("%a*b%".into())]);
    }

    #[test]
    fn highlights_skip_keys_and_negations() {
        let q = parse(
            r#"로그인 status:open -안쪽 title:화면* (body:/인증|권한/ | "두 낱말") -(x | y)"#,
        )
        .unwrap();
        assert_eq!(
            q.highlights(),
            vec![
                HitTerm::Text("로그인".into()),
                HitTerm::Text("화면".into()),
                HitTerm::Regex("인증|권한".into()),
                HitTerm::Text("두 낱말".into()),
            ]
        );
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
        assert!(err(Target::Library, "status:open").contains("tag, folder, created, updated"));
        assert!(err(Target::Quest, "folder:a").contains("status, type, tag"));
        assert!(
            parse_for(
                Target::Campaign,
                "status:[active, DONE] end:overdue quest:dev-3 title:출시"
            )
            .is_ok()
        );
        assert!(err(Target::Campaign, "status:open").contains("active"));
        assert!(err(Target::Campaign, "start:soon").contains("start"));
        assert!(
            parse_for(
                Target::Library,
                "tag:a folder:none updated:>=2026-01-01 낱말 body:/x/"
            )
            .is_ok()
        );
        // 퀘스트의 `status` 는 길드가 정하므로 아무 값이나 받는다.
        assert!(parse("status:anything").is_ok());

        let (sql, binds) = parse_for(Target::Library, "folder:a_b")
            .unwrap()
            .to_sql()
            .unwrap();
        assert!(sql.contains("library_docs.path LIKE ? ESCAPE"), "{sql}");
        assert_eq!(
            binds,
            vec![Bind::Text("a_b".into()), Bind::Text("a\\_b/%".into())]
        );
        let (sql, binds) = parse_for(Target::Campaign, "-quest:DEV-12")
            .unwrap()
            .to_sql()
            .unwrap();
        assert!(sql.starts_with(" AND NOT (EXISTS"), "{sql}");
        assert_eq!(binds, vec![Bind::Text("DEV".into()), Bind::Int(12)]);
    }
}
