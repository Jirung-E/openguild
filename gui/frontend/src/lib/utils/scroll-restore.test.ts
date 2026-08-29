import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { restoreScroll, cancelRestoreScroll } from './scroll-restore';

/**
 * rAF 를 수동으로 돌려 루프를 결정적으로 관찰한다.
 * jsdom 은 scrollTo 를 구현하지 않으므로 직접 스텁한다.
 */
let frames: FrameRequestCallback[] = [];
let scrolls: number[] = [];
let now = 0;

function pump(times = 1) {
	for (let i = 0; i < times; i++) {
		const batch = frames;
		frames = [];
		now += 16;
		for (const f of batch) f(now);
	}
}

beforeEach(() => {
	frames = [];
	scrolls = [];
	now = 0;
	vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
		frames.push(cb);
		return frames.length;
	});
	vi.stubGlobal('performance', { now: () => now });
	// BUG-257 이후 복원은 `scrollPageTo` 를 거친다 — jsdom 에는 `<main>` 이
	// 없으니 window 로 물러서는데, 그때 넘어오는 건 **옵션 객체**(`{top}`)다.
	// 위치 인자만 읽던 스텁은 전부 `undefined` 를 기록해 이 테스트가 무의미해졌다.
	// 두 형태를 모두 받는다.
	window.scrollTo = ((a: number | ScrollToOptions, b?: number): void => {
		scrolls.push(typeof a === 'object' ? (a.top ?? 0) : (b as number));
	}) as typeof window.scrollTo;
});

afterEach(() => {
	cancelRestoreScroll();
	vi.unstubAllGlobals();
});

describe('restoreScroll — 루프 경합 (REQ-004)', () => {
	it('목표 위치를 매 프레임 재적용한다', () => {
		restoreScroll(300);
		pump(3);
		expect(scrolls.length).toBeGreaterThan(0);
		expect(new Set(scrolls)).toEqual(new Set([300]));
	});

	/**
	 * 핵심: 빠른 back/forward. 새 복원이 시작되면 앞선 루프가 끊겨야 한다.
	 * 예전엔 두 루프가 살아남아 매 프레임 서로 다른 y 로 되돌렸다.
	 */
	it('새 복원이 시작되면 앞선 루프가 멈춘다', () => {
		restoreScroll(300);
		pump(1);
		scrolls.length = 0;
		restoreScroll(900); // 다른 목표로 재시작
		pump(3);
		expect(scrolls.length).toBeGreaterThan(0);
		expect(new Set(scrolls)).toEqual(new Set([900]));
		expect(scrolls).not.toContain(300);
	});

	it('cancelRestoreScroll 이 루프를 멈춘다', () => {
		restoreScroll(300);
		pump(1);
		cancelRestoreScroll();
		scrolls.length = 0;
		pump(3);
		expect(scrolls).toEqual([]);
	});

	it('사용자가 휠을 굴리면 즉시 중단한다', () => {
		restoreScroll(300);
		pump(1);
		window.dispatchEvent(new Event('wheel'));
		scrolls.length = 0;
		pump(3);
		expect(scrolls).toEqual([]);
	});
});
