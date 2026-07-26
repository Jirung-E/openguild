// DEV-167: 작업 기록 API. 활동은 index.db 기존 캐시 조회, 노트는
// `.guild/worklog/{YYYY-MM-DD}.md` (전역 공유, git tracked).

import { api } from './client';

export interface ActivityRow {
	ts: string;
	/** "status" | "type" | "comment" | "created" | "discussion" */
	kind: string;
	slug: string;
	summary: string;
}

export interface ActivityCounts {
	status_changes: number;
	comments: number;
	created: number;
	done_transitions: number;
	/** DEV-236: 토론 resolve/reopen 전환 수. */
	discussion_events: number;
	/** DEV-288: 규칙·도서관 문서 변경 수. */
	doc_changes: number;
}

export interface WorklogReport {
	from: string;
	to: string;
	activities: ActivityRow[];
	counts: ActivityCounts;
}

export interface DailyCount {
	date: string;
	count: number;
}

export interface WorklogNote {
	date: string;
	content: string | null;
}

export const worklogApi = {
	activities: (from: string, to: string) =>
		api.get<WorklogReport>(`/api/worklog?from=${from}&to=${to}`),
	summary: (from: string, to: string) =>
		api.get<DailyCount[]>(`/api/worklog/summary?from=${from}&to=${to}`),
	notes: (from: string, to: string) =>
		api.get<WorklogNote[]>(`/api/worklog/notes?from=${from}&to=${to}`),
	noteGet: (date: string) => api.get<WorklogNote>(`/api/worklog/note/${date}`),
	noteSet: (date: string, content: string) =>
		api.put<WorklogNote>(`/api/worklog/note/${date}`, { content })
};
