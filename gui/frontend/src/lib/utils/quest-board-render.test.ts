import { describe, expect, it } from 'vitest';

import { boardEdgePath, parallelEdgeBends } from './quest-board-render';

describe('quest board edge rendering', () => {
	it('단일 연결은 기존 보드처럼 직선으로 그린다', () => {
		const bends = parallelEdgeBends([{ id: 'pre-1-2', sourceId: 1, targetId: 2 }]);
		expect(bends.get('pre-1-2')).toBe(0);
		expect(boardEdgePath(0, 0, 500, 0, bends.get('pre-1-2')!, 284, 80)).toBe('M 142 0 L 358 0');
	});

	it('같은 두 노드의 병렬 연결만 양쪽으로 같은 간격만큼 벌린다', () => {
		const bends = parallelEdgeBends([
			{ id: 'pre-1-2', sourceId: 1, targetId: 2 },
			{ id: 'sub-2-1', sourceId: 2, targetId: 1 }
		]);
		expect(bends.get('pre-1-2')).toBe(-20);
		expect(bends.get('sub-2-1')).toBe(-20);
		expect(boardEdgePath(0, 0, 500, 0, -20, 284, 80)).toContain(' Q ');
		expect(boardEdgePath(500, 0, 0, 0, -20, 284, 80)).toContain(' Q ');
	});
});
