// DEV-012: Quest 별 댓글 (공개, tracked) / 메모 (비공개, gitignored) API.
// .guild/quests/{slug}.{comments,memo}.md 의 read / write.

import { api } from './client';

export interface ContentResponse {
	content: string | null;
}

export const commentsApi = {
	getComments: (slug: string) =>
		api.get<ContentResponse>(`/api/quests/by/${encodeURIComponent(slug)}/comments`),
	setComments: (slug: string, content: string) =>
		api.put<ContentResponse>(`/api/quests/by/${encodeURIComponent(slug)}/comments`, {
			content
		}),
	getMemo: (slug: string) =>
		api.get<ContentResponse>(`/api/quests/by/${encodeURIComponent(slug)}/memo`),
	setMemo: (slug: string, content: string) =>
		api.put<ContentResponse>(`/api/quests/by/${encodeURIComponent(slug)}/memo`, {
			content
		})
};
