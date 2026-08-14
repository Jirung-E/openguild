export type BoardOrientation = 'columns' | 'rows';

export interface BoardOrientationMetrics {
	nodeWidth: number;
	nodeHeight: number;
	nodeGap: number;
	lanePadding: number;
	columnLaneWidth: number;
	columnLaneStride: number;
	laneHeaderSize: number;
}

export interface BoardCoordinate {
	x: number;
	y: number;
}

/** 가로 행 하나가 세 개의 카드 행과 헤더를 수용하는 높이. */
export function rowLaneHeight(metrics: BoardOrientationMetrics): number {
	return metrics.laneHeaderSize + 16 + metrics.nodeHeight * 3 + metrics.nodeGap * 2 + 16;
}

export function canonicalGridBaseY(metrics: BoardOrientationMetrics): number {
	return metrics.laneHeaderSize + 16 + metrics.nodeHeight / 2;
}

export function rowGridBaseX(metrics: BoardOrientationMetrics): number {
	return metrics.lanePadding + metrics.nodeWidth / 2;
}

export function rowContentCenterY(metrics: BoardOrientationMetrics): number {
	return metrics.laneHeaderSize + 16 + (metrics.nodeHeight * 3 + metrics.nodeGap * 2) / 2;
}

/**
 * DB에 저장하는 기존 세로-열 좌표를 현재 orientation의 화면 월드 좌표로 바꾼다.
 * rows에서도 카드는 회전하지 않고, 셀 간격 비율만 교환한다.
 */
export function canonicalToBoardPoint(
	point: BoardCoordinate,
	absoluteLaneStart: number,
	visibleLaneStart: number,
	orientation: BoardOrientation,
	metrics: BoardOrientationMetrics
): BoardCoordinate {
	if (orientation === 'columns') {
		return {
			x: point.x - absoluteLaneStart + visibleLaneStart,
			y: point.y
		};
	}

	const cellW = metrics.nodeWidth + metrics.nodeGap;
	const cellH = metrics.nodeHeight + metrics.nodeGap;
	return {
		x: rowGridBaseX(metrics) + (point.y - canonicalGridBaseY(metrics)) * (cellW / cellH),
		y:
			visibleLaneStart +
			rowContentCenterY(metrics) +
			(point.x - absoluteLaneStart - metrics.columnLaneWidth / 2) * (cellH / cellW)
	};
}

/** canonicalToBoardPoint의 정확한 역변환. */
export function boardPointToCanonical(
	point: BoardCoordinate,
	absoluteLaneStart: number,
	visibleLaneStart: number,
	orientation: BoardOrientation,
	metrics: BoardOrientationMetrics
): BoardCoordinate {
	if (orientation === 'columns') {
		return {
			x: point.x + absoluteLaneStart - visibleLaneStart,
			y: point.y
		};
	}

	const cellW = metrics.nodeWidth + metrics.nodeGap;
	const cellH = metrics.nodeHeight + metrics.nodeGap;
	return {
		x:
			absoluteLaneStart +
			metrics.columnLaneWidth / 2 +
			(point.y - visibleLaneStart - rowContentCenterY(metrics)) * (cellW / cellH),
		y: canonicalGridBaseY(metrics) + (point.x - rowGridBaseX(metrics)) * (cellH / cellW)
	};
}

/** 숨김/접힘을 반영한 lane 크기 배열에서 주어진 cross-axis 좌표의 lane을 찾는다. */
export function laneIndexAtCrossCoordinate(cross: number, laneStrides: number[]): number {
	let start = 0;
	for (let index = 0; index < laneStrides.length; index += 1) {
		if (cross < start + laneStrides[index]) return index;
		start += laneStrides[index];
	}
	return Math.max(0, laneStrides.length - 1);
}
