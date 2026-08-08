export type BoardLod = 'detail' | 'compact' | 'overview';

export interface ScreenGridMetrics {
	stepY: number;
	dotRadius: number;
}

export interface BoardFrameStats {
	rafHz: number;
	medianMs: number;
	p95Ms: number;
	missed120Percent: number;
}

const OVERVIEW_ZOOM = 0.16;
const DETAIL_ZOOM = 0.55;

export function boardLodForZoom(zoom: number): BoardLod {
	if (zoom < OVERVIEW_ZOOM) return 'overview';
	if (zoom < DETAIL_ZOOM) return 'compact';
	return 'detail';
}

/**
 * 월드 격자를 현재 viewport에만 그리기 위한 screen-space 수치.
 * 점의 반지름은 화면 px 기준으로 유지하고, 배경 offset은 pan/zoom에
 * 따라 연속적으로 변한다.
 */
export function screenGridMetrics(
	zoom: number,
	cellH: number
): ScreenGridMetrics {
	const safeZoom = Math.max(zoom, 0.0001);
	const stepY = Math.max(cellH * safeZoom, 1);
	const dotRadius = Math.max(0.55, Math.min(1.35, stepY * 0.18));
	return {
		stepY,
		dotRadius
	};
}

/** 레인 안의 스냅 열 중심을 screen-space x 좌표로 바꾼다. */
export function screenGridColumnCenters(
	firstCenterX: number,
	cellW: number,
	zoom: number,
	cols: number
): number[] {
	return Array.from(
		{ length: Math.max(0, Math.floor(cols)) },
		(_, column) => (firstCenterX + column * cellW) * zoom
	);
}

/** 성능 HUD는 디버그 빌드에서만 Cmd/Ctrl+Shift+H로 토글한다. */
export function isPerformanceMonitorShortcut(
	code: string,
	ctrlKey: boolean,
	metaKey: boolean,
	shiftKey: boolean,
	debugEnabled: boolean
): boolean {
	return debugEnabled && (ctrlKey || metaKey) && shiftKey && code === 'KeyH';
}

function percentile(sorted: number[], ratio: number): number {
	if (sorted.length === 0) return 0;
	return sorted[Math.floor((sorted.length - 1) * ratio)];
}

export function summarizeBoardFrames(intervals: number[]): BoardFrameStats {
	const finite = intervals.filter((value) => Number.isFinite(value) && value > 0);
	if (finite.length === 0) {
		return { rafHz: 0, medianMs: 0, p95Ms: 0, missed120Percent: 0 };
	}
	const sorted = [...finite].sort((a, b) => a - b);
	const mean = finite.reduce((sum, value) => sum + value, 0) / finite.length;
	return {
		rafHz: 1000 / mean,
		medianMs: percentile(sorted, 0.5),
		p95Ms: percentile(sorted, 0.95),
		missed120Percent: (finite.filter((value) => value > 12.5).length / finite.length) * 100
	};
}
