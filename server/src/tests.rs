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
    // BUG: Windows SystemTime 해상도(~15ms)가 거칠어 병렬 테스트가 같은 ns 를
    // 받으면 temp 디렉토리(=guild)를 공유 → quest counter 충돌로 flaky
    // (debug 에서 재현, release 는 타이밍상 우연히 통과). 프로세스 내 원자
    // 카운터로 유일성 보장.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("og-test-{ns}-{seq}"));
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
        json!({ "quest_type_id": 1, "title": "implement login", "status_slug": "open" }),
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
        json!({ "quest_type_id": 1, "title": "first", "status_slug": "open" }),
    )
    .await;
    let (_, q2) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "second", "status_slug": "open" }),
    )
    .await;
    let (_, q3) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 2, "title": "bug", "status_slug": "open" }),
    )
    .await;

    assert_eq!(q1["quest_id"], "DEV-001");
    assert_eq!(q2["quest_id"], "DEV-002");
    assert_eq!(q3["quest_id"], "BUG-001"); // 타입별 독립 카운터
}

#[tokio::test]
async fn test_list_quests() {
    let app = setup().await;
    post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "q1", "status_slug": "open" })).await;
    post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "q2", "status_slug": "open" })).await;

    let (status, body) = get(app, "/api/quests").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 2);
}

// ─────────────── DEV-027: quest list 필터 / 정렬 / limit ───────────────

/// quest 3종 (DEV-001 open / BUG-001 open / DEV-002 done) 미리 만들어 둠.
async fn setup_with_mixed_quests() -> Router {
    let app = setup().await;
    // DEV-001 (urgency 2 — high)
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "dev-open", "status_slug": "open", "urgency": 2 })
    ).await;
    // DEV-002 (urgency 4 — low, 곧 done 으로 옮김)
    let (_, dev2) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "dev-done", "status_slug": "open", "urgency": 4 })
    ).await;
    let dev2_id = dev2["id"].as_i64().unwrap();
    // status_id 3 = Done (migration 시드)
    patch(app.clone(), &format!("/api/quests/{dev2_id}/status"),
        json!({ "status_slug": "done" })
    ).await;
    // BUG-001 (urgency 1 — critical)
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 2, "title": "bug-open", "status_slug": "open", "urgency": 1 })
    ).await;
    app
}

#[tokio::test]
async fn test_list_filter_by_type_prefix() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?type=DEV").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    for q in arr {
        assert_eq!(q["type_prefix"], "DEV");
    }
}

#[tokio::test]
async fn test_list_filter_type_case_insensitive() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?type=bug").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["quest_id"], "BUG-001");
}

#[tokio::test]
async fn test_list_filter_by_status_name_en() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?status=Open").await;
    assert_eq!(body.as_array().unwrap().len(), 2); // DEV-001 + BUG-001
}

#[tokio::test]
async fn test_list_filter_status_slug() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?status=open").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_filter_status_underscore_or_dash() {
    // 'in_progress' / 'in-progress' / 'In Progress' 동일 매칭.
    let app = setup().await;
    let (_, created) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "wip", "status_slug": "open" })
    ).await;
    let id = created["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id}/status"),
        json!({ "status_slug": "in_progress" })  // In Progress
    ).await;

    for variant in ["In Progress", "in progress", "in_progress", "in-progress", "IN_PROGRESS"] {
        let url = format!("/api/quests?status={}", urlencode_test(variant));
        let (_, body) = get(app.clone(), &url).await;
        assert_eq!(
            body.as_array().unwrap().len(),
            1,
            "variant {variant:?} 가 in_progress 1개 매칭해야 함"
        );
    }
}

#[tokio::test]
async fn test_list_filter_combined_type_and_status() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?type=DEV&status=Done").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["quest_id"], "DEV-002");
}

#[tokio::test]
async fn test_list_sort_by_urgency() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?sort=urgency").await;
    let arr = body.as_array().unwrap();
    // urgency ASC — 1 (BUG-001) 먼저, 2 (DEV-001), 4 (DEV-002)
    assert_eq!(arr[0]["quest_id"], "BUG-001");
    assert_eq!(arr[1]["quest_id"], "DEV-001");
    assert_eq!(arr[2]["quest_id"], "DEV-002");
}

#[tokio::test]
async fn test_list_sort_default_is_id_desc() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests").await;
    let arr = body.as_array().unwrap();
    // id DESC — 마지막에 만든 BUG-001 가 id 가장 큼.
    assert_eq!(arr[0]["quest_id"], "BUG-001");
}

#[tokio::test]
async fn test_list_sort_invalid_returns_bad_request() {
    let app = setup_with_mixed_quests().await;
    let (status, _) = get(app, "/api/quests?sort=titlexyz").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_limit() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?limit=2").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_limit_zero_returns_empty() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?limit=0").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_limit_negative_bad_request() {
    let app = setup_with_mixed_quests().await;
    let (status, _) = get(app, "/api/quests?limit=-1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_filter_returns_empty_when_no_match() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?type=REQ").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

// === DEV-027 보강: 빈 문자열 / sort case / multi-value / urgency / parent / no_parent / reverse / offset ===

#[tokio::test]
async fn test_list_empty_type_param_returns_all() {
    let app = setup_with_mixed_quests().await;
    // ?type= (빈 값) 은 필터 미지정으로 취급 → 전체.
    let (_, body) = get(app, "/api/quests?type=").await;
    assert_eq!(body.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_list_empty_status_param_returns_all() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?status=").await;
    assert_eq!(body.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_list_sort_case_insensitive() {
    let app = setup_with_mixed_quests().await;
    // ?sort=ID, ?sort=Urgency 도 통과해야 함.
    let (s1, _) = get(app.clone(), "/api/quests?sort=ID").await;
    assert_eq!(s1, StatusCode::OK);
    let (s2, _) = get(app, "/api/quests?sort=URGENCY").await;
    assert_eq!(s2, StatusCode::OK);
}

#[tokio::test]
async fn test_list_multi_type() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?type=DEV,BUG").await;
    assert_eq!(body.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_list_multi_type_one_unknown() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?type=DEV,REQ").await;
    // DEV 2개만 매칭 (REQ 는 quest 없음).
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_multi_status() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?status=open,done").await;
    assert_eq!(body.as_array().unwrap().len(), 3); // 전부 open or done
}

#[tokio::test]
async fn test_list_urgency_single() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?urgency=1").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["quest_id"], "BUG-001"); // urgency 1 = critical
}

#[tokio::test]
async fn test_list_urgency_out_of_range() {
    let app = setup_with_mixed_quests().await;
    let (s, _) = get(app, "/api/quests?urgency=5").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

// === DEV-028: urgency CSV / 범위 + 시간 범위 ===

#[tokio::test]
async fn test_list_urgency_csv() {
    let app = setup_with_mixed_quests().await;
    // mixed: BUG (urgency=1), DEV-1 (urgency=2), DEV-2 (urgency=4)
    let (_, body) = get(app, "/api/quests?urgency=1,2").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_urgency_range() {
    let app = setup_with_mixed_quests().await;
    // 1..=2 → 2개 (BUG + DEV-1).
    let (_, body) = get(app, "/api/quests?urgency=1-2").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_urgency_range_full() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?urgency=1-4").await;
    assert_eq!(body.as_array().unwrap().len(), 3); // 전체
}

#[tokio::test]
async fn test_list_urgency_range_invalid() {
    let app = setup_with_mixed_quests().await;
    // hi < lo
    let (s, _) = get(app, "/api/quests?urgency=3-1").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_urgency_range_out_of_bounds() {
    let app = setup_with_mixed_quests().await;
    let (s, _) = get(app, "/api/quests?urgency=1-5").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_created_after_filters() {
    let app = setup_with_mixed_quests().await;
    // 모두 방금 생성 → 미래 시점이면 0개.
    let (_, body) = get(app, "/api/quests?created_after=2099-01-01").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_created_after_includes_all() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?created_after=2000-01-01").await;
    assert_eq!(body.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_list_updated_before_filters() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?updated_before=2000-01-01").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_created_range_combined() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(
        app,
        "/api/quests?created_after=2000-01-01&created_before=2099-01-01",
    )
    .await;
    assert_eq!(body.as_array().unwrap().len(), 3);
}

// === DEV-028 후속: TZ-aware 시간 비교 ===
// DEV-041 후 stored ts 는 mixed format ("...Z" 와 "...+09:00"). lex 비교가
// 깨지는 회귀 시나리오를 strftime 변환으로 해결했는지 확인.

#[tokio::test]
async fn test_list_created_after_tz_aware_finds_recent_with_offset_format() {
    let app = setup().await;
    // setup() 직후 INSERT 되는 quest 들은 새 format (로컬 TZ +offset) 으로 저장.
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "recent", "status_slug": "open" })).await;
    let created = q["created_at"].as_str().unwrap().to_string();
    // created_at 직전 1분 → "after" 로 검색.
    // 단순 lex 라면 stored ("...+09:00") 가 input ("...Z") 보다 작게 비교될 수 있음
    // ('+' < 'Z' ASCII). strftime 사용 시 절대 시각 기반 → 정상 매치.
    let one_min_before = subtract_one_minute(&created);
    let (_, body) = get(app, &format!("/api/quests?created_after={}", url_encode(&one_min_before))).await;
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|x| x["title"] == "recent"),
        "방금 만든 quest 가 'after one minute ago' 검색에 포함되어야 함. \
         stored={created:?}, query={one_min_before:?}, results={arr:?}");
}

/// "2026-05-22T13:41:10+09:00" 같은 입력에서 1분 빼기.
///
/// 이전 구현은 분/시 자리만 문자열 치환 — 자정 (00:00) 에 실행되면 23:59 로
/// wrap 하면서 **날짜는 그대로** 라 미래 시각이 되어 테스트가 flaky 했음
/// (2026-06-12 00:00:52 실행에서 실제 발생). chrono 절대 시각 연산으로 교체.
fn subtract_one_minute(s: &str) -> String {
    let dt = chrono::DateTime::parse_from_rfc3339(s)
        .expect("test ts must be RFC 3339");
    (dt - chrono::Duration::minutes(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

fn url_encode(s: &str) -> String {
    // 최소: `:` `+` 만 인코딩 (queryString 에서 `+` 는 공백으로 해석되므로).
    s.replace('+', "%2B").replace(':', "%3A")
}

#[tokio::test]
async fn test_list_created_after_with_naked_iso_uses_local_tz() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "naked-tz", "status_slug": "open" })).await;
    // created_at 의 날짜 추출 (앞 10자, "YYYY-MM-DD").
    let created = q["created_at"].as_str().unwrap();
    let date_part = &created[..10];

    // 입력 "YYYY-MM-DDT00:00:00" (TZ 없음) — normalize_filter_ts 가 로컬 TZ 부착.
    let query_str = format!("{date_part}T00:00:00");
    let (_, body) = get(app, &format!("/api/quests?created_after={}", url_encode(&query_str))).await;
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|x| x["title"] == "naked-tz"),
        "naked datetime input 이 로컬 TZ 로 해석되어야 함. \
         stored={created:?}, query={query_str:?}");
}

