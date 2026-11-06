//! BUG-389: 서버의 쓰기 경로를 **전부** 파일이 밖에서 바뀐 길드 위에서 돌린다.
//!
//! `.guild` 파일이 진리원이고 `index.db` 는 캐시다. 그런데 많은 쓰기가 DB 행으로 파일을 통째로
//! 다시 만들어서, 경로 하나만 지키지 않아도 밖에서 고친 글이 덮인다. 문제가 난 곳만 시험하면 다음
//! 경로에서 또 난다 — 그래서 모든 경로를 같은 세 상황에서 돌린다.
//!
//! - **고친 파일** — 쓰기가 성공하고, 밖에서 넣은 글이 남고, 다시 색인해도 조회가 같다.
//! - **깨진 파일** — 깨진 파일을 덮지 않는다.
//! - **지운 파일** — 지운 파일을 되살리지 않는다.
//! - **쓰기만** — 파일을 건드리지 않고 쓰기만 해도, 다시 색인 전후로 목록까지 같다. 목록은 index 를 읽으므로,
//!   쓰기가 파일만 고치고 행을 빠뜨리면 여기서 드러난다. (밖에서 고친 파일은 켜 둔 서버의 목록이 다음 동기화까지
//!   모르는 것이 정상이라 위 첫 상황에서는 목록을 안 잰다.)
//!
//! 새 쓰기 경로를 더하면 [`every_write_route_is_in_the_matrix`] 가 실패한다 — [`cases`] 에 넣거나,
//! 문서를 쓰지 않는 경로면 이유와 함께 [`EXEMPT`] 에 적는다.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tower::ServiceExt;

use crate::routes;

/// 문서를 쓰지 않거나, 덮는 것이 그 일인 경로.
const EXEMPT: &[(&str, &str, &str)] = &[
    ("ANY", "/api", "없는 API 의 404"),
    ("ANY", "/api/{*rest}", "없는 API 의 404"),
    ("POST", "/api/plugins/reload", "플러그인 정의만 다시 읽는다"),
    (
        "POST",
        "/api/notices/seen",
        "알림함의 읽음 표시만 적는다 — 문서가 아니고 index 에도 없다",
    ),
    (
        "POST",
        "/api/notices/dismiss",
        "알림함에서 한 줄 지운다 — 문서가 아니고 index 에도 없다",
    ),
    (
        "POST",
        "/api/admin/restore",
        "스냅샷으로 되돌려 덮는 것이 그 일이다",
    ),
    (
        "DELETE",
        "/api/admin/snapshots/{ts}",
        "스냅샷 폴더만 지운다",
    ),
];

