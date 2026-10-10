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

/** 노드 하나 — 교차 세기용. `x`/`y` 는 좌상단. */
export type CrossingNode = { id: number; x: number; y: number; w: number; h: number };

/**
 * DEV-421: **노드를 가로지르는 선**의 수 — 고친 효과를 숫자로 말하기 위한 자.
 *
 * 선은 중심에서 중심으로 곧게 긋는다고 본다(지금 `boardEdgePath` 가 그렇다). 양끝 노드는 센다고
 * 치지 않는다. 선분과 사각형이 만나는지는 slab 방식으로 본다 — 매개변수 구간을 축마다 깎아
 * 남으면 교차다.
 */
export function countNodeCrossings(
	nodes: readonly CrossingNode[],
	edges: readonly { source: number; target: number }[]
): { edges: number; crossing: number; worst: number } {
	const byId = new Map(nodes.map((n) => [n.id, n]));
	const center = (n: CrossingNode) => ({ x: n.x + n.w / 2, y: n.y + n.h / 2 });
	let crossing = 0;
	let worst = 0;
	let counted = 0;
	for (const e of edges) {
		const a = byId.get(e.source);
		const b = byId.get(e.target);
		if (!a || !b || a === b) continue;
		counted++;
		const p = center(a);
		const q = center(b);
		let hits = 0;
		for (const n of nodes) {
			if (n.id === e.source || n.id === e.target) continue;
			if (segmentHitsRect(p, q, n)) hits++;
		}
		if (hits > 0) crossing++;
		worst = Math.max(worst, hits);
	}
	return { edges: counted, crossing, worst };
}

function segmentHitsRect(
	p: { x: number; y: number },
	q: { x: number; y: number },
	r: CrossingNode
): boolean {
	let t0 = 0;
	let t1 = 1;
	for (const [from, to, lo, hi] of [
		[p.x, q.x, r.x, r.x + r.w],
		[p.y, q.y, r.y, r.y + r.h]
	] as const) {
		const d = to - from;
		if (Math.abs(d) < 1e-9) {
			if (from < lo || from > hi) return false;
			continue;
		}
		const ta = (lo - from) / d;
		const tb = (hi - from) / d;
		t0 = Math.max(t0, Math.min(ta, tb));
		t1 = Math.min(t1, Math.max(ta, tb));
		if (t0 > t1) return false;
	}
	return true;
}

/** 그룹 안에서 노드 하나가 앉을 자리 — 레인, 그 레인 안의 열, 그룹 위에서부터의 줄. */
export type ClusterSlot = { lane: number; col: number; row: number };

export type ClusterMember = { id: number; status_id: number };

export type ClusterGeometry = {
	/** 한 칸의 가로 폭(노드 + 간격). */
	cellW: number;
	/** 한 칸의 세로 높이. */
	cellH: number;
	/** 레인 하나의 x 폭. */
	laneStride: number;
	nodeW: number;
	nodeH: number;
};

/**
 * DEV-421: 그룹 안 자리 — **레인마다 몇 열을 어디에 쓸지** 고른다.
 *
 * # 무엇이 문제였나
 *
 * admin: *"지금 사각형은 '최소 사각형' 이 아니다. 각 노드가 자신이 속한 레인의 **왼쪽부터** 채운다.
 * 최적의 배치는 다른 레인에 걸쳐 있더라도 조밀하게 모이는 것이다."*
 *
 * 예전 정렬은 `col = i % lcols` — 언제나 레인 왼쪽부터, 언제나 레인의 열을 전부 썼다. 레인 안에서
 * 열을 고를 자유를 안 쓴 것이다. 그래서 레인 둘에 걸친 그룹이 경계를 사이에 두고 마주 보지 못하고
 * 양쪽 끝으로 벌어졌고, 그 사이를 선이 길게 가로질렀다.
 *
 * # 어떻게
 *
 * 레인마다 (쓰는 열 수 `c`, 시작 열 `s`) 를 모두 시도해 **그룹 사각형 넓이 + 교차 벌점**이 가장
 * 작은 조합을 고른다. 줄 순서는 건드리지 않는다 — 깊이로 줄을 다시 잡아 보는 것은 네 번 해 봤고
 * 전부 더 나빴다(퀘스트 댓글의 실측표).
 *
 * 실측(실제 길드, 관계 있는 그룹 48개 · 선 309):
 *
 * | | 교차 | 사각형 넓이 합 | 선길이 합 |
 * |---|---|---|---|
 * | 예전(레인 왼쪽부터) | 163 | 27.3M | 274,566 |
 * | 넓이만 최소로 | 170 | 23.5M | 255,462 |
 * | 교차만 최소로 | 116 | 47.9M | 267,473 |
 * | **넓이 + 교차 벌점** | **122** | 28.1M | **245,327** |
 *
 * 마지막 것을 쓴다 — 교차 −25%, 선길이 −11%, 넓이는 거의 그대로(+3%). 폭이 좁아진 그룹 28개,
 * 넓어진 그룹 0개.
 *
 * 벌점은 교차 하나당 `CROSSING_PENALTY` 픽셀². 넓이만 보면 조밀해져 교차가 늘고, 교차만 보면
 * 사각형이 75% 부풀어 다른 그룹을 밀어낸다. 그 사이를 잡은 값이다.
 */
