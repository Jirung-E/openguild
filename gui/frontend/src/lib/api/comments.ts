// DEV-012 / DEV-094: Quest 댓글 (entry 단위, tracked) + 메모 (단일 텍스트, gitignored) API.

import { api } from './client';

export interface ContentResponse {
	content: string | null;
}

/** DEV-094: 한 댓글 entry. */
export interface CommentEntry {
	id: number;
	/** ISO 8601 + TZ (`2026-06-02T01:23:45+09:00`). legacy 단일-entry 의 경우 빈 문자열. */
	ts: string;
	/** 작성자 (자유 문자열, 빈 값 허용). */
	author: string;
	/** markdown 본문. */
	body: string;
	/**
	 * DEV-094 후속: 답글이면 부모 entry id. None (또는 미지정) = top-level.
	 * 1-level threading — 답글의 답글도 동일 root 의 직접 자식으로 flatten.
	 */
	parent_id?: number | null;
	/** DEV-108: 활성 이모지 반응 목록 (single-user — 이모지당 on/off). */
	reactions?: string[];
	/** DEV-142: 토론(discussion) 댓글 여부. true 면 resolve 전까지 완료 차단. */
	discussion?: boolean;
	/** DEV-142: 토론 해결 여부. discussion 이 아닐 땐 무의미. */
	resolved?: boolean;
	/** DEV-234: 상단 고정(pin) 여부. quest/campaign 댓글 둘 다 지원. */
	pinned?: boolean;
	/** DEV-182: 본문 편집 시각. 한 번도 수정 안 했으면 undefined. */
	edited_at?: string;
}

export interface CommentsListResponse {
	entries: CommentEntry[];
}

/**
 * DEV-100: quest / campaign 공용 댓글·메모 API 팩토리.
 * 엔드포인트 base 만 다르고 요청/응답 형식 동일.
 */
function makeCommentsApi(base: (slug: string) => string) {
	return {
		// ─── DEV-094: 댓글 entry CRUD ───
		listComments: (slug: string) => api.get<CommentsListResponse>(`${base(slug)}/comments`),
		addComment: (slug: string, body: string, author = '', parentId: number | null = null) =>
			api.post<CommentEntry>(`${base(slug)}/comments`, {
				author,
				body,
				parent_id: parentId
			}),
		updateComment: (slug: string, id: number, body: string) =>
			api.patch<CommentEntry>(`${base(slug)}/comments/${id}`, { body }),
		deleteComment: (slug: string, id: number) => api.delete(`${base(slug)}/comments/${id}`),
		// DEV-108: 이모지 반응 토글 — 갱신된 entry 반환.
		toggleReaction: (slug: string, id: number, emoji: string, author: string) =>
			api.post<CommentEntry>(`${base(slug)}/comments/${id}/reactions`, { emoji, author }),
		// DEV-142: 토론 플래그 / resolve 토글 — 갱신된 entry 반환.
		toggleDiscussion: (slug: string, id: number) =>
			api.post<CommentEntry>(`${base(slug)}/comments/${id}/discussion`, {}),
		toggleResolved: (slug: string, id: number) =>
			api.post<CommentEntry>(`${base(slug)}/comments/${id}/resolved`, {}),
		// DEV-234: 상단 고정(pin) 토글 — quest/campaign 둘 다 지원.
		togglePinned: (slug: string, id: number) =>
			api.post<CommentEntry>(`${base(slug)}/comments/${id}/pinned`, {}),

		// ─── DEV-012: 메모 (단일 텍스트) ───
		getMemo: (slug: string) => api.get<ContentResponse>(`${base(slug)}/memo`),
		setMemo: (slug: string, content: string) =>
			api.put<ContentResponse>(`${base(slug)}/memo`, { content })
	};
}

export type CommentsApi = ReturnType<typeof makeCommentsApi>;

export const commentsApi = makeCommentsApi((slug) => `/api/quests/by/${encodeURIComponent(slug)}`);
/** DEV-100: 캠페인 댓글 / 메모 — 경로는 기존 campaign 패턴. */
export const campaignCommentsApi = makeCommentsApi(
	(slug) => `/api/campaigns/${encodeURIComponent(slug)}`
);