/// 쓰기 하나. `$DEV-001` 은 그 퀘스트의 행 번호, `$PRE_ID` 는 마지막 준비 요청이 돌려준 `id` 로 바뀐다.
struct Case {
    name: &'static str,
    method: &'static str,
    path: String,
    body: Option<Value>,
    /// 이 쓰기가 일부러 바꾸는 표시. 표시 목록이 비면 그 파일 전체.
    allow: Vec<(String, Vec<&'static str>)>,
    /// 이 쓰기로 옮겨 가는 파일.
    moved: Vec<(String, String)>,
    /// 파일을 건드리기 전에 미리 보내는 요청.
    pre: Vec<(&'static str, String, Option<Value>)>,
}

impl Case {
    fn allow(mut self, rel: &str, marks: &[&'static str]) -> Self {
        self.allow.push((rel.to_string(), marks.to_vec()));
        self
    }
    fn moved(mut self, from: &str, to: &str) -> Self {
        self.moved.push((from.to_string(), to.to_string()));
        self
    }
    fn pre(mut self, method: &'static str, path: &str, body: Option<Value>) -> Self {
        self.pre.push((method, path.to_string(), body));
        self
    }
}

fn case(name: &'static str, method: &'static str, path: &str, body: Option<Value>) -> Case {
    Case {
        name,
        method,
        path: path.to_string(),
        body,
        allow: Vec::new(),
        moved: Vec::new(),
        pre: Vec::new(),
    }
}

const TITLE: &str = "밖제목";
const BODY: &str = "밖본문";

fn cases() -> Vec<Case> {
    let q1 = "quests/DEV-001.md";
    let attach = || json!({ "path": "attachments/x.txt", "name": "x" });
    let up = || json!({ "data_base64": "aGk=", "ext": "txt", "name": "x" });
    let zed = |c: Case| {
        (1..=7)
            .fold(c, |c, i| {
                c.moved(
                    &format!("quests/DEV-00{i}.md"),
                    &format!("quests/ZED-00{i}.md"),
                )
            })
            .moved("quests/DEV-001.comments.md", "quests/ZED-001.comments.md")
            .moved("quests/DEV-001.memo.md", "quests/ZED-001.memo.md")
            .moved("types/DEV.toml", "types/ZED.toml")
    };
    let new_status = || json!({ "name_en": "Zed", "name_ko": "젯", "slug": "zed", "color": "#111111", "category": "open", "sort_order": 99 });
    let parent = || json!({ "parent_quest_id": "$DEV-001" });
    let prereq = || json!({ "prerequisite_id": "$DEV-001" });
    let tag_def = || json!({ "slug": "zz", "color": "#123456", "description": "d" });
    let hl = || json!({ "part": "body", "exact": "원래" });
    vec![
        // 퀘스트
        case(
            "quest urgency",
            "PATCH",
            "/api/quests/$DEV-001",
            Some(json!({ "urgency": 1 })),
        ),
        case(
            "quest title",
            "PATCH",
            "/api/quests/$DEV-001",
            Some(json!({ "title": "새제목" })),
        )
        .allow(q1, &[TITLE]),
        case(
            "quest body",
            "PATCH",
            "/api/quests/$DEV-001",
            Some(json!({ "description": "새본문" })),
        )
        .allow(q1, &[BODY]),
        case(
            "quest status",
            "PATCH",
            "/api/quests/$DEV-001/status",
            Some(json!({ "status_slug": "in_progress" })),
        ),
        case(
            "quest parent",
            "PATCH",
            "/api/quests/$DEV-002/parent",
            Some(parent()),
        ),
        case(
            "quest due",
            "PATCH",
            "/api/quests/$DEV-001/due",
            Some(json!({ "desired_due": "2030-01-01", "required_due": null })),
        ),
        case(
            "quest tags set",
            "PATCH",
            "/api/quests/$DEV-001/tags",
            Some(json!({ "tags": ["x"] })),
        ),
        case(
            "quest tags edit",
            "POST",
            "/api/quests/$DEV-001/tags",
            Some(json!({ "add": ["x"], "remove": [] })),
        ),
        case(
            "quest prereq add",
            "POST",
            "/api/quests/$DEV-002/prerequisites",
            Some(prereq()),
        ),
        case(
            "quest prereq remove",
            "DELETE",
            "/api/quests/$DEV-002/prerequisites/$DEV-001",
            None,
        )
        .pre("POST", "/api/quests/$DEV-002/prerequisites", Some(prereq())),
        case(
            "quest retype",
            "PATCH",
            "/api/quests/$DEV-001/type",
            Some(json!({ "new_type_prefix": "BUG" })),
        )
        .moved(q1, "quests/BUG-001.md")
        .moved("quests/DEV-001.comments.md", "quests/BUG-001.comments.md")
        .moved("quests/DEV-001.memo.md", "quests/BUG-001.memo.md"),
        case("quest delete", "DELETE", "/api/quests/$DEV-003", None),
        case(
            "quest delete parent",
            "DELETE",
            "/api/quests/$DEV-001",
            None,
        )
        .pre("PATCH", "/api/quests/$DEV-002/parent", Some(parent())),
        case(
            "quest restore",
            "PATCH",
            "/api/quests/$DEV-003/restore",
            Some(json!({})),
        )
        .pre("DELETE", "/api/quests/$DEV-003", None),
        case(
            "quest new",
            "POST",
            "/api/quests",
            Some(json!({ "quest_type_id": 1, "title": "n", "status_slug": "open" })),
        ),
        case(
            "quest new sub",
            "POST",
            "/api/quests",
            Some(
                json!({ "quest_type_id": 1, "title": "n", "status_slug": "open", "parent_quest_id": "$DEV-001" }),
            ),
        ),
        case(
            "quest position",
            "PUT",
            "/api/quests/$DEV-001/position",
            Some(json!({ "x": 1.0, "y": 2.0 })),
        ),
        case(
            "quest positions",
            "PUT",
            "/api/quest-positions",
            Some(json!([{ "quest_id": "$DEV-001", "x": 1.0, "y": 2.0 }])),
        ),
        // 댓글 · 메모
        case(
            "comment add",
            "POST",
            "/api/quests/by/DEV-001/comments",
            Some(json!({ "author": "a", "body": "새" })),
        ),
        case(
            "comment update",
            "PATCH",
            "/api/quests/by/DEV-001/comments/1",
            Some(json!({ "body": "고침" })),
        ),
        case(
            "comment delete",
            "DELETE",
            "/api/quests/by/DEV-001/comments/1",
            None,
        ),
        case(
            "comment reaction",
            "POST",
            "/api/quests/by/DEV-001/comments/1/reactions",
            Some(json!({ "emoji": "👍", "author": "x" })),
        ),
        case(
            "comment discussion",
            "POST",
            "/api/quests/by/DEV-001/comments/1/discussion",
            Some(json!({})),
        ),
        case(
            "comment resolved",
            "POST",
            "/api/quests/by/DEV-001/comments/1/resolved",
            Some(json!({})),
        )
        .pre(
            "POST",
            "/api/quests/by/DEV-001/comments/1/discussion",
            Some(json!({})),
        ),
        case(
            "comment pinned",
            "POST",
            "/api/quests/by/DEV-001/comments/1/pinned",
            Some(json!({})),
        ),
        case(
            "memo set",
            "PUT",
            "/api/quests/by/DEV-001/memo",
            Some(json!({ "content": "메모" })),
        )
        .allow("quests/DEV-001.memo.md", &[]),
        // 캠페인
        case(
            "campaign update",
            "PATCH",
            "/api/campaigns/C-001",
            Some(json!({ "started_at": "2030-01-01" })),
        ),
        case(
            "campaign title",
            "PATCH",
            "/api/campaigns/C-001",
            Some(json!({ "title": "새" })),
        )
        .allow("campaigns/C-001.md", &[TITLE]),
        case("campaign delete", "DELETE", "/api/campaigns/C-002", None)
            .allow("campaigns/C-002.md", &[]),
        case(
            "campaign new",
            "POST",
            "/api/campaigns",
            Some(json!({ "title": "n" })),
        ),
        case(
            "campaign link",
            "POST",
            "/api/campaigns/C-001/quests",
            Some(json!({ "quest_slug": "DEV-002" })),
        ),
        case(
            "campaign unlink",
            "DELETE",
            "/api/campaigns/C-001/quests/DEV-001",
            None,
        ),
        case(
            "campaign check add",
            "POST",
            "/api/campaigns/C-001/checklist",
            Some(json!({ "text": "새" })),
        ),
        case(
            "campaign check set",
            "PATCH",
            "/api/campaigns/C-001/checklist/1",
            Some(json!({ "checked": true })),
        ),
        case(
            "campaign check rm",
            "DELETE",
            "/api/campaigns/C-001/checklist/1",
            None,
        ),
        case(
            "campaign comment",
            "POST",
            "/api/campaigns/C-001/comments",
            Some(json!({ "author": "a", "body": "새" })),
        ),
        case(
            "campaign comment pin",
            "POST",
            "/api/campaigns/C-001/comments/1/pinned",
            Some(json!({})),
        ),
        case(
            "campaign comment reaction",
            "POST",
            "/api/campaigns/C-001/comments/1/reactions",
            Some(json!({ "emoji": "👍", "author": "x" })),
        ),
        case(
            "campaign comment update",
            "PATCH",
            "/api/campaigns/C-001/comments/1",
            Some(json!({ "body": "고침" })),
        )
        .allow("campaigns/C-001.comments.md", &[]),
        case(
            "campaign comment delete",
            "DELETE",
            "/api/campaigns/C-001/comments/1",
            None,
        )
        .allow("campaigns/C-001.comments.md", &[]),
        case(
            "campaign memo",
            "PUT",
            "/api/campaigns/C-001/memo",
            Some(json!({ "content": "메모" })),
        ),
        case(
            "campaign banner",
            "POST",
            "/api/campaigns/C-001/banner?ext=png",
            Some(json!("그림")),
        ),
        case(
            "campaign banner clear",
            "DELETE",
            "/api/campaigns/C-001/banner",
            None,
        )
        .pre(
            "POST",
            "/api/campaigns/C-001/banner?ext=png",
            Some(json!("그림")),
        ),
        // 첨부
        case("attachment save", "POST", "/api/attachments", Some(up())),
        case(
            "attachment stream",
            "POST",
            "/api/attachments/stream?ext=txt",
            Some(json!("글")),
        ),
        case(
            "quest attach",
            "POST",
            "/api/quests/by/DEV-001/attachments",
            Some(attach()),
        ),
        case(
            "quest detach",
            "DELETE",
            "/api/quests/by/DEV-001/attachments?path=attachments/x.txt",
            None,
        )
        .pre("POST", "/api/quests/by/DEV-001/attachments", Some(attach())),
        case(
            "campaign attach",
            "POST",
            "/api/campaigns/C-001/attachments",
            Some(attach()),
        ),
        case(
            "campaign detach",
            "DELETE",
            "/api/campaigns/C-001/attachments?path=attachments/x.txt",
            None,
        )
        .pre("POST", "/api/campaigns/C-001/attachments", Some(attach())),
        case(
            "book attach",
            "POST",
            "/api/library/BOOK-001/attachments",
            Some(attach()),
        ),
        case(
            "book detach",
            "DELETE",
            "/api/library/BOOK-001/attachments?path=attachments/x.txt",
            None,
        )
        .pre("POST", "/api/library/BOOK-001/attachments", Some(attach())),
        // 도서관
        case(
            "book title",
            "PATCH",
            "/api/library/BOOK-001",
            Some(json!({ "title": "새" })),
        )
        .allow("library/BOOK-001.md", &[TITLE]),
        case(
            "book body",
            "PATCH",
            "/api/library/BOOK-001",
            Some(json!({ "body": "새" })),
        )
        .allow("library/BOOK-001.md", &[BODY]),
        case(
            "book move",
            "PATCH",
            "/api/library/BOOK-001",
            Some(json!({ "path": "a" })),
        ),
        case(
            "book tags",
            "PATCH",
            "/api/library/BOOK-001/tags",
            Some(json!({ "tags": ["x"] })),
        ),
        case(
            "book tags edit",
            "POST",
            "/api/library/BOOK-001/tags",
            Some(json!({ "add": ["x"], "remove": [] })),
        ),
        case("book delete", "DELETE", "/api/library/BOOK-002", None),
        case(
            "book new",
            "POST",
            "/api/library",
            Some(json!({ "title": "n" })),
        ),
        case(
            "folder new",
            "POST",
            "/api/library/folders",
            Some(json!({ "path": "f" })),
        ),
        case(
            "folder move",
            "PATCH",
            "/api/library/folders",
            Some(json!({ "from": "a", "to": "b" })),
        )
        .pre(
            "PATCH",
            "/api/library/BOOK-001",
            Some(json!({ "path": "a" })),
        ),
        case(
            "folder delete",
            "DELETE",
            "/api/library/folders?path=f",
            None,
        )
        .pre("POST", "/api/library/folders", Some(json!({ "path": "f" }))),
        // 규칙 · 템플릿 · 형광펜 · 일지
        case(
            "rule new",
            "POST",
            "/api/rules",
            Some(json!({ "slug": "r2", "content": "x" })),
        ),
        case(
            "rule set",
            "PUT",
            "/api/rules/r1",
            Some(json!({ "content": "새" })),
        )
        .allow("rules/r1.md", &[]),
        case(
            "rule rename",
            "PATCH",
            "/api/rules/r1",
            Some(json!({ "new_slug": "r3" })),
        )
        .moved("rules/r1.md", "rules/r3.md"),
        case("rule delete", "DELETE", "/api/rules/r1", None).allow("rules/r1.md", &[]),
        case(
            "rule tags",
            "PUT",
            "/api/rules/r1/tags",
            Some(json!({ "tags": ["x"] })),
        ),
        case(
            "rule tags edit",
            "POST",
            "/api/rules/r1/tags",
            Some(json!({ "add": ["x"], "remove": [] })),
        ),
        case(
            "rules single",
            "PUT",
            "/api/rules-single",
            Some(json!({ "content": "하나" })),
        ),
        case(
            "template save",
            "POST",
            "/api/templates",
            Some(json!({ "name": "t1", "frontmatter": {}, "body": "본문" })),
        ),
        case(
            "highlight add",
            "POST",
            "/api/highlights?target=quest:DEV-001",
            Some(hl()),
        ),
        case(
            "highlight remove",
            "DELETE",
            "/api/highlights/$PRE_ID?target=quest:DEV-001",
            None,
        )
        .pre("POST", "/api/highlights?target=quest:DEV-001", Some(hl())),
        case(
            "worklog note",
            "PUT",
            "/api/worklog/note/2030-01-01",
            Some(json!({ "content": "x" })),
        ),
        // 타입 · 상태 · 태그 정의
        case(
            "type color",
            "PATCH",
            "/api/admin/types/DEV",
            Some(json!({ "color": "#123456" })),
        ),
        case(
            "type desc",
            "PATCH",
            "/api/admin/types/BUG",
            Some(json!({ "description": "새" })),
        )
        .allow("types/BUG.toml", &[]),
        zed(case(
            "type rename",
            "PATCH",
            "/api/admin/types/DEV",
            Some(json!({ "new_prefix": "ZED" })),
        )),
        case(
            "type new",
            "POST",
            "/api/admin/types",
            Some(json!({ "prefix": "NEW", "color": "#111111" })),
        ),
        case("type delete", "DELETE", "/api/admin/types/REQ", None).allow("types/REQ.toml", &[]),
        case(
            "status new",
            "POST",
            "/api/admin/statuses",
            Some(new_status()),
        ),
        case(
            "status update",
            "PATCH",
            "/api/admin/statuses/open",
            Some(json!({ "color": "#123456" })),
        ),
        case(
            "status sort",
            "PATCH",
            "/api/admin/statuses/open",
            Some(json!({ "sort_order": 55 })),
        ),
        case(
            "status rename",
            "PATCH",
            "/api/admin/statuses/open",
            Some(json!({ "slug": "fresh" })),
        ),
        case("status delete", "DELETE", "/api/admin/statuses/zed", None).pre(
            "POST",
            "/api/admin/statuses",
            Some(new_status()),
        ),
        case("tagdef upsert", "POST", "/api/tag-defs", Some(tag_def())),
        case("tagdef delete", "DELETE", "/api/tag-defs/zz", None)
            .pre("POST", "/api/tag-defs", Some(tag_def()))
            .allow("tags/zz.toml", &[]),
        // 관리
        case(
            "admin snapshot",
            "POST",
            "/api/admin/snapshot",
            Some(json!({})),
        ),
        case("admin vacuum", "POST", "/api/admin/vacuum", Some(json!({}))),
        case(
            "admin counters",
            "POST",
            "/api/admin/counters",
            Some(json!({})),
        ),
        case(
            "admin reindex",
            "POST",
            "/api/admin/reindex",
            Some(json!({})),
        ),
    ]
}

async fn call(app: &Router, method: &str, path: &str, body: Option<&Value>) -> (StatusCode, Value) {
    let path = path.replace(' ', "%20");
    let req = Request::builder().method(method).uri(path);
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(b).unwrap())),
        None => req.body(Body::empty()),
    };
    let res = app.clone().oneshot(req.unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, value)
}