const CROSSING_PENALTY = 400_000;

/** 레인 수가 많으면 조합이 폭발한다 — 그 위로는 레인별로 따로(탐욕) 고른다. */
const MAX_COMBOS = 2048;

export function clusterSlots(
	members: readonly ClusterMember[],
	edges: readonly PlacementEdge[],
	laneOf: ReadonlyMap<number, number>,
	/** 레인이 내주는 열 수. */
	cols: number,
	/** 같은 레인 안의 순서 — 보통 슬러그. 결과가 실행마다 같게 한다. */
	tieBreak: (id: number) => string,
	geom: ClusterGeometry,
	/** 참이면 모든 선이 **아래**로. 거짓이면 다른 레인으로 가는 선은 **옆**(같은 줄)도 된다. */
	strictDown = false
): Map<number, ClusterSlot> {
	const C = Math.max(1, cols);
	const byLane = new Map<number, number[]>();
	for (const m of members) {
		const lane = laneOf.get(m.status_id) ?? 0;
		(byLane.get(lane) ?? byLane.set(lane, []).get(lane)!).push(m.id);
	}
	// 레인 안 채우는 순서 — **선행 · 부모가 먼저**(위상 순서), 같은 차례면 슬러그. 레인 안은 왼쪽→오른쪽,
	// 위→아래로 채우므로 먼저 놓인 쪽이 같은 줄 왼쪽이거나 윗줄이다. 그래서 같은 레인 안의 선은
	// 저절로 "아래 또는 옆" 이 된다.
	const order = topoOrder(members, edges, tieBreak);
	for (const ids of byLane.values()) ids.sort((a, b) => (order.get(a) ?? 0) - (order.get(b) ?? 0));
	const laneList = [...byLane.keys()].sort((a, b) => a - b);

	// 레인마다 고를 수 있는 (열 수, 시작 열).
	const options = laneList.map((lane) => {
		const out: { lane: number; c: number; s: number }[] = [];
		for (let c = 1; c <= C; c++) for (let s = 0; s + c <= C; s++) out.push({ lane, c, s });
		return out;
	});

	// 줄은 **띠**로 잡는다 — 레인을 가로질러 하나. 이게 "화살표가 위로 안 간다" 를 보장한다.
	const baseRank = rankOf(members, edges, laneOf, strictDown);
	// 무조건 아래 순위 — 모든 선이 더 높은 띠로 가므로 띠 안에서 위로 갈 일이 없다. 최후의 안전판.
	const strictRank = strictDown ? baseRank : rankOf(members, edges, laneOf, true);

	const place = (
		rank: ReadonlyMap<number, number>,
		choice: readonly { lane: number; c: number; s: number }[]
	) => {
		const slots = new Map<number, ClusterSlot>();
		const colsOfLane = new Map(choice.map((o) => [o.lane, o]));
		const bands = [...new Set(members.map((m) => rank.get(m.id) ?? 0))].sort((a, b) => a - b);
		let row = 0;
		for (const d of bands) {
			let bandRows = 1;
			for (const [lane, ids] of byLane) {
				const here = ids.filter((id) => (rank.get(id) ?? 0) === d);
				if (here.length === 0) continue;
				const { c, s } = colsOfLane.get(lane)!;
				here.forEach((id, i) => {
					slots.set(id, { lane, col: s + (i % c), row: row + Math.floor(i / c) });
				});
				bandRows = Math.max(bandRows, Math.ceil(here.length / c));
			}
			row += bandRows;
		}
		return slots;
	};
	const upEdges = (slots: ReadonlyMap<number, ClusterSlot>) =>
		edges.filter((e) => {
			const a = slots.get(e.prerequisite_id);
			const b = slots.get(e.quest_id);
			return a && b && b.row < a.row;
		});

	// 같은 띠라도 **같은 줄은 아니다** — 띠가 여러 줄이면 그 안에서 옆으로 둔 선이 위로 갈 수 있다.
	// 몇 줄이 되는지는 레인마다 고른 열 수에 달려 있으므로 **열 선택마다** 검사한다(예전엔 3열
	// 배치로 한 번만 봐서, 열을 적게 고른 그룹에서 위로 가는 선이 3개 남았다). 올라간 선의 도착
	// 노드를 출발 노드보다 한 띠 아래로 내리고 다시 놓는다. 멤버 수만큼 해도 남으면 무조건 아래 순위로 —
	// 그 순위에서는 위로 가는 선이 생길 수 없다.
	const build = (choice: readonly { lane: number; c: number; s: number }[]) => {
		const rank = new Map(baseRank);
		let slots = place(rank, choice);
		for (let pass = 0; pass <= members.length; pass++) {
			const up = upEdges(slots);
			if (up.length === 0) return slots;
			for (const e of up) {
				const need = (rank.get(e.prerequisite_id) ?? 0) + 1;
				rank.set(e.quest_id, Math.max(rank.get(e.quest_id) ?? 0, need));
			}
			slots = place(rank, choice);
		}
		return upEdges(slots).length === 0 ? slots : place(strictRank, choice);
	};

	const combos = options.reduce((n, o) => n * o.length, 1);
	if (combos > MAX_COMBOS) {
		// 레인별로 자기 넓이만 보고 고른다 — 조합 탐색이 너무 크다.
		const greedy = laneList.map((lane) => {
			const n = byLane.get(lane)!.length;
			let best = { lane, c: C, s: 0, area: Number.POSITIVE_INFINITY };
			for (let c = 1; c <= C; c++)
				for (let s = 0; s + c <= C; s++) {
					const area = c * geom.cellW * Math.ceil(n / c) * geom.cellH;
					if (area < best.area) best = { lane, c, s, area };
				}
			return best;
		});
		return build(greedy);
	}

	let best: { slots: Map<number, ClusterSlot>; cost: number } | null = null;
	const walk = (i: number, acc: { lane: number; c: number; s: number }[]) => {
		if (i === options.length) {
			const slots = build(acc);
			const cost = clusterCost(slots, edges, geom);
			if (!best || cost < best.cost) best = { slots, cost };
			return;
		}
		for (const o of options[i]) walk(i + 1, [...acc, o]);
	};
	walk(0, []);
	return best!.slots;
}

