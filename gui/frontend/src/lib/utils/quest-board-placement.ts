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
 *
 * # DEV-420: 화살표가 위로 안 가게
 *
 * admin: *"흐름이 잘 보이게 연관관계 화살표의 방향이 위로 향하지 않도록."*
 *
 * 레인(x)은 **상태**가 정한다 — 그건 이 보드의 뜻이라 안 바꾼다. 바꾸는 것은 **줄(y)** 이다.
 * 선행 관계로 깊이를 재고(선행이 없으면 0, 있으면 가장 깊은 선행 + 1), 같은 깊이끼리 **띠**를
 * 이뤄 놓는다. 깊이가 커질수록 아래로 가므로 선행 → 후속 화살표는 언제나 아래를 향한다.
 *
 * 띠는 **레인을 가로질러 하나**다. 레인마다 따로 세면 같은 깊이가 서로 다른 높이에 놓여
 * 화살표가 다시 위로 갈 수 있다.
 *
 * **고리(순환)가 있으면 전부를 아래로 향하게 할 수는 없다** — 수학적으로 불가능하다. 고리에
 * 속한 것은 같은 깊이로 묶어 둔다(서로 옆에 서므로 화살표가 짧고, 고리라는 것이 눈에 띈다).
 */

import type { BoardPoint } from './quest-board-model';

export type PlacementQuest = { id: number; status_id: number };

/** 선행 관계 한 줄 — `prerequisite_id` 를 끝내야 `quest_id` 를 한다. */
export type PlacementEdge = { quest_id: number; prerequisite_id: number };

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
	m: PlacementMetrics,
	/** DEV-420: 선행 관계. 안 주면 예전처럼 순서대로만 채운다(깊이 전부 0과 같다). */
	edges: readonly PlacementEdge[] = []
): Map<number, BoardPoint> {
	// 자동 배치를 시작할 y — 손으로 옮겨 둔 노드보다 아래에서 시작한다.
	//
	// 레인마다 따로 잡지 않고 **가장 아래 것 하나**를 기준으로 삼는다: 띠가 레인을 가로지르기
	// 때문에, 레인마다 시작점이 다르면 같은 깊이가 서로 다른 높이에 놓여 화살표가 위로 갈 수
	// 있다(그걸 막자고 띠를 만든 것이다).
	let startY = m.initialY;
	for (const q of quests) {
		const p = stored.get(q.id);
		if (p) startY = Math.max(startY, p.y + m.rowStep);
	}

	const unplaced = quests.filter((q) => !stored.has(q.id)).sort((a, b) => a.id - b.id);
	const depth = depthOf(quests, edges);

	// 깊이별로 레인마다 몇 칸이 필요한가 → 그 띠의 높이(줄 수).
	const byDepth = new Map<number, PlacementQuest[]>();
	for (const q of unplaced) {
		const d = depth.get(q.id) ?? 0;
		(byDepth.get(d) ?? byDepth.set(d, []).get(d)!).push(q);
	}
	const depths = [...byDepth.keys()].sort((a, b) => a - b);

	const out = new Map<number, BoardPoint>();
	let y = startY;
	for (const d of depths) {
		const here = byDepth.get(d)!;
		const perLane = new Map<number, number>();
		for (const q of here) {
			const n = perLane.get(q.status_id) ?? 0;
			perLane.set(q.status_id, n + 1);
			const lane = laneOf.get(q.status_id) ?? 0;
			out.set(q.id, {
				x: lane * m.laneStride + m.colOffsets[n % 3],
				y: y + Math.floor(n / 3) * m.rowStep
			});
		}
		// 이 띠에서 가장 많이 쓴 레인의 줄 수만큼 내려간다 — 다음 띠가 겹치지 않게.
		const rows = Math.max(1, ...[...perLane.values()].map((n) => Math.ceil(n / 3)));
		y += rows * m.rowStep;
	}
	return out;
}

/**
 * 선행 깊이 — 선행이 없으면 0, 있으면 `가장 깊은 선행 + 1`.
 *
 * 고리가 있으면 그 안은 더 깊어질 수 없으므로 **같은 깊이로 묶는다**(위상 정렬에서 빠지는
 * 것들이 곧 고리다). 전부를 아래로 향하게 하는 것은 고리가 있으면 불가능하다.
 */
export function depthOf(
	quests: readonly PlacementQuest[],
	edges: readonly PlacementEdge[]
): Map<number, number> {
	const ids = new Set(quests.map((q) => q.id));
	const prereqs = new Map<number, number[]>();
	const dependents = new Map<number, number[]>();
	let indeg = new Map<number, number>();
	for (const id of ids) {
		prereqs.set(id, []);
		dependents.set(id, []);
		indeg.set(id, 0);
	}
	for (const e of edges) {
		// 보드에 없는 퀘스트를 가리키는 관계는 무시한다(지워졌거나 걸러진 것).
		if (!ids.has(e.quest_id) || !ids.has(e.prerequisite_id)) continue;
		if (e.quest_id === e.prerequisite_id) continue;
		prereqs.get(e.quest_id)!.push(e.prerequisite_id);
		dependents.get(e.prerequisite_id)!.push(e.quest_id);
		indeg.set(e.quest_id, (indeg.get(e.quest_id) ?? 0) + 1);
	}

	const depth = new Map<number, number>();
	// 들어오는 관계가 없는 것부터 — id 순으로 훑어 결과가 실행마다 같게 한다.
	let queue = [...ids].filter((id) => (indeg.get(id) ?? 0) === 0).sort((a, b) => a - b);
	for (const id of queue) depth.set(id, 0);
	while (queue.length > 0) {
		const next: number[] = [];
		for (const id of queue) {
			for (const dep of dependents.get(id) ?? []) {
				const d = Math.max(depth.get(dep) ?? 0, (depth.get(id) ?? 0) + 1);
				depth.set(dep, d);
				const left = (indeg.get(dep) ?? 0) - 1;
				indeg.set(dep, left);
				if (left === 0) next.push(dep);
			}
		}
		queue = next.sort((a, b) => a - b);
	}
	// 남은 것 = 고리에 속한 것. 같은 깊이(지금까지의 최대 + 1)로 묶는다.
	const leftover = [...ids].filter((id) => !depth.has(id));
	if (leftover.length > 0) {
		const maxDepth = Math.max(0, ...depth.values());
		for (const id of leftover) depth.set(id, maxDepth + 1);
	}
	return depth;
}