async fn must(app: &Router, method: &str, path: &str, body: Value) -> Value {
    let (status, value) = call(app, method, path, Some(&body)).await;
    assert!(status.is_success(), "{method} {path}: {status} {value}");
    value
}

fn temp_dir(label: &str) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ns = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("og-matrix-{label}-{ns}-{seq}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

async fn close(store: &openguild_core::Store) {
    store.index_pool.close().await;
    store.journal_pool.close().await;
}

/// 모든 경우가 복사해서 쓰는 길드.
async fn base_guild() -> PathBuf {
    let dir = temp_dir("base");
    std::fs::write(
        dir.join("m.guild"),
        openguild_core::guild_file::marker_content("m", &openguild_core::time::now_local_iso8601()),
    )
    .unwrap();
    openguild_core::repo::seed_guild_dir(&dir).unwrap();
    let store = openguild_core::Store::open(&dir).await.unwrap();
    let app = routes::create_router(store.clone());
    let comment = json!({ "author": "a", "body": "원래 댓글" });
    for i in 1..=7 {
        let dev = must(&app, "GET", "/api/quest-types", json!(null)).await;
        let dev = dev
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["prefix"] == "DEV")
            .unwrap()["id"]
            .clone();
        must(
            &app,
            "POST",
            "/api/quests",
            json!({ "quest_type_id": dev, "title": format!("q{i}"), "status_slug": "open" }),
        )
        .await;
    }
    let id = |v: Value| v["id"].clone();
    let q1 = id(must(&app, "GET", "/api/quests/by/DEV-001", json!(null)).await);
    let q4 = id(must(&app, "GET", "/api/quests/by/DEV-004", json!(null)).await);
    let q5 = id(must(&app, "GET", "/api/quests/by/DEV-005", json!(null)).await);
    let q7 = id(must(&app, "GET", "/api/quests/by/DEV-007", json!(null)).await);
    must(
        &app,
        "PATCH",
        &format!("/api/quests/{q1}"),
        json!({ "description": "원래 본문\n" }),
    )
    .await;
    must(
        &app,
        "POST",
        "/api/quests/by/DEV-001/comments",
        comment.clone(),
    )
    .await;
    must(
        &app,
        "POST",
        "/api/quests/by/DEV-001/comments",
        comment.clone(),
    )
    .await;
    must(&app, "POST", "/api/campaigns", json!({ "title": "c1" })).await;
    must(&app, "POST", "/api/campaigns", json!({ "title": "c2" })).await;
    must(
        &app,
        "POST",
        "/api/campaigns/C-001/checklist",
        json!({ "text": "항목" }),
    )
    .await;
    must(
        &app,
        "POST",
        "/api/library",
        json!({ "title": "b1", "body": "원래 본문\n" }),
    )
    .await;
    must(
        &app,
        "POST",
        "/api/library",
        json!({ "title": "b2", "body": "원래 본문\n" }),
    )
    .await;
    must(
        &app,
        "POST",
        "/api/rules",
        json!({ "slug": "r1", "content": "규칙 본문\n" }),
    )
    .await;
    must(
        &app,
        "POST",
        &format!("/api/quests/{q1}/tags"),
        json!({ "add": ["keep"], "remove": [] }),
    )
    .await;
    must(
        &app,
        "PUT",
        "/api/quests/by/DEV-001/memo",
        json!({ "content": "원래 메모" }),
    )
    .await;
    must(
        &app,
        "POST",
        "/api/campaigns/C-001/quests",
        json!({ "quest_slug": "DEV-001" }),
    )
    .await;
    must(
        &app,
        "POST",
        &format!("/api/quests/{q4}/prerequisites"),
        json!({ "prerequisite_id": q5 }),
    )
    .await;
    let (status, _) = call(&app, "DELETE", &format!("/api/quests/{q7}"), None).await;
    assert!(status.is_success());
    must(&app, "POST", "/api/campaigns/C-001/comments", comment).await;
    // 처음 연 길드의 상태 순서는 씨앗 값이다 — 파일 기준으로 맞춘 뒤의 차이만 본다.
    must(&app, "POST", "/api/admin/reindex", json!({})).await;
    std::fs::create_dir_all(dir.join(".guild/attachments")).unwrap();
    std::fs::write(dir.join(".guild/attachments/x.txt"), "x").unwrap();
    close(&store).await;
    dir
}

