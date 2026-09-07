//! DEV-374: 이벤트가 **실제 mutation 에서 나오는지** 검증한다.
//!
//! `catalog.rs` / `names.rs` 의 테스트는 표와 문자열만 본다 — 그것만으로는
//! "emit 을 코드에 안 달았다" 를 못 잡는다. 여기서는 진짜 길드를 만들고
//! 퀘스트·댓글을 건드려 sink 에 무엇이 도착하는지 본다.

use super::{Event, EventSink, Events, Phase};
use crate::Store;
use crate::models::{ChangeStatusRequest, CreateQuestRequest};
use crate::repo::seed_guild_dir;
use std::sync::{Arc, Mutex};

/// 받은 이벤트를 그대로 쌓아 두는 sink.
#[derive(Default)]
struct Recorder {
    /// 구독 패턴. 비어 있으면 **아무것도 원하지 않는다**(게이팅 검증용).
    patterns: Vec<String>,
    got: Mutex<Vec<Event>>,
}

impl Recorder {
    fn all() -> Arc<Self> {
        Arc::new(Self {
            patterns: vec!["*".into(), "pre:*".into()],
            got: Mutex::new(Vec::new()),
        })
    }
    fn nothing() -> Arc<Self> {
        Arc::new(Self::default())
    }
    fn names(&self) -> Vec<String> {
        self.got
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.name.to_string())
            .collect()
    }
    fn find(&self, name: &str, phase: Phase) -> Option<Event> {
        self.got
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.name == name && e.phase == phase)
            .cloned()
    }
}

impl EventSink for Recorder {
    fn wants(&self, name: &str, phase: Phase) -> bool {
        self.patterns
            .iter()
            .any(|p| super::names::matches(p, name, phase))
    }
    fn dispatch(&self, event: Event) {
        self.got.lock().unwrap().push(event);
    }
}

