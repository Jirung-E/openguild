use axum::{body::Body, Router};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::routes;

// --- 테스트 헬퍼 ---

/// 각 테스트마다 독립 temp dir + 시드 + Store + 라우터 생성.
/// 디렉토리 정리는 OS 기본 temp 정리에 위임 (테스트 결정성 우선).
async fn setup() -> Router {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("og-test-{ns}"));
    std::fs::create_dir_all(&dir).unwrap();
    openguild_core::repo::seed_guild_dir(&dir).unwrap();
    let store = openguild_core::Store::open(&dir).await.unwrap();
    routes::create_router(store)
}

async fn get(app: Router, uri: &str) -> (StatusCode, Value) {
    let res = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

async fn post(app: Router, uri: &str, payload: Value) -> (StatusCode, Value) {
    let res = app
        .oneshot(
            Request::post(uri)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

async fn patch(app: Router, uri: &str, payload: Value) -> (StatusCode, Value) {
    let res = app
        .oneshot(
            Request::patch(uri)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

async fn delete(app: Router, uri: &str) -> StatusCode {
    app.oneshot(Request::delete(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

async fn delete_with_body(app: Router, uri: &str) -> (StatusCode, Value) {
    let res = app
        .oneshot(Request::delete(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

async fn put(app: Router, uri: &str, payload: Value) -> (StatusCode, Value) {
    let res = app
        .oneshot(
            Request::put(uri)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

// --- 테스트 ---

#[tokio::test]
async fn test_health() {
    let (status, _) = get(setup().await, "/health").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_list_quest_types() {
    let (status, body) = get(setup().await, "/api/quest-types").await;
    assert_eq!(status, StatusCode::OK);
    let types = body.as_array().unwrap();
    assert_eq!(types.len(), 3);
    assert_eq!(types[0]["prefix"], "DEV");
    assert_eq!(types[1]["prefix"], "BUG");
    assert_eq!(types[2]["prefix"], "REQ");
}

#[tokio::test]
async fn test_list_quest_statuses() {
    let (status, body) = get(setup().await, "/api/quest-statuses").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn test_create_quest() {
    let (status, body) = post(
        setup().await,
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "implement login", "status_id": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["quest_id"], "DEV-001");
    assert_eq!(body["title"], "implement login");
    assert_eq!(body["urgency"], 3); // 기본값 Medium
}

#[tokio::test]
async fn test_quest_id_increments_per_type() {
    let app = setup().await;

    let (_, q1) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "first", "status_id": 1 }),
    )
    .await;
    let (_, q2) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "second", "status_id": 1 }),
    )
    .await;
    let (_, q3) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 2, "title": "bug", "status_id": 1 }),
    )
    .await;

    assert_eq!(q1["quest_id"], "DEV-001");
    assert_eq!(q2["quest_id"], "DEV-002");
    assert_eq!(q3["quest_id"], "BUG-001"); // 타입별 독립 카운터
}

#[tokio::test]
async fn test_list_quests() {
    let app = setup().await;
    post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "q1", "status_id": 1 })).await;
    post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "q2", "status_id": 1 })).await;

    let (status, body) = get(app, "/api/quests").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_get_quest_detail() {
    let app = setup().await;
    let (_, created) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "detail test", "status_id": 1 }),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let (status, body) = get(app, &format!("/api/quests/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["title"], "detail test");
    assert!(body["sub_quests"].is_array());
    assert!(body["prerequisites"].is_array());
}

#[tokio::test]
async fn test_get_quest_not_found() {
    let (status, _) = get(setup().await, "/api/quests/999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_quest() {
    let app = setup().await;
    let (_, created) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "original", "status_id": 1 }),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let (status, body) = patch(
        app,
        &format!("/api/quests/{id}"),
        json!({ "title": "updated", "urgency": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["title"], "updated");
    assert_eq!(body["urgency"], 1);
}

#[tokio::test]
async fn test_change_status() {
    let app = setup().await;
    let (_, created) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "status test", "status_id": 1 }),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let (status, body) = patch(
        app,
        &format!("/api/quests/{id}/status"),
        json!({ "status_id": 2 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status_id"], 2);
}

#[tokio::test]
async fn test_delete_quest() {
    let app = setup().await;
    let (_, created) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "to delete", "status_id": 1 }),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let status = delete(app.clone(), &format!("/api/quests/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = get(app, &format!("/api/quests/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_prerequisites() {
    let app = setup().await;
    let (_, q1) = post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "q1", "status_id": 1 })).await;
    let (_, q2) = post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "q2", "status_id": 1 })).await;
    let id1 = q1["id"].as_i64().unwrap();
    let id2 = q2["id"].as_i64().unwrap();

    // q2는 q1이 선행 퀘스트
    let (status, _) = post(
        app.clone(),
        &format!("/api/quests/{id2}/prerequisites"),
        json!({ "prerequisite_id": id1 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, detail) = get(app, &format!("/api/quests/{id2}")).await;
    assert_eq!(detail["prerequisites"].as_array().unwrap().len(), 1);
    assert_eq!(detail["prerequisites"][0]["id"], id1);
}

#[tokio::test]
async fn test_sub_quests() {
    let app = setup().await;
    let (_, parent) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "parent", "status_id": 1 }),
    )
    .await;
    let parent_id = parent["id"].as_i64().unwrap();

    // 서브퀘스트 2개 생성
    post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "sub 1", "status_id": 1, "parent_quest_id": parent_id })).await;
    post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "sub 2", "status_id": 1, "parent_quest_id": parent_id })).await;

    let (status, detail) = get(app, &format!("/api/quests/{parent_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["sub_quests"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_update_position() {
    let app = setup().await;
    let (_, created) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "position test", "status_id": 1 }),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let (status, body) = put(
        app,
        &format!("/api/quests/{id}/position"),
        json!({ "x": 120.5, "y": 300.0 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["x"], 120.5);
    assert_eq!(body["y"], 300.0);
}

// --- 7단계: 신규 / 사이클 검증 / cascade / candidates 테스트 ---

/// 헬퍼: 빠르게 퀘스트 생성하고 id 반환
async fn mk_quest(app: Router, title: &str, parent: Option<i64>) -> i64 {
    let mut payload = json!({
        "quest_type_id": 1,
        "title": title,
        "status_id": 1
    });
    if let Some(pid) = parent {
        payload["parent_quest_id"] = json!(pid);
    }
    let (_, body) = post(app, "/api/quests", payload).await;
    body["id"].as_i64().unwrap()
}

// === 선행 퀘스트 사이클 ===

#[tokio::test]
async fn test_prereq_self_rejected() {
    let app = setup().await;
    let id = mk_quest(app.clone(), "self", None).await;

    let (status, body) = post(
        app,
        &format!("/api/quests/{id}/prerequisites"),
        json!({ "prerequisite_id": id }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("its own prerequisite"));
}

#[tokio::test]
async fn test_prereq_cycle_rejected() {
    let app = setup().await;
    let a = mk_quest(app.clone(), "A", None).await;
    let b = mk_quest(app.clone(), "B", None).await;

    // A 의 선행으로 B 추가 (성공)
    let (s1, _) = post(
        app.clone(),
        &format!("/api/quests/{a}/prerequisites"),
        json!({ "prerequisite_id": b }),
    )
    .await;
    assert_eq!(s1, StatusCode::CREATED);

    // B 의 선행으로 A 추가 시도 → 사이클 → 거부
    let (s2, body) = post(
        app,
        &format!("/api/quests/{b}/prerequisites"),
        json!({ "prerequisite_id": a }),
    )
    .await;
    assert_eq!(s2, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("cycle"));
}

#[tokio::test]
async fn test_prereq_transitive_cycle_rejected() {
    let app = setup().await;
    let a = mk_quest(app.clone(), "A", None).await;
    let b = mk_quest(app.clone(), "B", None).await;
    let c = mk_quest(app.clone(), "C", None).await;

    // A → B (B 가 A 의 선행)
    post(
        app.clone(),
        &format!("/api/quests/{a}/prerequisites"),
        json!({ "prerequisite_id": b }),
    )
    .await;
    // B → C
    post(
        app.clone(),
        &format!("/api/quests/{b}/prerequisites"),
        json!({ "prerequisite_id": c }),
    )
    .await;

    // C 의 선행으로 A 시도 → 사이클(A→B→C→A)
    let (status, _) = post(
        app,
        &format!("/api/quests/{c}/prerequisites"),
        json!({ "prerequisite_id": a }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// === 부모 변경 (change_parent) ===

#[tokio::test]
async fn test_change_parent_basic() {
    let app = setup().await;
    let parent = mk_quest(app.clone(), "parent", None).await;
    let child = mk_quest(app.clone(), "child", None).await;

    let (status, body) = patch(
        app.clone(),
        &format!("/api/quests/{child}/parent"),
        json!({ "parent_quest_id": parent }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["parent_quest_id"], parent);
}

#[tokio::test]
async fn test_change_parent_detach() {
    let app = setup().await;
    let parent = mk_quest(app.clone(), "p", None).await;
    let child = mk_quest(app.clone(), "c", Some(parent)).await;

    // null 보내서 분리
    let (status, body) = patch(
        app,
        &format!("/api/quests/{child}/parent"),
        json!({ "parent_quest_id": null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["parent_quest_id"].is_null());
}

#[tokio::test]
async fn test_change_parent_self_rejected() {
    let app = setup().await;
    let id = mk_quest(app.clone(), "self", None).await;
    let (status, _) = patch(
        app,
        &format!("/api/quests/{id}/parent"),
        json!({ "parent_quest_id": id }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_change_parent_cycle_rejected() {
    let app = setup().await;
    let a = mk_quest(app.clone(), "A", None).await;
    let b = mk_quest(app.clone(), "B", Some(a)).await; // B 는 A 의 자식

    // A 의 부모를 B 로 → 사이클(A→B→A)
    let (status, body) = patch(
        app,
        &format!("/api/quests/{a}/parent"),
        json!({ "parent_quest_id": b }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("cycle"));
}

// === Candidates ===

#[tokio::test]
async fn test_candidates_sub_excludes_orphans_with_parent() {
    let app = setup().await;
    let a = mk_quest(app.clone(), "A", None).await;
    let _b = mk_quest(app.clone(), "B-sub-of-A", Some(a)).await;
    let c = mk_quest(app.clone(), "C-free", None).await;

    // A 의 sub 후보: 부모 없는 것 중 자기/조상 제외 → C 만 OK
    let (status, body) =
        get(app, &format!("/api/quests/{a}/candidates?relation=sub")).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], c);
}

#[tokio::test]
async fn test_candidates_parent_excludes_descendants() {
    let app = setup().await;
    let a = mk_quest(app.clone(), "A", None).await;
    let b = mk_quest(app.clone(), "B", Some(a)).await; // A→B
    let c = mk_quest(app.clone(), "C", None).await;

    // A 의 parent 후보: 자기/자손 제외 → C, B 제외 → C 만
    let (_, body) = get(
        app.clone(),
        &format!("/api/quests/{a}/candidates?relation=parent"),
    )
    .await;
    let ids: Vec<i64> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&c));
    assert!(!ids.contains(&a));
    assert!(!ids.contains(&b));
}

#[tokio::test]
async fn test_candidates_prereq_excludes_cycles() {
    let app = setup().await;
    let a = mk_quest(app.clone(), "A", None).await;
    let b = mk_quest(app.clone(), "B", None).await;
    let c = mk_quest(app.clone(), "C", None).await;

    // A→B (A 가 B 의 선행)... 즉 B 의 선행으로 A 추가
    post(
        app.clone(),
        &format!("/api/quests/{b}/prerequisites"),
        json!({ "prerequisite_id": a }),
    )
    .await;

    // A 의 prereq 후보: A 의 선행으로 B 를 넣으면 사이클(B→A→B)이므로 B 제외
    let (_, body) =
        get(app, &format!("/api/quests/{a}/candidates?relation=prereq")).await;
    let ids: Vec<i64> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&c));
    assert!(!ids.contains(&a));
    assert!(!ids.contains(&b));
}

#[tokio::test]
async fn test_candidates_invalid_relation() {
    let app = setup().await;
    let id = mk_quest(app.clone(), "x", None).await;
    let (status, _) = get(
        app,
        &format!("/api/quests/{id}/candidates?relation=foo"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// === Cascade 삭제 ===

#[tokio::test]
async fn test_delete_with_no_children() {
    let app = setup().await;
    let id = mk_quest(app.clone(), "lonely", None).await;
    let status = delete(app.clone(), &format!("/api/quests/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_delete_parent_detaches_children_by_default() {
    let app = setup().await;
    let p = mk_quest(app.clone(), "P", None).await;
    let c1 = mk_quest(app.clone(), "C1", Some(p)).await;
    let c2 = mk_quest(app.clone(), "C2", Some(p)).await;

    let status = delete(app.clone(), &format!("/api/quests/{p}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // 자식들은 살아있고 parent_quest_id == null
    let (_, c1_body) = get(app.clone(), &format!("/api/quests/{c1}")).await;
    assert!(c1_body["parent_quest_id"].is_null());
    let (_, c2_body) = get(app, &format!("/api/quests/{c2}")).await;
    assert!(c2_body["parent_quest_id"].is_null());
}

#[tokio::test]
async fn test_delete_with_cascade() {
    let app = setup().await;
    let p = mk_quest(app.clone(), "P", None).await;
    let c1 = mk_quest(app.clone(), "C1", Some(p)).await;
    let c2 = mk_quest(app.clone(), "C2", Some(p)).await;

    // c1 만 같이 삭제, c2 는 분리
    let status = delete(
        app.clone(),
        &format!("/api/quests/{p}?cascade={c1}"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (s1, _) = get(app.clone(), &format!("/api/quests/{c1}")).await;
    assert_eq!(s1, StatusCode::NOT_FOUND);

    let (s2, c2_body) = get(app, &format!("/api/quests/{c2}")).await;
    assert_eq!(s2, StatusCode::OK);
    assert!(c2_body["parent_quest_id"].is_null());
}

#[tokio::test]
async fn test_delete_cascade_rejects_non_child() {
    let app = setup().await;
    let p = mk_quest(app.clone(), "P", None).await;
    let other = mk_quest(app.clone(), "Other", None).await;

    // other 는 P 의 자식이 아니므로 cascade 거부
    let (status, body) = delete_with_body(
        app,
        &format!("/api/quests/{p}?cascade={other}"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("not a direct child"));
}

// === 선행 영향 격리 ===

#[tokio::test]
async fn test_prereq_quest_unaffected_when_dependent_deleted() {
    let app = setup().await;
    let prereq = mk_quest(app.clone(), "prereq", None).await;
    let dep = mk_quest(app.clone(), "dep", None).await;

    // dep 의 선행으로 prereq 추가
    post(
        app.clone(),
        &format!("/api/quests/{dep}/prerequisites"),
        json!({ "prerequisite_id": prereq }),
    )
    .await;

    // dep 삭제
    let s = delete(app.clone(), &format!("/api/quests/{dep}")).await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // prereq 자체는 그대로 존재
    let (status, body) = get(app.clone(), &format!("/api/quests/{prereq}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], prereq);

    // dependencies 테이블에서도 관계는 사라짐
    let (_, deps) = get(app, "/api/quest-dependencies").await;
    assert_eq!(deps.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_dependent_quest_unaffected_when_prereq_deleted() {
    let app = setup().await;
    let prereq = mk_quest(app.clone(), "prereq", None).await;
    let dep = mk_quest(app.clone(), "dep", None).await;

    post(
        app.clone(),
        &format!("/api/quests/{dep}/prerequisites"),
        json!({ "prerequisite_id": prereq }),
    )
    .await;

    delete(app.clone(), &format!("/api/quests/{prereq}")).await;

    // dep 자체는 그대로
    let (status, body) = get(app, &format!("/api/quests/{dep}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], dep);
    // 관계는 정리됨
    assert_eq!(body["prerequisites"].as_array().unwrap().len(), 0);
}

// === sub / prereq 상호 배제 ===

#[tokio::test]
async fn test_cannot_add_prereq_if_already_sub() {
    let app = setup().await;
    let parent = mk_quest(app.clone(), "P", None).await;
    let child = mk_quest(app.clone(), "C", Some(parent)).await;

    // P 의 선행으로 C 추가 시도 → C 는 이미 P 의 sub 이므로 거부
    let (status, body) = post(
        app,
        &format!("/api/quests/{parent}/prerequisites"),
        json!({ "prerequisite_id": child }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("sub-quest"));
}

#[tokio::test]
async fn test_cannot_make_sub_if_already_prereq() {
    let app = setup().await;
    let p = mk_quest(app.clone(), "P", None).await;
    let q = mk_quest(app.clone(), "Q", None).await;
    // Q 를 P 의 선행으로 추가
    post(
        app.clone(),
        &format!("/api/quests/{p}/prerequisites"),
        json!({ "prerequisite_id": q }),
    )
    .await;

    // Q 의 부모를 P 로 변경 시도 → 거부 (prereq + sub 동시 불가)
    let (status, body) = patch(
        app,
        &format!("/api/quests/{q}/parent"),
        json!({ "parent_quest_id": p }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("prerequisite"));
}

#[tokio::test]
async fn test_candidates_sub_excludes_existing_prereqs() {
    let app = setup().await;
    let p = mk_quest(app.clone(), "P", None).await;
    let q = mk_quest(app.clone(), "Q", None).await;
    let r = mk_quest(app.clone(), "R", None).await;

    // Q 를 P 의 선행으로 추가
    post(
        app.clone(),
        &format!("/api/quests/{p}/prerequisites"),
        json!({ "prerequisite_id": q }),
    )
    .await;

    // P 의 sub 후보에서 Q 제외, R 만 포함
    let (_, body) = get(app, &format!("/api/quests/{p}/candidates?relation=sub")).await;
    let ids: Vec<i64> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&r));
    assert!(!ids.contains(&q));
}

#[tokio::test]
async fn test_cannot_add_parent_as_prereq() {
    let app = setup().await;
    let parent = mk_quest(app.clone(), "P", None).await;
    let child = mk_quest(app.clone(), "C", Some(parent)).await;

    // C 의 prereq 로 P (C 의 부모) 추가 시도 → 거부
    let (status, body) = post(
        app,
        &format!("/api/quests/{child}/prerequisites"),
        json!({ "prerequisite_id": parent }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("parent"));
}

#[tokio::test]
async fn test_candidates_prereq_excludes_parent() {
    let app = setup().await;
    let parent = mk_quest(app.clone(), "P", None).await;
    let child = mk_quest(app.clone(), "C", Some(parent)).await;
    let other = mk_quest(app.clone(), "O", None).await;

    // C 의 prereq 후보 — P 는 부모이므로 제외, other 는 포함
    let (_, body) =
        get(app, &format!("/api/quests/{child}/candidates?relation=prereq")).await;
    let ids: Vec<i64> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&other));
    assert!(!ids.contains(&parent));
}

#[tokio::test]
async fn test_candidates_prereq_excludes_existing_subs() {
    let app = setup().await;
    let p = mk_quest(app.clone(), "P", None).await;
    let s = mk_quest(app.clone(), "S", Some(p)).await; // P 의 sub
    let r = mk_quest(app.clone(), "R", None).await;

    // P 의 prereq 후보에서 S 제외, R 만 포함
    let (_, body) =
        get(app, &format!("/api/quests/{p}/candidates?relation=prereq")).await;
    let ids: Vec<i64> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&r));
    assert!(!ids.contains(&s));
}

// === Soft delete / restore ===

#[tokio::test]
async fn test_soft_delete_hides_from_list() {
    let app = setup().await;
    let id = mk_quest(app.clone(), "to-soft", None).await;
    let s = delete(app.clone(), &format!("/api/quests/{id}")).await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // alive list 에서 빠짐
    let (_, body) = get(app.clone(), "/api/quests").await;
    assert_eq!(body.as_array().unwrap().len(), 0);

    // deleted list 에는 나타남
    let (_, deleted) = get(app, "/api/deleted-quests").await;
    let arr = deleted.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], id);
}

#[tokio::test]
async fn test_restore_quest() {
    let app = setup().await;
    let id = mk_quest(app.clone(), "to-restore", None).await;
    delete(app.clone(), &format!("/api/quests/{id}")).await;

    // restore 호출
    let (status, body) = patch(
        app.clone(),
        &format!("/api/quests/{id}/restore"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);

    // alive list 에 다시 나타남
    let (_, alive) = get(app.clone(), "/api/quests").await;
    assert_eq!(alive.as_array().unwrap().len(), 1);

    // deleted list 에서 빠짐
    let (_, deleted) = get(app, "/api/deleted-quests").await;
    assert_eq!(deleted.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_restore_alive_returns_404() {
    let app = setup().await;
    let id = mk_quest(app.clone(), "alive", None).await;
    // 살아있는 quest 를 restore 시도 → 404 (이미 alive)
    let (status, _) = patch(
        app,
        &format!("/api/quests/{id}/restore"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_soft_delete_preserves_dependencies_table_but_filters_out() {
    let app = setup().await;
    let prereq = mk_quest(app.clone(), "p", None).await;
    let dep = mk_quest(app.clone(), "d", None).await;
    post(
        app.clone(),
        &format!("/api/quests/{dep}/prerequisites"),
        json!({ "prerequisite_id": prereq }),
    )
    .await;
    delete(app.clone(), &format!("/api/quests/{dep}")).await;

    // dependencies 응답에선 join 필터로 빠짐
    let (_, deps) = get(app.clone(), "/api/quest-dependencies").await;
    assert_eq!(deps.as_array().unwrap().len(), 0);

    // restore 하면 다시 나타남 (dependency 행은 보존되어 있었음)
    patch(
        app.clone(),
        &format!("/api/quests/{dep}/restore"),
        json!({}),
    )
    .await;
    let (_, deps) = get(app, "/api/quest-dependencies").await;
    assert_eq!(deps.as_array().unwrap().len(), 1);
}

// === 마이그레이션 0002 데이터 보존 검증 ===
//
// 0002 가 데이터를 날리는 회귀를 막는다. 0001 적용 후 데이터를 직접 INSERT 하고,
// 0002 가 모두 적용된 시점에서 parent_quest_id / quest_dependencies / quest_positions
// 가 그대로 살아있는지 확인. (`setup()` 은 모든 마이그레이션이 한 번에 적용된 풀을
// 주므로 동일 흐름이지만, 데이터를 0001 직후가 아닌 마이그레이션 이후에 넣더라도
// 결과적으로 새 스키마 + 데이터가 보존되어 있어야 한다.)

#[tokio::test]
async fn test_migration_preserves_subquest_dep_position() {
    let app = setup().await;
    let parent = mk_quest(app.clone(), "P", None).await;
    let child = mk_quest(app.clone(), "C", Some(parent)).await;
    let other = mk_quest(app.clone(), "O", None).await;

    // 선행 + 위치 추가
    post(
        app.clone(),
        &format!("/api/quests/{child}/prerequisites"),
        json!({ "prerequisite_id": other }),
    )
    .await;
    put(
        app.clone(),
        &format!("/api/quests/{child}/position"),
        json!({ "x": 10.0, "y": 20.0 }),
    )
    .await;

    // 부모-자식, 선행, 위치 모두 살아있는지
    let (_, child_detail) = get(app.clone(), &format!("/api/quests/{child}")).await;
    assert_eq!(child_detail["parent_quest_id"], parent);
    assert_eq!(child_detail["prerequisites"].as_array().unwrap().len(), 1);
    assert_eq!(child_detail["position"]["x"], 10.0);
    assert_eq!(child_detail["position"]["y"], 20.0);

    let (_, deps) = get(app.clone(), "/api/quest-dependencies").await;
    assert_eq!(deps.as_array().unwrap().len(), 1);

    let (_, positions) = get(app, "/api/quest-positions").await;
    assert_eq!(positions.as_array().unwrap().len(), 1);
}

// === 서브퀘스트 생성 시 부모 검증 ===

#[tokio::test]
async fn test_create_subquest_invalid_parent() {
    let app = setup().await;
    let (status, body) = post(
        app,
        "/api/quests",
        json!({
            "quest_type_id": 1,
            "title": "orphan",
            "status_id": 1,
            "parent_quest_id": 999
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("parent quest"));
}