#[tokio::test]
async fn test_list_urgency_empty_string_no_filter() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?urgency=").await;
    assert_eq!(body.as_array().unwrap().len(), 3); // 미지정 동일
}

// === DEV-029: 관계 필터 (has-prereq / has-sub) ===

async fn setup_with_relations() -> Router {
    let app = setup().await;
    // q1, q2 만 생성. q2 의 parent = q1. q3 추가 후 q3 prereq q1.
    let (_, q1) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "q1-leaf-parent", "status_slug": "open" })).await;
    let q1_id = q1["id"].as_i64().unwrap();
    let (_, _q2) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "q2-child", "status_slug": "open", "parent_quest_id": q1_id })).await;
    let (_, q3) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "q3-has-prereq", "status_slug": "open" })).await;
    let q3_id = q3["id"].as_i64().unwrap();
    post(app.clone(),
        &format!("/api/quests/{q3_id}/prerequisites"),
        json!({ "prerequisite_id": q1_id })).await;
    app
}

#[tokio::test]
async fn test_list_has_prereq() {
    let app = setup_with_relations().await;
    let (_, body) = get(app, "/api/quests?has_prereq=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["title"], "q3-has-prereq");
}

#[tokio::test]
async fn test_list_no_prereq() {
    let app = setup_with_relations().await;
    let (_, body) = get(app, "/api/quests?no_prereq=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 2); // q1, q2
}

#[tokio::test]
async fn test_list_has_sub() {
    let app = setup_with_relations().await;
    let (_, body) = get(app, "/api/quests?has_sub=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["title"], "q1-leaf-parent");
}

#[tokio::test]
async fn test_list_no_sub() {
    let app = setup_with_relations().await;
    let (_, body) = get(app, "/api/quests?no_sub=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 2); // q2, q3 (둘 다 sub 없음)
}

#[tokio::test]
async fn test_list_no_prereq_and_no_sub_combined() {
    let app = setup_with_relations().await;
    // q2: parent 있지만 prereq 없음, sub 없음
    let (_, body) = get(app, "/api/quests?no_prereq=true&no_sub=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["title"], "q2-child");
}

#[tokio::test]
async fn test_list_has_prereq_and_no_prereq_mutually_exclusive() {
    let app = setup_with_relations().await;
    let (s, _) = get(app, "/api/quests?has_prereq=true&no_prereq=true").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_has_sub_and_no_sub_mutually_exclusive() {
    let app = setup_with_relations().await;
    let (s, _) = get(app, "/api/quests?has_sub=true&no_sub=true").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

// === DEV-030: --search 검색 ===

async fn setup_for_search() -> Router {
    let app = setup().await;
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "Tauri invoke handler",
                "description": "Rust 측 commands.rs 작성", "status_slug": "open" })).await;
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "Frontend transport adapter",
                "description": "HTTP / Tauri 자동 분기", "status_slug": "open" })).await;
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 2, "title": "Quest list 검색",
                "description": "title / description 부분 일치", "status_slug": "open" })).await;
    app
}

#[tokio::test]
async fn test_list_search_in_title() {
    let app = setup_for_search().await;
    let (_, body) = get(app, "/api/quests?search=Tauri").await;
    let arr = body.as_array().unwrap();
    // title 또는 description 둘 다 검사 → "Tauri invoke handler"
    // + "Frontend transport adapter" (description 에 Tauri 포함) = 2.
    assert_eq!(arr.len(), 2);
}

#[tokio::test]
async fn test_list_search_in_description_only() {
    let app = setup_for_search().await;
    let (_, body) = get(app, "/api/quests?search=commands.rs").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["title"], "Tauri invoke handler");
}

#[tokio::test]
async fn test_list_search_case_insensitive() {
    let app = setup_for_search().await;
    let (_, body1) = get(app.clone(), "/api/quests?search=TAURI").await;
    let (_, body2) = get(app, "/api/quests?search=tauri").await;
    assert_eq!(
        body1.as_array().unwrap().len(),
        body2.as_array().unwrap().len()
    );
}

#[tokio::test]
async fn test_list_search_multi_token_and() {
    let app = setup_for_search().await;
    // 두 토큰 모두 매치 → "title / description" 가 description 에 있는 1개.
    let (_, body) = get(app, "/api/quests?search=title%20description").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["title"], "Quest list 검색");
}

#[tokio::test]
async fn test_list_search_no_match() {
    let app = setup_for_search().await;
    let (_, body) = get(app, "/api/quests?search=NonexistentKeyword").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_search_korean() {
    let app = setup_for_search().await;
    let (_, body) = get(app, "/api/quests?search=%EA%B2%80%EC%83%89").await; // "검색"
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["title"], "Quest list 검색");
}

#[tokio::test]
async fn test_list_search_empty_no_filter() {
    let app = setup_for_search().await;
    let (_, body) = get(app, "/api/quests?search=").await;
    assert_eq!(body.as_array().unwrap().len(), 3);
}

// === DEV-037: title_only 옵션 ===

