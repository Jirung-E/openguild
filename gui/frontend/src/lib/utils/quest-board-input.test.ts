import { describe, expect, it } from 'vitest';

import { isBoardPanSurfaceTarget } from './quest-board-input';

describe('quest board touch target routing', () => {
	it('빈 보드와 그 자식만 pan 입력면으로 취급한다', () => {
		const board = Object.assign(document.createElement('div'), { className: 'board' });
		const child = document.createElement('span');
		board.appendChild(child);

		expect(isBoardPanSurfaceTarget(board)).toBe(true);
		expect(isBoardPanSurfaceTarget(child)).toBe(true);
	});

	it('버튼, popup, toolbar와 lane header는 pan 입력면이 아니다', () => {
		const selectors = ['button', '[role="dialog"]', '.toolbar', '.tb-newquest-wrap', '.lane-hdr'];
		for (const selector of selectors) {
			const el =
				selector === 'button'
					? document.createElement('button')
					: selector.startsWith('.')
						? Object.assign(document.createElement('div'), { className: selector.slice(1) })
						: Object.assign(document.createElement('div'), { role: 'dialog' });
			expect(isBoardPanSurfaceTarget(el), selector).toBe(false);
		}
	});

	it('quest node와 null도 pan 입력면이 아니다', () => {
		const node = Object.assign(document.createElement('div'), { className: 'board-node' });
		expect(isBoardPanSurfaceTarget(node)).toBe(false);
		expect(isBoardPanSurfaceTarget(null)).toBe(false);
	});
});
