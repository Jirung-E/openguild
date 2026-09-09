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

/// DEV-389: **이벤트의 `guild` 는 사용자가 보는 이름이어야 한다.**
///
/// 예전엔 디렉터리명을 실었다. `openguild init --name runlab` 을 `/tmp/work/g`
/// 에서 하면 화면 어디에도 없는 `"guild": "g"` 가 플러그인으로 갔다 — 실환경
/// 시험에서 실제로 그랬다. 텔레그램 알림이 "어느 프로젝트인가" 를 이걸로 말한다.
#[tokio::test]
async fn events_carry_the_guild_name_not_the_directory_name() {
    let dir = fresh_tmp("guildname");
    // 디렉터리명과 길드 이름을 **일부러 다르게** 만든다. 같으면 시험이
    // 아무것도 안 보는 것과 같다.
    let inner = dir.join("g");
    std::fs::create_dir_all(&inner).unwrap();
    seed_guild_dir(&inner).unwrap();
    std::fs::write(inner.join("runlab.guild"), "name = \"runlab\"\n").unwrap();

    let store = Store::open(&inner).await.unwrap();
    let rec = Recorder::all();
    store.events.set_sink(rec.clone());
    crate::ops::quests::create_quest(&store, new_quest("이름 확인"))
        .await
        .unwrap();

    let j = rec
        .find(super::names::QUEST_CREATED, Phase::Post)
        .unwrap()
        .to_json();
    assert_eq!(
        j["guild"], "runlab",
        "디렉터리명이 실렸다 — 사용자가 보는 이름과 다르다: {j}"
    );

    let _ = std::fs::remove_dir_all(&dir);
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
    assert_eq!(
        j["target"],
        serde_json::json!({ "kind": "quest", "id": q.quest_id })
    );
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
/// DEV-386: **21종이 전부 실제로 나가고, 페이로드에 빈 자리가 없는지.**
///
/// 카탈로그가 `Emitted` 라고 적어 두는 것과 실제로 나가는 것은 다른 문제다.
/// 그리고 나가더라도 `tags` 처럼 **비어서 나가는** 경우가 있었다 — 구독자는
/// 그걸 "태그가 다 지워졌다" 로 읽는다. 이름만 세는 검사로는 그게 안 잡힌다.
///
/// 이벤트 종류를 늘리기 전에(41개가 대기 중) 지금 것이 성한지부터 본다.
#[tokio::test]
async fn all_declared_events_fire_with_a_usable_payload() {
    use serde_json::Value;
    let (dir, store, rec) = setup("all21").await;
    // ── 퀘스트 11종 ──
    let q = crate::ops::quests::create_quest(&store, new_quest("전수 검사"))
        .await
        .unwrap();
    let other = crate::ops::quests::create_quest(&store, new_quest("부모"))
        .await
        .unwrap();
    // 부모는 선행으로 못 건다 — 따로 만든다.
    let pre = crate::ops::quests::create_quest(&store, new_quest("선행"))
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
    crate::ops::quests::set_quest_tags(&store, q.id, vec!["api".into(), "core".into()])
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
    crate::ops::quests::change_quest_type(
        &store,
        q.id,
        crate::models::ChangeTypeRequest {
            new_type_prefix: "BUG".into(),
        },
    )
    .await
    .unwrap();
    crate::ops::quests::change_parent(
        &store,
        q.id,
        crate::models::ChangeParentRequest {
            parent_quest_id: Some(other.id),
        },
    )
    .await
    .unwrap();
    // 두 번 건다 — 한 번만 걸면 이전 값이 원래 없어서, "이전 값을 안 싣는"
    // 결함을 시험이 못 잡는다.
    crate::ops::quests::set_due_dates(&store, q.id, Some(Some("2026-11-30".into())), None)
        .await
        .unwrap();
    crate::ops::quests::set_due_dates(&store, q.id, Some(Some("2026-12-31".into())), None)
        .await
        .unwrap();
    crate::ops::quests::add_prerequisite(
        &store,
        q.id,
        crate::models::AddPrerequisiteRequest {
            prerequisite_id: pre.id,
        },
    )
    .await
    .unwrap();
    crate::ops::quests::remove_prerequisite(&store, q.id, pre.id)
        .await
        .unwrap();

    // ── 댓글 10종 ──
    let c = crate::ops::comments::add_comment_entry(
        &store,
        &q.quest_id,
        "kim".into(),
        "확인 바랍니다".into(),
        None,
        false,
    )
    .await
    .unwrap();
    crate::ops::comments::update_comment_entry(&store, &q.quest_id, c.id, "고쳤습니다".into())
        .await
        .unwrap();
    crate::ops::comments::toggle_comment_pinned(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    crate::ops::comments::toggle_comment_pinned(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    crate::ops::comments::toggle_comment_discussion(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    crate::ops::comments::toggle_comment_resolved(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    crate::ops::comments::toggle_comment_resolved(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    crate::ops::comments::toggle_comment_discussion(&store, &q.quest_id, c.id)
        .await
        .unwrap();
    crate::ops::comments::toggle_comment_reaction(&store, &q.quest_id, c.id, "👍", "kim")
        .await
        .unwrap();
    crate::ops::comments::delete_comment_entry(&store, &q.quest_id, c.id)
        .await
        .unwrap();

    // ── DEV-388: 나머지 리소스 33종 ──
    //
    // 선언한 이름은 **전부 실제로 나와야 한다.** 카탈로그가 `Emitted` 라고
    // 적어 두는 것만으로는 증거가 안 된다.
    let camp = crate::ops::campaigns::create_campaign(
        &store,
        crate::models::CreateCampaignRequest {
            title: "베타 출시".into(),
            description: Some("본문".into()),
            started_at: None,
            ended_at: None,
        },
    )
    .await
    .unwrap();
    crate::ops::campaigns::update_campaign(
        &store,
        camp.id,
        crate::models::UpdateCampaignRequest {
            title: Some("베타 출시 v2".into()),
            description: None,
            status: None,
            started_at: None,
            ended_at: None,
            display_order: None,
        },
    )
    .await
    .unwrap();
    crate::ops::campaigns::link_quest_by_slug(&store, camp.id, &other.quest_id)
        .await
        .unwrap();
    crate::ops::campaigns::unlink_quest_by_slug(&store, camp.id, &other.quest_id)
        .await
        .unwrap();
    crate::ops::campaigns::add_checklist_line(&store, camp.id, "배포 스크립트 점검")
        .await
        .unwrap();
    crate::ops::campaigns::set_checklist_checked_by_index(&store, camp.id, 1, true)
        .await
        .unwrap();
    crate::ops::campaigns::remove_checklist_by_index(&store, camp.id, 1)
        .await
        .unwrap();
    // 배너는 실제 파일이 있어야 한다.
    let img = dir.join("banner.png");
    std::fs::write(&img, b"\x89PNG\r\n\x1a\n").unwrap();
    crate::ops::campaigns::set_banner_image(&store, &camp.campaign_slug, &img)
        .await
        .unwrap();
    crate::ops::campaigns::clear_banner_image(&store, &camp.campaign_slug)
        .await
        .unwrap();

    // 캠페인 댓글 — **같은 `comment.*` 이름**을 쓰고 target 만 다르다.
    let cc = crate::ops::campaign_comments::add_entry(
        &store,
        &camp.campaign_slug,
        "lee".into(),
        "일정 확인 부탁".into(),
        None,
    )
    .await
    .unwrap();
    crate::ops::campaign_comments::update_entry(
        &store,
        &camp.campaign_slug,
        cc.id,
        "일정 확정했습니다".into(),
    )
    .await
    .unwrap();
    crate::ops::campaign_comments::toggle_pinned(&store, &camp.campaign_slug, cc.id)
        .await
        .unwrap();
    crate::ops::campaign_comments::toggle_reaction(&store, &camp.campaign_slug, cc.id, "🎉", "lee")
        .await
        .unwrap();
    crate::ops::campaign_comments::delete_entry(&store, &camp.campaign_slug, cc.id)
        .await
        .unwrap();

    // 도서관
    let bk = crate::ops::library::create_book(&store, "설계 노트", "본문", "")
        .await
        .unwrap();
    crate::ops::library::update_book(&store, &bk.book_id(), Some("설계 노트 v2"), None, None)
        .await
        .unwrap();
    crate::ops::library::set_book_tags(&store, &bk.book_id(), vec!["design".into()])
        .await
        .unwrap();
    crate::ops::library::delete_book(&store, &bk.book_id())
        .await
        .unwrap();
    crate::ops::library::create_folder(&store, "연구")
        .await
        .unwrap();
    crate::ops::library::delete_folder(&store, "연구")
        .await
        .unwrap();

    // 규칙
    crate::ops::rules::create_rule(&store, "branch-policy", "내용".into())
        .await
        .unwrap();
    crate::ops::rules::set_rule(&store, "branch-policy", "바뀐 내용".into())
        .await
        .unwrap();
    crate::ops::rules::set_rule_tags(&store, "branch-policy", vec!["git".into()])
        .await
        .unwrap();
    crate::ops::rules::rename_rule(&store, "branch-policy", "branching")
        .await
        .unwrap();
    crate::ops::rules::delete_rule(&store, "branching")
        .await
        .unwrap();

    // 첨부
    let att = dir.join("spec.txt");
    std::fs::write(&att, b"spec").unwrap();
    crate::ops::attachments::add_quest_attachment(
        &store,
        &other.quest_id,
        "attachments/spec.txt",
        "spec.txt",
    )
    .await
    .unwrap();
    crate::ops::attachments::remove_quest_attachment(
        &store,
        &other.quest_id,
        "attachments/spec.txt",
    )
    .await
    .unwrap();

    // 길드 설정 — 타입·상태·태그 정의
    crate::ops::meta::create_type(&store, "OPS".into(), "#888888".into(), None)
        .await
        .unwrap();
    crate::ops::meta::update_type(&store, "OPS".into(), None, Some("#999999".into()), None)
        .await
        .unwrap();
    crate::ops::meta::rename_type(&store, "OPS".into(), "OPX".into())
        .await
        .unwrap();
    crate::ops::meta::delete_type(&store, "OPX".into())
        .await
        .unwrap();
    let st = crate::ops::meta::create_status(
        &store,
        "Blocked".into(),
        "막힘".into(),
        "#aa0000".into(),
        None,
    )
    .await
    .unwrap();
    crate::ops::meta::update_status(
        &store,
        st.slug.clone(),
        None,
        None,
        Some("차단됨".into()),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    crate::ops::meta::rename_status_slug(&store, st.slug.clone(), "stuck".into())
        .await
        .unwrap();
    crate::ops::meta::delete_status(&store, "stuck".into())
        .await
        .unwrap();
    crate::ops::meta::upsert_tag_def(&store, "api".into(), "#00aa00".into(), "API".into())
        .await
        .unwrap();
    crate::ops::meta::delete_tag_def(&store, "api".into())
        .await
        .unwrap();

    // 작업기록
    crate::ops::worklog::set_note(&store, "2026-09-09", "오늘 한 일".into())
        .await
        .unwrap();

    // 캠페인 삭제는 위의 캠페인 이벤트를 다 낸 뒤에.
    crate::ops::campaigns::delete_campaign(&store, camp.id)
        .await
        .unwrap();

    // ── 마지막: 삭제와 복원 ──
    crate::ops::quests::delete_quest(&store, q.id, &[])
        .await
        .unwrap();
    crate::ops::quests::restore_quest(&store, q.id)
        .await
        .unwrap();

    // ── 1. 선언한 21종이 전부 나왔나 ──
    let got: std::collections::HashSet<_> = rec.names().into_iter().collect();
    let missing: Vec<_> = super::names::ALL
        .iter()
        .filter(|n| !got.contains(**n))
        .collect();
    assert!(
        missing.is_empty(),
        "선언은 했는데 안 나오는 이벤트: {missing:?}\n         카탈로그가 `Emitted` 라고 적어 둔 것과 실제가 어긋난다."
    );

    // ── 2. 페이로드에 빈 자리가 없나 ──
    //
    // `tags` 가 항상 `[]` 로 나가던 것이 이 검사가 없어서 늦게 잡혔다.
    // "있어야 할 자리에 값이 있나" 를 종류별로 본다.
    let all = rec.got.lock().unwrap().clone();
    let mut checked = 0;
    for e in &all {
        let j = e.to_json();
        let name = e.name;
        // 모든 이벤트의 공통 자리.
        assert!(
            j["ts"].as_str().is_some_and(|s| !s.is_empty()),
            "{name}: ts 가 비었다"
        );
        assert!(
            j["guild"].as_str().is_some_and(|s| !s.is_empty()),
            "{name}: guild 가 비었다"
        );

        if name.starts_with("quest.") {
            let qq = &j["quest"];
            assert!(
                qq["id"].as_str().is_some_and(|s| !s.is_empty()),
                "{name}: quest.id 가 비었다 — {j}"
            );
            // 삭제/복원처럼 참조만 싣는 것은 제외하고, 전체를 싣는 것은
            // title 과 status 가 채워져야 한다.
            if qq.get("title").is_some() {
                assert!(
                    qq["title"].as_str().is_some_and(|s| !s.is_empty()),
                    "{name}: quest.title 이 비었다 — {j}"
                );
                assert!(
                    qq["status"].as_str().is_some_and(|s| !s.is_empty()),
                    "{name}: quest.status 가 비었다 — {j}"
                );
                assert!(
                    matches!(qq["tags"], Value::Array(_)),
                    "{name}: quest.tags 가 배열이 아니다 — {j}"
                );
            }
            checked += 1;
        }
        if name.starts_with("comment.") {
            assert!(
                j["target"]["id"].as_str().is_some_and(|s| !s.is_empty()),
                "{name}: 어느 문서의 댓글인지가 없다 — {j}"
            );
            let cc = &j["comment"];
            // pre 에는 id 가 없는 것이 맞다 — 아직 안 만들어졌으니 번호가 없다.
            if e.phase == Phase::Post {
                assert!(
                    cc["id"].as_u64().is_some(),
                    "{name}: comment.id 가 없다 — {j}"
                );
            }
            // 본문을 싣기로 한 것들은 실제로 실려야 한다 — AI 전송이 주
            // 용도라 없으면 쓸모가 없다.
            if cc.get("body").is_some() {
                assert!(
                    cc["body"].as_str().is_some_and(|s| !s.is_empty()),
                    "{name}: comment.body 가 비었다 — {j}"
                );
                assert!(
                    cc["author"].as_str().is_some_and(|s| !s.is_empty()),
                    "{name}: comment.author 가 비었다 — {j}"
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= 21, "검사한 이벤트가 {checked}건뿐이다");

    // ── 3. 변경 이벤트는 from/to 가 실제로 달라야 한다 ──
    for name in [
        super::names::QUEST_STATUS_CHANGED,
        super::names::QUEST_TYPE_CHANGED,
    ] {
        let e = rec.find(name, Phase::Post).unwrap();
        let j = e.to_json();
        assert!(
            j["change"]["from"] != j["change"]["to"],
            "{name}: from 과 to 가 같다 — {j}"
        );
    }

    // ── 2b. DEV-388: 새로 붙인 리소스도 식별자가 실려야 한다 ──
    //
    // 이름이 나오는 것만으로는 부족하다. `campaign.created` 가 어느 캠페인인지
    // 안 실으면 구독자가 할 수 있는 일이 없다.
    let locator = |prefix: &str, path: &[&str]| {
        for e in &all {
            if !e.name.starts_with(prefix) {
                continue;
            }
            let j = e.to_json();
            let mut v = &j;
            for k in path {
                v = &v[*k];
            }
            assert!(
                v.as_str().is_some_and(|s| !s.is_empty()),
                "{}: {} 가 비었다 — {j}",
                e.name,
                path.join(".")
            );
        }
    };
    locator("campaign.", &["campaign", "id"]);
    locator("book.", &["book", "id"]);
    locator("folder.", &["folder", "path"]);
    locator("rule.", &["rule", "slug"]);
    locator("type.", &["type", "prefix"]);
    locator("status.", &["status", "slug"]);
    locator("tag.", &["tag", "slug"]);
    locator("worklog.", &["date"]);
    locator("attachment.", &["target", "id"]);
    locator("attachment.", &["attachment", "path"]);

    // 캠페인 댓글은 **같은 `comment.*` 이름**을 쓰되 target 으로 갈린다.
    // 이게 없으면 구독자가 퀘스트 댓글과 구분을 못 한다.
    let kinds: std::collections::HashSet<String> = all
        .iter()
        .filter(|e| e.name.starts_with("comment."))
        .filter_map(|e| e.to_json()["target"]["kind"].as_str().map(str::to_string))
        .collect();
    assert!(
        kinds.contains("quest") && kinds.contains("campaign"),
        "댓글 이벤트가 어디 것인지 구분이 안 된다: {kinds:?}"
    );

    // ── 3b. "무엇이 바뀌었나" 가 핵심인 이벤트는 그걸 실어야 한다 ──
    //
    // DEV-386 이 여기서 셋을 잡았다. 이름이 `*_changed` 인데 **바뀐 내용이
    // 없으면** 구독자가 할 수 있는 일이 없다.
    let tc = rec
        .find(super::names::QUEST_TYPE_CHANGED, Phase::Post)
        .unwrap()
        .to_json();
    assert_eq!(tc["change"]["from"], "DEV", "{tc}");
    assert_eq!(tc["change"]["to"], "BUG", "{tc}");
    // 타입이 바뀌면 **공개 식별자 자체가 바뀐다** — 안 알리면 구독자는 새
    // 퀘스트가 생긴 줄 안다.
    assert_eq!(tc["renamed"]["from"], "DEV-001", "{tc}");
    assert_eq!(tc["renamed"]["to"], "BUG-001", "{tc}");

    let pc = rec
        .find(super::names::QUEST_PARENT_CHANGED, Phase::Post)
        .unwrap()
        .to_json();
    assert!(
        pc["change"]["from"].is_null(),
        "부모가 없었는데 있다고 한다 — {pc}"
    );
    assert_eq!(pc["change"]["to"], "DEV-002", "새 부모를 안 실었다 — {pc}");

    // 마지막(두 번째) due 이벤트를 본다.
    let dc = rec
        .got
        .lock()
        .unwrap()
        .iter()
        .rfind(|e| e.name == super::names::QUEST_DUE_CHANGED && e.phase == Phase::Post)
        .expect("quest.due_changed 가 안 나왔다")
        .to_json();
    assert_eq!(
        dc["change"]["from"]["desired"], "2026-11-30",
        "이전 기한을 안 실었다 — 당겨졌는지 밀렸는지 알 수 없다: {dc}"
    );
    assert_eq!(dc["change"]["to"]["desired"], "2026-12-31", "{dc}");

    // DEV-388: 새로 붙인 `*_changed` / `*_renamed` 도 같은 기준을 받는다.
    for (name, from, to) in [
        (super::names::RULE_RENAMED, "branch-policy", "branching"),
        (super::names::TYPE_RENAMED, "OPS", "OPX"),
    ] {
        let j = rec.find(name, Phase::Post).unwrap().to_json();
        assert_eq!(j["change"]["from"], from, "{name}: {j}");
        assert_eq!(j["change"]["to"], to, "{name}: {j}");
    }
    for name in [
        super::names::BOOK_TAGS_CHANGED,
        super::names::RULE_TAGS_CHANGED,
    ] {
        let j = rec.find(name, Phase::Post).unwrap().to_json();
        assert!(
            j["change"]["to"].as_array().is_some_and(|a| !a.is_empty()),
            "{name}: 바뀐 태그를 안 실었다 — {j}"
        );
    }
    // 체크리스트는 인덱스만으로는 쓸모가 없다 — 어느 줄인지 실려야 한다.
    for name in [
        super::names::CAMPAIGN_CHECKLIST_ADDED,
        super::names::CAMPAIGN_CHECKLIST_CHECKED,
        super::names::CAMPAIGN_CHECKLIST_REMOVED,
    ] {
        let j = rec.find(name, Phase::Post).unwrap().to_json();
        assert_eq!(
            j["item"]["text"], "배포 스크립트 점검",
            "{name}: 어느 항목인지 안 실렸다 — {j}"
        );
    }

    // ── 4. 태그 이벤트는 태그를 실어야 한다 (DEV-381 회귀 방지) ──
    let tj = rec
        .find(super::names::QUEST_TAGS_CHANGED, Phase::Post)
        .unwrap()
        .to_json();
    assert_eq!(
        tj["quest"]["tags"],
        serde_json::json!(["api", "core"]),
        "태그를 알리는 이벤트가 태그를 안 실었다"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

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
