pub mod admin;
pub mod attachments;
pub mod campaigns;
pub mod comments;
pub mod library;
pub mod meta;
pub mod quests;
pub mod rules;

use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};
use openguild_core::Store;

pub fn create_router(store: Store) -> Router {
    Router::new()
        .route("/health", get(health))
        // meta
        .route("/api/guild-info", get(meta::get_guild_info))
        .route("/api/quest-types", get(meta::list_quest_types))
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
        .route("/api/attachments", post(attachments::save_attachment))
        .route(
            "/api/quests/by/{slug}/attachments",
            post(attachments::add_quest_attachment).delete(attachments::remove_quest_attachment),
        )
        .route(
            "/api/campaigns/{slug}/attachments",
            post(attachments::add_campaign_attachment)
                .delete(attachments::remove_campaign_attachment),
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
        .with_state(store)
}

async fn health() -> &'static str {
    "ok"
}