fn list_files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == ext))
        .collect();
    out.sort();
    out
}

/// 첫 `key = "..."` 줄의 값 끝에 글을 붙인다.
fn append_to_field(text: &str, key: &str, add: &str) -> Option<String> {
    let start = format!("{key} = \"");
    let line = text
        .lines()
        .find(|l| l.starts_with(&start) && l.ends_with('"'))?;
    let new = format!("{} {add}\"", &line[..line.len() - 1]);
    Some(text.replacen(line, &new, 1))
}

/// 파일 시각을 앞으로 민다 — 파일 시스템의 시각 단위가 거칠어도(Windows) 바뀐 파일로 보인다.
fn bump_mtime(path: &Path) {
    let later = SystemTime::now() + Duration::from_secs(2);
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(later).unwrap();
}

/// 모든 문서 파일에 표시를 넣는다. `{.guild 기준 경로: [표시…]}`
fn edit_all(dot_guild: &Path) -> BTreeMap<String, Vec<&'static str>> {
    let mut marks = BTreeMap::new();
    let mut put = |rel: String, new: String, ms: Vec<&'static str>| {
        let path = dot_guild.join(&rel);
        assert_ne!(std::fs::read_to_string(&path).unwrap(), new, "{rel}");
        std::fs::write(&path, new).unwrap();
        bump_mtime(&path);
        marks.insert(rel, ms);
    };
    for sub in ["quests", "campaigns", "library", "rules"] {
        for path in list_files(&dot_guild.join(sub), "md") {
            let file = path.file_name().unwrap().to_str().unwrap().to_string();
            let rel = format!("{sub}/{file}");
            let text = std::fs::read_to_string(&path).unwrap();
            if file.contains(".comments.") {
                put(rel, text.replace("원래 댓글", "밖댓글"), vec!["밖댓글"]);
            } else if file.contains(".memo.") {
                put(rel, format!("{text}\n밖메모\n"), vec!["밖메모"]);
            } else {
                let titled = append_to_field(&text, "title", TITLE);
                let ms = if titled.is_some() {
                    vec![TITLE, BODY]
                } else {
                    vec![BODY]
                };
                let text = titled.unwrap_or(text);
                const AUTO: &str = "<!-- openguild:auto-begin";
                let new = if text.contains(AUTO) {
                    text.replacen(AUTO, &format!("{BODY}\n\n{AUTO}"), 1)
                } else {
                    format!("{}\n\n{BODY}\n", text.trim_end_matches('\n'))
                };
                put(rel, new, ms);
            }
        }
    }
    for sub in ["types", "statuses", "tags"] {
        for path in list_files(&dot_guild.join(sub), "toml") {
            let file = path.file_name().unwrap().to_str().unwrap();
            let text = std::fs::read_to_string(&path).unwrap();
            if let Some(new) = append_to_field(&text, "description", "밖정의") {
                put(format!("{sub}/{file}"), new, vec!["밖정의"]);
            }
        }
    }
    marks
}

