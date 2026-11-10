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
	splitKindPrefix,
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

// REQ-040 다시 엶(admin): 팔레트처럼 `library:` 로 도서관에서만 찾는다.
describe('종류 접두', () => {
	it('팔레트와 같은 별칭을 탭으로 바꾸고 나머지 글만 남긴다', () => {
		expect(splitKindPrefix('library:설계')).toEqual({ tab: 'book', rest: '설계' });
		expect(splitKindPrefix('Lib: 설계')).toEqual({ tab: 'book', rest: '설계' });
		expect(splitKindPrefix('q:코끼리')).toEqual({ tab: 'quest', rest: '코끼리' });
		expect(splitKindPrefix('c:')).toEqual({ tab: 'campaign', rest: '' });
		expect(splitKindPrefix('r:zoo')).toEqual({ tab: 'rule', rest: 'zoo' });
	});
	it('별칭이 아니면 그대로', () => {
		expect(splitKindPrefix('http://x')).toEqual({ tab: null, rest: 'http://x' });
		expect(splitKindPrefix('설계')).toEqual({ tab: null, rest: '설계' });
	});
	it('주소의 q 에 접두가 있으면 그 탭으로 연다 — 쿼리는 건드리지 않는다', () => {
		const s = readSearchState(new URLSearchParams('q=library:설계'));
		expect([s.tab, s.q]).toEqual(['book', '설계']);
		const qy = readSearchState(new URLSearchParams('q=id:BOOK-001&kind=book&mode=query'));
		expect([qy.tab, qy.q, qy.mode]).toEqual(['book', 'id:BOOK-001', 'query']);
	});
	it('팔레트의 "모든 결과 보기" 는 고른 종류를 탭으로 들고 간다', () => {
		expect(searchPageHref('설계', 'book')).toBe(
			`/search?q=${encodeURIComponent('설계')}&kind=book`
		);
	});
});
