import { describe, it, expect, vi, afterEach } from 'vitest';
import { hoverSelect, isPointerDrivenHover } from './anchor-scroll';

/**
 * DEV-359: 호버로 선택이 옮겨갈 때 **커서 밑 행이 움직이면 안 된다** — 움직이면
 * 브라우저가 이웃 항목에 hover 를 쏘고 두 항목이 핑퐁한다. 보정이 반드시
 * `flushSync` **직후 같은 태스크**에서 끝나야 하는 이유이기도 하다(다음 프레임에
 * 하면 브라우저가 이미 새 레이아웃으로 hit-test 를 끝낸 뒤다).
 */
vi.mock('svelte', () => ({ flushSync: (fn: () => void) => fn() }));

/**
 * 상태 변경 전/후 두 레이아웃을 갖는 가짜 행. `phase.after` 가 켜지면 그때부터
 * 변경 후 값을 돌려준다 — 실제로도 `flushSync` 를 경계로 값이 바뀐다.
 * 화면상 top 은 스크롤 보정분을 반영한다(보정이 실제로 먹히는지 보기 위해).
 */
function fakeRow(
	before: { top: number; height: number },
	after: { top: number; height: number },
	scroller: { scrollTop: number },
	phase: { after: boolean }
) {
	const base = scroller.scrollTop;
	return {
		isConnected: true,
		classList: { add: vi.fn(), remove: vi.fn() },
		style: {} as CSSStyleDeclaration,
		animate: vi.fn(() => ({ finished: Promise.resolve(), cancel: vi.fn() })),
		getBoundingClientRect: () => {
			const st = phase.after ? after : before;
			return { top: st.top + (base - scroller.scrollTop), height: st.height };
		}
	} as unknown as HTMLElement & { animate: ReturnType<typeof vi.fn> };
}

afterEach(() => vi.restoreAllMocks());

describe('hoverSelect', () => {
	it('선택을 바꾼 뒤 같은 태스크에서 스크롤을 보정한다 — 커서 밑 행이 제자리', () => {
		vi.spyOn(globalThis, 'requestAnimationFrame').mockImplementation(() => 0);
		const scroller = { scrollTop: 1000 } as HTMLElement;
		const phase = { after: false };
		// 위 행이 접혀 이 행이 22px 위로 밀린 상황.
		const next = fakeRow({ top: 300, height: 36 }, { top: 278, height: 58 }, scroller, phase);
		let applied = false;
		hoverSelect({
			scroller,
			prev: null,
			next,
			apply: () => {
				applied = true;
				phase.after = true;
			}
		});
		expect(applied).toBe(true);
		expect(scroller.scrollTop).toBe(978);
	});

	it('밀리지 않았으면 스크롤을 건드리지 않는다', () => {
		vi.spyOn(globalThis, 'requestAnimationFrame').mockImplementation(() => 0);
		const scroller = { scrollTop: 500 } as HTMLElement;
		const phase = { after: false };
		const next = fakeRow({ top: 120, height: 36 }, { top: 120, height: 36 }, scroller, phase);
		hoverSelect({ scroller, prev: null, next, apply: () => (phase.after = true) });
		expect(scroller.scrollTop).toBe(500);
	});

	it('펼치는 행과 접히는 행 양쪽에 높이 애니메이션을 건다', () => {
		vi.spyOn(globalThis, 'requestAnimationFrame').mockImplementation(() => 0);
		const scroller = { scrollTop: 0 } as HTMLElement;
		const phase = { after: false };
		const prev = fakeRow({ top: 100, height: 58 }, { top: 100, height: 36 }, scroller, phase);
		const next = fakeRow({ top: 200, height: 36 }, { top: 200, height: 58 }, scroller, phase);
		hoverSelect({ scroller, prev, next, apply: () => (phase.after = true) });
		expect(prev.animate.mock.calls[0][0]).toEqual([{ height: '58px' }, { height: '36px' }]);
		expect(next.animate.mock.calls[0][0]).toEqual([{ height: '36px' }, { height: '58px' }]);
	});

	it('스크롤러가 없으면 상태 변경만 하고 조용히 넘어간다', () => {
		let applied = false;
		hoverSelect({ scroller: null, prev: null, next: null, apply: () => (applied = true) });
		expect(applied).toBe(true);
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
