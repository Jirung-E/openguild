// BUG-318: 휴대폰에서는 브라우저가 주는 크기 손잡이를 못 쓴다 — iOS 사파리는 `resize` 를
// 무시하고 안드로이드 크롬은 손가락으로 안 끌린다. 직접 둔 손잡이가 **손가락·마우스·키보드**
// 셋 다로 동작하는지 본다.

import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import SizeGrip from './SizeGrip.svelte';

// `target` 은 testing-library 의 예약 이름(그릴 자리)이라 `props:` 로 감싸 넘긴다 —
// 안 그러면 컴포넌트가 아니라 렌더러가 가져간다.
//
// jsdom 에는 PointerEvent 가 없다 — 필요한 만큼만 흉내낸다.
class FakePointerEvent extends MouseEvent {
	pointerId: number;
	constructor(type: string, init: MouseEventInit & { pointerId?: number } = {}) {
		super(type, init);
		this.pointerId = init.pointerId ?? 1;
	}
}
if (typeof PointerEvent === 'undefined') {
	(globalThis as Record<string, unknown>).PointerEvent = FakePointerEvent;
}

function targetEl(h: number): HTMLElement {
	const el = document.createElement('div');
	el.style.height = `${h}px`;
	// jsdom 은 배치를 안 한다 — 높이를 물으면 style 값을 돌려주게 한다.
	el.getBoundingClientRect = () => ({ height: parseFloat(el.style.height) || 0 }) as DOMRect;
	document.body.appendChild(el);
	return el;
}

function grip(): HTMLElement {
	return document.querySelector('.size-grip') as HTMLElement;
}

function drag(dy: number) {
	const g = grip();
	g.setPointerCapture = () => {};
	g.releasePointerCapture = () => {};
	g.dispatchEvent(new FakePointerEvent('pointerdown', { clientY: 100, bubbles: true }));
	g.dispatchEvent(new FakePointerEvent('pointermove', { clientY: 100 + dy, bubbles: true }));
	g.dispatchEvent(new FakePointerEvent('pointerup', { clientY: 100 + dy, bubbles: true }));
}

describe('BUG-318 크기 손잡이', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
		// 한계값의 단위는 rem 이다 — 기준을 못 박아 두고 잰다(jsdom 기본도 16px).
		document.documentElement.style.fontSize = '16px';
	});

	it('끌면 그만큼 커진다', async () => {
		const el = targetEl(200);
		render(SizeGrip, { props: { target: el } });
		await tick();
		drag(120);
		expect(el.style.height).toBe('320px');
	});

	it('위로 끌면 작아진다', async () => {
		const el = targetEl(300);
		render(SizeGrip, { props: { target: el } });
		await tick();
		drag(-100);
		expect(el.style.height).toBe('200px');
	});

	// 손이 미끄러져도 쓸 수 없게 줄어들면 안 된다.
	it('최소보다 작아지지 않는다', async () => {
		const el = targetEl(100);
		render(SizeGrip, { props: { target: el, min: 4 } }); // 4rem = 64px
		await tick();
		drag(-500);
		expect(el.style.height).toBe('64px');
	});

	it('최대보다 커지지 않는다', async () => {
		const el = targetEl(100);
		render(SizeGrip, { props: { target: el, max: 18.75 } }); // 18.75rem = 300px
		await tick();
		drag(9999);
		expect(el.style.height).toBe('300px');
	});

	// 손가락도 마우스도 못 쓰는 사람이 있다.
	it('화살표로도 바꾼다', async () => {
		const el = targetEl(200);
		render(SizeGrip, { props: { target: el, step: 1.5 } }); // 1.5rem = 24px
		await tick();
		grip().dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
		expect(el.style.height).toBe('224px');
		grip().dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
		expect(el.style.height).toBe('200px');
	});

	// BUG-319: 붙은 입력창은 입력칸에 높이 상한을 걸어 둔다. 그걸 같이 안 풀면 손잡이로
	// 정한 높이가 그냥 덮어써져 "끌어도 안 움직인다" 가 된다(admin).
	it('상한도 같이 풀어 준다', async () => {
		const el = targetEl(200);
		render(SizeGrip, { props: { target: el } });
		await tick();
		drag(150);
		expect(el.style.height).toBe('350px');
		expect(el.style.getPropertyValue('--compose-cap')).toBe('350px');
	});

	// BUG-321(admin): "사이즈 조절 핸들은 ui 크기에 따라 달라져야한다." 이 앱은 UI 크기를
	// 루트 글꼴로 조절한다 — 한계값을 px 로 못 박으면 크게 쓰는 사람에게 최소 높이가
	// 상대적으로 쪼그라들고 화살표 한 칸도 너무 잘다.
	it('UI 크기를 키우면 최소 높이도 같이 커진다', async () => {
		document.documentElement.style.fontSize = '32px'; // 200%
		const el = targetEl(500);
		render(SizeGrip, { props: { target: el, min: 4 } });
		await tick();
		drag(-9999);
		expect(el.style.height).toBe('128px'); // 4rem × 32px
	});

	it('UI 크기를 키우면 화살표 한 칸도 같이 커진다', async () => {
		document.documentElement.style.fontSize = '32px';
		const el = targetEl(200);
		render(SizeGrip, { props: { target: el, step: 1.5 } });
		await tick();
		grip().dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
		expect(el.style.height).toBe('248px'); // 200 + 1.5rem × 32px
	});

	// 아직 안 그려진 편집기에 붙을 수 있다 — 그때 터져서는 안 된다.
	it('대상이 없어도 안 터진다', async () => {
		render(SizeGrip, { props: {} });
		await tick();
		expect(() => drag(50)).not.toThrow();
	});
});