/// index 를 읽는 목록들. 행 번호로만 말하는 `/api/quest-dependencies` 는 뺀다 — 내용은 퀘스트 상세가 잰다.
/// `/api/tags/used` 는 지운 퀘스트의 태그를 다시 색인 전까지 보여 주는 것이 알려져 있어 아직 뺀다.
const LISTS: &[&str] = &[
    "/api/comments",
    "/api/quests",
    "/api/campaigns",
    "/api/library",
    "/api/library/folders",
    "/api/search?q=%EC%9B%90%EB%9E%98",
    "/api/deleted-quests",
    "/api/admin/types",
    "/api/admin/statuses",
    "/api/tag-defs",
];

/// 깨뜨리거나 지울 파일.
const TARGETS: &[&str] = &[
    "quests/DEV-001.md",
    "quests/DEV-002.md",
    "campaigns/C-001.md",
    "library/BOOK-001.md",
    "quests/DEV-001.comments.md",
    "quests/DEV-001.memo.md",
    "campaigns/C-001.comments.md",
];

/// 행 번호 · 시각처럼 다시 색인하면 달라져도 되는 것을 뺀 조회 결과.
fn strip(v: &Value) -> Value {
    const SKIP: &[&str] = &[
        "cached_mtime",
        "version",
        "id",
        "quest_type_id",
        "status_id",
        "parent_quest_id",
        "quest_id_num",
        "file_version",
        "updated_at_local",
    ];
    // 사람이 쓰는 `quest_id`("DEV-001") 는 남기고, 행 번호인 것만 뺀다.
    let row_number = |k: &str, x: &Value| k == "quest_id" && x.is_number();
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, x)| !SKIP.contains(&k.as_str()) && !row_number(k, x))
                .map(|(k, x)| (k.clone(), strip(x)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(strip).collect()),
        other => other.clone(),
    }
}

