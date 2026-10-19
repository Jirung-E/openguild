// DEV-443: 개인 형광펜 — 대상(`quest:DEV-001` · `campaign:C-001` · `rule:{slug}` · `library:BOOK-001`)마다
// 목록. 앱은 로컬 길드에, 웹 · 원격은 서버에 묻는다(transport 가 고른다).
import { api } from './client';

export interface Highlight {
	id: string;
	/** `body` · `memo` · `comment:{번호}`. */
	part: string;
	exact: string;
	prefix: string;
	suffix: string;
	created_at: string;
}

export interface NewHighlight {
	part: string;
	exact: string;
	prefix: string;
	suffix: string;
}

const q = (target: string) => `?target=${encodeURIComponent(target)}`;

export const highlightsApi = {
	list: (target: string) => api.get<Highlight[]>(`/api/highlights${q(target)}`),
	add: (target: string, body: NewHighlight) =>
		api.post<Highlight>(`/api/highlights${q(target)}`, body),
	remove: (target: string, id: string) =>
		api.delete<{ removed: boolean }>(`/api/highlights/${encodeURIComponent(id)}${q(target)}`)
};
