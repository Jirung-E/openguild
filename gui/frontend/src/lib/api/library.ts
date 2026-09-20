// DEV-217: 도서관 API. `.guild/library/{BOOK-NNN}.md` — 자체 BOOK 번호,
// quest 번호와 별개 (단조 증가, 재사용 금지). server routes/library.rs 와 1:1.
// DEV-239: 폴더(계층) — path 필드 + 별도 folders 엔드포인트.
// DEV-237: 첨부(이미지/동영상 외 임의 파일) — quest/campaign 과 동일 sidecar 패턴.

import { api } from './client';
import type { QuestAttachment, SidecarHistoryEntry } from '$lib/types';
import type { TagEdit } from './quests';

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

/** REQ-028: 내보낸 파일 하나. */
export interface LibraryExported {
	book_id: string;
	/** 내보낸 곳 기준 상대 경로 — `아키텍처/라우터 설계.md`. */
	rel: string;
	path: string;
	attachments: number;
}

export const libraryApi = {
	list: () => api.get<Book[]>('/api/library'),
	get: (bookId: string) => api.get<Book>(`/api/library/${encodeURIComponent(bookId)}`),
	// DEV-290: BOOK 변경 이력(최신→과거).
	history: (bookId: string) =>
		api.get<SidecarHistoryEntry[]>(`/api/library/${encodeURIComponent(bookId)}/history`),
	create: (title: string, body = '', path = '') =>
		api.post<Book>('/api/library', { title, body, path }),
	update: (bookId: string, fields: { title?: string; body?: string; path?: string }) =>
		api.patch<Book>(`/api/library/${encodeURIComponent(bookId)}`, fields),
	delete: (bookId: string) => api.delete(`/api/library/${encodeURIComponent(bookId)}`),

	// DEV-243: 태그 전체 교체.
	setTags: (bookId: string, tags: string[]) =>
		api.patch<Book>(`/api/library/${encodeURIComponent(bookId)}/tags`, { tags }),
	// BUG-287: 붙이기·떼기 — 읽은 뒤 남이 붙인 태그를 안 지운다.
	editTags: (bookId: string, edit: TagEdit) =>
		api.post<Book>(`/api/library/${encodeURIComponent(bookId)}/tags`, edit),

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

	/**
	 * REQ-028: 문서를 파일 시스템으로 펼친다 — 폴더 구조 그대로. **데스크톱 전용**이다
	 * (이 기계에 파일을 쓴다). `id` 하나 또는 `folder` 하나, 둘 다 없으면 도서관 전체.
	 */
	exportTo: async (dest: string, pick: { folder?: string; id?: string } = {}) => {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke<LibraryExported[]>('library_export', {
			dest,
			folder: pick.folder ?? null,
			id: pick.id ?? null
		});
	},
	/** REQ-028: 같은 것을 임시 폴더에 펼친 뒤 **클립보드에 파일로** 올린다(탐색기에 붙여넣기). */
	copyToClipboard: async (pick: { folder?: string; id?: string } = {}) => {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke<number>('library_copy_to_clipboard', {
			folder: pick.folder ?? null,
			id: pick.id ?? null
		});
	},

	folders: {
		list: () => api.get<LibraryFolder[]>('/api/library/folders'),
		create: (path: string) => api.post<LibraryFolder>('/api/library/folders', { path }),
		/** DEV-397: 옮기기·이름 바꾸기 — `to` 는 새 전체 경로. 바뀐 폴더 목록을 돌려준다. */
		move: (from: string, to: string) =>
			api.patch<LibraryFolder[]>('/api/library/folders', { from, to }),
		delete: (path: string) =>
			api.delete(`/api/library/folders?path=${encodeURIComponent(path)}`)
	}
};