async fn views(app: &Router, dot_guild: &Path, lists: bool) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let mut read = async |key: String, path: String| {
        let (status, value) = call(app, "GET", &path, None).await;
        let shown = if status.is_success() {
            strip(&value)
        } else {
            json!(status.as_u16())
        };
        // 행 번호가 다시 매겨지면 같은 내용도 순서가 바뀐다 — 목록은 내용만 잰다.
        let shown = match shown {
            Value::Array(mut items) if key.starts_with("목록 ") => {
                items.sort_by_key(|x| x.to_string());
                Value::Array(items)
            }
            other => other,
        };
        out.insert(key, shown);
    };
    if lists {
        for path in LISTS {
            read(format!("목록 {path}"), path.to_string()).await;
        }
    }
    let slugs = |sub: &str| -> Vec<String> {
        list_files(&dot_guild.join(sub), "md")
            .iter()
            .filter_map(|p| p.file_stem()?.to_str().map(str::to_string))
            .filter(|s| !s.contains('.'))
            .collect()
    };
    for s in slugs("quests") {
        read(format!("quest {s}"), format!("/api/quests/by/{s}")).await;
        read(
            format!("comments {s}"),
            format!("/api/quests/by/{s}/comments"),
        )
        .await;
        read(format!("memo {s}"), format!("/api/quests/by/{s}/memo")).await;
    }
    for s in slugs("campaigns") {
        read(format!("campaign {s}"), format!("/api/campaigns/{s}")).await;
        read(
            format!("comments {s}"),
            format!("/api/campaigns/{s}/comments"),
        )
        .await;
    }
    for s in slugs("library") {
        if s.starts_with("BOOK-") {
            read(format!("book {s}"), format!("/api/library/{s}")).await;
        }
    }
    out
}

