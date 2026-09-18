// BUG-296: 자식창(새 창으로 열기)에 '← 목록' 버튼이 남아 있었다. 돌아갈 목록이 없는 창이라
// 눌러도 말이 안 되는 자리다. 트리도 같은 이유로 안 그린다.

import { describe, it, expect } from 'vitest';
import { showsTree, showsBackToList, isSingleColumn } from './library-view';

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
