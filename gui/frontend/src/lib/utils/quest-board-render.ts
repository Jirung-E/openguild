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