/// `$DEV-001` 을 행 번호로.
fn fill_path(path: &str, ids: &BTreeMap<String, i64>, pre_id: &str) -> String {
    let mut out = path.replace("$PRE_ID", pre_id);
    for (slug, id) in ids {
        out = out.replace(&format!("${slug}"), &id.to_string());
    }
    out
}

fn fill_body(v: &Value, ids: &BTreeMap<String, i64>) -> Value {
    match v {
        Value::String(s) if s.starts_with('$') => json!(ids[&s[1..]]),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, x)| (k.clone(), fill_body(x, ids)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|x| fill_body(x, ids)).collect()),
        other => other.clone(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Edited,
    Broken,
    Gone,
    WriteOnly,
}

/// 경우 하나를 돌려 문제 목록을 돌려준다.
async fn run_case(base: &Path, mode: Mode, case: Case) -> Vec<String> {
    let dir = temp_dir("case");
    copy_dir(base, &dir);
    let dot = dir.join(".guild");
    let store = openguild_core::Store::open(&dir).await.unwrap();
    let app = routes::create_router(store.clone());

    let mut ids = BTreeMap::new();
    for i in 1..=6 {
        let slug = format!("DEV-00{i}");
        let (_, v) = call(&app, "GET", &format!("/api/quests/by/{slug}"), None).await;
        ids.insert(slug, v["id"].as_i64().unwrap());
    }
    let mut problems = Vec::new();
    let mut pre_id = String::new();
    for (method, path, body) in &case.pre {
        let body = body.as_ref().map(|b| fill_body(b, &ids));
        let path = fill_path(path, &ids, &pre_id);
        let (status, value) = call(&app, method, &path, body.as_ref()).await;
        if !status.is_success() {
            problems.push(format!("준비 요청 실패 {method} {path}: {status} {value}"));
        }
        if let Some(id) = value["id"].as_str() {
            pre_id = id.to_string();
        } else if let Some(id) = value["id"].as_i64() {
            pre_id = id.to_string();
        }
    }

    let mut marks = BTreeMap::new();
    let mut snapshot: Vec<(&str, Option<Vec<u8>>)> = Vec::new();
    match mode {
        Mode::Edited => marks = edit_all(&dot),
        Mode::WriteOnly => {}
        Mode::Broken => {
            for rel in TARGETS {
                let sibling = rel.contains(".comments.") || rel.contains(".memo.");
                let data: &[u8] = if sibling {
                    b"\xff\xfe not utf8"
                } else {
                    b"+++\nnot toml [[[\n+++\nhand\n"
                };
                std::fs::write(dot.join(rel), data).unwrap();
                bump_mtime(&dot.join(rel));
                snapshot.push((rel, Some(data.to_vec())));
            }
        }
        Mode::Gone => {
            for rel in TARGETS {
                std::fs::remove_file(dot.join(rel)).unwrap();
                snapshot.push((rel, None));
            }
        }
    }

    let body = case.body.as_ref().map(|b| fill_body(b, &ids));
    let path = fill_path(&case.path, &ids, &pre_id);
    let (status, value) = call(&app, case.method, &path, body.as_ref()).await;

    if matches!(mode, Mode::Edited | Mode::WriteOnly) {
        let lists = mode == Mode::WriteOnly;
        if !status.is_success() {
            problems.push(format!("요청 실패: {status} {value}"));
        } else {
            let moved: BTreeMap<_, _> = case.moved.iter().cloned().collect();
            for (rel, ms) in &marks {
                let allowed = case.allow.iter().find(|(r, _)| r == rel);
                if allowed.is_some_and(|(_, a)| a.is_empty()) {
                    continue;
                }
                let now = moved.get(rel).unwrap_or(rel);
                let Ok(text) = std::fs::read_to_string(dot.join(now)) else {
                    problems.push(format!("{now} 파일이 사라짐"));
                    continue;
                };
                for m in ms {
                    if !allowed.is_some_and(|(_, a)| a.contains(m)) && !text.contains(m) {
                        problems.push(format!("{now}: 밖에서 넣은 '{m}' 이 지워짐"));
                    }
                }
            }
            // 조회가 게으르게 파일을 읽어 맞추는 곳이 있어, 한 번 읽고 나서 잰다.
            views(&app, &dot, lists).await;
            let before = views(&app, &dot, lists).await;
            let (status, _) = call(&app, "POST", "/api/admin/reindex", Some(&json!({}))).await;
            assert!(status.is_success());
            let after = views(&app, &dot, lists).await;
            for (key, a) in &before {
                let b = after.get(key).unwrap_or(&Value::Null);
                if b != a {
                    problems.push(format!(
                        "다시 색인하면 조회가 달라짐: {key}\n  전 {a}\n  뒤 {b}"
                    ));
                }
            }
        }
    } else {
        for (rel, data) in &snapshot {
            let now = std::fs::read(dot.join(rel)).ok();
            match data {
                None if now.is_some() => problems.push(format!("{rel}: 지운 파일이 되살아남")),
                Some(d) if now.as_ref() != Some(d) => {
                    problems.push(format!("{rel}: 깨진 파일을 덮음"))
                }
                _ => {}
            }
        }
    }
    close(&store).await;
    let _ = std::fs::remove_dir_all(&dir);
    problems
        .into_iter()
        .map(|p| format!("[{}] {p}", case.name))
        .collect()
}

