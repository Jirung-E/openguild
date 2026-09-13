/**
 * BUG-278: 위치가 저장되지 않은 퀘스트를 보드 레인에 자동으로 놓는 규칙.
 *
 * # 왜 떼어 냈나
 *
 * 이 규칙이 `QuestBoard.svelte` 안에 **두 벌** 있었고 서로 달랐다.
 *
 * - 초기 적재는 3열 격자(`colOffsets`)에 채웠다.
 * - 보드를 켜 둔 채 퀘스트를 만들면 `handleFlash` 가 x 를 **레인 한가운데**로
 *   잡았다. 그래서 살아 있는 보드에 추가한 노드만 열이 안 맞는 자리에 섰다.
 *
 * 한 함수를 둘이 같이 쓰면 어긋날 수가 없다. 그리고 이 규칙은 순수 계산이라
 * cytoscape 없이 시험할 수 있다.
 *
 * # 순서가 전부다
 *
 * admin: *"퀘스트 추가하면 자동정렬되는데 이거 뭐임"* / *"새 노드가 가장 앞으로
 * 오는 느낌임."*
 *
 * 보드는 `questsApi.list()` 로 퀘스트를 받는데 그 기본 정렬이 **`id DESC`**
 * (생성 역순)다. 예전 배치는 받은 배열 순서대로 칸을 채웠으므로 **방금 만든
 * 퀘스트가 0번 칸**(레인 좌상단)을 가져가고, 같은 레인의 위치 없는 노드가 전부
 * 한 칸씩 밀렸다. 손으로 옮긴 적 있는 노드는 안 움직여서 더 종잡을 수 없었다.
 *
 * 그래서 **id 오름차순**으로 채운다. 새 퀘스트는 가장 큰 id 라 언제나 마지막
 * 칸에 붙고, 기존 노드는 자기 칸을 지킨다. 받은 배열의 정렬은 목록 화면이 쓰는
 * 것이라 건드리지 않는다 — 보드가 자기 순서를 정한다.
 */

import type { BoardPoint } from './quest-board-model';

export type PlacementQuest = { id: number; status_id: number };

export type PlacementMetrics = {
	/** 레인 하나의 x 폭(다음 레인 시작까지). */
	laneStride: number;
	/** 레인 안 3개 열의 중심 x (레인 왼쪽 기준). */
	colOffsets: readonly [number, number, number];
	/** 아무것도 없는 레인의 첫 줄 y. */
	initialY: number;
	/** 한 줄 아래로 가는 거리(노드 높이 + 간격). */
	rowStep: number;
};

/**
 * 위치가 저장 안 된 퀘스트들의 자리. 저장된 퀘스트는 결과에 없다.
 *
 * - 입력 배열의 **순서와 무관**하다(id 오름차순으로 채운다).
 * - 저장된 노드가 있는 레인은 **그 아래부터** 채운다 — 손으로 옮겨 둔 노드와
 *   겹치지 않게.
 */
export function autoPlace(
	quests: readonly PlacementQuest[],
	stored: ReadonlyMap<number, BoardPoint>,
	laneOf: ReadonlyMap<number, number>,
	m: PlacementMetrics
): Map<number, BoardPoint> {
	// 레인마다 자동 배치를 시작할 y — 저장된 노드 중 가장 아래 것의 한 줄 아래.
	const startY = new Map<number, number>();
	for (const q of quests) {
		const p = stored.get(q.id);
		if (!p) continue;
		const cur = startY.get(q.status_id) ?? m.initialY;
		startY.set(q.status_id, Math.max(cur, p.y + m.rowStep));
	}

	const unplaced = quests.filter((q) => !stored.has(q.id)).sort((a, b) => a.id - b.id);

	const count = new Map<number, number>();
	const out = new Map<number, BoardPoint>();
	for (const q of unplaced) {
		const n = count.get(q.status_id) ?? 0;
		count.set(q.status_id, n + 1);
		const lane = laneOf.get(q.status_id) ?? 0;
		out.set(q.id, {
			x: lane * m.laneStride + m.colOffsets[n % 3],
			y: (startY.get(q.status_id) ?? m.initialY) + Math.floor(n / 3) * m.rowStep
		});
	}
	return out;
}
