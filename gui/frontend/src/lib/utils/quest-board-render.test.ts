import { describe, expect, it } from 'vitest';

import { boardEdgePath, groupRects, parallelEdgeBends } from './quest-board-render';

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

describe('boardEdgePath — 가까운 노드 (BUG-242)', () => {
	const W = 284;
	const H = 80;
	const half = W / 2; // 142 — 가로로 이웃할 때 중심에서 테두리까지

	/** `M x1 y1 L x2 y2` 에서 x1 / x2 만 뽑는다. */
	function endpointsX(d: string): [number, number] {
		const m = d.match(/^M (-?[\d.]+) -?[\d.]+ L (-?[\d.]+) /);
		if (!m) throw new Error(`직선 경로가 아님: ${d}`);
		return [Number(m[1]), Number(m[2])];
	}

	/**
	 * 핵심 회귀: 노드가 가까워도 끝점은 **노드 경계**에 있어야 한다.
	 * 예전엔 `dist/3` 클램프 때문에 경계보다 안쪽에 찍혀 화살표가 노드에
	 * 파묻혔다(간격 40 이면 142 대신 108).
	 */
	it('이웃한 노드에서도 끝점이 노드 경계에 붙는다', () => {
		const gap = 40;
		const dist = W + gap; // 324 — 중심 간 거리
		const [x1, x2] = endpointsX(boardEdgePath(0, 0, dist, 0, 0, W, H));
		expect(x1).toBeCloseTo(half, 5);
		expect(x2).toBeCloseTo(dist - half, 5);
		// 그 사이가 정확히 간격만큼 남는다.
		expect(x2 - x1).toBeCloseTo(gap, 5);
	});

	/** 멀리 있을 때는 예전과 동일해야 한다. */
	it('멀리 있는 노드는 기존 동작 그대로', () => {
		const [x1, x2] = endpointsX(boardEdgePath(0, 0, 500, 0, 0, W, H));
		expect(x1).toBeCloseTo(half, 5);
		expect(x2).toBeCloseTo(500 - half, 5);
	});

	/**
	 * 겹칠 만큼 가까우면 줄이되 **뒤집히면 안 된다** — 끝점 순서가 역전되면
	 * 화살표가 반대 방향으로 그려진다. `dist/3` 클램프가 원래 막으려던 상황.
	 */
	it('노드가 겹쳐도 경로가 뒤집히지 않는다', () => {
		for (const dist of [200, 100, 30, 10]) {
			const [x1, x2] = endpointsX(boardEdgePath(0, 0, dist, 0, 0, W, H));
			expect(x2).toBeGreaterThanOrEqual(x1);
		}
	});

	/** 세로로 이웃할 때도 같은 규칙 — 이때 경계는 높이의 절반이다. */
	it('세로 이웃도 경계에 붙는다', () => {
		const dist = H + 30; // 110
		const d = boardEdgePath(0, 0, 0, dist, 0, W, H);
		const m = d.match(/^M -?[\d.]+ (-?[\d.]+) L -?[\d.]+ (-?[\d.]+)$/);
		expect(m).not.toBeNull();
		expect(Number(m![1])).toBeCloseTo(H / 2, 5);
		expect(Number(m![2])).toBeCloseTo(dist - H / 2, 5);
	});
});

describe('groupRects (DEV-437)', () => {
	const W = 100, H = 40;
	const g = (ids: number[]) => {
		const s = new Set(ids);
		return ids.map((i) => [i, s] as const);
	};

	it('그룹 노드 전부를 감싸고 pad 만큼 넓힌다 (노드 좌표는 중심)', () => {
		const groupOf = new Map(g([1, 2]));
		const r = groupRects(
			[
				{ id: 1, x: 50, y: 20 },
				{ id: 2, x: 250, y: 120 }
			],
			groupOf,
			W,
			H,
			10
		);
		expect(r).toEqual([{ key: 1, x: -10, y: -10, w: 320, h: 160 }]);
	});

	it('단독 노드 · 보이는 멤버가 하나뿐인 그룹은 안 그린다', () => {
		const groupOf = new Map([...g([1]), ...g([2, 3])]);
		const r = groupRects(
			[
				{ id: 1, x: 0, y: 0 },
				{ id: 2, x: 0, y: 0 },
				{ id: 3, x: 0, y: 0, hidden: true }
			],
			groupOf,
			W,
			H,
			10
		);
		expect(r).toEqual([]);
	});

	it('그룹마다 하나씩, key 는 가장 작은 id', () => {
		const groupOf = new Map([...g([5, 3]), ...g([9, 7])]);
		const r = groupRects(
			[3, 5, 7, 9].map((id) => ({ id, x: id * 10, y: 0 })),
			groupOf,
			W,
			H,
			0
		);
		expect(r.map((x) => x.key)).toEqual([3, 7]);
	});
});
