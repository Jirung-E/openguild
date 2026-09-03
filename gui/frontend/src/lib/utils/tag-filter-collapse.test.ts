// REQ-019: 태그 필터 줄 접기.
//
// 가장 중요한 것은 **접혀 있어도 고른 태그는 보인다**는 것이다. 필터가 걸린
// 채 통째로 숨으면 목록이 왜 줄었는지 알 수 없고 되돌릴 수단도 없다 — 원래
// 불편(줄이 길다)보다 나쁜 상태가 된다.
import { describe, it, expect, beforeEach } from 'vitest';
import {
	loadTagFilterOpen,
	saveTagFilterOpen,
	visibleTags,
	toggleCount
} from './tag-filter-collapse';

describe('visibleTags', () => {
	const tags = ['backend', 'api', 'ui', 'docs'];

	it('펼쳤으면 전부', () => {
		expect(visibleTags(tags, new Set(), true)).toEqual(tags);
	});

	it('접었고 고른 게 없으면 하나도 안 그린다', () => {
		expect(visibleTags(tags, new Set(), false)).toEqual([]);
	});

	it('접었어도 **고른 태그는 보인다** — 이게 이 파일의 핵심', () => {
		expect(visibleTags(tags, new Set(['api']), false)).toEqual(['api']);
		expect(visibleTags(tags, new Set(['api', 'docs']), false)).toEqual(['api', 'docs']);
	});

	it('원래 목록 순서를 지킨다 — 고른 것만 앞으로 당기면 펼쳤을 때 칩이 자리를 옮긴다', () => {
		expect(visibleTags(tags, new Set(['docs', 'backend']), false)).toEqual(['backend', 'docs']);
	});

	it('목록에 없는 태그가 선택돼 있어도 만들어내지 않는다', () => {
		expect(visibleTags(tags, new Set(['사라진태그']), false)).toEqual([]);
	});

	it('원본 배열을 건드리지 않는다', () => {
		const orig = [...tags];
		visibleTags(tags, new Set(['api']), true);
		expect(tags).toEqual(orig);
	});
});

describe('toggleCount', () => {
	it('전체 개수를 센다 — 눌러 볼 이유가 되는 숫자다', () => {
		expect(toggleCount(['a', 'b', 'c'])).toBe(3);
		expect(toggleCount([])).toBe(0);
	});
});

describe('접힘 상태 영속', () => {
	beforeEach(() => {
		sessionStorage.clear();
	});

	it('저장된 게 없으면 접힘이 기본', () => {
		expect(loadTagFilterOpen('library')).toBe(false);
	});

	it('왕복한다', () => {
		saveTagFilterOpen('library', true);
		expect(loadTagFilterOpen('library')).toBe(true);
		saveTagFilterOpen('library', false);
		expect(loadTagFilterOpen('library')).toBe(false);
	});

	it('화면마다 따로 기억한다 — 도서관을 폈다고 규칙까지 펴지면 안 된다', () => {
		saveTagFilterOpen('library', true);
		expect(loadTagFilterOpen('rules')).toBe(false);
		expect(loadTagFilterOpen('quests')).toBe(false);
	});

	it('이상한 값이 들어 있으면 접힘으로 본다', () => {
		sessionStorage.setItem('openguild.tagFilterOpen.library', 'yes');
		expect(loadTagFilterOpen('library')).toBe(false);
	});
});
