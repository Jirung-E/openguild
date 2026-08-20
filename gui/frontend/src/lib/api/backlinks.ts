// REQ-008: cross-link backlink — "이 문서를 참조하는 문서".
//
// 색인은 reindex 가 index.db 에 만든다(doc_links). 여기서는 조회만 한다.

import { api } from './client';

export type BacklinkKind = 'quest' | 'campaign' | 'rule' | 'book';

export interface Backlink {
	kind: BacklinkKind;
	/** quest_id / campaign_slug / rule slug / BOOK-NNN */
	id: string;
	/** 표시용 제목. 규칙은 slug 가 곧 제목이라 id 와 같은 값이 온다. */
	title: string;
}

export const backlinksApi = {
	/** `kind`/`id` 문서를 참조하는 문서 목록. */
	list(kind: BacklinkKind, id: string): Promise<Backlink[]> {
		return api.get<Backlink[]>(
			`/api/backlinks/${encodeURIComponent(kind)}/${encodeURIComponent(id)}`
		);
	}
};
