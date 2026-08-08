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
		expect(boardLodForZoom(0.099)).toBe('overview');
		expect(boardLodForZoom(0.1)).toBe('compact');
		expect(boardLodForZoom(0.239)).toBe('compact');
		expect(boardLodForZoom(0.24)).toBe('detail');
	});

	it('screen grid는 점 크기를 화면 px로 유지한다', () => {
		const metrics = screenGridMetrics(0.03, 108);
		expect(metrics.stepY).toBeCloseTo(3.24);
		expect(metrics.dotRadius).toBeCloseTo(0.5832);
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
