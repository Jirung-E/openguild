pub mod admin;
pub mod attachments;
pub mod campaigns;
pub mod comments;
pub mod library;
pub mod meta;
pub mod quests;
pub mod rules;
pub mod templates;
pub mod worklog;

use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post, put},
    Router,
};
use tower_http::compression::{
    predicate::{DefaultPredicate, NotForContentType, Predicate},
    CompressionLayer,
};
use openguild_core::Store;

/// DEV-332: 응답 gzip 압축 레이어.
///
/// 우리 응답은 대부분 한글 JSON / 마크다운이고 압축을 전혀 하지 않고 있었다
/// (`content-encoding` 헤더 없음). 원격 접속(폰 등)에서 체감이 큰 부분이라
/// 붙인다. 정적 자산(_app/*.js)도 같이 이득을 본다.
///
/// **압축 제외 대상**: 이미 압축된 바이트를 다시 압축하면 CPU 만 쓰고 크기는
/// 오히려 늘 수 있다. `DefaultPredicate` 가 32바이트 미만 / gRPC / `image/*` /
/// SSE 를 걸러주므로, 여기에 첨부 다운로드에서 나오는 타입들을 더한다
/// (`/api/guild-files/{*rel}` 는 확장자를 못 알아보면 octet-stream 으로 준다 —
/// zip·동영상 첨부가 그 경로로 나간다. BUG-188 의 1.5GB 첨부를 생각하면
/// 스트림을 통째로 압축하는 일은 반드시 피해야 한다).
pub fn compression_layer() -> CompressionLayer<impl Predicate> {
    CompressionLayer::new().compress_when(compression_predicate())
}

/// 위 레이어가 쓰는 판정만 따로 — 테스트에서 content-type 별로 직접 확인한다.
pub fn compression_predicate() -> impl Predicate {
    DefaultPredicate::new()
        .and(NotForContentType::const_new("application/octet-stream"))
        .and(NotForContentType::const_new("application/zip"))
        .and(NotForContentType::const_new("video/"))
        .and(NotForContentType::const_new("audio/"))
}

