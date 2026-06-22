// DEV-016 (multi-file): 길드 규칙 API. `.guild/rules/{slug}.md` 다중 파일.
// content === null = 파일 부재.

import { api } from './client';

export interface RuleEntry {
	slug: string;
	content: string;
}

export interface RulesListResponse {
	entries: RuleEntry[];
}

export interface RuleResponse {
	slug: string;
	content: string | null;
}

/** legacy 단일-파일 응답 — backward compat 호출용. */
export interface RulesResponse {
	content: string | null;
}

export const rulesApi = {
	// ─── multi-file CRUD ───
	list: () => api.get<RulesListResponse>('/api/rules'),
	get: (slug: string) => api.get<RuleResponse>(`/api/rules/${encodeURIComponent(slug)}`),
	set: (slug: string, content: string) =>
		api.put<RuleResponse>(`/api/rules/${encodeURIComponent(slug)}`, { content }),
	create: (slug: string, content = '') => api.post<RuleResponse>('/api/rules', { slug, content }),
	delete: (slug: string) => api.delete(`/api/rules/${encodeURIComponent(slug)}`),
	rename: (slug: string, newSlug: string) =>
		api.patch<RuleResponse>(`/api/rules/${encodeURIComponent(slug)}`, {
			new_slug: newSlug
		}),

	// ─── (deprecated) legacy 단일 ───
	getSingle: () => api.get<RulesResponse>('/api/rules-single'),
	setSingle: (content: string) => api.put<RulesResponse>('/api/rules-single', { content })
};
