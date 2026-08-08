import { describe, expect, it } from 'vitest';

import {
	boardLodForZoom,
	isPerformanceMonitorShortcut,
	screenGridColumnCenters,
	screenGridMetrics,
	summarizeBoardFrames
} from './quest-board-viewport';

describe('quest board viewport helpers', () => {
	it('깊은 축소에서는 overview, 중간은 compact, 확대에서는 detail LOD를 쓴다', () => {
		expect(boardLodForZoom(0.03)).toBe('overview');
		expect(boardLodForZoom(0.16)).toBe('compact');
		expect(boardLodForZoom(0.54)).toBe('compact');
		expect(boardLodForZoom(0.55)).toBe('detail');
	});

	it('screen grid는 점 크기를 화면 px로 유지하고 viewport보다 넉넉히 덮는다', () => {
		const metrics = screenGridMetrics(0.03, -200, 108, 312, 108, 800);
		expect(metrics.stepX).toBeCloseTo(9.36);
		expect(metrics.stepY).toBeCloseTo(3.24);
		expect(metrics.dotRadius).toBeCloseTo(0.5832);
		expect(metrics.phaseY).toBeGreaterThanOrEqual(0);
		expect(metrics.phaseY).toBeLessThan(metrics.stepY);
		expect(metrics.top).toBeCloseTo(-3.24);
		expect(metrics.height).toBeGreaterThan(800);
	});

	it('레인의 열 수만큼 독립적인 screen-space 스냅 열 중심을 만든다', () => {
		expect(screenGridColumnCenters(170, 328, 0.5, 3)).toEqual([85, 249, 413]);
		expect(screenGridColumnCenters(170, 328, 0.5, 1)).toEqual([85]);
	});

	it('성능 HUD 단축키는 디버그 빌드의 Cmd/Ctrl+Shift+H에서만 동작한다', () => {
		expect(isPerformanceMonitorShortcut('KeyH', false, true, true, true)).toBe(true);
		expect(isPerformanceMonitorShortcut('KeyH', true, false, true, true)).toBe(true);
		expect(isPerformanceMonitorShortcut('KeyH', false, true, true, false)).toBe(false);
		expect(isPerformanceMonitorShortcut('KeyH', false, true, false, true)).toBe(false);
	});

	it('grid phase는 큰 양수/음수 pan에서도 한 tile 범위로 감싼다', () => {
		for (const panY of [-100_000, -1, 0, 100_000]) {
			const metrics = screenGridMetrics(0.2, panY, 108, 312, 108, 600);
			expect(metrics.phaseY).toBeGreaterThanOrEqual(0);
			expect(metrics.phaseY).toBeLessThan(metrics.stepY);
		}
	});

	it('frame interval에서 실제 rAF Hz와 120Hz 누락 비율을 계산한다', () => {
		const perfect120 = summarizeBoardFrames(Array.from({ length: 120 }, () => 1000 / 120));
		expect(perfect120.rafHz).toBeCloseTo(120);
		expect(perfect120.medianMs).toBeCloseTo(1000 / 120);
		expect(perfect120.missed120Percent).toBe(0);

		const capped60 = summarizeBoardFrames(Array.from({ length: 60 }, () => 1000 / 60));
		expect(capped60.rafHz).toBeCloseTo(60);
		expect(capped60.missed120Percent).toBe(100);
	});
});
