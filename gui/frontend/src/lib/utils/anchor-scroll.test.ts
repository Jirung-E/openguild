import { describe, it, expect, vi, afterEach } from 'vitest';
import { keepRowAnchored } from './anchor-scroll';

/**
 * DEV-359: 호버로 항목이 펼쳐질 때 목록이 밀리면 커서 밑의 행이 바뀌어 펼침이
 * 연쇄한다. 그 보정을 담당하는 유틸 — 실측(브라우저)로는 확인했고, 여기서는
 * 계산이 뒤집히거나(부호) 조용히 빠지는 회귀를 막는다.
 */
function fakeRow(tops: number[]) {
	let i = 0;
	return {
		isConnected: true,
		getBoundingClientRect: () => ({ top: tops[Math.min(i++, tops.length - 1)] })
	} as unknown as HTMLElement;
}

function runFrame() {
	// requestAnimationFrame 을 즉시 실행으로 바꿔 놓고 호출한다.
	const raf = vi.spyOn(globalThis, 'requestAnimationFrame').mockImplementation((cb) => {
		(cb as FrameRequestCallback)(0);
		return 0;
	});
	return raf;
}

afterEach(() => vi.restoreAllMocks());

describe('keepRowAnchored', () => {
	it('행이 위로 밀리면 그만큼 scrollTop 을 줄여 제자리에 붙든다', () => {
		runFrame();
		const scroller = { scrollTop: 1076 } as HTMLElement;
		// 위 항목이 접히며 대상 행이 22px 위로 올라간 상황.
		keepRowAnchored(scroller, fakeRow([255, 233]));
		expect(scroller.scrollTop).toBe(1054);
	});

	it('행이 아래로 밀리면 scrollTop 을 늘린다', () => {
		runFrame();
		const scroller = { scrollTop: 100 } as HTMLElement;
		keepRowAnchored(scroller, fakeRow([50, 80]));
		expect(scroller.scrollTop).toBe(130);
	});

	it('위치가 그대로면 건드리지 않는다 — 짧은 제목은 높이가 안 변한다', () => {
		runFrame();
		const scroller = { scrollTop: 42 } as HTMLElement;
		keepRowAnchored(scroller, fakeRow([120, 120]));
		expect(scroller.scrollTop).toBe(42);
	});

	it('행이 DOM 에서 빠졌으면 아무것도 하지 않는다', () => {
		runFrame();
		const scroller = { scrollTop: 10 } as HTMLElement;
		const row = fakeRow([50, 80]);
		Object.defineProperty(row, 'isConnected', { value: false });
		keepRowAnchored(scroller, row);
		expect(scroller.scrollTop).toBe(10);
	});

	it('인자가 없으면 조용히 무시한다', () => {
		const raf = runFrame();
		keepRowAnchored(null, fakeRow([0, 0]));
		keepRowAnchored({ scrollTop: 0 } as HTMLElement, null);
		expect(raf).not.toHaveBeenCalled();
	});
});
