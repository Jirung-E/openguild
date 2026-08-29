// REQ-015: 사이드바 폭 store.
//
// 핵심은 **rem 저장**이다 — px 로 저장하면 BUG-254 가 고친 문제(배율을 올리면
// 칸은 그대로인데 내용이 넘침)가 사용자 값에서 그대로 되살아난다.
import { describe, it, expect, beforeEach } from 'vitest';
import {
	paneWidth,
	resetPaneWidth,
	clampPaneRem,
	applyDragDelta,
	defaultPaneRem,
	MIN_PANE_REM,
	MAX_PANE_REM
} from './paneWidth';
import { get } from 'svelte/store';

describe('clampPaneRem', () => {
	it('범위 밖은 잘라낸다', () => {
		expect(clampPaneRem(0)).toBe(MIN_PANE_REM);
		expect(clampPaneRem(-50)).toBe(MIN_PANE_REM);
		expect(clampPaneRem(1000)).toBe(MAX_PANE_REM);
	});

	it('범위 안은 그대로', () => {
		expect(clampPaneRem(15)).toBe(15);
		expect(clampPaneRem(MIN_PANE_REM)).toBe(MIN_PANE_REM);
		expect(clampPaneRem(MAX_PANE_REM)).toBe(MAX_PANE_REM);
	});

	it('숫자가 아니면 기본값으로 — NaN 이 store 에 들어가면 레이아웃이 통째로 깨진다', () => {
		expect(Number.isFinite(clampPaneRem(Number.NaN))).toBe(true);
		expect(Number.isFinite(clampPaneRem(Number.POSITIVE_INFINITY))).toBe(true);
	});
});

describe('applyDragDelta — 배율 대응', () => {
	it('기본 배율에서는 끈 픽셀만큼 rem 이 는다', () => {
		// root 16px, 오른쪽으로 32px → 2rem 증가
		expect(applyDragDelta(15, 32, 16)).toBeCloseTo(17, 5);
		expect(applyDragDelta(15, -32, 16)).toBeCloseTo(13, 5);
	});

	it('배율 200% 에서는 같은 픽셀이 절반의 rem — 손이 움직인 거리와 칸이 커지는 거리가 같아야 한다', () => {
		// root 32px, 오른쪽으로 32px → 1rem 증가 (화면에서는 똑같이 32px 넓어진다)
		expect(applyDragDelta(15, 32, 32)).toBeCloseTo(16, 5);
	});

	it('배율 50% 에서는 두 배', () => {
		expect(applyDragDelta(15, 32, 8)).toBeCloseTo(19, 5);
	});

	it('결과도 범위 안으로 잘린다', () => {
		expect(applyDragDelta(MIN_PANE_REM, -9999, 16)).toBe(MIN_PANE_REM);
		expect(applyDragDelta(MAX_PANE_REM, 9999, 16)).toBe(MAX_PANE_REM);
	});

	it('root 가 0/음수로 읽혀도 16 으로 대체 — 0 이면 Infinity 가 된다', () => {
		expect(applyDragDelta(15, 32, 0)).toBeCloseTo(17, 5);
		expect(applyDragDelta(15, 32, -4)).toBeCloseTo(17, 5);
	});
});

describe('paneWidth store', () => {
	beforeEach(() => {
		localStorage.clear();
	});

	it('pane 마다 기본값이 다르다 — BUG-254 가 정한 값 그대로', () => {
		expect(defaultPaneRem('library')).toBe(16.25);
		expect(defaultPaneRem('rules')).toBe(15);
	});

	it('같은 pane 은 같은 store — 재마운트해도 구독이 갈라지지 않는다', () => {
		expect(paneWidth('library')).toBe(paneWidth('library'));
		expect(paneWidth('library')).not.toBe(paneWidth('rules'));
	});

	it('pane 끼리 값이 섞이지 않는다', () => {
		paneWidth('library').set(20);
		paneWidth('rules').set(12);
		expect(get(paneWidth('library'))).toBe(20);
		expect(get(paneWidth('rules'))).toBe(12);
	});

	it('reset 은 그 pane 의 기본값으로 되돌린다', () => {
		paneWidth('rules').set(28);
		resetPaneWidth('rules');
		expect(get(paneWidth('rules'))).toBe(defaultPaneRem('rules'));
	});

	it('localStorage 에 rem 값이 남는다', async () => {
		paneWidth('library').set(22.5);
		// rAF 병합이라 한 프레임 기다린다.
		await new Promise((r) => requestAnimationFrame(() => r(null)));
		expect(localStorage.getItem('openguild.paneWidth.library')).toBe('22.5');
	});
});
