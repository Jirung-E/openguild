use axum::{body::Body, Router};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{db, routes};

// --- 테스트 헬퍼 ---

async fn setup() -> Router {
    // 테스트마다 독립된 in-memory DB 사용
    let pool = db::create_pool("sqlite::memory:").await.unwrap();
    db::run_migrations(&pool).await.unwrap();
    routes::create_router(pool)
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
