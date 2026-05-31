// DEV-016: 길드 규칙 API. `.guild/rules.md` 의 read / write.
// 파일이 없으면 content === null.

import { api } from './client';

export interface RulesResponse {
	content: string | null;
}

export const rulesApi = {
	get: () => api.get<RulesResponse>('/api/rules'),
	set: (content: string) => api.put<RulesResponse>('/api/rules', { content })
};
