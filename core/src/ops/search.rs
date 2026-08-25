//! REQ-009: 강화된 검색 — 본문뿐 아니라 댓글 / 메모 / 첨부 이름까지 훑는다.
//!
//! **기본 검색(`quest list --search`)을 바꾸지 않는다.** 그쪽은 title +
//! description + slug 만 보는 빠르고 예측 가능한 경로로 그대로 두고, 더 넓게
//! 찾고 싶을 때 쓰는 별도 진입점이다(사용자 결정).
//!
//! 설계 판단:
//!
//! - **메모는 기본 제외.** `.guild/.gitignore` 가 `quests/*.memo.md` 를 빼므로
//!   메모는 공유되지 않는 개인 기록이다. 게다가 이 서버는 인증이 없고 공개
//!   바인드가 가능하다 — 기본 포함이면 같은 네트워크의 누구나 호스트의 사적
//!   메모를 훑을 수 있다. 명시적으로 요청할 때만 검색한다.
//! - **첨부는 이름만, 캐시 없이.** 첨부 사이드카는 *첨부가 있는 문서만* 갖는다
//!   (실측: 퀘스트 607개에 사이드카 2개, 전부 읽는 데 0.54ms). 역인덱스를 만들면
//!   reindex 단계 + 증분 갱신 훅이 붙고 staleness 버그 여지가 생기는데
//!   ([[REQ-008]] 에서 실제로 두 번 겪었다), 정방향 스캔은 그럴 이유가 없다.
//!   파일 **내용**은 색인하지 않는다(BUG-188 의 대용량 첨부).
//! - **매칭은 `LIKE` 와 같은 부분일치.** 한국어는 조사가 붙고 띄어쓰기가
//!   불규칙해 토큰화 기반 검색이 잘 안 맞는다("테마전환" 으로 "다크테마전환" 을
//!   찾으려면 부분일치여야 한다). 기존 검색 동작과도 일치한다.

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::store::Store;

/// 검색 대상 영역. `title` 은 항상 포함이라 별도 항목이 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchField {
    /// 문서 본문 (quest/campaign description, library body, rule content).
    Body,
    /// 댓글 (quest + campaign).
    Comment,
    /// 메모 — 개인 기록이라 **명시적으로 요청할 때만**.
    Memo,
    /// 첨부 **파일 이름** (내용 아님).
    Attachment,
}

impl SearchField {
    pub fn as_str(self) -> &'static str {
        match self {
            SearchField::Body => "body",
            SearchField::Comment => "comment",
            SearchField::Memo => "memo",
            SearchField::Attachment => "attachment",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "body" => Some(SearchField::Body),
            "comment" | "comments" => Some(SearchField::Comment),
            "memo" | "memos" => Some(SearchField::Memo),
            "attachment" | "attachments" | "attach" => Some(SearchField::Attachment),
            _ => None,
        }
    }

    /// 기본 검색 영역 — **메모는 빠져 있다**(위 설계 판단 참고).
    pub fn defaults() -> Vec<SearchField> {
        vec![SearchField::Body, SearchField::Comment, SearchField::Attachment]
    }
}

/// 검색 결과 한 건.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// 'quest' | 'campaign' | 'rule' | 'book'
    pub kind: String,
    /// quest_id / campaign_slug / rule slug / BOOK-NNN
    pub id: String,
    pub title: String,
    /// **어디서 맞았는지** — 이게 없으면 왜 이 문서가 나왔는지 알 수 없다.
    pub matched_in: Vec<String>,
    /// 처음 맞은 지점 주변 발췌.
    pub excerpt: String,
}

/// 한 문서의 검색 대상 텍스트 묶음.
#[derive(Default)]
struct DocText {
    title: String,
    body: String,
    comments: Vec<String>,
    memo: Option<String>,
    attachments: Vec<String>,
}