pub fn create_router(store: Store) -> Router {
    Router::new()
        .route("/health", get(health))
        // meta
        .route("/api/guild-info", get(meta::get_guild_info))
        .route("/api/quest-types", get(meta::list_quest_types))
        // REQ-009: 강화된 검색 (댓글/첨부 이름/메모까지). 기본 검색과 별개 경로.
        .route("/api/search", get(meta::enhanced_search))
        // REQ-008: 이 문서를 참조하는 문서 (cross-link backlink).
        .route("/api/backlinks/{kind}/{id}", get(meta::list_backlinks))
        .route("/api/quest-statuses", get(meta::list_quest_statuses))
        // DEV-193: admin types/statuses CRUD — Tauri invoke(admin_* commands)
        // 와 HTTP 파리티. transport.ts 의 routeToInvoke 매핑이 이 경로를 그대로 씀.
        .route(
            "/api/admin/types",
            get(meta::admin_list_types).post(meta::admin_create_type),
        )
        .route(
            "/api/admin/types/{prefix}",
            patch(meta::admin_update_type).delete(meta::admin_delete_type),
        )
        .route(
            "/api/admin/statuses",
            get(meta::admin_list_statuses).post(meta::admin_create_status),
        )
        .route(
            "/api/admin/statuses/{slug}",
            patch(meta::admin_update_status).delete(meta::admin_delete_status),
        )
        // DEV-068: tag defs — `.guild/tags/{slug}.toml` 진리원.
        .route(
            "/api/tag-defs",
            get(meta::list_tag_defs).post(meta::upsert_tag_def),
        )
        .route("/api/tag-defs/{slug}", delete(meta::delete_tag_def))
        .route("/api/tags/used", get(meta::list_tags_in_use))
        // BUG-231: CLI template list/show/new 및 quest new --template 원격 파리티.
        .route(
            "/api/templates",
            get(templates::list_templates).post(templates::save_template),
        )
        .route("/api/templates/{name}", get(templates::get_template))
        .route("/api/comments", get(comments::search_comments))
        // DEV-016 (multi-file): 길드 규칙 — `.guild/rules/{slug}.md`.
        // 단일 (legacy) endpoint 도 backward compat 으로 다른 경로에 유지.
        .route(
            "/api/rules",
            get(rules::list_rules).post(rules::create_rule),
        )
        .route(
            "/api/rules/{slug}",
            get(rules::get_rule)
                .put(rules::set_rule)
                .patch(rules::rename_rule)
                .delete(rules::delete_rule),
        )
        // DEV-243: 규칙 태그 전체 교체.
        .route("/api/rules/{slug}/tags", put(rules::set_tags))
        // DEV-290: 규칙 변경 이력.
        .route("/api/rules/{slug}/history", get(rules::list_history))
        // DEV-016 legacy 단일 파일 — 기존 호출자 호환.
        .route(
            "/api/rules-single",
            get(rules::get_rules).put(rules::set_rules),
        )
        // DEV-216: 도서관 — `.guild/library/{BOOK-NNN}.md`.
        .route(
            "/api/library",
            get(library::list_books).post(library::create_book),
        )
        .route(
            "/api/library/{book_id}",
            get(library::get_book)
                .patch(library::update_book)
                .delete(library::delete_book),
        )
        // DEV-243: 도서관 문서 태그 전체 교체.
        .route("/api/library/{book_id}/tags", patch(library::set_tags))
        // DEV-290: 도서관 문서 변경 이력.
        .route("/api/library/{book_id}/history", get(library::list_history))
        // DEV-239: 도서관 폴더(계층) — `.guild/library/folders.toml`.
        .route(
            "/api/library/folders",
            get(library::list_folders)
                .post(library::create_folder)
                .delete(library::delete_folder),
        )
        // DEV-167: 작업 기록 — 활동 타임라인 / 히트맵 집계 / 날짜별 노트.
        .route("/api/worklog", get(worklog::get_activities))
        .route("/api/worklog/summary", get(worklog::get_summary))
        .route("/api/worklog/notes", get(worklog::list_notes))
        .route(
            "/api/worklog/note/{date}",
            get(worklog::get_note).put(worklog::set_note),
        )
        // DEV-012 / DEV-094: Quest 댓글 (entry 단위, tracked) + 메모 (단일, gitignored).
        // GET = entries 목록, POST = 새 entry 추가.
        .route(
            "/api/quests/by/{slug}/comments",
            get(comments::list_comments).post(comments::add_comment),
        )
        .route(
            "/api/quests/by/{slug}/comments/{id}",
            patch(comments::update_comment).delete(comments::delete_comment),
        )
        // DEV-108: 이모지 반응 토글.
        .route(
            "/api/quests/by/{slug}/comments/{id}/reactions",
            post(comments::toggle_reaction),
        )
        // DEV-142: 토론 플래그 / resolve 토글.
        .route(
            "/api/quests/by/{slug}/comments/{id}/discussion",
            post(comments::toggle_discussion),
        )
        .route(
            "/api/quests/by/{slug}/comments/{id}/resolved",
            post(comments::toggle_resolved),
        )
        // DEV-234: 상단 고정(pin) 토글 — quest 전용 게이트 없음.
        .route(
            "/api/quests/by/{slug}/comments/{id}/pinned",
            post(comments::toggle_pinned),
        )
        .route(
            "/api/quests/by/{slug}/memo",
            get(comments::get_memo).put(comments::set_memo),
        )
        // DEV-100: Campaign 댓글 / 메모 — 응답 형식은 quest 와 동일,
        // 경로는 기존 campaign 라우트 패턴 (`/api/campaigns/{slug}/...`) 따름.
        .route(
            "/api/campaigns/{slug}/comments",
            get(comments::camp_list_comments).post(comments::camp_add_comment),
        )
        .route(
            "/api/campaigns/{slug}/comments/{id}",
            patch(comments::camp_update_comment).delete(comments::camp_delete_comment),
        )
        .route(
            "/api/campaigns/{slug}/comments/{id}/reactions",
            post(comments::camp_toggle_reaction),
        )
        // DEV-234: 상단 고정(pin) 토글.
        .route(
            "/api/campaigns/{slug}/comments/{id}/pinned",
            post(comments::camp_toggle_pinned),
        )
        .route(
            "/api/campaigns/{slug}/memo",
            get(comments::camp_get_memo).put(comments::camp_set_memo),
        )
        // DEV-087: 배너 이미지 bytes (브라우저 모드 표시).
        .route(
            "/api/campaigns/{slug}/image",
            get(campaigns::get_banner_image),
        )
        // DEV-069: 본문 첨부 / 자산 — attachments/ + assets/ 한정 서빙.
        .route("/api/guild-files/{*rel}", get(admin::get_guild_file))
        // DEV-152: 첨부 업로드(remote 모드) — bytes 저장 + quest/campaign 목록 등록.
        // BUG-168: bytes 를 받는 유일한 라우트 — axum 기본 body limit(2 MiB)은
        // base64 팽창까지 감안하면 원본 1.5 MB 에서 413 이 난다. 이 라우트에만
        // 한도를 올리고 나머지는 기본값을 유지한다.
        // DEV-337: 스트리밍 업로드 — base64 왕복 없이 원문 body 를 파일로 흘려쓴다.
        // DefaultBodyLimit::disable() — 크기 상한 없음(메모리는 상수).
        .route(
            "/api/attachments/stream",
            post(attachments::save_attachment_stream).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/api/attachments",
            post(attachments::save_attachment)
                .layer(DefaultBodyLimit::max(attachments::MAX_ATTACHMENT_BODY_BYTES)),
        )
        .route(
            "/api/quests/by/{slug}/attachments",
            post(attachments::add_quest_attachment).delete(attachments::remove_quest_attachment),
        )
        .route(
            "/api/campaigns/{slug}/attachments",
            post(attachments::add_campaign_attachment)
                .delete(attachments::remove_campaign_attachment),
        )
        // DEV-237: 도서관 문서 첨부 — 이미지/동영상 외 임의 파일.
        .route(
            "/api/library/{book_id}/attachments",
            post(attachments::add_book_attachment).delete(attachments::remove_book_attachment),
        )
        // BUG-241: 첨부 일괄 다운로드 — zip 스트리밍. 폰에서 `http://<LAN IP>` 로
        // 접속하면 보안 컨텍스트가 아니라 폴더 쓰기(File System Access)를 쓸 수
        // 없고, 모바일 브라우저는 여러 파일 자동 다운로드도 막는다. 다운로드를
        // 1건으로 만드는 이 경로가 그 환경의 유일한 방법이다.
        .route(
            "/api/quests/by/{slug}/attachments.zip",
            get(attachments::quest_attachments_zip),
        )
        .route(
            "/api/campaigns/{slug}/attachments.zip",
            get(attachments::campaign_attachments_zip),
        )
        .route(
            "/api/library/{book_id}/attachments.zip",
            get(attachments::book_attachments_zip),
        )
        // quests
        .route("/api/quests", get(quests::list_quests).post(quests::create_quest))
        .route(
            "/api/quests/{id}",
            get(quests::get_quest)
                .patch(quests::update_quest)
                .delete(quests::delete_quest),
        )
        .route("/api/quests/{id}/status", patch(quests::change_status))
        .route("/api/quests/{id}/parent", patch(quests::change_parent))
        // DEV-076: 희망 / 필수 기한 설정 / 해제.
        .route("/api/quests/{id}/due", patch(quests::set_due_dates))
        // DEV-068: 태그 전체 교체. body: { "tags": [...] }
        .route("/api/quests/{id}/tags", patch(quests::set_tags))
        .route("/api/quests/{id}/restore", patch(quests::restore_quest))
        .route("/api/quests/{id}/candidates", get(quests::list_candidates))
        .route("/api/quests/{id}/prerequisites", post(quests::add_prerequisite))
        .route(
            "/api/quests/{id}/prerequisites/{prereq_id}",
            delete(quests::remove_prerequisite),
        )
        .route("/api/quests/{id}/position", put(quests::update_position))
        .route("/api/quests/{id}/history", get(quests::list_history))
        .route("/api/quests/by/{slug}", get(quests::get_quest_by_slug))
        // DEV-011: quest 가 속한 campaigns 목록 — Quest Detail UI 의 Campaigns 섹션.
        .route("/api/quests/{id}/campaigns", get(campaigns::list_for_quest))
        .route("/api/quest-positions", get(quests::list_positions))
        .route("/api/quest-dependencies", get(quests::list_dependencies))
        .route("/api/deleted-quests", get(quests::list_deleted_quests))
        // campaigns (DEV-011)
        .route(
            "/api/campaigns",
            get(campaigns::list_campaigns).post(campaigns::create_campaign),
        )
        // 목록 화면용 — 전체(진행도 포함). `/active` 보다 먼저 둘 필요는 없지만
        // 같은 접두를 쓰므로 함께 모아 둔다.
        .route("/api/campaigns/summaries", get(campaigns::list_all_summaries))
        .route("/api/campaigns/summaries/active", get(campaigns::list_active_summaries))
        .route(
            "/api/campaigns/summaries/upcoming",
            get(campaigns::list_upcoming_summaries),
        )
        .route(
            "/api/campaigns/{slug}",
            get(campaigns::get_campaign)
                .patch(campaigns::update_campaign)
                .delete(campaigns::delete_campaign),
        )
        // DEV-226: 캠페인 변경 이력 — quest history 와 대칭.
        .route("/api/campaigns/{slug}/history", get(campaigns::list_history))
        .route("/api/campaigns/{slug}/quests", post(campaigns::link_quest))
        .route(
            "/api/campaigns/{slug}/quests/{quest_slug}",
            delete(campaigns::unlink_quest),
        )
        .route("/api/campaigns/{slug}/checklist", post(campaigns::add_checklist))
        .route(
            "/api/campaigns/{slug}/checklist/{index}",
            patch(campaigns::set_checklist).delete(campaigns::remove_checklist),
        )
        // admin
        .route("/api/admin/snapshot", post(admin::create_snapshot))
        .route("/api/admin/snapshots", get(admin::list_snapshots))
        .route("/api/admin/snapshots/{ts}", delete(admin::delete_snapshot))
        .route("/api/admin/restore", post(admin::restore))
        .route("/api/admin/drift", get(admin::check_drift))
        .route("/api/admin/reindex", post(admin::run_reindex))
        // DEV-162: 런타임 정비 — vacuum / journal tail.
        .route("/api/admin/vacuum", post(admin::vacuum))
        .route("/api/admin/journal", get(admin::journal_tail))
        .route("/api/admin/counters", post(admin::check_counters))
        .route("/api/admin/info", get(admin::info))
        .with_state(store)
}

async fn health() -> &'static str {
    "ok"
}
