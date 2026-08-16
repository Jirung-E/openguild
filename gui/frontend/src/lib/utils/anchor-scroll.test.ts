import { describe, it, expect, vi, afterEach } from 'vitest';
import { beginOverlayExpand, beginOverlayCollapse, isPointerDrivenHover } from './anchor-scroll';

/**
 * DEV-359: 펼침은 **흐름을 밀지 않아야** 한다 — 아래 줄이 밀리면 스크롤 보정이
 * 필요해지고, 그 한 프레임 사이에 이웃 항목으로 hover 가 옮겨가며 떨린다.
 * 여기서는 "바깥 상자 높이를 접힌 값으로 붙박아 둔다"는 핵심 계약을 지킨다.
 */
function fakeEl(heights: number[]) {
	let i = 0;
	return {
		isConnected: true,
		style: {} as CSSStyleDeclaration,
		getBoundingClientRect: () => ({ height: heights[Math.min(i++, heights.length - 1)] }),
		animate: vi.fn(() => ({ finished: Promise.resolve(), cancel: vi.fn() }))
	} as unknown as HTMLElement & { animate: ReturnType<typeof vi.fn> };
}

/** requestAnimationFrame 을 즉시 실행으로. */
function runFrames() {
	return vi.spyOn(globalThis, 'requestAnimationFrame').mockImplementation((cb) => {
		(cb as FrameRequestCallback)(0);
		return 0;
	});
}

afterEach(() => vi.restoreAllMocks());

describe('beginOverlayExpand', () => {
	it('바깥 상자 높이를 접힌 값으로 고정한다 — 이게 빠지면 아래 줄이 전부 밀린다', () => {
		runFrames();
		const outer = fakeEl([36]);
		const inner = fakeEl([58]);
		beginOverlayExpand(outer, inner);
		expect(outer.style.height).toBe('36px');
	});

	it('안쪽 내용만 접힌 높이 → 펼친 높이로 애니메이션한다', () => {
		runFrames();
		const outer = fakeEl([36]);
		const inner = fakeEl([58]);
		beginOverlayExpand(outer, inner);
		const [frames] = inner.animate.mock.calls[0];
		expect(frames).toEqual([{ height: '36px' }, { height: '58px' }]);
	});

	it('높이가 안 변하면 애니메이션을 걸지 않는다 — 짧은 제목', () => {
		runFrames();
		const outer = fakeEl([36]);
		const inner = fakeEl([36]);
		beginOverlayExpand(outer, inner);
		expect(inner.animate).not.toHaveBeenCalled();
	});
});

describe('beginOverlayCollapse', () => {
	it('접히고 나면 고정을 풀어 원래 흐름으로 되돌린다', async () => {
		runFrames();
		const outer = fakeEl([58]);
		outer.style.height = '36px';
		const inner = fakeEl([58, 36]);
		beginOverlayCollapse(outer, inner);
		await Promise.resolve();
		await Promise.resolve();
		expect(outer.style.height).toBe('');
	});

	it('줄어드는 동안 내용을 잘라둔다 — 안 그러면 접히는 게 안 보인다', () => {
		runFrames();
		const outer = fakeEl([58]);
		const inner = fakeEl([58, 36]);
		beginOverlayCollapse(outer, inner);
		expect(inner.style.overflow).toBe('hidden');
	});
});

describe('isPointerDrivenHover', () => {
	it('좌표가 같은 hover 는 휠/레이아웃이 만든 것 — 무시한다', () => {
		const at = (x: number, y: number) => ({ clientX: x, clientY: y }) as MouseEvent;
		expect(isPointerDrivenHover(at(10, 20))).toBe(true); // 사람이 움직여 들어옴
		expect(isPointerDrivenHover(at(10, 20))).toBe(false); // 커서 그대로
		expect(isPointerDrivenHover(at(10, 21))).toBe(true); // 1px 이라도 움직이면 진짜
		expect(isPointerDrivenHover(at(10, 21))).toBe(false);
	});
});
