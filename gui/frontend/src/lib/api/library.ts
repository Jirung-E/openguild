// DEV-217: 도서관 API. `.guild/library/{BOOK-NNN}.md` — 자체 BOOK 번호,
// quest 번호와 별개 (단조 증가, 재사용 금지). server routes/library.rs 와 1:1.

import { api } from './client';

export interface Book {
	book_id: string;
	number: number;
	title: string;
	body: string;
	created_at: string;
	updated_at: string;
	deleted_at: string | null;
}

export const libraryApi = {
	list: () => api.get<Book[]>('/api/library'),
	get: (bookId: string) => api.get<Book>(`/api/library/${encodeURIComponent(bookId)}`),
	create: (title: string, body = '') => api.post<Book>('/api/library', { title, body }),
	update: (bookId: string, fields: { title?: string; body?: string }) =>
		api.patch<Book>(`/api/library/${encodeURIComponent(bookId)}`, fields),
	delete: (bookId: string) => api.delete(`/api/library/${encodeURIComponent(bookId)}`)
};