#[tokio::test]
async fn test_list_search_title_only_excludes_description() {
    let app = setup_for_search().await;
    // "Tauri" 는 title 1건 + description 1건. title_only=true → 1건.
    let (_, body) = get(app, "/api/quests?search=Tauri&title_only=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["title"], "Tauri invoke handler");
}

#[tokio::test]
async fn test_list_search_title_only_description_keyword_no_match() {
    let app = setup_for_search().await;
    // "commands.rs" 는 description 에만. title_only=true → 0건.
    let (_, body) = get(app, "/api/quests?search=commands.rs&title_only=true").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_search_title_only_false_default_behavior() {
    let app = setup_for_search().await;
    // title_only=false 명시 → 기본 동작 (title + description).
    let (_, body) = get(app, "/api/quests?search=Tauri&title_only=false").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_search_title_only_multi_token_and() {
    let app = setup_for_search().await;
    // "Quest" + "검색" 둘 다 title 에 있어야 매치 → 1건.
    let (_, body) = get(app, "/api/quests?search=Quest%20%EA%B2%80%EC%83%89&title_only=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["title"], "Quest list 검색");
}

// === DEV-040: slug (quest_id) 검색 ===

#[tokio::test]
async fn test_list_search_full_slug() {
    let app = setup_for_search().await;
    // 첫 quest 의 slug 는 DEV-001 (test type seed 의 DEV).
    let (_, body) = get(app, "/api/quests?search=DEV-001").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["quest_id"], "DEV-001");
}

#[tokio::test]
async fn test_list_search_partial_number() {
    let app = setup_for_search().await;
    // "002" 는 DEV-002 의 slug 만 매치 (title / description 에 002 없음).
    let (_, body) = get(app, "/api/quests?search=002").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["quest_id"], "DEV-002");
}

#[tokio::test]
async fn test_list_search_prefix_matches_all_of_type() {
    let app = setup_for_search().await;
    // "BUG-" 는 prefix BUG 전체 (1건 — setup 에서 BUG 1개 만듦).
    let (_, body) = get(app, "/api/quests?search=BUG-").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["quest_id"].as_str().unwrap().starts_with("BUG-"));
}

#[tokio::test]
async fn test_list_search_slug_with_title_only_still_matches() {
    let app = setup_for_search().await;
    // title_only=true 여도 slug 는 매치 (slug 는 메타 정보).
    let (_, body) = get(app, "/api/quests?search=DEV-001&title_only=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["quest_id"], "DEV-001");
}

// === DEV-013: Quest history ===

#[tokio::test]
async fn test_history_empty_initially() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "h-empty", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    let (s, body) = get(app, &format!("/api/quests/{id}/history")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_history_change_status_recorded() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "h-status", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id}/status"),
        json!({ "status_slug": "in_progress" })).await;
    let (_, body) = get(app, &format!("/api/quests/{id}/history")).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["op"], "change_status");
    // DEV-042: slug 기반. migration 0001 seed: id 1=Open, id 2=In Progress.
    assert_eq!(arr[0]["old_value"], "open");
    assert_eq!(arr[0]["new_value"], "in_progress");
}

#[tokio::test]
async fn test_history_multiple_status_changes_ordered_desc() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "h-multi", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_slug": "in_progress" })).await;
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_slug": "done" })).await;
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_slug": "open" })).await;
    let (_, body) = get(app, &format!("/api/quests/{id}/history")).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    // 최신 → 과거. DEV-042: slug. migration 0001: 1=Open, 2=In Progress, 3=Done.
    assert_eq!(arr[0]["new_value"], "open");
    assert_eq!(arr[1]["new_value"], "done");
    assert_eq!(arr[2]["new_value"], "in_progress");
}

// === DEV-042: slug 기반 history ===

#[tokio::test]
async fn test_history_records_slugs_not_ids() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "slug-hist", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    // migration 0001 seed 의 id 5 = "On Hold" → slug "on_hold".
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_slug": "on_hold" })).await;
    let (_, body) = get(app, &format!("/api/quests/{id}/history")).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr[0]["old_value"], "open");
    assert_eq!(arr[0]["new_value"], "on_hold");
    // 숫자 문자열이 아니어야 함.
    assert!(arr[0]["new_value"].as_str().unwrap().parse::<i64>().is_err(),
        "new_value 가 숫자면 안 됨: {:?}", arr[0]["new_value"]);
}

/// DEV-113 후속: 사용자 보고("원격 길드 접속 시 제목이 안 보임") — 브라우저/
/// 원격(HTTP) 모드의 Nav 가 길드 이름을 가져오는 라우트.
#[tokio::test]
async fn test_guild_info_exposes_name() {
    let app = setup().await;
    let (status, body) = get(app, "/api/guild-info").await;
    assert_eq!(status, StatusCode::OK);
    let name = body["name"].as_str().expect("name must be string");
    assert!(!name.is_empty(), "guild name 이 비어있으면 안 됨");
}

#[tokio::test]
async fn test_status_endpoint_exposes_slug() {
    let app = setup().await;
    let (_, body) = get(app, "/api/quest-statuses").await;
    let arr = body.as_array().unwrap();
    // 모든 행이 slug 필드 + non-empty.
    for s in arr {
        let slug = s["slug"].as_str().expect("slug must be string");
        assert!(!slug.is_empty(), "empty slug in {s:?}");
    }
    // migration 0001 seed 의 5개 slug (name_en 에서 파생).
    let slugs: Vec<&str> = arr.iter().map(|s| s["slug"].as_str().unwrap()).collect();
    for expected in ["open", "in_progress", "done", "cancelled", "on_hold"] {
        assert!(slugs.contains(&expected), "missing slug {expected} in {slugs:?}");
    }
}

// === BUG-011: no-op status change 는 history 에 기록 안 함 ===

#[tokio::test]
async fn test_change_status_to_same_does_not_record_history() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "noop-status", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    let original_updated = q["updated_at"].as_str().unwrap().to_string();

    // 같은 상태 (1=Open) 로 재요청 — 변화 없어야 함.
    let (s, body) = patch(app.clone(), &format!("/api/quests/{id}/status"),
        json!({ "status_slug": "open" })).await;
    assert_eq!(s, StatusCode::OK);
    // updated_at 도 변경 없음.
    assert_eq!(body["updated_at"].as_str().unwrap(), original_updated,
        "no-op 시 updated_at 변경되면 안 됨");

    // history 0건.
    let (_, hist) = get(app, &format!("/api/quests/{id}/history")).await;
    assert_eq!(hist.as_array().unwrap().len(), 0,
        "no-op 시 history 가 기록되면 안 됨: {hist}");
}

#[tokio::test]
async fn test_change_status_actual_change_still_records() {
    // 회귀: BUG-011 수정 후 정상 변경은 여전히 기록되는지.
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "real-change", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_slug": "in_progress" })).await;
    let (_, hist) = get(app, &format!("/api/quests/{id}/history")).await;
    assert_eq!(hist.as_array().unwrap().len(), 1);
}

// === DEV-047: QuestDetail.parent 노출 ===

#[tokio::test]
async fn test_quest_detail_includes_parent_row() {
    let app = setup().await;
    let (_, parent) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "parent-q", "status_slug": "open" })).await;
    let parent_id = parent["id"].as_i64().unwrap();
    let (_, child) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "child-q", "status_slug": "open", "parent_quest_id": parent_id })).await;
    let child_id = child["id"].as_i64().unwrap();

    let (_, body) = get(app, &format!("/api/quests/{child_id}")).await;
    let parent_obj = body.get("parent").expect("parent field must exist");
    assert!(parent_obj.is_object(), "parent 는 객체여야 함: {parent_obj}");
    assert_eq!(parent_obj["title"], "parent-q");
    assert_eq!(parent_obj["id"], parent_id);
}

#[tokio::test]
async fn test_quest_detail_parent_omitted_or_null_for_root() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "root", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    let (_, body) = get(app, &format!("/api/quests/{id}")).await;
    // serde skip_serializing_if=Option::is_none — root 는 parent 키 없음 또는 null.
    let p = body.get("parent");
    assert!(p.is_none() || p.unwrap().is_null(),
        "root quest 의 parent 는 null/생략 이어야 함: {body}");
}

// === DEV-049: history/position 이 quest_slug 로 stable identifier ===

