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

// ─────────────── DEV-027: quest list 필터 / 정렬 / limit ───────────────

/// quest 3종 (DEV-001 open / BUG-001 open / DEV-002 done) 미리 만들어 둠.
async fn setup_with_mixed_quests() -> Router {
    let app = setup().await;
    // DEV-001 (urgency 2 — high)
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "dev-open", "status_id": 1, "urgency": 2 })
    ).await;
    // DEV-002 (urgency 4 — low, 곧 done 으로 옮김)
    let (_, dev2) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "dev-done", "status_id": 1, "urgency": 4 })
    ).await;
    let dev2_id = dev2["id"].as_i64().unwrap();
    // status_id 3 = Done (migration 시드)
    patch(app.clone(), &format!("/api/quests/{dev2_id}/status"),
        json!({ "status_id": 3 })
    ).await;
    // BUG-001 (urgency 1 — critical)
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 2, "title": "bug-open", "status_id": 1, "urgency": 1 })
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
        json!({ "quest_type_id": 1, "title": "wip", "status_id": 1 })
    ).await;
    let id = created["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id}/status"),
        json!({ "status_id": 2 })  // In Progress
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
        json!({ "quest_type_id": 1, "title": "recent", "status_id": 1 })).await;
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

/// "2026-05-22T13:41:10+09:00" 같은 입력에서 분 부분만 -1.
/// 매우 간단한 변환 — 초과 / 일자 경계는 무시 (이 테스트 시나리오에선 안 발생).
fn subtract_one_minute(s: &str) -> String {
    // 분은 14..16 위치.
    let mut chars: Vec<char> = s.chars().collect();
    let minute_str: String = chars[14..16].iter().collect();
    let mut m: i32 = minute_str.parse().unwrap_or(0);
    m -= 1;
    if m < 0 {
        m += 60;
        // 시간을 -1 — 시 위치 11..13.
        let hour_str: String = chars[11..13].iter().collect();
        let mut h: i32 = hour_str.parse().unwrap_or(0);
        h -= 1;
        if h < 0 { h += 24; }
        let h_s = format!("{h:02}");
        chars[11] = h_s.chars().next().unwrap();
        chars[12] = h_s.chars().nth(1).unwrap();
    }
    let m_s = format!("{m:02}");
    chars[14] = m_s.chars().next().unwrap();
    chars[15] = m_s.chars().nth(1).unwrap();
    chars.iter().collect()
}

fn url_encode(s: &str) -> String {
    // 최소: `:` `+` 만 인코딩 (queryString 에서 `+` 는 공백으로 해석되므로).
    s.replace('+', "%2B").replace(':', "%3A")
}

#[tokio::test]
async fn test_list_created_after_with_naked_iso_uses_local_tz() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "naked-tz", "status_id": 1 })).await;
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
        json!({ "quest_type_id": 1, "title": "q1-leaf-parent", "status_id": 1 })).await;
    let q1_id = q1["id"].as_i64().unwrap();
    let (_, _q2) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "q2-child", "status_id": 1, "parent_quest_id": q1_id })).await;
    let (_, q3) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "q3-has-prereq", "status_id": 1 })).await;
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
                "description": "Rust 측 commands.rs 작성", "status_id": 1 })).await;
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "Frontend transport adapter",
                "description": "HTTP / Tauri 자동 분기", "status_id": 1 })).await;
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 2, "title": "Quest list 검색",
                "description": "title / description 부분 일치", "status_id": 1 })).await;
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
        json!({ "quest_type_id": 1, "title": "h-empty", "status_id": 1 })).await;
    let id = q["id"].as_i64().unwrap();
    let (s, body) = get(app, &format!("/api/quests/{id}/history")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_history_change_status_recorded() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "h-status", "status_id": 1 })).await;
    let id = q["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id}/status"),
        json!({ "status_id": 2 })).await;
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
        json!({ "quest_type_id": 1, "title": "h-multi", "status_id": 1 })).await;
    let id = q["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_id": 2 })).await;
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_id": 3 })).await;
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_id": 1 })).await;
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
        json!({ "quest_type_id": 1, "title": "slug-hist", "status_id": 1 })).await;
    let id = q["id"].as_i64().unwrap();
    // migration 0001 seed 의 id 5 = "On Hold" → slug "on_hold".
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_id": 5 })).await;
    let (_, body) = get(app, &format!("/api/quests/{id}/history")).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr[0]["old_value"], "open");
    assert_eq!(arr[0]["new_value"], "on_hold");
    // 숫자 문자열이 아니어야 함.
    assert!(arr[0]["new_value"].as_str().unwrap().parse::<i64>().is_err(),
        "new_value 가 숫자면 안 됨: {:?}", arr[0]["new_value"]);
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

#[tokio::test]
async fn test_history_isolated_per_quest() {
    let app = setup().await;
    let (_, q1) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "h-iso-1", "status_id": 1 })).await;
    let (_, q2) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "h-iso-2", "status_id": 1 })).await;
    let id1 = q1["id"].as_i64().unwrap();
    let id2 = q2["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id1}/status"), json!({ "status_id": 2 })).await;
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
        json!({ "quest_type_id": 1, "title": "ts-test", "status_id": 1 })).await;
    let created = q["created_at"].as_str().expect("created_at must be string");
    let updated = q["updated_at"].as_str().expect("updated_at must be string");
    assert_iso8601_with_tz(created, "created_at");
    assert_iso8601_with_tz(updated, "updated_at");
}

#[tokio::test]
async fn test_history_ts_has_tz_marker() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "hist-ts", "status_id": 1 })).await;
    let id = q["id"].as_i64().unwrap();
    patch(app.clone(), &format!("/api/quests/{id}/status"), json!({ "status_id": 2 })).await;
    let (_, body) = get(app, &format!("/api/quests/{id}/history")).await;
    let ts = body[0]["ts"].as_str().expect("ts must be string");
    assert_iso8601_with_tz(ts, "history.ts");
}

#[tokio::test]
async fn test_update_quest_bumps_updated_at_to_new_format() {
    let app = setup().await;
    let (_, q) = post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "upd-ts", "status_id": 1 })).await;
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
        json!({ "quest_type_id": 1, "title": "sub-of-dev2", "status_id": 1, "parent_quest_id": dev2 })
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
        json!({ "quest_type_id": 1, "title": "sub-A", "status_id": 1, "parent_quest_id": parent_id })
    ).await;
    post(app.clone(), "/api/quests",
        json!({ "quest_type_id": 1, "title": "sub-B", "status_id": 1, "parent_quest_id": parent_id })
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
