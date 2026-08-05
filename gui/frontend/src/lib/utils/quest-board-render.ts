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
	const inset = Math.min(
		Math.abs(ux) > 0.0001 ? nodeWidth / 2 / Math.abs(ux) : Number.POSITIVE_INFINITY,
		Math.abs(uy) > 0.0001 ? nodeHeight / 2 / Math.abs(uy) : Number.POSITIVE_INFINITY,
		dist / 3
	);
	const x1 = sx + ux * inset;
	const y1 = sy + uy * inset;
	const x2 = tx - ux * inset;
	const y2 = ty - uy * inset;
	if (Math.abs(bend) < 0.0001) return `M ${x1} ${y1} L ${x2} ${y2}`;
	const cx = (x1 + x2) / 2 - uy * bend;
	const cy = (y1 + y2) / 2 + ux * bend;
	return `M ${x1} ${y1} Q ${cx} ${cy} ${x2} ${y2}`;
}
