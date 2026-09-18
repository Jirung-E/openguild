// BUG-296: 자식창(새 창으로 열기)에 '← 목록' 버튼이 남아 있었다. 돌아갈 목록이 없는 창이라
// 눌러도 말이 안 되는 자리다. 트리도 같은 이유로 안 그린다.

import { describe, it, expect } from 'vitest';
import { showsTree, showsBackToList, isSingleColumn, shouldRefresh } from './library-view';

describe('BUG-296 자식창에는 목록 쪽을 안 그린다', () => {
	it('보통 창은 그대로다', () => {
		expect(showsTree('tree', false)).toBe(true);
		expect(showsBackToList('explorer', false)).toBe(true);
		expect(isSingleColumn('tree', false)).toBe(false);
	});

	it('자식창에서는 트리도 목록 버튼도 없다', () => {
		expect(showsTree('tree', true)).toBe(false);
		expect(showsBackToList('explorer', true)).toBe(false);
	});

	it('목록이 없으면 문서가 창 전체를 쓴다', () => {
		expect(isSingleColumn('explorer', false)).toBe(true);
		expect(isSingleColumn('tree', true)).toBe(true);
	});
});

describe('BUG-298 밖에서 생긴 문서가 보이려면 다시 받아야 한다', () => {
	it('받은 지 오래됐으면 다시 받는다', () => {
		expect(shouldRefresh(10_000, 0, false)).toBe(true);
	});

	it('방금 받았으면 건너뛴다 — 폴더를 옮길 때마다 통째로 받지 않게', () => {
		expect(shouldRefresh(1_000, 500, false)).toBe(false);
	});

	it('받는 중이면 겹쳐 부르지 않는다', () => {
		expect(shouldRefresh(10_000, 0, true)).toBe(false);
	});
});
