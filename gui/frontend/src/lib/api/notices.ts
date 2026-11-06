// DEV-478: 알림함 — 플러그인의 `notify` 와 실패. 앱은 로컬 길드의 `.guild/notices/` 를, 웹 · 원격은 서버를 읽는다.
import { api } from './client';

export interface Notice {
	/** 적은 시각(밀리초까지) — 읽음 표시의 기준. */
	at: string;
	kind: 'notify' | 'problem';
	plugin?: string;
	text: string;
	open?: string;
	comment?: number;
}

export interface Inbox {
	/** 최근 것이 먼저. */
	notices: Notice[];
	unread: number;
	unread_problems: number;
}

export const noticesApi = {
	get: () => api.get<Inbox>('/api/notices'),
	markSeen: (at: string) => api.post<unknown>('/api/notices/seen', { at }),
	/** 하나 지운다 — 시각과 글이 같은 줄. */
	dismiss: (at: string, text: string) => api.post<unknown>('/api/notices/dismiss', { at, text }),
	/** 모두 지운다. */
	clear: () => api.post<unknown>('/api/notices/clear', {})
};
