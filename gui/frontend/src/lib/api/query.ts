// REQ-040: 쿼리 결과에 칠할 말 — 해석은 코어 한 곳(`query::highlights_for`). 화면은 받은 말로 칠하기만 한다.

import { api } from './client';
import type { HitTerm } from '$lib/utils/highlight';

export type QueryOf = 'quest' | 'campaign' | 'library';

export const queryApi = {
	/** 틀린 쿼리면 던진다 — 부르는 쪽은 칠하지 않으면 된다(오류는 목록 조회가 이미 보여 준다). */
	highlights: (q: string, of: QueryOf) =>
		api.get<HitTerm[]>(`/api/query/highlights?${new URLSearchParams({ q, of }).toString()}`)
};
