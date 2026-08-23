import { describe, it, expect } from 'vitest';
import {
	EMPTY_FILTER,
	isFilterActive,
	serializeFilter,
	deserializeFilter,
	type QuestFilterState
} from './quest-filter';

function filter(over: Partial<QuestFilterState> = {}): QuestFilterState {
	return { ...EMPTY_FILTER, ...over };
}

describe('serializeFilter / deserializeFilter', () => {
	/**
	 * BUG-243 의 핵심: 필드를 하나라도 빠뜨리면 그 필터만 조용히 초기화된다.
	 * 값이 채워진 상태를 통째로 왕복시켜 **모든 필드**가 살아남는지 본다.
	 */
	it('모든 필드가 왕복에서 살아남는다', () => {
		const src = filter({
			typeIds: new Set([1, 2]),
			statusIds: new Set([3]),
			search: '젤리곰',
			titleOnly: true,
			searchComments: true,
			searchAttachments: true,
			tags: new Set(['ui', 'perf']),
			urgencies: new Set([1, 4]),
			prereq: 'has',
			sub: 'none',
			createdAfter: '2026-08-01',
			createdBefore: '2026-08-31',
			updatedAfter: '2026-08-10',
			updatedBefore: '2026-08-20'
		});
		const out = deserializeFilter(serializeFilter(src));
		expect(out).toEqual(src);
		// 인터페이스에 필드가 추가됐는데 직렬화에서 빠지면 여기서 깨진다.
		expect(Object.keys(out!).sort()).toEqual(Object.keys(src).sort());
	});

	it('REQ-010 검색 범위 플래그가 유지된다 (BUG-243)', () => {
		const out = deserializeFilter(
			serializeFilter(filter({ search: '수달', searchComments: true, searchAttachments: true }))
		);
		expect(out?.searchComments).toBe(true);
		expect(out?.searchAttachments).toBe(true);
	});

	/** 이 필드들이 없던 시절 저장된 값도 읽혀야 한다 — 기존 사용자 localStorage. */
	it('구버전 저장값은 새 필드를 false 로 채운다', () => {
		const legacy = JSON.stringify({
			typeIds: [1],
			statusIds: [],
			search: '검색어',
			titleOnly: false,
			tags: [],
			urgencies: [],
			prereq: 'any',
			sub: 'any',
			createdAfter: '',
			createdBefore: '',
			updatedAfter: '',
			updatedBefore: ''
		});
		const out = deserializeFilter(legacy);
		expect(out?.search).toBe('검색어');
		expect(out?.searchComments).toBe(false);
		expect(out?.searchAttachments).toBe(false);
	});

	it('Set 은 배열로 나갔다가 Set 으로 돌아온다', () => {
		const out = deserializeFilter(serializeFilter(filter({ typeIds: new Set([7]) })));
		expect(out?.typeIds).toBeInstanceOf(Set);
		expect([...out!.typeIds]).toEqual([7]);
	});

	it('깨진 입력 / null 은 null', () => {
		expect(deserializeFilter(null)).toBeNull();
		expect(deserializeFilter('')).toBeNull();
		expect(deserializeFilter('{ 이건 JSON 이 아니다')).toBeNull();
	});

	it('알 수 없는 TriState 는 any 로 떨어진다', () => {
		const out = deserializeFilter(JSON.stringify({ prereq: '엉뚱한값', sub: 'has' }));
		expect(out?.prereq).toBe('any');
		expect(out?.sub).toBe('has');
	});
});

describe('isFilterActive', () => {
	it('빈 필터는 비활성', () => {
		expect(isFilterActive(EMPTY_FILTER)).toBe(false);
	});

	it('공백뿐인 검색어는 활성이 아니다', () => {
		expect(isFilterActive(filter({ search: '   ' }))).toBe(false);
	});

	/**
	 * 검색 범위 플래그는 일부러 제외한다 — 검색어가 비면 결과가 달라지지 않아,
	 * 포함하면 보이는 변화 없이 '필터 활성' 표시만 켜진다.
	 */
	it('검색어 없이 범위 플래그만 켜면 비활성', () => {
		expect(isFilterActive(filter({ searchComments: true, searchAttachments: true }))).toBe(false);
	});

	it('검색어가 있으면 활성', () => {
		expect(isFilterActive(filter({ search: '수달', searchComments: true }))).toBe(true);
	});

	it('각 필터 종류가 단독으로 활성화한다', () => {
		expect(isFilterActive(filter({ typeIds: new Set([1]) }))).toBe(true);
		expect(isFilterActive(filter({ statusIds: new Set([1]) }))).toBe(true);
		expect(isFilterActive(filter({ tags: new Set(['x']) }))).toBe(true);
		expect(isFilterActive(filter({ urgencies: new Set([2]) }))).toBe(true);
		expect(isFilterActive(filter({ prereq: 'has' }))).toBe(true);
		expect(isFilterActive(filter({ sub: 'none' }))).toBe(true);
		expect(isFilterActive(filter({ createdAfter: '2026-01-01' }))).toBe(true);
		expect(isFilterActive(filter({ updatedBefore: '2026-01-01' }))).toBe(true);
	});

	/** titleOnly 는 단독으로는 아무것도 거르지 않는다 — 검색어의 수식어. */
	it('titleOnly 단독은 비활성', () => {
		expect(isFilterActive(filter({ titleOnly: true }))).toBe(false);
	});
});
