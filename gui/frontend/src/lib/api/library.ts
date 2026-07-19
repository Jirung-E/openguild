// DEV-217: 도서관 API. `.guild/library/{BOOK-NNN}.md` — 자체 BOOK 번호,
// quest 번호와 별개 (단조 증가, 재사용 금지). server routes/library.rs 와 1:1.
// DEV-239: 폴더(계층) — path 필드 + 별도 folders 엔드포인트.
// DEV-237: 첨부(이미지/동영상 외 임의 파일) — quest/campaign 과 동일 sidecar 패턴.

import { api } from './client';
import type { QuestAttachment } from '$lib/types';

export interface Book {
	book_id: string;
	number: number;
	title: string;
	body: string;
	/** 소속 폴더 경로 ("" = 최상위). */
	path: string;
	created_at: string;
	updated_at: string;
	deleted_at: string | null;
	/** DEV-237: get() 에서만 채움 — list() 는 빈 배열. */
	attachments: QuestAttachment[];
	/** DEV-243: 자유 태그. */
	tags: string[];
}

export interface LibraryFolder {
	path: string;
	created_at: string;
	updated_at: string;
}

export const libraryApi = {
	list: () => api.get<Book[]>('/api/library'),
	get: (bookId: string) => api.get<Book>(`/api/library/${encodeURIComponent(bookId)}`),
	create: (title: string, body = '', path = '') =>
		api.post<Book>('/api/library', { title, body, path }),
	update: (bookId: string, fields: { title?: string; body?: string; path?: string }) =>
		api.patch<Book>(`/api/library/${encodeURIComponent(bookId)}`, fields),
	delete: (bookId: string) => api.delete(`/api/library/${encodeURIComponent(bookId)}`),

	// DEV-243: 태그 전체 교체.
	setTags: (bookId: string, tags: string[]) =>
		api.patch<Book>(`/api/library/${encodeURIComponent(bookId)}/tags`, { tags }),

	// DEV-237: 첨부 — quests/campaigns 의 attachToSection 과 동일 시맨틱.
	addAttachment: (bookId: string, path: string, name: string) =>
		api.post<QuestAttachment[]>(`/api/library/${encodeURIComponent(bookId)}/attachments`, {
			path,
			name
		}),
	removeAttachment: (bookId: string, path: string) =>
		api.delete<QuestAttachment[]>(
			`/api/library/${encodeURIComponent(bookId)}/attachments?path=${encodeURIComponent(path)}`
		),

	folders: {
		list: () => api.get<LibraryFolder[]>('/api/library/folders'),
		create: (path: string) => api.post<LibraryFolder>('/api/library/folders', { path }),
		delete: (path: string) =>
			api.delete(`/api/library/folders?path=${encodeURIComponent(path)}`)
	}
};
