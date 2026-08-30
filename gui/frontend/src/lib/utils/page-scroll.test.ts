// BUG-258: `?comment=N` 딥링크가 엉뚱한 곳에 서던 문제의 방지선.
//
// 핵심은 **언제** 스크롤하느냐다. 앵커가 생기자마자 부드럽게 스크롤하면,
// 애니메이션이 도는 동안 위쪽 컨텐츠가 자라 대상이 밀려나고 엉뚱한 곳에
// 선다. 그래서 높이가 잦아들 때까지 기다렸다가 가고, 간 뒤에도 변하면
// 다시 맞춘다.
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { scrollIntoViewWhenSettled } from './page-scroll';

/** `<main>` 을 세워 `pageScrollHeight()` 가 그 높이를 읽게 한다. */
function mountMain(): HTMLElement {
	const main = document.createElement('main');
	document.body.appendChild(main);
	return main;
}

/** scrollHeight 는 jsdom 에서 0 고정이라 직접 정의해 조작한다. */
function setHeight(el: HTMLElement, h: number) {
	Object.defineProperty(el, 'scrollHeight', { value: h, configurable: true });
}

describe('scrollIntoViewWhenSettled', () => {
	let main: HTMLElement;
	let target: HTMLElement;
	let calls: ScrollBehavior[];

	beforeEach(() => {
		vi.useFakeTimers();
		document.body.innerHTML = '';
		main = mountMain();
		setHeight(main, 1000);
		target = document.createElement('div');
		target.id = 'anchor';
		main.appendChild(target);
		calls = [];
		target.scrollIntoView = ((opts?: ScrollIntoViewOptions) => {
			calls.push((opts?.behavior ?? 'auto') as ScrollBehavior);
		}) as typeof target.scrollIntoView;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	const get = () => document.getElementById('anchor');

	it('높이가 잦아들기 전에는 스크롤하지 않는다', () => {
		scrollIntoViewWhenSettled(get, { settleMs: 150, pollMs: 32 });
		// 계속 자라는 동안에는 가만히 있어야 한다 — 지금 가면 대상이 밀려난다.
		// 높이를 **먼저** 바꾸고 나아간다. 순서를 뒤집으면 첫 회에 같은 높이가
		// 유지돼 안정 구간이 생겨 버린다(처음 이 테스트를 그렇게 썼다가 헛
		// 실패를 봤다).
		for (let i = 1; i <= 6; i++) {
			setHeight(main, 1000 + i * 200);
			vi.advanceTimersByTime(100);
		}
		expect(calls).toEqual([]);
	});

	it('잦아들면 부드럽게 한 번 간다', () => {
		scrollIntoViewWhenSettled(get, { settleMs: 150, pollMs: 32 });
		vi.advanceTimersByTime(200);
		expect(calls).toEqual(['smooth']);
	});

	it('계속 자라기만 해도 maxWaitMs 에서는 포기하고 간다 — 영영 안 가면 그게 더 나쁘다', () => {
		scrollIntoViewWhenSettled(get, { settleMs: 150, maxWaitMs: 500, pollMs: 32 });
		for (let i = 1; i <= 20; i++) {
			setHeight(main, 1000 + i * 50);
			vi.advanceTimersByTime(32);
		}
		// 포기하고 간 뒤에도 높이가 계속 변하니 뒤따라 보정한다 — 그게 맞다.
		// 확인할 것은 **처음 한 번만 부드럽고 나머지는 auto** 라는 점이다.
		expect(calls[0]).toBe('smooth');
		expect(calls.slice(1).every((b) => b === 'auto')).toBe(true);
	});

	it('간 뒤에 높이가 또 변하면 다시 맞춘다 — 이때는 auto (애니메이션끼리 싸우지 않게)', () => {
		scrollIntoViewWhenSettled(get, { settleMs: 150, watchMs: 1000, pollMs: 32 });
		vi.advanceTimersByTime(200);
		expect(calls).toEqual(['smooth']);
		setHeight(main, 2400); // 늦게 뜬 이미지가 위쪽을 밀어냈다
		vi.advanceTimersByTime(64);
		expect(calls.slice(1)).toContain('auto');
		expect(calls.slice(1).every((b) => b === 'auto')).toBe(true);
	});

	it('watchMs 가 지나면 더 이상 따라가지 않는다', () => {
		scrollIntoViewWhenSettled(get, { settleMs: 150, watchMs: 300, pollMs: 32 });
		vi.advanceTimersByTime(200);
		const afterFirst = calls.length;
		vi.advanceTimersByTime(1000);
		setHeight(main, 5000);
		vi.advanceTimersByTime(500);
		expect(calls.length).toBe(afterFirst);
	});

	it('취소하면 그 뒤로 아무것도 안 한다 — 다른 댓글로 다시 눌렀을 때 둘이 싸우면 안 된다', () => {
		const cancel = scrollIntoViewWhenSettled(get, { settleMs: 150, pollMs: 32 });
		cancel();
		vi.advanceTimersByTime(2000);
		expect(calls).toEqual([]);
	});

	it('reduced-motion 이면 처음부터 auto', () => {
		scrollIntoViewWhenSettled(get, { settleMs: 150, pollMs: 32, smooth: false });
		vi.advanceTimersByTime(200);
		expect(calls).toEqual(['auto']);
	});

	it('실제로 스크롤한 순간에만 onScrolled 가 불린다', () => {
		const seen: string[] = [];
		scrollIntoViewWhenSettled(get, {
			settleMs: 150,
			pollMs: 32,
			onScrolled: (el) => seen.push(el.id)
		});
		expect(seen).toEqual([]);
		vi.advanceTimersByTime(200);
		expect(seen).toEqual(['anchor']);
		// 뒤따르는 보정에서 또 부르면 강조가 계속 되살아난다.
		setHeight(main, 3000);
		vi.advanceTimersByTime(200);
		expect(seen).toEqual(['anchor']);
	});
});