/**
 * DEV-421: 각 노드가 몇 번째 **띠**에 설지 — "화살표가 위로 안 간다" 를 보장하는 순위.
 *
 * admin 이 정한 규칙: **"아래 또는 옆"** — 같은 레인 안이든 레인을 건너가든, 자식 · 후행은 부모 ·
 * 선행과 **같은 줄(옆)이거나 아래**에 선다. 위로만 안 가면 된다.
 *
 * (중간에 "같은 레인 안은 무조건 아래" 로 넣었었다. 같은 줄에 붙으면 선이 짧아 안 보일 거라는 내
 * 짐작이었고 admin 에게 묻지 않았다. 그래서 부모 · 자식이 같은 레인인 묶음은 옆이 하나도 안 나왔다.
 * admin 이 같은 레인 안에서도 옆을 허용하기로 정했다, 2026-10-10.)
 *
 * 그래서 기본은 모든 간선 비용이 0 — 모두 같은 띠에서 시작한다. 같은 레인 안은 위상 순서로 채우므로
 * 저절로 아래 또는 옆이 되고, 레인을 건너가다 위로 간 선만 `clusterSlots` 의 바로잡기가 한 띠씩
 * 내린다.
 *
 * `strictDown` 이면 전부 1 — 모든 선이 아래를 향한다([[DEV-438]] 의 옵션 켠 상태).
 *
 * 고리가 있으면 전부를 아래로 보낼 수 없다(수학적으로). 고리에 속한 것은 같은 띠로 묶는다.
 */