impl DocText {
    /// `field` 영역의 텍스트들 — 없으면 빈 슬라이스.
    fn texts(&self, field: SearchField) -> Vec<&str> {
        match field {
            SearchField::Body => vec![self.body.as_str()],
            SearchField::Comment => self.comments.iter().map(|s| s.as_str()).collect(),
            SearchField::Memo => self.memo.iter().map(|s| s.as_str()).collect(),
            SearchField::Attachment => self.attachments.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// 매치 지점 주변을 잘라 발췌를 만든다. **char 단위로 자른다** — 바이트로
/// 자르면 한글이 깨진다.
///
/// BUG-249: 예전엔 `text.to_lowercase()` 에서 찾은 바이트 위치를 **원문에
/// 그대로** 썼다(`text[..byte_pos]`). `to_lowercase()` 는 바이트 길이를 바꿀 수
/// 있어서 — `İ`(U+0130) 는 2바이트인데 소문자 `i̇` 는 3바이트다 — 그런 문자가
/// 앞에 하나만 있어도 이후 인덱스가 전부 밀리고, 밀린 위치가 한글 중간이면
/// 그 자리에서 패닉했다(서버는 연결이 끊기고 CLI 는 죽는다).
///
/// 그래서 소문자화하면서 **소문자 char 인덱스 → 원문 char 인덱스** 대응표를
/// 함께 만든다. `char::to_lowercase()` 가 한 글자를 여러 글자로 펼칠 수 있어
/// 1:1 이 아니므로 매핑이 필요하다.
fn excerpt_around(text: &str, needle_lower: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut lower = String::with_capacity(text.len());
    // lower 의 char 순서대로, 그 글자가 원문 몇 번째 char 에서 나왔는지.
    let mut origin: Vec<usize> = Vec::with_capacity(chars.len());
    for (i, ch) in chars.iter().enumerate() {
        for lc in ch.to_lowercase() {
            lower.push(lc);
            origin.push(i);
        }
    }
    let byte_pos = lower.find(needle_lower).unwrap_or(0);
    // byte_pos 는 lower 안의 char 경계다 — lower 로 세야 안전하다.
    let lower_char_pos = lower[..byte_pos].chars().count();
    let char_pos = origin.get(lower_char_pos).copied().unwrap_or(0);
    let start = char_pos.saturating_sub(width / 2);
    let end = (char_pos + width).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    // 줄바꿈은 한 줄로 접는다 — 목록 표시용이라 원문 레이아웃이 의미 없다.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 강화된 검색. `query` 는 공백으로 나눠 **모든 토큰이** 그 문서의 검색 대상
/// 어딘가에 있어야 매치다(기존 `--search` 의 AND 시맨틱과 동일).
pub async fn search(
    store: &Store,
    query: &str,
    fields: &[SearchField],
    limit: Option<usize>,
) -> AppResult<Vec<SearchHit>> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| t.to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let mut docs: std::collections::BTreeMap<(String, String), DocText> = Default::default();

    // ── 문서 본문 ──
    let quests: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT t.prefix || '-' || printf('%03d', q.number), q.title, q.description
           FROM quests q JOIN quest_types t ON t.id = q.quest_type_id
          WHERE q.deleted_at IS NULL",
    )
    .fetch_all(&store.index_pool)
    .await?;
    for (id, title, body) in quests {
        let e = docs.entry(("quest".into(), id)).or_default();
        e.title = title;
        e.body = body.unwrap_or_default();
    }

    let camps: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT campaign_slug, title, description FROM campaigns WHERE deleted_at IS NULL",
    )
    .fetch_all(&store.index_pool)
    .await?;
    for (id, title, body) in camps {
        let e = docs.entry(("campaign".into(), id)).or_default();
        e.title = title;
        e.body = body.unwrap_or_default();
    }

    let books: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT number, title, body FROM library_docs WHERE deleted_at IS NULL")
            .fetch_all(&store.index_pool)
            .await?;
    for (n, title, body) in books {
        let e = docs.entry(("book".into(), crate::repo::library::book_slug(n))).or_default();
        e.title = title;
        e.body = body;
    }

    // 규칙은 index.db 캐시가 없다(file-truth-db-cache §4) — 파일 직독.
    for r in crate::repo::rules::list_rules(&store.paths).map_err(crate::error::AppError::Internal)?
    {
        let e = docs.entry(("rule".into(), r.slug.clone())).or_default();
        e.title = r.slug; // 규칙은 slug 가 곧 이름.
        e.body = r.content;
    }

    // ── 댓글 ──
    if fields.contains(&SearchField::Comment) {
        let qc: Vec<(String, String)> = sqlx::query_as(
            "SELECT t.prefix || '-' || printf('%03d', q.number), c.body
               FROM quest_comments c
               JOIN quests q ON q.id = c.quest_id
               JOIN quest_types t ON t.id = q.quest_type_id
              WHERE q.deleted_at IS NULL",
        )
        .fetch_all(&store.index_pool)
        .await?;
        for (id, body) in qc {
            if let Some(e) = docs.get_mut(&("quest".to_string(), id)) {
                e.comments.push(body);
            }
        }
        let cc: Vec<(String, String)> = sqlx::query_as(
            "SELECT c.campaign_slug, cm.body
               FROM campaign_comments cm
               JOIN campaigns c ON c.id = cm.campaign_id
              WHERE c.deleted_at IS NULL",
        )
        .fetch_all(&store.index_pool)
        .await?;
        for (id, body) in cc {
            if let Some(e) = docs.get_mut(&("campaign".to_string(), id)) {
                e.comments.push(body);
            }
        }
    }

    // ── 메모 (opt-in) ──
    if fields.contains(&SearchField::Memo) {
        let ms: Vec<(String, String)> = sqlx::query_as(
            "SELECT t.prefix || '-' || printf('%03d', q.number), m.content
               FROM quest_memos m
               JOIN quests q ON q.id = m.quest_id
               JOIN quest_types t ON t.id = q.quest_type_id
              WHERE q.deleted_at IS NULL",
        )
        .fetch_all(&store.index_pool)
        .await?;
        for (id, content) in ms {
            if let Some(e) = docs.get_mut(&("quest".to_string(), id)) {
                e.memo = Some(content);
            }
        }
    }

    // ── 첨부 이름 (사이드카 직독 — 캐시 없음) ──
    if fields.contains(&SearchField::Attachment) {
        for ((kind, id), e) in docs.iter_mut() {
            let names: Vec<String> = match kind.as_str() {
                "quest" => crate::ops::attachments::list_quest_attachments(store, id),
                "campaign" => crate::ops::attachments::list_campaign_attachments(store, id),
                "book" => crate::ops::attachments::list_book_attachments(store, id),
                _ => Vec::new(),
            }
            .into_iter()
            .map(|a| a.name)
            .collect();
            e.attachments = names;
        }
    }

    // ── 필터 ──
    let mut hits: Vec<SearchHit> = Vec::new();
    for ((kind, id), d) in &docs {
        // 어느 토큰이 어느 영역에서 맞았는지 모아, 전 토큰이 커버되는지 본다.
        let mut matched_fields: Vec<SearchField> = Vec::new();
        let mut title_matched = false;
        let mut all_covered = true;

        for tok in &tokens {
            let mut covered = false;
            if d.title.to_lowercase().contains(tok) || id.to_lowercase().contains(tok) {
                covered = true;
                title_matched = true;
            }
            for f in fields {
                if d.texts(*f).iter().any(|t| t.to_lowercase().contains(tok)) {
                    covered = true;
                    if !matched_fields.contains(f) {
                        matched_fields.push(*f);
                    }
                }
            }
            if !covered {
                all_covered = false;
                break;
            }
        }
        if !all_covered {
            continue;
        }

        // 발췌는 **본문 아닌 곳에서 맞았을 때** 특히 중요하다 — 댓글에서 맞았는데
        // 본문 앞부분을 보여주면 왜 나왔는지 알 수 없다.
        let first = &tokens[0];
        let excerpt = matched_fields
            .iter()
            .find_map(|f| {
                d.texts(*f)
                    .into_iter()
                    .find(|t| t.to_lowercase().contains(first))
                    .map(|t| excerpt_around(t, first, 80))
            })
            .unwrap_or_default();

        let mut matched_in: Vec<String> =
            matched_fields.iter().map(|f| f.as_str().to_string()).collect();
        if title_matched {
            matched_in.insert(0, "title".to_string());
        }

        hits.push(SearchHit {
            kind: kind.clone(),
            id: id.clone(),
            title: d.title.clone(),
            matched_in,
            excerpt,
        });
    }

    if let Some(n) = limit {
        hits.truncate(n);
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CreateQuestRequest;
    use crate::repo::seed_guild_dir;

    async fn setup(label: &str) -> (std::path::PathBuf, Store) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-search-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        (dir, store)
    }

    async fn make_quest(store: &Store, title: &str, body: &str) -> String {
        let q = crate::ops::quests::create_quest(
            store,
            CreateQuestRequest {
                quest_type_id: 1,
                title: title.into(),
                description: Some(body.into()),
                status_slug: "open".into(),
                urgency: Some(3),
                parent_quest_id: None,
            },
        )
        .await
        .unwrap();
        q.quest_id
    }

    /// 본문에만 있는 단어로 찾힌다 — 기본 동작.
    #[tokio::test]
    async fn finds_in_body() {
        let (_d, store) = setup("body").await;
        make_quest(&store, "제목", "본문에 마법사가 있다").await;
        let hits = search(&store, "마법사", &SearchField::defaults(), None).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].matched_in.contains(&"body".to_string()));
        assert!(hits[0].excerpt.contains("마법사"));
    }

    /// **댓글에만** 있는 단어로도 찾힌다 — 이게 이 기능의 요점.
    #[tokio::test]
    async fn finds_in_comment_and_reports_source() {
        let (_d, store) = setup("comment").await;
        let id = make_quest(&store, "제목", "본문").await;
        crate::ops::comments::add_comment_entry(
            &store,
            &id,
            "tester".into(),
            "댓글에만 있는 도깨비".into(),
            None,
            false,
        )
        .await
        .unwrap();

        let hits = search(&store, "도깨비", &SearchField::defaults(), None).await.unwrap();
        assert_eq!(hits.len(), 1, "댓글에만 있는 단어를 찾아야 한다");
        assert!(hits[0].matched_in.contains(&"comment".to_string()));
        assert!(hits[0].excerpt.contains("도깨비"), "발췌가 맞은 지점이어야 한다");
    }

    /// REQ-009 의 핵심 계약: **메모는 기본으로 검색되지 않는다.**
    /// 메모는 gitignore 되는 개인 기록이고, 이 서버는 인증이 없다.
    #[tokio::test]
    async fn memo_is_excluded_by_default_and_opt_in_works() {
        let (_d, store) = setup("memo").await;
        let id = make_quest(&store, "제목", "본문").await;
        crate::ops::comments::set_memo(&store, &id, "메모에만 있는 비밀".into())
            .await
            .unwrap();

        let default_hits = search(&store, "비밀", &SearchField::defaults(), None).await.unwrap();
        assert!(default_hits.is_empty(), "기본 검색에 메모가 새면 안 된다");

        let mut with_memo = SearchField::defaults();
        with_memo.push(SearchField::Memo);
        let opt_in = search(&store, "비밀", &with_memo, None).await.unwrap();
        assert_eq!(opt_in.len(), 1, "명시적으로 요청하면 찾아야 한다");
        assert!(opt_in[0].matched_in.contains(&"memo".to_string()));
    }

    /// 여러 토큰은 AND — 서로 다른 영역에 흩어져 있어도 된다.
    #[tokio::test]
    async fn tokens_are_anded_across_fields() {
        let (_d, store) = setup("and").await;
        let id = make_quest(&store, "제목", "본문에 사과").await;
        crate::ops::comments::add_comment_entry(
            &store,
            &id,
            "tester".into(),
            "댓글에 바나나".into(),
            None,
            false,
        )
        .await
        .unwrap();

        let both = search(&store, "사과 바나나", &SearchField::defaults(), None).await.unwrap();
        assert_eq!(both.len(), 1, "본문+댓글에 흩어져 있어도 AND 매치");

        let missing = search(&store, "사과 포도", &SearchField::defaults(), None).await.unwrap();
        assert!(missing.is_empty(), "한 토큰이라도 없으면 매치 아님");
    }

    /// BUG-249: 소문자화로 **바이트 길이가 바뀌는** 문자가 앞에 있으면, 예전
    /// 구현은 밀린 인덱스로 원문을 잘라 한글 중간에서 패닉했다.
    ///
    /// `İ`(U+0130) 는 2바이트인데 소문자 `i̇` 는 3바이트다. 이 함수는 이미
    /// 한글 안전성 테스트가 있었지만 이 경우가 빠져 있었다.
    #[test]
    fn excerpt_survives_lowercase_length_change() {
        // 앞에 İ 를 두어 이후 바이트 인덱스가 밀리게 만든다.
        let text = format!("İ{} 근데 이건 찾아야 한다", "가".repeat(800));
        // 패닉하지 않아야 한다.
        let out = excerpt_around(&text, "근데", 40);
        assert!(out.contains("근데"), "발췌에 매치가 들어 있어야 한다: {out}");
        // 잘린 조각이 온전한 문자열이어야 한다(깨진 바이트 없음).
        assert!(out.chars().all(|c| c != '\u{FFFD}'), "깨진 문자가 있다: {out}");
    }

    /// 패닉만 막는 걸로는 부족하다 — **위치가 밀리는 것**도 잡아야 한다.
    ///
    /// 앞의 `İ` 3개가 소문자화되며 3바이트 늘어나, 예전 구현은 매치 지점을
    /// 한 글자 뒤로 집었다(`…분 표적…`). 경계에 우연히 걸리면 패닉 없이
    /// **조용히 어긋난 발췌**만 나온다.
    #[test]
    fn excerpt_maps_position_through_expanding_chars() {
        let text = "İİİ 앞부분 표적 뒷부분";
        // char 기준: 표(8) 을 중심으로 앞 3, 총 6글자 창.
        assert_eq!(excerpt_around(text, "표적", 6), "…부분 표적 뒷부분");
    }

    /// 대상이 없으면 앞에서부터 — 예전과 동일(회귀).
    #[test]
    fn excerpt_without_match_starts_at_head() {
        let out = excerpt_around("가나다라마바사", "없는말", 3);
        assert!(out.starts_with("가나다"), "{out}");
    }

    /// 한글 부분일치 — 조사가 붙어도 찾혀야 한다(FTS 대신 LIKE 를 고른 이유).
    #[tokio::test]
    async fn korean_substring_matches() {
        let (_d, store) = setup("ko").await;
        make_quest(&store, "제목", "다크테마전환이 느리다").await;
        let hits = search(&store, "테마전환", &SearchField::defaults(), None).await.unwrap();
        assert_eq!(hits.len(), 1, "띄어쓰기 없는 한글 부분일치가 되어야 한다");
    }

    #[tokio::test]
    async fn empty_query_returns_nothing() {
        let (_d, store) = setup("empty").await;
        make_quest(&store, "제목", "본문").await;
        assert!(search(&store, "   ", &SearchField::defaults(), None).await.unwrap().is_empty());
    }
}
