// DEV-217: 도서관 API. `.guild/library/{BOOK-NNN}.md` — 자체 BOOK 번호,
// quest 번호와 별개 (단조 증가, 재사용 금지). server routes/library.rs 와 1:1.
// DEV-239: 폴더(계층) — path 필드 + 별도 folders 엔드포인트.

import { api } from './client';

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

	folders: {
		list: () => api.get<LibraryFolder[]>('/api/library/folders'),
		create: (path: string) => api.post<LibraryFolder>('/api/library/folders', { path }),
		delete: (path: string) =>
			api.delete(`/api/library/folders?path=${encodeURIComponent(path)}`)
	}
};