function rankOf(
	members: readonly ClusterMember[],
	edges: readonly PlacementEdge[],
	laneOf: ReadonlyMap<number, number>,
	strictDown: boolean
): Map<number, number> {
	const laneById = new Map(members.map((m) => [m.id, laneOf.get(m.status_id) ?? 0]));
	const ids = new Set(members.map((m) => m.id));
	const out = new Map<number, { to: number; w: number }[]>();
	const indeg = new Map<number, number>();
	for (const id of ids) {
		out.set(id, []);
		indeg.set(id, 0);
	}
	for (const e of edges) {
		const a = e.prerequisite_id;
		const b = e.quest_id;
		if (a === b || !ids.has(a) || !ids.has(b)) continue;
		out.get(a)!.push({ to: b, w: strictDown ? 1 : 0 });
		indeg.set(b, (indeg.get(b) ?? 0) + 1);
	}
	const rank = new Map<number, number>();
	let queue = [...ids].filter((id) => (indeg.get(id) ?? 0) === 0).sort((a, b) => a - b);
	for (const id of queue) rank.set(id, 0);
	while (queue.length > 0) {
		const next: number[] = [];
		for (const id of queue) {
			for (const { to, w } of out.get(id) ?? []) {
				rank.set(to, Math.max(rank.get(to) ?? 0, (rank.get(id) ?? 0) + w));
				const left = (indeg.get(to) ?? 0) - 1;
				indeg.set(to, left);
				if (left === 0) next.push(to);
			}
		}
		queue = next.sort((a, b) => a - b);
	}
	// 남은 것 = 고리. 지금까지의 최대 + 1 로 묶는다.
	const leftover = [...ids].filter((id) => !rank.has(id));
	if (leftover.length > 0) {
		const mx = Math.max(0, ...rank.values());
		for (const id of leftover) rank.set(id, mx + 1);
	}
	return rank;
}

/**
 * 위상 순서 — 선행 · 부모가 먼저. 같은 차례에서는 `tieBreak`(슬러그) 순. 고리에 걸린 것은 맨 뒤에
 * 슬러그 순으로 붙인다.
 */
function topoOrder(
	members: readonly ClusterMember[],
	edges: readonly PlacementEdge[],
	tieBreak: (id: number) => string
): Map<number, number> {
	const ids = new Set(members.map((m) => m.id));
	const out = new Map<number, number[]>();
	const indeg = new Map<number, number>();
	for (const id of ids) {
		out.set(id, []);
		indeg.set(id, 0);
	}
	for (const e of edges) {
		const a = e.prerequisite_id;
		const b = e.quest_id;
		if (a === b || !ids.has(a) || !ids.has(b)) continue;
		out.get(a)!.push(b);
		indeg.set(b, (indeg.get(b) ?? 0) + 1);
	}
	const bySlug = (a: number, b: number) => tieBreak(a).localeCompare(tieBreak(b));
	const ready = [...ids].filter((id) => indeg.get(id) === 0).sort(bySlug);
	const order = new Map<number, number>();
	while (ready.length > 0) {
		const id = ready.shift()!;
		order.set(id, order.size);
		for (const b of out.get(id) ?? []) {
			const left = (indeg.get(b) ?? 0) - 1;
			indeg.set(b, left);
			if (left === 0) {
				ready.push(b);
				ready.sort(bySlug);
			}
		}
	}
	for (const id of [...ids].filter((x) => !order.has(x)).sort(bySlug)) order.set(id, order.size);
	return order;
}

/** 사각형 넓이 + 교차 벌점. 작을수록 좋다. */
function clusterCost(
	slots: ReadonlyMap<number, ClusterSlot>,
	edges: readonly PlacementEdge[],
	geom: ClusterGeometry
): number {
	const nodes: CrossingNode[] = [];
	for (const [id, s] of slots) {
		nodes.push({
			id,
			x: s.lane * geom.laneStride + s.col * geom.cellW,
			y: s.row * geom.cellH,
			w: geom.nodeW,
			h: geom.nodeH
		});
	}
	const x1 = Math.min(...nodes.map((n) => n.x));
	const x2 = Math.max(...nodes.map((n) => n.x + n.w));
	const y1 = Math.min(...nodes.map((n) => n.y));
	const y2 = Math.max(...nodes.map((n) => n.y + n.h));
	const { crossing } = countNodeCrossings(
		nodes,
		edges.map((e) => ({ source: e.prerequisite_id, target: e.quest_id }))
	);
	return (x2 - x1) * (y2 - y1) + crossing * CROSSING_PENALTY;
}
