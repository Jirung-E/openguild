import { describe, it, expect, vi, afterEach } from 'vitest';
import { keepRowAnchored } from './anchor-scroll';

/**
 * DEV-359: 호버로 항목이 펼쳐질 때 목록이 밀리면 커서 밑의 행이 바뀌어 펼침이
 * 연쇄한다. 그 보정을 담당하는 유틸 — 실측(브라우저)로는 확인했고, 여기서는
 * 계산이 뒤집히거나(부호) 조용히 빠지는 회귀를 막는다.
 */
/**
 * 프레임마다 레이아웃상 위치가 `tops` 로 바뀌는 행. 화면상 위치는 스크롤을
 * 따라 움직이므로 `scrollTop` 보정분을 반영한다 — 이걸 빼먹으면 보정이 영원히
 * 수렴하지 않아 실제 동작과 다른 것을 재게 된다.
 */
function fakeRow(tops: number[], scroller: { scrollTop: number } = { scrollTop: 0 }) {
	const base = scroller.scrollTop;
	let i = 0;
	return {
		isConnected: true,
		getBoundingClientRect: () => {
			const top = tops[Math.min(i, tops.length - 1)] + (base - scroller.scrollTop);
			i += 1;
			return { top };
		}
	} as unknown as HTMLElement;
}

/** requestAnimationFrame 을 즉시 실행으로 바꾼다 — 프레임 루프가 동기로 돈다. */
function runFrames() {
	return vi.spyOn(globalThis, 'requestAnimationFrame').mockImplementation((cb) => {
		(cb as FrameRequestCallback)(0);
		return 0;
	});
}

afterEach(() => vi.restoreAllMocks());

describe('keepRowAnchored', () => {
	it('행이 위로 밀리면 그만큼 scrollTop 을 줄여 제자리에 붙든다', () => {
		runFrames();
		const scroller = { scrollTop: 1076 } as HTMLElement;
		// 위 항목이 접히며 대상 행이 22px 위로 올라간 상황.
		keepRowAnchored(scroller, fakeRow([255, 233], scroller));
		expect(scroller.scrollTop).toBe(1054);
	});

	it('행이 아래로 밀리면 scrollTop 을 늘린다', () => {
		runFrames();
		const scroller = { scrollTop: 100 } as HTMLElement;
		keepRowAnchored(scroller, fakeRow([50, 80], scroller));
		expect(scroller.scrollTop).toBe(130);
	});

	it('여러 프레임에 걸친 전환도 끝까지 따라간다 — 한 프레임만 보정하면 흘러간다', () => {
		runFrames();
		const scroller = { scrollTop: 500 } as HTMLElement;
		// 120ms 전환이 프레임마다 조금씩 위치를 옮기는 상황(200 → 190 → 184 → 180).
		keepRowAnchored(scroller, fakeRow([200, 190, 184, 180], scroller));
		// 누적 이동량 20px 을 전부 되밀어야 한다.
		expect(scroller.scrollTop).toBe(480);
	});

	it('새 호출이 들어오면 이전 루프는 물러난다 — 기준점이 다른 루프끼리 scrollTop 을 두고 싸우면 안 된다', () => {
		// rAF 를 수동으로 돌려 두 루프를 교대로 진행시킨다.
		const queue: FrameRequestCallback[] = [];
		vi.spyOn(globalThis, 'requestAnimationFrame').mockImplementation((cb) => {
			queue.push(cb as FrameRequestCallback);
			return queue.length;
		});
		const scroller = { scrollTop: 200 } as HTMLElement;
		keepRowAnchored(scroller, fakeRow([100, 90], scroller)); // 옛 루프 (기준 100)
		keepRowAnchored(scroller, fakeRow([300, 280], scroller)); // 새 루프 (기준 300)
		// 옛 루프부터 실행 — 세대가 지났으므로 아무것도 하지 않아야 한다.
		queue.shift()?.(0);
		expect(scroller.scrollTop).toBe(200);
		// 새 루프만 보정한다(20px 위로 밀렸으므로 그만큼 빼기).
		queue.shift()?.(0);
		expect(scroller.scrollTop).toBe(180);
	});

	it('위치가 그대로면 건드리지 않는다 — 짧은 제목은 높이가 안 변한다', () => {
		runFrames();
		const scroller = { scrollTop: 42 } as HTMLElement;
		keepRowAnchored(scroller, fakeRow([120, 120], scroller));
		expect(scroller.scrollTop).toBe(42);
	});

	it('행이 DOM 에서 빠졌으면 아무것도 하지 않는다', () => {
		runFrames();
		const scroller = { scrollTop: 10 } as HTMLElement;
		const row = fakeRow([50, 80], scroller);
		Object.defineProperty(row, 'isConnected', { value: false });
		keepRowAnchored(scroller, row);
		expect(scroller.scrollTop).toBe(10);
	});

	it('인자가 없으면 조용히 무시한다', () => {
		const raf = runFrames();
		keepRowAnchored(null, fakeRow([0, 0]));
		keepRowAnchored({ scrollTop: 0 } as HTMLElement, null);
		expect(raf).not.toHaveBeenCalled();
	});
});
