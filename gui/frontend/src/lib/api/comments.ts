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
}

export interface CommentsListResponse {
	entries: CommentEntry[];
}

export const commentsApi = {
	// ─── DEV-094: 댓글 entry CRUD ───
	listComments: (slug: string) =>
		api.get<CommentsListResponse>(
			`/api/quests/by/${encodeURIComponent(slug)}/comments`
		),
	addComment: (slug: string, body: string, author = '', parentId: number | null = null) =>
		api.post<CommentEntry>(
			`/api/quests/by/${encodeURIComponent(slug)}/comments`,
			{ author, body, parent_id: parentId }
		),
	updateComment: (slug: string, id: number, body: string) =>
		api.patch<CommentEntry>(
			`/api/quests/by/${encodeURIComponent(slug)}/comments/${id}`,
			{ body }
		),
	deleteComment: (slug: string, id: number) =>
		api.delete(`/api/quests/by/${encodeURIComponent(slug)}/comments/${id}`),
	// DEV-108: 이모지 반응 토글 — 갱신된 entry 반환.
	toggleReaction: (slug: string, id: number, emoji: string) =>
		api.post<CommentEntry>(
			`/api/quests/by/${encodeURIComponent(slug)}/comments/${id}/reactions`,
			{ emoji }
		),

	// ─── DEV-012: 메모 (단일 텍스트) ───
	getMemo: (slug: string) =>
		api.get<ContentResponse>(`/api/quests/by/${encodeURIComponent(slug)}/memo`),
	setMemo: (slug: string, content: string) =>
		api.put<ContentResponse>(`/api/quests/by/${encodeURIComponent(slug)}/memo`, {
			content
		})
};