async fn run_matrix(mode: Mode, label: &str) {
    let base = Arc::new(base_guild().await);
    // 경우마다 길드가 따로라 같이 돌려도 된다. 전부 동시에 열면 파일 핸들이 모자란다.
    let limit = Arc::new(tokio::sync::Semaphore::new(8));
    let mut set = tokio::task::JoinSet::new();
    for case in cases() {
        let (base, limit) = (base.clone(), limit.clone());
        let name = case.name;
        set.spawn(async move {
            let _permit = limit.acquire().await.unwrap();
            (name, run_case(&base, mode, case).await)
        });
    }
    let mut problems = Vec::new();
    while let Some(done) = set.join_next().await {
        match done {
            Ok((_, found)) => problems.extend(found),
            // 쓰기 하나가 서버를 죽이는 것도 실패다.
            Err(e) => problems.push(format!("서버가 죽음: {e}")),
        }
    }
    let _ = std::fs::remove_dir_all(&*base);
    problems.sort();
    assert!(problems.is_empty(), "{label}:\n{}", problems.join("\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn no_write_route_loses_what_was_edited_outside() {
    run_matrix(Mode::Edited, "밖에서 고친 파일").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_write_route_overwrites_a_broken_file() {
    run_matrix(Mode::Broken, "깨진 파일").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_write_route_revives_a_deleted_file() {
    run_matrix(Mode::Gone, "지운 파일").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_write_route_leaves_the_lists_behind() {
    run_matrix(Mode::WriteOnly, "쓰기만").await;
}

/// `routes/mod.rs` 의 `.route("경로", post(..).delete(..))` 에서 (메서드, 경로) 를 읽는다.
fn declared_routes(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find(".route(") {
        rest = &rest[at + ".route(".len()..];
        let bytes = rest.as_bytes();
        let (mut i, mut depth, mut path) = (0, 1, None);
        let mut word = String::new();
        while depth > 0 {
            match bytes[i] as char {
                '"' => {
                    let end = rest[i + 1..].find('"').unwrap();
                    path.get_or_insert_with(|| rest[i + 1..i + 1 + end].to_string());
                    i += end + 1;
                }
                '/' if bytes[i + 1] == b'/' => i += rest[i..].find('\n').unwrap(),
                '(' => {
                    if depth == 1
                        && ["get", "post", "patch", "put", "delete", "any"].contains(&word.as_str())
                    {
                        out.push((word.to_uppercase(), path.clone().unwrap()));
                    }
                    depth += 1;
                    word.clear();
                }
                ')' => {
                    depth -= 1;
                    word.clear();
                }
                c if c.is_ascii_alphanumeric() || c == '_' => word.push(c),
                _ => word.clear(),
            }
            i += 1;
        }
    }
    out
}

/// `/api/quests/$DEV-001?x=1` 이 `/api/quests/{id}` 에 맞는가.
fn path_matches(template: &str, path: &str) -> bool {
    let path = path.split('?').next().unwrap();
    let (t, p): (Vec<_>, Vec<_>) = (template.split('/').collect(), path.split('/').collect());
    t.len() == p.len() && t.iter().zip(&p).all(|(a, b)| a.starts_with('{') || a == b)
}

/// 새 쓰기 경로가 이 시험을 조용히 비껴가면 안 된다 — 표에 넣기 전까지 실패한다.
#[test]
fn every_write_route_is_in_the_matrix() {
    let routes = declared_routes(include_str!("routes/mod.rs"));
    let writes: Vec<_> = routes.iter().filter(|(m, _)| m != "GET").collect();
    assert!(writes.len() > 60, "경로를 못 읽었다: {}", writes.len());
    let cases = cases();
    let mut problems = Vec::new();
    for (method, template) in &writes {
        let exempt = EXEMPT.iter().any(|(m, t, _)| m == method && t == template);
        let covered = cases
            .iter()
            .any(|c| c.method == method && path_matches(template, &c.path));
        match (exempt, covered) {
            (false, false) => problems.push(format!("표에 없는 쓰기 경로: {method} {template}")),
            (true, true) => {
                problems.push(format!("표에도 있고 예외에도 있다: {method} {template}"))
            }
            _ => {}
        }
    }
    for (method, template, _) in EXEMPT {
        if !writes.iter().any(|(m, t)| m == method && t == template) {
            problems.push(format!("없어진 경로가 예외에 남았다: {method} {template}"));
        }
    }
    for c in &cases {
        if !writes
            .iter()
            .any(|(m, t)| m == c.method && path_matches(t, &c.path))
        {
            problems.push(format!("[{}] 없는 경로: {} {}", c.name, c.method, c.path));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