#[tokio::test]
async fn test_history_row_has_quest_slug() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "slug-hist", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    let slug = q["quest_id"].as_str().unwrap().to_string();
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_slug": "in_progress" })).await;
    // history 의 raw DB 확인은 안 되지만 fetch 한 응답에 quest_slug 가 있는지 확인.
    let (_, body) = get(app, &format!("/api/quests/{id}/history")).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    // QuestHistoryEntry 가 quest_slug 필드를 노출하는지.
    let qs = arr[0].get("quest_slug").and_then(|v| v.as_str());
    assert_eq!(qs, Some(slug.as_str()),
        "history row 의 quest_slug 가 quest 슬러그와 일치해야 함: {body}");
}

#[tokio::test]
async fn test_position_row_has_quest_slug() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "slug-pos", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    let slug = q["quest_id"].as_str().unwrap().to_string();
    // position 업데이트.
    let (s, _) = put(app.clone(), &format!("/api/quests/{id}/position"),
        json!({ "x": 100.0, "y": 200.0 })).await;
    assert_eq!(s, StatusCode::OK);
    // 전체 position 목록에서 해당 slug 의 위치 확인 — API 응답에 quest_slug 노출 여부.
    let (_, positions) = get(app, "/api/quest-positions").await;
    let arr = positions.as_array().unwrap();
    let found = arr.iter().find(|p| p.get("quest_slug").and_then(|v| v.as_str()) == Some(&slug));
    assert!(found.is_some(),
        "position 응답에 quest_slug 가 노출되어야 함: {positions}");
}

#[tokio::test]
async fn test_history_isolated_per_quest() {
    let app = setup().await;
    let (_, q1) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "h-iso-1", "status_slug": "open" })).await;
    let (_, q2) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "h-iso-2", "status_slug": "open" })).await;
    let id1 = q1["id"].as_i64().unwrap();
    let id2 = q2["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id1}/status"), json!({ "status_slug": "in_progress" })).await;
    let (_, body1) = get(app.clone(), &format!("/api/quests/{id1}/history")).await;
    let (_, body2) = get(app, &format!("/api/quests/{id2}/history")).await;
    assert_eq!(body1.as_array().unwrap().len(), 1);
    assert_eq!(body2.as_array().unwrap().len(), 0);
}

// === DEV-041: 타임스탬프 ISO 8601 + TZ offset 형식 ===

/// 새로 INSERT 된 행의 ts / created_at 가 `YYYY-MM-DDTHH:MM:SS±HH:MM` 패턴
/// (또는 Z) 인지 확인. 절대 시각 자체는 검증하지 않음 (실행 시점 의존).
fn assert_iso8601_with_tz(ts: &str, field: &str) {
    // 길이 20 (Z) 또는 25 (+HH:MM).
    let ok = (ts.len() == 20 && ts.ends_with('Z'))
        || (ts.len() == 25 && (ts.as_bytes()[19] == b'+' || ts.as_bytes()[19] == b'-'));
    assert!(ok, "{field} = {ts:?} — ISO 8601 with TZ marker 기대");
    assert_eq!(&ts[4..5], "-", "{field}: year-month sep");
    assert_eq!(&ts[7..8], "-", "{field}: month-day sep");
    assert_eq!(&ts[10..11], "T", "{field}: date-time sep");
}

#[tokio::test]
async fn test_create_quest_timestamps_have_tz_marker() {
    let app = setup().await;
    let (_, q) = post(app, "/api/quests",
        json!({ "quest_type_id": 1, "title": "ts-test", "status_slug": "open" })).await;
    let created = q["created_at"].as_str().expect("created_at must be string");
    let updated = q["updated_at"].as_str().expect("updated_at must be string");
    assert_iso8601_with_tz(created, "created_at");
    assert_iso8601_with_tz(updated, "updated_at");
}

#[tokio::test]
async fn test_history_ts_has_tz_marker() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "hist-ts", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_slug": "in_progress" })).await;
    let (_, body) = get(app, &format!("/api/quests/{id}/history")).await;
    let ts = body[0]["ts"].as_str().expect("ts must be string");
    assert_iso8601_with_tz(ts, "history.ts");
}

#[tokio::test]
async fn test_update_quest_bumps_updated_at_to_new_format() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "upd-ts", "status_slug": "open" })).await;
    let id = q["id"].as_i64().unwrap();
    let (_, updated) = patch(app, &format!("/api/quests/{id}"), json!({ "title": "upd-ts-new" })).await;
    assert_iso8601_with_tz(updated["updated_at"].as_str().unwrap(), "updated_at after PATCH");
}

#[tokio::test]
async fn test_list_no_parent_filter() {
    let app = setup_with_mixed_quests().await;
    // DEV-002 의 sub 로 DEV-003 만들기.
    let dev2: i64 = {
        let (_, b) = get(app.clone(), "/api/quests?type=DEV&sort=id").await;
        // sort=id default DESC, DEV-002 가 첫 (마지막 만든 BUG-001 제외).
        // 더 명확히: 'dev-done' title 찾기.
        b.as_array().unwrap()
            .iter()
            .find(|q| q["title"] == "dev-done")
            .unwrap()["id"]
            .as_i64()
            .unwrap()
    };
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "sub-of-dev2", "status_slug": "open", "parent_quest_id": dev2 })
    ).await;

    // ?no_parent=true → sub 제외, 원래 3개만.
    let (_, body) = get(app, "/api/quests?no_parent=true").await;
    assert_eq!(body.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_list_child_of_slug() {
    let app = setup_with_mixed_quests().await;
    let parent_id: i64 = {
        let (_, b) = get(app.clone(), "/api/quests").await;
        b.as_array().unwrap()
            .iter()
            .find(|q| q["title"] == "dev-open")
            .unwrap()["id"]
            .as_i64()
            .unwrap()
    };
    // dev-open 밑에 sub 추가.
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "sub-A", "status_slug": "open", "parent_quest_id": parent_id })
    ).await;
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "sub-B", "status_slug": "open", "parent_quest_id": parent_id })
    ).await;

    // dev-open 의 slug 는 DEV-001 (mixed setup 의 첫 quest).
    let (_, body) = get(app, "/api/quests?child_of=DEV-001").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_child_of_slug_not_found() {
    let app = setup_with_mixed_quests().await;
    let (s, _) = get(app, "/api/quests?child_of=DEV-999").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_child_of_and_no_parent_mutually_exclusive() {
    let app = setup_with_mixed_quests().await;
    let (s, _) = get(app, "/api/quests?child_of=DEV-001&no_parent=true").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_reverse_default_sort() {
    let app = setup_with_mixed_quests().await;
    // 기본 id DESC. reverse → id ASC.
    let (_, body) = get(app, "/api/quests?reverse=true").await;
    let arr = body.as_array().unwrap();
    // 첫 quest 가 가장 먼저 만든 dev-open (id 1).
    assert_eq!(arr[0]["title"], "dev-open");
}

#[tokio::test]
async fn test_list_reverse_urgency_sort() {
    let app = setup_with_mixed_quests().await;
    // sort=urgency default ASC (1 먼저). reverse → DESC (4 먼저).
    let (_, body) = get(app, "/api/quests?sort=urgency&reverse=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr[0]["urgency"], 4);
}

#[tokio::test]
async fn test_list_sort_status() {
    let app = setup_with_mixed_quests().await;
    let (s, _) = get(app, "/api/quests?sort=status").await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn test_list_sort_updated() {
    let app = setup_with_mixed_quests().await;
    let (s, _) = get(app, "/api/quests?sort=updated").await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn test_list_sort_created() {
    let app = setup_with_mixed_quests().await;
    let (s, _) = get(app, "/api/quests?sort=created").await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn test_list_sort_multi_csv() {
    let app = setup_with_mixed_quests().await;
    // urgency,id — urgency ASC 우선, 동일 urgency 내 id DESC tiebreaker.
    let (_, body) = get(app, "/api/quests?sort=urgency,id").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr[0]["quest_id"], "BUG-001"); // urgency=1
    assert_eq!(arr[1]["quest_id"], "DEV-001"); // urgency=2
    assert_eq!(arr[2]["quest_id"], "DEV-002"); // urgency=4
}

#[tokio::test]
async fn test_list_sort_multi_with_reverse() {
    let app = setup_with_mixed_quests().await;
    // urgency,id + reverse → urgency DESC, id ASC.
    let (_, body) = get(app, "/api/quests?sort=urgency,id&reverse=true").await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr[0]["urgency"], 4);
    assert_eq!(arr[2]["urgency"], 1);
}

#[tokio::test]
async fn test_list_sort_invalid_key_in_multi() {
    let app = setup_with_mixed_quests().await;
    let (s, _) = get(app, "/api/quests?sort=urgency,badkey").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_offset() {
    let app = setup_with_mixed_quests().await;
    // 3개 quest, offset 1 → 2개 반환.
    let (_, body) = get(app, "/api/quests?offset=1").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_offset_with_limit() {
    let app = setup_with_mixed_quests().await;
    let (_, body) = get(app, "/api/quests?limit=1&offset=1").await;
    assert_eq!(body.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_list_offset_negative_bad_request() {
    let app = setup_with_mixed_quests().await;
    let (s, _) = get(app, "/api/quests?offset=-1").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

/// 테스트 전용 minimal URL encoding (공백 → %20).
fn urlencode_test(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[tokio::test]
async fn test_get_quest_detail() {
    let app = setup().await;
    let (_, created) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "detail test", "status_slug": "open" }),
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
        json!({ "quest_type_id": 1, "title": "original", "status_slug": "open" }),
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
        json!({ "quest_type_id": 1, "title": "status test", "status_slug": "open" }),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let (status, body) = patch(
        app,
        &format!("/api/quests/{id}/status"),
        json!({ "status_slug": "in_progress" }),
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
        json!({ "quest_type_id": 1, "title": "to delete", "status_slug": "open" }),
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
    let (_, q1) = post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "q1", "status_slug": "open" })).await;
    let (_, q2) = post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "q2", "status_slug": "open" })).await;
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
        json!({ "quest_type_id": 1, "title": "parent", "status_slug": "open" }),
    )
    .await;
    let parent_id = parent["id"].as_i64().unwrap();

    // 서브퀘스트 2개 생성
    post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "sub 1", "status_slug": "open", "parent_quest_id": parent_id })).await;
    post(app.clone(), "/api/quests", json!({ "quest_type_id": 1, "title": "sub 2", "status_slug": "open", "parent_quest_id": parent_id })).await;

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
        json!({ "quest_type_id": 1, "title": "position test", "status_slug": "open" }),
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
        "status_slug": "open"
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
            "status_slug": "open",
            "parent_quest_id": 999
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("parent quest"));
}

