import { describe, expect, it } from 'vitest';
import { autoPlace, type PlacementMetrics, type PlacementQuest } from './quest-board-placement';

const M: PlacementMetrics = {
	laneStride: 1000,
	colOffsets: [100, 400, 700],
	initialY: 50,
	rowStep: 110
};
const LANES = new Map([
	[1, 0], // open
	[2, 1] // in progress
]);
const NONE = new Map<number, { x: number; y: number }>();

function q(id: number, status_id = 1): PlacementQuest {
	return { id, status_id };
}

describe('BUG-278 보드 자동 배치', () => {
	/**
	 * **이 결함의 본체.** 퀘스트를 하나 추가하면 기존 노드가 한 칸씩 밀렸다.
	 *
	 * 보드는 목록을 `id DESC` 로 받는다 — 새 퀘스트가 배열의 맨 앞이다. 예전
	 * 배치는 받은 순서대로 칸을 채워서 새 것이 0번 칸을 가져갔다.
	 */
	it('퀘스트를 추가해도 기존 노드의 자리가 안 바뀐다', () => {
		const before = autoPlace([q(3), q(2), q(1)], NONE, LANES, M); // id DESC — 실제 받는 순서
		const after = autoPlace([q(4), q(3), q(2), q(1)], NONE, LANES, M);

		for (const id of [1, 2, 3]) {
			expect(after.get(id), `DEV-${id} 가 움직였다`).toEqual(before.get(id));
		}
	});

	it('새 퀘스트는 레인의 **마지막** 칸에 붙는다', () => {
		const after = autoPlace([q(4), q(3), q(2), q(1)], NONE, LANES, M);
		// 0,1,2 번 칸은 첫 줄, 3번 칸은 둘째 줄 첫 열.
		expect(after.get(4)).toEqual({ x: 100, y: 50 + 110 });
		// 예전 규칙이면 4 가 0번 칸(좌상단)이었다.
		expect(after.get(4)).not.toEqual({ x: 100, y: 50 });
	});

	it('입력 순서와 무관하다', () => {
		const asc = autoPlace([q(1), q(2), q(3), q(4)], NONE, LANES, M);
		const desc = autoPlace([q(4), q(3), q(2), q(1)], NONE, LANES, M);
		const shuffled = autoPlace([q(3), q(1), q(4), q(2)], NONE, LANES, M);
		expect(desc).toEqual(asc);
		expect(shuffled).toEqual(asc);
	});

	it('3열로 채우고 넘치면 다음 줄로 간다', () => {
		const got = autoPlace([1, 2, 3, 4].map((i) => q(i)), NONE, LANES, M);
		expect([1, 2, 3].map((i) => got.get(i)!.x)).toEqual([100, 400, 700]);
		expect([1, 2, 3].every((i) => got.get(i)!.y === 50)).toBe(true);
		expect(got.get(4)).toEqual({ x: 100, y: 160 });
	});

	it('레인마다 따로 센다', () => {
		const got = autoPlace([q(1, 1), q(2, 2), q(3, 1)], NONE, LANES, M);
		expect(got.get(1)).toEqual({ x: 100, y: 50 }); // 레인 0 의 0번
		expect(got.get(3)).toEqual({ x: 400, y: 50 }); // 레인 0 의 1번
		expect(got.get(2)).toEqual({ x: 1000 + 100, y: 50 }); // 레인 1 의 0번
	});

	/**
	 * **보드를 켜 둔 채 만든 노드는 저장하면 안 된다** — 그 이유를 못 박는다.
	 *
	 * 자동 배치는 저장된 노드를 기준점 삼아 그 아래부터 채운다. 같은 레인의
	 * 다른 노드는 저장된 적이 없으므로, 방금 만든 노드를 저장하면 새로고침 때
	 * 그게 기준점이 되어 나머지가 전부 밀린다. `handleFlash` 가 예전에 정확히
	 * 그렇게 했다.
	 *
	 * 저장하지 않으면 새로고침해도 모든 노드가 제자리다 — 새 노드는 가장 큰
	 * id 라 규칙이 늘 같은 마지막 칸을 준다.
	 */
	it('자동으로 놓은 새 노드를 저장하지 않으면 새로고침해도 아무도 안 움직인다', () => {
		const live = autoPlace([q(4), q(3), q(2), q(1)], NONE, LANES, M);
		const d = live.get(4)!;

		// 저장하지 않고 새로고침 — 전부 제자리.
		const reloaded = autoPlace([q(4), q(3), q(2), q(1)], NONE, LANES, M);
		for (const id of [1, 2, 3, 4]) expect(reloaded.get(id)).toEqual(live.get(id));

		// 저장하고 새로고침 — 나머지가 밀린다(예전 동작).
		const pinned = autoPlace([q(4), q(3), q(2), q(1)], new Map([[4, d]]), LANES, M);
		expect(pinned.get(1), '저장했는데도 안 밀렸다 — 이 시험의 전제가 틀렸다').not.toEqual(
			live.get(1)
		);
	});

	/** 손으로 옮겨 둔 노드와 겹치지 않게 그 아래부터 채운다. */
	it('저장된 노드가 있으면 그 아래부터 채우고, 저장된 것은 결과에 없다', () => {
		const stored = new Map([[1, { x: 400, y: 300 }]]);
		const got = autoPlace([q(1), q(2)], stored, LANES, M);
		expect(got.has(1)).toBe(false);
		expect(got.get(2)).toEqual({ x: 100, y: 300 + 110 });
	});
});