fn fresh_tmp(label: &str) -> std::path::PathBuf {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("og-ev-{label}-{ns}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

async fn setup(label: &str) -> (std::path::PathBuf, Store, Arc<Recorder>) {
    let dir = fresh_tmp(label);
    seed_guild_dir(&dir).unwrap();
    let store = Store::open(&dir).await.unwrap();
    let rec = Recorder::all();
    store.events.set_sink(rec.clone());
    (dir, store, rec)
}

fn new_quest(title: &str) -> CreateQuestRequest {
    CreateQuestRequest {
        quest_type_id: 1,
        title: title.into(),
        description: Some("본문".into()),
        status_slug: "open".into(),
        urgency: Some(2),
        parent_quest_id: None,
    }
}

/// DEV-381: **태그를 싣는 이벤트가 태그를 안 실었다.** `QuestRow.tags` 는
/// `#[sqlx(skip)]` 이라 `list()` 만 채웠고, mutation 은 전부 `fetch_by_id` 로
/// 돌아오므로 모든 퀘스트 이벤트가 `"tags": []` 였다. 구독자는 "태그가 다
/// 지워졌다" 로 읽는다.
#[tokio::test]
async fn quest_events_carry_the_actual_tags() {
    let (dir, store, rec) = setup("tags").await;
    let q = crate::ops::quests::create_quest(&store, new_quest("태그"))
        .await
        .unwrap();
    crate::ops::quests::set_quest_tags(&store, q.id, vec!["api".into(), "core".into()])
        .await
        .unwrap();

    let ev = rec
        .find(super::names::QUEST_TAGS_CHANGED, Phase::Post)
        .expect("quest.tags_changed 가 안 나왔다");
    assert_eq!(
        ev.to_json()["quest"]["tags"],
        serde_json::json!(["api", "core"]),
        "태그를 알리는 이벤트가 태그를 안 실었다"
    );

    // 이후의 다른 이벤트에도 실려야 한다 — 태그로 거르는 플러그인이 있다.
    crate::ops::quests::change_status(
        &store,
        q.id,
        crate::models::ChangeStatusRequest {
            status_slug: "in_progress".into(),
        },
    )
    .await
    .unwrap();
    let ev2 = rec
        .find(super::names::QUEST_STATUS_CHANGED, Phase::Post)
        .unwrap();
    assert_eq!(
        ev2.to_json()["quest"]["tags"],
        serde_json::json!(["api", "core"])
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// DEV-381: **cascade 로 함께 지워진 하위 퀘스트도 지워진 것이다.** 부모 것만
/// 내보내면 미러링 플러그인이 하위 퀘스트를 영원히 열어 둔다.
#[tokio::test]
async fn cascade_deleted_children_emit_their_own_deleted_event() {
    let (dir, store, rec) = setup("cascade").await;
    let parent = crate::ops::quests::create_quest(&store, new_quest("부모"))
        .await
        .unwrap();
    let mut child_req = new_quest("자식");
    child_req.parent_quest_id = Some(parent.id);
    let child = crate::ops::quests::create_quest(&store, child_req)
        .await
        .unwrap();

    crate::ops::quests::delete_quest(&store, parent.id, &[child.id])
        .await
        .unwrap();

    let deleted: Vec<String> = rec
        .got
        .lock()
        .unwrap()
        .iter()
        .filter(|e| e.name == super::names::QUEST_DELETED && e.phase == Phase::Post)
        .map(|e| {
            e.to_json()["quest"]["id"]
                .as_str()
                .unwrap_or("")
                .to_string()
        })
        .collect();
    assert!(
        deleted.contains(&parent.quest_id) && deleted.contains(&child.quest_id),
        "함께 지워진 자식이 빠졌다: {deleted:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// DEV-381: 지운 뒤에는 어디서도 못 읽는다 — **무엇이 지워졌는지** 실어야 한다.
#[tokio::test]
async fn comment_deleted_says_what_was_deleted() {
    let (dir, store, rec) = setup("cdel").await;
    let q = crate::ops::quests::create_quest(&store, new_quest("댓글 삭제"))
        .await
        .unwrap();
    let c = crate::ops::comments::add_comment_entry(
        &store,
        &q.quest_id,
        "admin".into(),
        "지워질 내용".into(),
        None,
        false,
    )
    .await
    .unwrap();
    crate::ops::comments::delete_comment_entry(&store, &q.quest_id, c.id)
        .await
        .unwrap();

    let ev = rec
        .find(super::names::COMMENT_DELETED, Phase::Post)
        .expect("comment.deleted 가 안 나왔다");
    let j = ev.to_json();
    assert_eq!(j["comment"]["id"], c.id);
    assert_eq!(j["comment"]["author"], "admin");
    assert_eq!(
        j["comment"]["body"], "지워질 내용",
        "무엇이 지워졌는지 안 실렸다: {j}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// DEV-381: **실패도 이벤트다.** [[DEV-373]] 이 "이벤트 수를 안 늘리고
/// `ok`/`error` 만 갈린다" 로 확정했는데, `emit_post_failed` 의 호출 지점이
/// 0곳이라 구독자는 성공만 봤다 — 결정이 코드에 없었다.
#[tokio::test]
async fn a_rejected_mutation_emits_ok_false_with_the_reason() {
    let (dir, store, rec) = setup("failed").await;
    let q = crate::ops::quests::create_quest(&store, new_quest("완료 막힘"))
        .await
        .unwrap();
    // 미해결 토론 댓글이 하나 있으면 완료 상태로 못 간다.
    crate::ops::comments::add_comment_entry(
        &store,
        &q.quest_id,
        "admin".into(),
        "이거 확인 필요".into(),
        None,
        true,
    )
    .await
    .unwrap();

    let err = crate::ops::quests::change_status(
        &store,
        q.id,
        crate::models::ChangeStatusRequest {
            status_slug: "done".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("미해결"), "{err}");

    let ev = rec
        .find(super::names::QUEST_STATUS_CHANGED, Phase::Post)
        .expect("실패했는데 이벤트가 안 나왔다");
    let j = ev.to_json();
    // 이름은 성공과 **같다** — 구독자가 늘지 않고 ok 만 갈린다.
    assert_eq!(j["event"], "quest.status_changed");
    assert_eq!(j["ok"], false, "실패인데 ok 가 true 다");
    assert!(
        j["error"].as_str().unwrap_or_default().contains("미해결"),
        "실패 이유가 안 실렸다: {j}"
    );
    assert_eq!(j["quest"]["id"], q.quest_id);
    assert_eq!(j["change"]["to"], "done");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn quest_create_emits_post_with_public_shape() {
    let (dir, store, rec) = setup("create").await;
    let q = crate::ops::quests::create_quest(&store, new_quest("첫 퀘스트"))
        .await
        .unwrap();

    let ev = rec
        .find(super::names::QUEST_CREATED, Phase::Post)
        .expect("quest.created 가 안 나왔다");
    let j = ev.to_json();
    assert_eq!(j["event"], "quest.created");
    assert_eq!(j["phase"], "post");
    assert_eq!(j["ok"], true);
    // **결과가 실린다** — journal 의 args 에는 없는 값이다(번호는 저장 시 부여).
    assert_eq!(j["quest"]["id"], q.quest_id);
    assert_eq!(j["quest"]["title"], "첫 퀘스트");
    assert_eq!(j["quest"]["status"], "open");
    assert!(j["ts"].is_string());
    // 내부 숫자 id 는 새어 나가면 안 된다 — 공개 식별자는 slug 다.
    assert!(j["quest"]["quest_type_id"].is_null());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn status_change_carries_from_and_to() {
    let (dir, store, rec) = setup("status").await;
    let q = crate::ops::quests::create_quest(&store, new_quest("상태"))
        .await
        .unwrap();
    crate::ops::quests::change_status(
        &store,
        q.id,
        ChangeStatusRequest {
            status_slug: "in_progress".into(),
        },
    )
    .await
    .unwrap();

    let ev = rec
        .find(super::names::QUEST_STATUS_CHANGED, Phase::Post)
        .expect("quest.status_changed 가 안 나왔다");
    let j = ev.to_json();
    assert_eq!(j["change"]["from"], "open");
    assert_eq!(j["change"]["to"], "in_progress");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `toggle_*` 하나가 **방향으로 갈린다** — 이게 내부명을 그대로 안 쓰는 이유다.
#[tokio::test]
async fn resolve_and_unresolve_are_different_events() {
    let (dir, store, rec) = setup("resolve").await;
    let q = crate::ops::quests::create_quest(&store, new_quest("토론"))
        .await
        .unwrap();
    let c = crate::ops::comments::add_comment_entry(
        &store,
        &q.quest_id,
        "작성자".into(),
        "본문".into(),
        None,
        true,
    )
    .await
    .unwrap();

    crate::ops::comments::toggle_comment_resolved(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    assert!(
        rec.find(super::names::COMMENT_RESOLVED, Phase::Post)
            .is_some()
    );
    assert!(
        rec.find(super::names::COMMENT_UNRESOLVED, Phase::Post)
            .is_none()
    );

    crate::ops::comments::toggle_comment_resolved(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    assert!(
        rec.find(super::names::COMMENT_UNRESOLVED, Phase::Post)
            .is_some(),
        "해제가 같은 이름으로 나오면 '해결됨' 구독이 쓸모없어진다"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn comment_add_emits_pre_then_post_and_pre_has_no_ok() {
    let (dir, store, rec) = setup("prepost").await;
    let q = crate::ops::quests::create_quest(&store, new_quest("댓글"))
        .await
        .unwrap();
    crate::ops::comments::add_comment_entry(
        &store,
        &q.quest_id,
        "글쓴이".into(),
        "안녕".into(),
        None,
        false,
    )
    .await
    .unwrap();

    let pre = rec
        .find(super::names::COMMENT_ADDED, Phase::Pre)
        .expect("pre 가 안 나왔다");
    let post = rec
        .find(super::names::COMMENT_ADDED, Phase::Post)
        .expect("post 가 안 나왔다");

    // pre 에는 `ok` 가 **아예 없다** — false 로 실으면 실패로 오해된다.
    assert!(pre.to_json().get("ok").is_none());
    assert_eq!(post.to_json()["ok"], true);
    // 순서: pre 가 먼저.
    let names = rec.names();
    let ip = names
        .iter()
        .position(|n| n == super::names::COMMENT_ADDED)
        .unwrap();
    assert_eq!(rec.got.lock().unwrap()[ip].phase, Phase::Pre);
    // post 는 저장된 결과(본문·id)를 싣는다.
    assert_eq!(post.to_json()["comment"]["body"], "안녕");
    assert!(post.to_json()["comment"]["id"].is_number());
    let _ = std::fs::remove_dir_all(&dir);
}

/// 삭제는 **지우기 전에** 정체를 잡아 둬야 한다 — 뒤에는 알 수 없다.
#[tokio::test]
async fn delete_still_knows_what_was_deleted() {
    let (dir, store, rec) = setup("delete").await;
    let q = crate::ops::quests::create_quest(&store, new_quest("지울 것"))
        .await
        .unwrap();
    crate::ops::quests::delete_quest(&store, q.id, &[])
        .await
        .unwrap();

    let ev = rec
        .find(super::names::QUEST_DELETED, Phase::Post)
        .expect("quest.deleted");
    assert_eq!(ev.to_json()["quest"]["id"], q.quest_id);
    assert_eq!(ev.to_json()["quest"]["title"], "지울 것");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **구독자가 없으면 아무 일도 안 한다.** 이게 없으면 플러그인을 안 쓰는
/// 사용자와 기존 테스트가 비용을 치른다.
#[tokio::test]
async fn no_subscriber_means_no_event() {
    let dir = fresh_tmp("gated");
    seed_guild_dir(&dir).unwrap();
    let store = Store::open(&dir).await.unwrap();
    let rec = Recorder::nothing();
    store.events.set_sink(rec.clone());

    crate::ops::quests::create_quest(&store, new_quest("조용히"))
        .await
        .unwrap();
    assert!(rec.names().is_empty(), "구독하지 않았는데 이벤트가 왔다");

    // sink 자체가 없을 때도 마찬가지(기본 상태).
    let store2 = Store::open(&dir).await.unwrap();
    assert!(!store2.events.has_sink());
    crate::ops::quests::create_quest(&store2, new_quest("역시 조용히"))
        .await
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// replay 중에는 내지 않는다 — 백업 복원이 과거 이벤트를 통째로 흘려보내면
/// 플러그인 입장에서는 "방금 퀘스트가 200개 생겼다" 가 된다.
#[tokio::test]
async fn replay_suppresses_events() {
    let (dir, store, rec) = setup("replay").await;
    store.set_replaying(true);
    crate::ops::quests::create_quest(&store, new_quest("복원 중"))
        .await
        .unwrap();
    assert!(rec.names().is_empty(), "replay 중에 이벤트가 나갔다");

    store.set_replaying(false);
    crate::ops::quests::create_quest(&store, new_quest("복원 끝"))
        .await
        .unwrap();
    assert!(!rec.names().is_empty(), "replay 를 끄면 다시 나와야 한다");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 카탈로그가 `Emitted` 라고 적은 이벤트는 **실제로 나와야 한다.**
/// 표와 코드가 어긋나면 표가 거짓말이 된다.
#[tokio::test]
async fn emitted_catalog_entries_actually_fire() {
    let (dir, store, rec) = setup("catalog").await;
    let q = crate::ops::quests::create_quest(&store, new_quest("전수"))
        .await
        .unwrap();
    crate::ops::quests::update_quest(
        &store,
        q.id,
        crate::models::UpdateQuestRequest {
            title: Some("바뀐 제목".into()),
            description: None,
            urgency: None,
        },
    )
    .await
    .unwrap();
    crate::ops::quests::set_quest_tags(&store, q.id, vec!["api".into()])
        .await
        .unwrap();
    crate::ops::quests::change_status(
        &store,
        q.id,
        ChangeStatusRequest {
            status_slug: "in_progress".into(),
        },
    )
    .await
    .unwrap();
    let c = crate::ops::comments::add_comment_entry(
        &store,
        &q.quest_id,
        "a".into(),
        "b".into(),
        None,
        true,
    )
    .await
    .unwrap();
    crate::ops::comments::toggle_comment_pinned(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    crate::ops::comments::toggle_comment_discussion(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    crate::ops::comments::update_comment_entry(&store, &q.quest_id, c.id, "c".into())
        .await
        .unwrap();
    crate::ops::comments::delete_comment_entry(&store, &q.quest_id, c.id)
        .await
        .unwrap();

    let got: std::collections::HashSet<_> = rec.names().into_iter().collect();
    for want in [
        super::names::QUEST_CREATED,
        super::names::QUEST_UPDATED,
        super::names::QUEST_TAGS_CHANGED,
        super::names::QUEST_STATUS_CHANGED,
        super::names::COMMENT_ADDED,
        super::names::COMMENT_PINNED,
        super::names::COMMENT_UPDATED,
        super::names::COMMENT_DELETED,
    ] {
        assert!(got.contains(want), "{want} 가 안 나왔다");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn events_without_sink_are_cheap_and_silent() {
    let e = Events::default();
    assert!(!e.has_sink());
    assert!(!e.wants(super::names::QUEST_CREATED, Phase::Post));
}
