export interface BoardEdgeGeometry {
	id: string;
	sourceId: number;
	targetId: number;
}

/** Cytoscape의 기본 bezier와 비슷하게 단일 연결은 직선, 병렬 연결만 대칭으로 벌린다. */
export function parallelEdgeBends(edges: BoardEdgeGeometry[], step = 40): Map<string, number> {
	const groups = new Map<string, BoardEdgeGeometry[]>();
	for (const edge of edges) {
		const low = Math.min(edge.sourceId, edge.targetId);
		const high = Math.max(edge.sourceId, edge.targetId);
		const key = `${low}:${high}`;
		const group = groups.get(key);
		if (group) group.push(edge);
		else groups.set(key, [edge]);
	}

	const bends = new Map<string, number>();
	for (const group of groups.values()) {
		group.sort((a, b) => a.id.localeCompare(b.id));
		group.forEach((edge, index) => {
			const canonicalBend = (index - (group.length - 1) / 2) * step;
			// 아래 path의 법선은 source→target 방향을 따르므로, 역방향 edge는
			// 부호를 뒤집어 같은 canonical 좌표계에서 대칭이 되게 한다.
			const direction = edge.sourceId <= edge.targetId ? 1 : -1;
			bends.set(edge.id, canonicalBend * direction);
		});
	}
	return bends;
}

/**
 * BUG-242: 노드가 겹칠 만큼 가까울 때도 남겨 둘 최소 선 길이(px).
 * 화살표 마커(markerWidth 7)가 그려질 자리는 있어야 방향을 알 수 있다.
 */
const MIN_EDGE_VISIBLE = 8;

export function boardEdgePath(
	sx: number,
	sy: number,
	tx: number,
	ty: number,
	bend: number,
	nodeWidth: number,
	nodeHeight: number
): string {
	const dx = tx - sx;
	const dy = ty - sy;
	const rawDistance = Math.hypot(dx, dy);
	if (rawDistance < 0.0001) return '';
	const dist = Math.max(1, rawDistance);
	const ux = dx / dist;
	const uy = dy / dist;
	// 선을 노드 **경계**까지만 물린다 — 중심에서 테두리까지의 거리.
	const boundary = Math.min(
		Math.abs(ux) > 0.0001 ? nodeWidth / 2 / Math.abs(ux) : Number.POSITIVE_INFINITY,
		Math.abs(uy) > 0.0001 ? nodeHeight / 2 / Math.abs(uy) : Number.POSITIVE_INFINITY
	);
	// BUG-242: 예전엔 여기에 `dist / 3` 클램프가 함께 걸려 있었다. 노드가 가까우면
	// 그 값이 `boundary` 보다 작아져 **끝점이 노드 안쪽**에 찍혔고, 화살표 마커는
	// 경로 끝에 그려지므로 노드에 파묻혔다(폭 284 · 간격 40 이면 경계 142 vs
	// dist/3 = 108 → 34px 안쪽).
	//
	// 그 클램프는 노드가 겹칠 때 양끝 inset 합이 거리를 넘어 **경로가 뒤집히는**
	// 것을 막으려던 장치로 보인다. 그래서 없애는 대신, 뒤집힘만 정확히 막는다 —
	// 화살촉이 보일 최소 길이를 남기고 그 안에서만 줄인다.
	const inset = Math.min(boundary, Math.max(0, (dist - MIN_EDGE_VISIBLE) / 2));
	const x1 = sx + ux * inset;
	const y1 = sy + uy * inset;
	const x2 = tx - ux * inset;
	const y2 = ty - uy * inset;
	if (Math.abs(bend) < 0.0001) return `M ${x1} ${y1} L ${x2} ${y2}`;
	const cx = (x1 + x2) / 2 - uy * bend;
	const cy = (y1 + y2) / 2 + ux * bend;
	return `M ${x1} ${y1} Q ${cx} ${cy} ${x2} ${y2}`;
}

/** DEV-437: 그룹 사각형 하나 — 월드 좌표(좌상단 + 크기). */
export interface GroupRect {
	/** 그룹 안 가장 작은 퀘스트 id — 렌더 key. */
	key: number;
	x: number;
	y: number;
	w: number;
	h: number;
}

/**
 * DEV-437: 관계로 묶인 그룹(연결 성분)마다 **그 노드 전부를 감싸는 최소 사각형**.
 *
 * 정렬의 설계 규칙이 "각 그룹은 최소 크기 사각형 안에, 사각형끼리 안 겹친다" 인데 그 사각형이
 * 안 보여서 지켜지는지 눈으로 알 수 없었다. 그걸 그린다.
 *
 * - 노드 좌표는 **중심**이다(보드 노드가 그렇다). 사각형은 노드 상자 바깥으로 `pad` 만큼 넓힌다.
 * - 숨긴 노드는 뺀다. 보이는 멤버가 둘 미만인 그룹은 그리지 않는다 — 관계 없는 단독 노드가
 *   수백 개라 다 그리면 화면이 사각형으로 뒤덮인다.
 */
export function groupRects(
	nodes: readonly { id: number; x: number; y: number; hidden?: boolean }[],
	groupOf: ReadonlyMap<number, ReadonlySet<number>>,
	nodeW: number,
	nodeH: number,
	pad: number
): GroupRect[] {
	const byKey = new Map<number, { x1: number; y1: number; x2: number; y2: number; n: number }>();
	for (const node of nodes) {
		if (node.hidden) continue;
		const group = groupOf.get(node.id);
		if (!group || group.size < 2) continue;
		let key = node.id;
		for (const m of group) if (m < key) key = m;
		const x1 = node.x - nodeW / 2;
		const y1 = node.y - nodeH / 2;
		const box = byKey.get(key);
		if (box) {
			box.x1 = Math.min(box.x1, x1);
			box.y1 = Math.min(box.y1, y1);
			box.x2 = Math.max(box.x2, x1 + nodeW);
			box.y2 = Math.max(box.y2, y1 + nodeH);
			box.n++;
		} else {
			byKey.set(key, { x1, y1, x2: x1 + nodeW, y2: y1 + nodeH, n: 1 });
		}
	}
	const out: GroupRect[] = [];
	for (const [key, b] of byKey) {
		if (b.n < 2) continue;
		out.push({ key, x: b.x1 - pad, y: b.y1 - pad, w: b.x2 - b.x1 + pad * 2, h: b.y2 - b.y1 + pad * 2 });
	}
	return out.sort((a, b) => a.key - b.key);
}