// ═══════════════════ DEV-196: campaigns ═══════════════════

#[tokio::test]
async fn test_campaign_create_get_update_delete() {
    let app = setup().await;
    let (status, c) = post(
        app.clone(),
        "/api/campaigns",
        json!({ "title": "beta", "description": "d1" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let slug = c["campaign_slug"].as_str().unwrap().to_string();
    assert_eq!(slug, "C-001");

    let (status, detail) = get(app.clone(), &format!("/api/campaigns/{slug}")).await;
    assert_eq!(status, StatusCode::OK);
    // CampaignDetail.campaign 은 #[serde(flatten)] — 중첩 키 없이 최상위 필드.
    assert_eq!(detail["title"], "beta");

    let (status, updated) = patch(
        app.clone(),
        &format!("/api/campaigns/{slug}"),
        json!({ "title": "beta v2" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["title"], "beta v2");

    let status = delete(app.clone(), &format!("/api/campaigns/{slug}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // soft-delete 후 목록(alive 전용)에서 제외.
    let (_, list) = get(app, "/api/campaigns").await;
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_campaign_list_filter_by_status() {
    let app = setup().await;
    post(app.clone(), "/api/campaigns", json!({ "title": "c1" })).await;
    let (_, c2) = post(app.clone(), "/api/campaigns", json!({ "title": "c2" })).await;
    let slug2 = c2["campaign_slug"].as_str().unwrap();
    patch(
        app.clone(),
        &format!("/api/campaigns/{slug2}"),
        json!({ "status": "done" }),
    )
    .await;

    let (_, active) = get(app.clone(), "/api/campaigns?status=active").await;
    assert_eq!(active.as_array().unwrap().len(), 1);
    let (_, done) = get(app, "/api/campaigns?status=done").await;
    assert_eq!(done.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_campaign_link_unlink_quest() {
    let app = setup().await;
    post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "q1", "status_slug": "open" }),
    )
    .await;
    post(app.clone(), "/api/campaigns", json!({ "title": "camp" })).await;

    let status = {
        let (s, _) = post(
            app.clone(),
            "/api/campaigns/C-001/quests",
            json!({ "quest_slug": "DEV-001" }),
        )
        .await;
        s
    };
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, detail) = get(app.clone(), "/api/campaigns/C-001").await;
    assert_eq!(detail["linked_quests"].as_array().unwrap().len(), 1);
    assert_eq!(detail["quest_total"], 1);

    let status = delete(app.clone(), "/api/campaigns/C-001/quests/DEV-001").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, detail) = get(app, "/api/campaigns/C-001").await;
    assert_eq!(detail["linked_quests"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_campaign_checklist_add_set_remove() {
    let app = setup().await;
    post(app.clone(), "/api/campaigns", json!({ "title": "camp" })).await;

    let (status, item) = post(
        app.clone(),
        "/api/campaigns/C-001/checklist",
        json!({ "text": "item one" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(item["text"], "item one");
    assert_eq!(item["checked"], false);

    // ops::set/remove_checklist_by_index 는 1-based index (0 은 BadRequest).
    let status = {
        let (s, _) = patch(
            app.clone(),
            "/api/campaigns/C-001/checklist/1",
            json!({ "checked": true }),
        )
        .await;
        s
    };
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, detail) = get(app.clone(), "/api/campaigns/C-001").await;
    assert_eq!(detail["checklists"][0]["checked"], true);

    let status = delete(app.clone(), "/api/campaigns/C-001/checklist/1").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, detail) = get(app, "/api/campaigns/C-001").await;
    assert!(detail["checklists"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_campaign_checklist_set_without_checked_is_bad_request() {
    let app = setup().await;
    post(app.clone(), "/api/campaigns", json!({ "title": "camp" })).await;
    post(
        app.clone(),
        "/api/campaigns/C-001/checklist",
        json!({ "text": "item" }),
    )
    .await;
    let (status, _) = patch(app, "/api/campaigns/C-001/checklist/0", json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_campaign_summaries_active_and_upcoming() {
    let app = setup().await;
    post(app.clone(), "/api/campaigns", json!({ "title": "active-one" })).await;
    let (status, _) = get(app.clone(), "/api/campaigns/summaries/active").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(app, "/api/campaigns/summaries/upcoming?days=30").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_campaign_list_for_quest() {
    let app = setup().await;
    let (_, q) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "q1", "status_slug": "open" }),
    )
    .await;
    let qid = q["id"].as_i64().unwrap();
    post(app.clone(), "/api/campaigns", json!({ "title": "camp" })).await;
    post(
        app.clone(),
        "/api/campaigns/C-001/quests",
        json!({ "quest_slug": "DEV-001" }),
    )
    .await;
    let (status, list) = get(app, &format!("/api/quests/{qid}/campaigns")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_campaign_get_banner_image_404_when_none() {
    let app = setup().await;
    post(app.clone(), "/api/campaigns", json!({ "title": "camp" })).await;
    let status = app
        .oneshot(
            Request::get("/api/campaigns/C-001/image")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ═══════════════════ DEV-196: comments (quest) ═══════════════════

async fn seed_quest(app: Router) -> Router {
    post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "q1", "status_slug": "open" }),
    )
    .await;
    app
}

#[tokio::test]
async fn test_quest_comment_add_list_update_delete() {
    let app = seed_quest(setup().await).await;
    let (status, entry) = post(
        app.clone(),
        "/api/quests/by/DEV-001/comments",
        json!({ "author": "alice", "body": "hello" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = entry["id"].as_u64().unwrap();

    let (status, list) = get(app.clone(), "/api/quests/by/DEV-001/comments").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["entries"].as_array().unwrap().len(), 1);

    let (status, updated) = patch(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}"),
        json!({ "body": "edited" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["body"], "edited");

    let status = delete(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}"),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = get(app, "/api/quests/by/DEV-001/comments").await;
    assert!(list["entries"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_quest_comment_reply_threading() {
    let app = seed_quest(setup().await).await;
    let (_, root) = post(
        app.clone(),
        "/api/quests/by/DEV-001/comments",
        json!({ "author": "a", "body": "root" }),
    )
    .await;
    let root_id = root["id"].as_u64().unwrap();
    let (status, reply) = post(
        app.clone(),
        "/api/quests/by/DEV-001/comments",
        json!({ "author": "b", "body": "reply", "parent_id": root_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply["parent_id"], root_id);
}

#[tokio::test]
async fn test_quest_comment_reaction_toggle() {
    let app = seed_quest(setup().await).await;
    let (_, entry) = post(
        app.clone(),
        "/api/quests/by/DEV-001/comments",
        json!({ "author": "a", "body": "x" }),
    )
    .await;
    let id = entry["id"].as_u64().unwrap();

    let (status, after_add) = post(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}/reactions"),
        json!({ "emoji": "+1", "author": "bob" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after_add["reactions"].as_array().unwrap().len(), 1);

    // 같은 author+emoji 다시 → 토글 해제. reactions 는
    // skip_serializing_if = "Vec::is_empty" 라 비면 키 자체가 응답에서 빠진다.
    let (status, after_remove) = post(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}/reactions"),
        json!({ "emoji": "+1", "author": "bob" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(after_remove.get("reactions").is_none());
}

#[tokio::test]
async fn test_quest_comment_discussion_and_resolved_toggle() {
    let app = seed_quest(setup().await).await;
    let (_, entry) = post(
        app.clone(),
        "/api/quests/by/DEV-001/comments",
        json!({ "author": "a", "body": "needs discussion" }),
    )
    .await;
    let id = entry["id"].as_u64().unwrap();

    // discussion 아닌 상태에서 resolve 시도 → BadRequest.
    let (status, _) = post(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}/resolved"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, after_disc) = post(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}/discussion"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after_disc["discussion"], true);

    // DEV-142: 미해결 discussion 이면 done(counts_as_done) 전환 차단.
    let (status, _) = patch(
        app.clone(),
        "/api/quests/1/status",
        json!({ "status_slug": "done" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, after_resolved) = post(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}/resolved"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after_resolved["resolved"], true);

    // 해소 후엔 done 전환 가능.
    let (status, _) = patch(app, "/api/quests/1/status", json!({ "status_slug": "done" })).await;
    assert_eq!(status, StatusCode::OK);
}

/// DEV-234: 상단 고정(pin) 토글 — discussion 과 달리 quest 완료 전환 게이트 없음.
#[tokio::test]
async fn test_quest_comment_pinned_toggle() {
    let app = seed_quest(setup().await).await;
    let (_, entry) = post(
        app.clone(),
        "/api/quests/by/DEV-001/comments",
        json!({ "author": "a", "body": "결정사항" }),
    )
    .await;
    let id = entry["id"].as_u64().unwrap();

    let (status, on) = post(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}/pinned"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(on["pinned"], true);

    let (status, off) = post(
        app,
        &format!("/api/quests/by/DEV-001/comments/{id}/pinned"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // pinned=false 는 discussion/resolved 와 같은 skip_serializing_if 라 필드
    // 자체가 응답에서 생략됨(null) — "true 아님"으로 검증.
    assert!(!off["pinned"].as_bool().unwrap_or(false));
}

#[tokio::test]
async fn test_quest_memo_get_set() {
    let app = seed_quest(setup().await).await;
    let (status, empty) = get(app.clone(), "/api/quests/by/DEV-001/memo").await;
    assert_eq!(status, StatusCode::OK);
    assert!(empty["content"].is_null());

    let (status, set) = put(
        app.clone(),
        "/api/quests/by/DEV-001/memo",
        json!({ "content": "private note" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(set["content"], "private note");

    let (_, got) = get(app, "/api/quests/by/DEV-001/memo").await;
    assert_eq!(got["content"], "private note");
}

// ═══════════════════ DEV-196: comments (campaign) ═══════════════════

#[tokio::test]
async fn test_campaign_comment_add_list_update_delete() {
    let app = setup().await;
    post(app.clone(), "/api/campaigns", json!({ "title": "camp" })).await;

    let (status, entry) = post(
        app.clone(),
        "/api/campaigns/C-001/comments",
        json!({ "author": "alice", "body": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = entry["id"].as_u64().unwrap();

    let (_, list) = get(app.clone(), "/api/campaigns/C-001/comments").await;
    assert_eq!(list["entries"].as_array().unwrap().len(), 1);

    let (status, updated) = patch(
        app.clone(),
        &format!("/api/campaigns/C-001/comments/{id}"),
        json!({ "body": "edited" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["body"], "edited");

    let status = delete(app.clone(), &format!("/api/campaigns/C-001/comments/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = get(app, "/api/campaigns/C-001/comments").await;
    assert!(list["entries"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_campaign_comment_reaction_toggle() {
    let app = setup().await;
    post(app.clone(), "/api/campaigns", json!({ "title": "camp" })).await;
    let (_, entry) = post(
        app.clone(),
        "/api/campaigns/C-001/comments",
        json!({ "author": "a", "body": "x" }),
    )
    .await;
    let id = entry["id"].as_u64().unwrap();
    let (status, after) = post(
        app,
        &format!("/api/campaigns/C-001/comments/{id}/reactions"),
        json!({ "emoji": "check", "author": "carol" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["reactions"].as_array().unwrap().len(), 1);
}

/// DEV-234: 캠페인 댓글도 pin 지원 (discussion 과 달리 quest 전용 아님).
#[tokio::test]
async fn test_campaign_comment_pinned_toggle() {
    let app = setup().await;
    post(app.clone(), "/api/campaigns", json!({ "title": "camp" })).await;
    let (_, entry) = post(
        app.clone(),
        "/api/campaigns/C-001/comments",
        json!({ "author": "a", "body": "x" }),
    )
    .await;
    let id = entry["id"].as_u64().unwrap();
    let (status, on) = post(
        app,
        &format!("/api/campaigns/C-001/comments/{id}/pinned"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(on["pinned"], true);
}

#[tokio::test]
async fn test_campaign_memo_get_set() {
    let app = setup().await;
    post(app.clone(), "/api/campaigns", json!({ "title": "camp" })).await;
    let (_, empty) = get(app.clone(), "/api/campaigns/C-001/memo").await;
    assert!(empty["content"].is_null());
    let (status, set) = put(
        app.clone(),
        "/api/campaigns/C-001/memo",
        json!({ "content": "memo text" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, got) = get(app, "/api/campaigns/C-001/memo").await;
    assert_eq!(got["content"], set["content"]);
}

// ═══════════════════ DEV-196: rules ═══════════════════

#[tokio::test]
async fn test_rules_multi_file_crud() {
    let app = setup().await;
    let (status, created) = post(
        app.clone(),
        "/api/rules",
        json!({ "slug": "my-rule", "content": "# Rule" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["slug"], "my-rule");

    let (status, list) = get(app.clone(), "/api/rules").await;
    assert_eq!(status, StatusCode::OK);
    assert!(list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["slug"] == "my-rule"));

    let (status, got) = get(app.clone(), "/api/rules/my-rule").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["content"], "# Rule");

    let (status, updated) = put(
        app.clone(),
        "/api/rules/my-rule",
        json!({ "content": "# Rule v2" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["content"], "# Rule v2");

    let (status, renamed) = patch(
        app.clone(),
        "/api/rules/my-rule",
        json!({ "new_slug": "renamed-rule" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(renamed["slug"], "renamed-rule");

    let status = delete(app, "/api/rules/renamed-rule").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

/// DEV-243: 규칙 태그 — PUT /api/rules/{slug}/tags, 본문 저장은 보존.
#[tokio::test]
async fn test_rule_tags_set_and_preserved_on_body_save() {
    let app = setup().await;
    let (_, _) = post(
        app.clone(),
        "/api/rules",
        json!({ "slug": "tagged-rule", "content": "본문" }),
    )
    .await;

    let (status, updated) = put(
        app.clone(),
        "/api/rules/tagged-rule/tags",
        json!({ "tags": ["git", "convention"] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["tags"], json!(["git", "convention"]));

    let (status, got) = get(app.clone(), "/api/rules/tagged-rule").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["tags"], json!(["git", "convention"]));

    // 본문만 저장 — 태그는 그대로 남아야 함.
    let (status, saved) = put(
        app,
        "/api/rules/tagged-rule",
        json!({ "content": "새 본문" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["tags"], json!(["git", "convention"]), "본문 저장이 태그를 지우면 안 됨");
}

/// DEV-216: 도서관 CRUD — 생성/목록/조회/수정/soft delete + 번호 재사용 금지.
#[tokio::test]
async fn test_library_crud_and_number_monotonic() {
    let app = setup().await;

    let (status, b1) = post(
        app.clone(),
        "/api/library",
        json!({ "title": "설계 결정", "body": "본문 A" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(b1["book_id"], "BOOK-001");
    assert_eq!(b1["title"], "설계 결정");
    assert_eq!(b1["body"], "본문 A");

    let (status, _) = post(app.clone(), "/api/library", json!({ "title": "둘째" })).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, list) = get(app.clone(), "/api/library").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 2);

    let (status, got) = get(app.clone(), "/api/library/BOOK-001").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["body"], "본문 A");

    let (status, updated) = patch(
        app.clone(),
        "/api/library/BOOK-001",
        json!({ "title": "바뀐 제목" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["title"], "바뀐 제목");
    assert_eq!(updated["body"], "본문 A", "body 미지정 시 보존");

    let status = delete(app.clone(), "/api/library/BOOK-001").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = get(app.clone(), "/api/library/BOOK-001").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "soft delete 후 조회 제외");

    // 삭제된 번호 재사용 금지 — 카운터 단조 증가.
    let (_, b3) = post(app.clone(), "/api/library", json!({ "title": "셋째" })).await;
    assert_eq!(b3["book_id"], "BOOK-003");

    // 잘못된 id 형식.
    let (status, _) = get(app, "/api/library/DEV-001").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// DEV-237: 도서관 문서 첨부 — 이미지/동영상 외 임의 파일(zip)도 등록/삭제.
#[tokio::test]
async fn test_library_attachment_add_remove() {
    let app = setup().await;
    let (_, b1) = post(app.clone(), "/api/library", json!({ "title": "설계" })).await;
    let book_id = b1["book_id"].as_str().unwrap().to_string();

    let (status, list) = post(
        app.clone(),
        &format!("/api/library/{book_id}/attachments"),
        json!({ "path": "attachments/spec.zip", "name": "spec.zip" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "spec.zip");

    let (status, after) = delete_with_body(
        app,
        &format!("/api/library/{book_id}/attachments?path=attachments/spec.zip"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(after.as_array().unwrap().is_empty());
}

/// BUG-124(admin 보고): 첨부가 있는 문서를 본문/제목/폴더 수정(PATCH)하면
/// 응답의 attachments 가 빈 배열이라 GUI 가 book 객체를 통째로 교체하며
/// 화면에서 기존 첨부파일이 사라졌음 — update_book 도 get_book 처럼 항상
/// 채워야 한다.
#[tokio::test]
async fn test_library_update_preserves_attachments_in_response() {
    let app = setup().await;
    let (_, b1) = post(app.clone(), "/api/library", json!({ "title": "설계" })).await;
    let book_id = b1["book_id"].as_str().unwrap().to_string();

    let (_, list) = post(
        app.clone(),
        &format!("/api/library/{book_id}/attachments"),
        json!({ "path": "attachments/spec.zip", "name": "spec.zip" }),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let (status, updated) = patch(
        app.clone(),
        &format!("/api/library/{book_id}"),
        json!({ "body": "새 본문" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        updated["attachments"].as_array().unwrap().len(),
        1,
        "PATCH 응답도 get_book 처럼 attachments 를 채워야 함"
    );
    assert_eq!(updated["attachments"][0]["name"], "spec.zip");

    // list_books 는 여전히 payload 절약을 위해 빈 배열 유지(의도적, 회귀 아님).
    let (_, list_resp) = get(app.clone(), "/api/library").await;
    assert!(list_resp[0]["attachments"].as_array().unwrap().is_empty());
}

/// DEV-243: 도서관 문서 태그 — PATCH /api/library/{book_id}/tags, 본문 저장은 보존.
#[tokio::test]
async fn test_library_tags_set_and_preserved_on_update() {
    let app = setup().await;
    let (_, b1) = post(app.clone(), "/api/library", json!({ "title": "설계" })).await;
    let book_id = b1["book_id"].as_str().unwrap().to_string();
    assert!(b1["tags"].as_array().unwrap().is_empty());

    let (status, updated) = patch(
        app.clone(),
        &format!("/api/library/{book_id}/tags"),
        json!({ "tags": ["architecture", "decision"] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["tags"], json!(["architecture", "decision"]));

    // 본문만 수정 — 태그는 그대로 남아야 함.
    let (status, saved) = patch(
        app.clone(),
        &format!("/api/library/{book_id}"),
        json!({ "body": "새 본문" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        saved["tags"],
        json!(["architecture", "decision"]),
        "본문 저장이 태그를 지우면 안 됨"
    );

    let (_, got) = get(app.clone(), &format!("/api/library/{book_id}")).await;
    assert_eq!(got["tags"], json!(["architecture", "decision"]));
}

/// DEV-239: 도서관 폴더 — 생성/목록/삭제 + 문서 path 이동, 빈 폴더만 삭제 허용.
#[tokio::test]
async fn test_library_folders_and_doc_path() {
    let app = setup().await;

    let (status, folder) =
        post(app.clone(), "/api/library/folders", json!({ "path": "아키텍처" })).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(folder["path"], "아키텍처");

    let (status, list) = get(app.clone(), "/api/library/folders").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);

    let (status, b1) = post(
        app.clone(),
        "/api/library",
        json!({ "title": "라우터 설계", "path": "아키텍처" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(b1["path"], "아키텍처");

    // 문서가 있으면 폴더 삭제 거부.
    let status = delete(app.clone(), "/api/library/folders?path=아키텍처").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 루트로 이동.
    let (status, moved) = patch(
        app.clone(),
        &format!("/api/library/{}", b1["book_id"].as_str().unwrap()),
        json!({ "path": "" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(moved["path"], "");

    // 이제 비었으니 삭제 가능.
    let status = delete(app.clone(), "/api/library/folders?path=아키텍처").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

/// DEV-239: 잘못된 폴더 경로(`..`)는 400, 하위 폴더 있으면(문서 없어도) 삭제 거부.
#[tokio::test]
async fn test_library_folder_path_validation_and_subfolder_guard() {
    let app = setup().await;

    let (status, _) = post(
        app.clone(),
        "/api/library",
        json!({ "title": "잘못된 경로", "path": "아키텍처/.." }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) =
        post(app.clone(), "/api/library/folders", json!({ "path": "아키텍처" })).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) =
        post(app.clone(), "/api/library/folders", json!({ "path": "아키텍처/서브" })).await;
    assert_eq!(status, StatusCode::CREATED);

    let status = delete(app.clone(), "/api/library/folders?path=아키텍처").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let status = delete(app.clone(), "/api/library/folders?path=아키텍처/서브").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let status = delete(app, "/api/library/folders?path=아키텍처").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

/// DEV-167: worklog — 활동 타임라인/집계/히트맵 + 날짜별 노트 CRUD.
#[tokio::test]
async fn test_worklog_activities_and_note() {
    let app = setup().await;

    // quest 생성 + done 전환 → created/status 활동 발생.
    let (_, created) = post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "작업기록", "status_slug": "open" }),
    )
    .await;
    let qid = created["id"].as_i64().unwrap();
    let (status, _) = patch(
        app.clone(),
        &format!("/api/quests/{qid}/status"),
        json!({ "status_slug": "done" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let today = &openguild_core::time::now_local_iso8601()[..10];

    let (status, report) =
        get(app.clone(), &format!("/api/worklog?from={today}&to={today}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["counts"]["created"], 1);
    assert_eq!(report["counts"]["status_changes"], 1);
    assert_eq!(report["counts"]["done_transitions"], 1);
    assert!(!report["activities"].as_array().unwrap().is_empty());

    let (status, summary) =
        get(app.clone(), &format!("/api/worklog/summary?from={today}&to={today}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary.as_array().unwrap().len(), 1);
    assert_eq!(summary[0]["date"], today.to_string());

    // 노트 CRUD.
    let (status, note) = put(
        app.clone(),
        &format!("/api/worklog/note/{today}"),
        json!({ "content": "오늘 노트" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(note["content"], "오늘 노트");
    let (_, got) = get(app.clone(), &format!("/api/worklog/note/{today}")).await;
    assert_eq!(got["content"], "오늘 노트");
    let (_, notes) =
        get(app.clone(), &format!("/api/worklog/notes?from={today}&to={today}")).await;
    assert_eq!(notes.as_array().unwrap().len(), 1);
    // 빈 본문 = 삭제.
    let (_, cleared) = put(
        app.clone(),
        &format!("/api/worklog/note/{today}"),
        json!({ "content": "" }),
    )
    .await;
    assert!(cleared["content"].is_null());

    // 잘못된 날짜 형식.
    let (status, _) = get(app, "/api/worklog/note/evil..path").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// DEV-236: 토론 resolve 전환이 worklog 타임라인/집계에 나타남 (이전엔 journal
/// 감사로그에만 남아 안 보였던 설계 공백).
#[tokio::test]
async fn test_worklog_shows_discussion_resolve_events() {
    let app = seed_quest(setup().await).await;
    let (_, entry) = post(
        app.clone(),
        "/api/quests/by/DEV-001/comments",
        json!({ "author": "a", "body": "결정 필요" }),
    )
    .await;
    let id = entry["id"].as_u64().unwrap();
    post(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}/discussion"),
        json!({}),
    )
    .await;
    post(
        app.clone(),
        &format!("/api/quests/by/DEV-001/comments/{id}/resolved"),
        json!({}),
    )
    .await;

    let today = &openguild_core::time::now_local_iso8601()[..10];
    let (status, report) =
        get(app, &format!("/api/worklog?from={today}&to={today}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["counts"]["discussion_events"], 1);
    let activities = report["activities"].as_array().unwrap();
    let ev = activities
        .iter()
        .find(|a| a["kind"] == "discussion")
        .expect("discussion 활동이 있어야");
    assert!(ev["summary"].as_str().unwrap().contains("해결"));
}

#[tokio::test]
async fn test_rules_legacy_single_file() {
    let app = setup().await;
    let (status, set) = put(
        app.clone(),
        "/api/rules-single",
        json!({ "content": "legacy rules" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(set["content"], "legacy rules");
    let (_, got) = get(app, "/api/rules-single").await;
    assert_eq!(got["content"], "legacy rules");
}

// ═══════════════════ DEV-196: meta (tag-defs) ═══════════════════

#[tokio::test]
async fn test_tag_defs_upsert_list_delete() {
    let app = setup().await;
    let (status, created) = post(
        app.clone(),
        "/api/tag-defs",
        json!({ "slug": "urgent", "color": "#ff0000", "description": "기급" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["slug"], "urgent");

    let (status, list) = get(app.clone(), "/api/tag-defs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);

    // 같은 slug 다시 upsert → 갱신(개수 그대로).
    post(
        app.clone(),
        "/api/tag-defs",
        json!({ "slug": "urgent", "color": "#00ff00", "description": "기급(수정)" }),
    )
    .await;
    let (_, list2) = get(app.clone(), "/api/tag-defs").await;
    assert_eq!(list2.as_array().unwrap().len(), 1);
    assert_eq!(list2[0]["color"], "#00ff00");

    let status = delete(app.clone(), "/api/tag-defs/urgent").await;
    assert_eq!(status, StatusCode::OK);
    let (_, list3) = get(app, "/api/tag-defs").await;
    assert!(list3.as_array().unwrap().is_empty());
}

// ═══════════════════ DEV-196: admin ═══════════════════

#[tokio::test]
async fn test_admin_snapshot_create_list_delete() {
    let app = setup().await;
    let (status, info) = post(app.clone(), "/api/admin/snapshot", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let ts = info["timestamp"].as_str().unwrap().to_string();

    let (status, list) = get(app.clone(), "/api/admin/snapshots").await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().unwrap().iter().any(|s| s["timestamp"] == ts));

    let (status, _) = delete_with_body(app, &format!("/api/admin/snapshots/{ts}")).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_admin_restore_to_latest_snapshot() {
    let app = setup().await;
    let _ = seed_quest(app.clone()).await;
    post(app.clone(), "/api/admin/snapshot", json!({})).await;
    // snapshot 이후 변경.
    post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "after-snap", "status_slug": "open" }),
    )
    .await;
    let (_, before) = get(app.clone(), "/api/quests").await;
    assert_eq!(before.as_array().unwrap().len(), 2);

    let (status, _) = post(app.clone(), "/api/admin/restore", json!({})).await;
    assert_eq!(status, StatusCode::OK);

    let (_, after) = get(app, "/api/quests").await;
    assert_eq!(
        after.as_array().unwrap().len(),
        1,
        "restore 가 snapshot 이전 상태로 되돌려야"
    );
}

#[tokio::test]
async fn test_admin_restore_at_replays_journal_to_timestamp() {
    let app = setup().await;
    let _ = seed_quest(app.clone()).await; // DEV-001
    post(app.clone(), "/api/admin/snapshot", json!({})).await;
    post(
        app.clone(),
        "/api/quests",
        json!({ "quest_type_id": 1, "title": "after-snap", "status_slug": "open" }),
    )
    .await; // DEV-002, snapshot 이후 journal 에 기록됨.

    // 먼 미래 시점까지 replay → snapshot 이후 ops 모두 재적용.
    let (status, body) = post(
        app.clone(),
        "/api/admin/restore",
        json!({ "at": "9999-12-31T23:59:59Z" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["applied"], 1);
    // DEV-212: journal 이 있었으므로 실행 직전 자동 백업 스냅샷 ts 가 응답에 포함.
    assert!(
        body["pre_backup"].as_str().is_some(),
        "pre_backup 이 응답에 있어야 (DEV-212): {body}"
    );

    let (_, after) = get(app.clone(), "/api/quests").await;
    assert_eq!(
        after.as_array().unwrap().len(),
        2,
        "replay 가 DEV-002 생성을 재적용해야"
    );

    // DEV-212: pre_backup 스냅샷이 backup 목록에 실재.
    let pre = body["pre_backup"].as_str().unwrap().to_string();
    let (_, list) = get(app, "/api/admin/snapshots").await;
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .any(|s| s["timestamp"] == pre.as_str()),
        "자동 백업이 snapshot 목록에 존재해야 (DEV-212)"
    );
}

#[tokio::test]
async fn test_admin_drift_check() {
    let app = setup().await;
    let (status, report) = get(app, "/api/admin/drift").await;
    assert_eq!(status, StatusCode::OK);
    assert!(report.is_object());
}

#[tokio::test]
async fn test_admin_reindex() {
    let app = setup().await;
    let _ = seed_quest(app.clone()).await;
    let (status, report) = post(app, "/api/admin/reindex", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(report["quests_loaded"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn test_admin_vacuum() {
    let app = setup().await;
    let (status, _) = post(app, "/api/admin/vacuum", json!({})).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_admin_journal_tail() {
    let app = setup().await;
    let _ = seed_quest(app.clone()).await;
    let (status, tail) = get(app, "/api/admin/journal?count=10").await;
    assert_eq!(status, StatusCode::OK);
    assert!(tail.is_object());
}

// ═══════════════════ DEV-196: assets (guild-files) ═══════════════════

#[tokio::test]
async fn test_guild_files_rejects_disallowed_prefix() {
    let app = setup().await;
    let (status, _) = get(app, "/api/guild-files/quests/DEV-001.md").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_guild_files_rejects_path_traversal() {
    let app = setup().await;
    let (status, _) = get(app, "/api/guild-files/attachments/../rules.md").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_guild_files_not_found_for_missing_file() {
    let app = setup().await;
    let (status, _) = get(app, "/api/guild-files/attachments/nope.png").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
