// REQ-040: 검색 페이지의 규칙 — 여는 주소 · 순서 · 탭 개수 · 주소 상태.
import { describe, it, expect } from 'vitest';
import {
	canQuery,
	countByTab,
	openHref,
	readSearchState,
	rowHref,
	rowsOfTab,
	searchPageHref,
	searchStateParams,
	sortRows,
	type SearchRow
} from './search-page';

const row = (kind: SearchRow['kind'], id: string, matched_in: string[]): SearchRow => ({
	kind,
	id,
	title: id,
	matched_in,
	excerpt: ''
});

describe('REQ-040 검색 페이지', () => {
	it('종류마다 팔레트와 같은 주소로 연다', () => {
		expect(rowHref('quest', 'DEV-001')).toBe('/quests/DEV-001');
		expect(rowHref('campaign', 'C-001')).toBe('/campaigns/C-001');
		expect(rowHref('rule', 'a b')).toBe('/rules?slug=a%20b');
		expect(rowHref('book', 'BOOK-001')).toBe('/library?id=BOOK-001');
	});

	it('본문 등에서만 맞은 문서는 찾기 바를 띄운 채 연다 — 제목에서 맞았거나 쿼리 결과면 그냥', () => {
		expect(openHref(row('quest', 'DEV-001', ['body']), '코끼리')).toBe(
			'/quests/DEV-001?find=%EC%BD%94%EB%81%BC%EB%A6%AC'
		);
		expect(openHref(row('quest', 'DEV-001', ['title', 'body']), '코끼리')).toBe('/quests/DEV-001');
		expect(openHref(row('quest', 'DEV-001', []), 'status:open')).toBe('/quests/DEV-001');
		expect(openHref(row('book', 'BOOK-001', ['comment']), '')).toBe('/library?id=BOOK-001');
	});

	it('제목에서 맞은 것이 먼저, 나머지는 받은 순서', () => {
		const rows = [
			row('quest', 'DEV-001', ['body']),
			row('quest', 'DEV-002', ['title']),
			row('book', 'BOOK-001', ['comment']),
			row('campaign', 'C-001', ['title', 'body'])
		];
		expect(sortRows(rows).map((r) => r.id)).toEqual(['DEV-002', 'C-001', 'DEV-001', 'BOOK-001']);
	});

	it('탭마다 개수와 줄', () => {
		const rows = [row('quest', 'A', []), row('quest', 'B', []), row('rule', 'r', [])];
		expect(countByTab(rows)).toEqual({
			all: 3,
			quest: 2,
			campaign: 0,
			book: 0,
			rule: 1,
			comment: 0
		});
		expect(rowsOfTab(rows, 'rule').map((r) => r.id)).toEqual(['r']);
		expect(rowsOfTab(rows, 'all')).toHaveLength(3);
	});

	it('쿼리는 퀘스트 · 캠페인 · 도서관에서만', () => {
		expect(canQuery('quest')).toBe(true);
		expect(canQuery('book')).toBe(true);
		expect(canQuery('rule')).toBe(false);
		expect(canQuery('all')).toBe(false);
	});

	it('주소 상태 — 기본값은 싣지 않고, 읽을 때 틀린 값은 기본으로', () => {
		const s = { q: ' 코끼리 ', tab: 'book' as const, mode: 'query' as const, memo: true };
		const p = searchStateParams(s);
		expect(p.toString()).toBe('q=%EC%BD%94%EB%81%BC%EB%A6%AC&kind=book&mode=query&memo=1');
		expect(readSearchState(p)).toEqual({ ...s, q: '코끼리' });
		expect(searchStateParams({ q: '', tab: 'all', mode: 'search', memo: false }).toString()).toBe(
			''
		);
		// 모르는 종류 → 전체, 쿼리를 못 쓰는 탭의 쿼리 → 검색.
		expect(readSearchState(new URLSearchParams('kind=nope&mode=query&q=x'))).toEqual({
			q: 'x',
			tab: 'all',
			mode: 'search',
			memo: false
		});
	});

	it('팔레트의 "모든 결과 보기" 주소', () => {
		expect(searchPageHref('로그인')).toBe('/search?q=%EB%A1%9C%EA%B7%B8%EC%9D%B8');
		expect(searchPageHref('  ')).toBe('/search');
	});
});
