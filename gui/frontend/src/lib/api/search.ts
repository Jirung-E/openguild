// REQ-009: 강화된 검색 — 본문뿐 아니라 댓글 / 첨부 이름까지.
//
// 기본 검색(`/api/quests?search=`)과 **별개 경로**다. 메모는 gitignore 되는
// 개인 기록이라 서버가 기본으로 제외한다 — 넣으려면 명시해야 한다.

import { api } from './client';

export type SearchField = 'body' | 'comment' | 'attachment' | 'memo';

export interface SearchHit {
	kind: 'quest' | 'campaign' | 'rule' | 'book';
	/** quest_id / campaign_slug / rule slug / BOOK-NNN */
	id: string;
	title: string;
	/** 어디서 맞았는지 — 왜 이 문서가 나왔는지 알려주는 값. */
	matched_in: string[];
	/** 처음 맞은 지점 주변 발췌. */
	excerpt: string;
}

export const searchApi = {
	enhanced(q: string, fields?: SearchField[], limit?: number): Promise<SearchHit[]> {
		const p = new URLSearchParams({ q });
		if (fields?.length) p.set('in', fields.join(','));
		if (limit) p.set('limit', String(limit));
		return api.get<SearchHit[]>(`/api/search?${p.toString()}`);
	}
};
