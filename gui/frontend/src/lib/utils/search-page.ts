// REQ-040: 전체 문서 검색 페이지(`/search`)의 규칙 — 화면은 이걸 부르기만 한다(룬을 안 써 시험이 붙는다).
//
// 팔레트는 빨리 가기(제목 · 최근 본 문서), 이 페이지는 찾아보기다. 검색은 서버의 강화 검색(`/api/search`,
// REQ-009)을 그대로 쓰고, 쿼리는 종류마다 키가 달라 한 종류를 골랐을 때 그 목록 화면의 쿼리를 쓴다.
// 댓글 탭은 문서 종류가 아니다 — 검색이면 댓글에서 맞은 문서, 쿼리면 맞는 댓글 하나하나가 한 줄이다.

import type { SearchHit } from '$lib/api/search';
import type { SearchMode } from '$lib/components/SearchField.svelte';
import { withFindParam } from './find-in-page';

export type SearchKind = SearchHit['kind'];
/** 쿼리를 쓸 수 있는 탭 — 규칙은 쿼리가 없고, '전체' 는 종류마다 키가 달라 막는다. */
export type QueryKind = 'quest' | 'campaign' | 'book' | 'comment';
/** 탭 — 전체 또는 한 종류, 또는 댓글. */
export type SearchTab = 'all' | SearchKind | 'comment';

export const SEARCH_TABS: readonly SearchTab[] = [
	'all',
	'quest',
	'campaign',
	'book',
	'rule',
	'comment'
];

export const QUERY_KINDS: readonly QueryKind[] = ['quest', 'campaign', 'book', 'comment'];

export function canQuery(tab: SearchTab): tab is QueryKind {
	return (QUERY_KINDS as readonly string[]).includes(tab);
}

/** 한 줄 — 검색 결과(`SearchHit`)와 쿼리 결과(맞는 문서)가 같은 모양이 된다. */
export interface SearchRow {
	kind: SearchKind;
	id: string;
	title: string;
	/** 어디서 맞았는지(`title` · `body` · `comment` · `attachment` · `memo`). 쿼리 결과는 비어 있다. */
	matched_in: string[];
	excerpt: string;
	/** 쿼리로 찾은 댓글이면 그 문서 안의 번호 — 누르면 그 댓글로 간다. */
	comment?: number;
}

/** 문서를 여는 주소 — 팔레트와 같다. */
export function rowHref(kind: SearchKind, id: string): string {
	switch (kind) {
		case 'quest':
			return `/quests/${id}`;
		case 'campaign':
			return `/campaigns/${encodeURIComponent(id)}`;
		case 'rule':
			return `/rules?slug=${encodeURIComponent(id)}`;
		case 'book':
			return `/library?id=${encodeURIComponent(id)}`;
	}
}

/**
 * 누르면 갈 주소. 제목 · 번호로는 안 맞고 본문 · 댓글 등에서만 맞았으면 찾기 바를 그 검색어로 띄운다(DEV-467 과
 * 같다) — 긴 문서에서 왜 나왔는지 바로 보인다.
 */
export function openHref(row: SearchRow, term: string): string {
	const href = rowHref(row.kind, row.id);
	if (row.comment !== undefined) return `${href}?comment=${row.comment}`;
	if (!term.trim() || row.matched_in.length === 0 || row.matched_in.includes('title')) return href;
	return withFindParam(href, term.trim());
}

/**
 * 순서 — 제목 · 번호에서 맞은 것이 먼저, 그 안에서는 받은 순서(종류 · 번호 순)를 지킨다. 본문에서 한 번 스친
 * 문서보다 제목에 그 말이 있는 문서가 대개 찾던 것이다.
 */
export function sortRows(rows: readonly SearchRow[]): SearchRow[] {
	const titled = rows.filter((r) => r.matched_in.includes('title'));
	const rest = rows.filter((r) => !r.matched_in.includes('title'));
	return [...titled, ...rest];
}

/** 탭마다 몇 개인지 — `all` 은 전부. */
export function countByTab(rows: readonly SearchRow[]): Record<SearchTab, number> {
	const out: Record<SearchTab, number> = {
		all: rows.length,
		quest: 0,
		campaign: 0,
		book: 0,
		rule: 0,
		comment: 0
	};
	for (const r of rows) {
		out[r.kind] += 1;
		if (r.matched_in.includes('comment')) out.comment += 1;
	}
	return out;
}

export function rowsOfTab(rows: readonly SearchRow[], tab: SearchTab): SearchRow[] {
	if (tab === 'all') return [...rows];
	if (tab === 'comment') return rows.filter((r) => r.matched_in.includes('comment'));
	return rows.filter((r) => r.kind === tab);
}

/** 주소에 싣는 상태 — 새로고침 · 공유한 링크가 같은 결과를 연다. */
export interface SearchPageState {
	q: string;
	tab: SearchTab;
	mode: SearchMode;
	memo: boolean;
}

export function readSearchState(params: URLSearchParams): SearchPageState {
	const kind = params.get('kind') ?? 'all';
	const tab = (SEARCH_TABS as readonly string[]).includes(kind) ? (kind as SearchTab) : 'all';
	const wantQuery = params.get('mode') === 'query';
	return {
		q: params.get('q') ?? '',
		tab,
		// 쿼리를 못 쓰는 탭이면 검색으로 연다 — 고칠 방법이 없는 오류 화면을 내지 않는다.
		mode: wantQuery && canQuery(tab) ? 'query' : 'search',
		memo: params.get('memo') === '1'
	};
}

/** 기본값은 싣지 않는다 — 주소가 짧게 남는다. */
export function searchStateParams(s: SearchPageState): URLSearchParams {
	const p = new URLSearchParams();
	if (s.q.trim()) p.set('q', s.q.trim());
	if (s.tab !== 'all') p.set('kind', s.tab);
	if (s.mode === 'query') p.set('mode', 'query');
	if (s.memo) p.set('memo', '1');
	return p;
}

/** 팔레트의 "모든 결과 보기" — 검색어를 들고 이 페이지로. */
export function searchPageHref(term: string): string {
	const p = searchStateParams({ q: term, tab: 'all', mode: 'search', memo: false });
	const qs = p.toString();
	return qs ? `/search?${qs}` : '/search';
}
