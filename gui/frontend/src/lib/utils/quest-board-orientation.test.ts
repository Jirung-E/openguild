import { describe, expect, it } from 'vitest';

import {
	boardPointToCanonical,
	canonicalToBoardPoint,
	laneIndexAtCrossCoordinate,
	rowLaneHeight,
	type BoardOrientationMetrics
} from './quest-board-orientation';

const metrics: BoardOrientationMetrics = {
	nodeWidth: 284,
	nodeHeight: 80,
	nodeGap: 28,
	lanePadding: 20,
	columnLaneWidth: 948,
	columnLaneStride: 984,
	laneHeaderSize: 52
};

describe('quest board orientation geometry', () => {
	it('기존 columns 좌표는 숨긴 lane 압축만 적용한다', () => {
		expect(canonicalToBoardPoint({ x: 984 + 474, y: 216 }, 984, 120, 'columns', metrics)).toEqual({
			x: 594,
			y: 216
		});
	});

	it('rows는 기존 열을 행으로, 기존 아래 방향을 오른쪽 방향으로 바꾼다', () => {
		const laneStart = rowLaneHeight(metrics) + 36;
		expect(
			canonicalToBoardPoint({ x: 984 + 162, y: 108 }, 984, laneStart, 'rows', metrics)
		).toEqual({
			x: 162,
			y: laneStart + 108
		});
		expect(
			canonicalToBoardPoint({ x: 984 + 474, y: 216 }, 984, laneStart, 'rows', metrics)
		).toEqual({
			x: 474,
			y: laneStart + 216
		});
	});

	it('자유 배치 좌표도 orientation 왕복 시 정본 좌표가 보존된다', () => {
		const canonical = { x: 984 * 2 + 713.25, y: 947.5 };
		const visual = canonicalToBoardPoint(canonical, 984 * 2, 420, 'rows', metrics);
		const restored = boardPointToCanonical(visual, 984 * 2, 420, 'rows', metrics);
		expect(restored.x).toBeCloseTo(canonical.x);
		expect(restored.y).toBeCloseTo(canonical.y);
	});

	it('가변 크기 lane의 cross-axis hit test가 접힌 lane을 건너뛴다', () => {
		expect(laneIndexAtCrossCoordinate(39, [76, 416, 416])).toBe(0);
		expect(laneIndexAtCrossCoordinate(80, [76, 416, 416])).toBe(1);
		expect(laneIndexAtCrossCoordinate(9999, [76, 416, 416])).toBe(2);
	});
});
