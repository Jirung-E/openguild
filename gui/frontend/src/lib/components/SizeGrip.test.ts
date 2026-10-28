// BUG-318: 휴대폰에서는 브라우저가 주는 크기 손잡이를 못 쓴다 — iOS 사파리는 `resize` 를
// 무시하고 안드로이드 크롬은 손가락으로 안 끌린다. 직접 둔 손잡이가 **손가락·마우스·키보드**
// 셋 다로 동작하는지 본다.

import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import SizeGrip from './SizeGrip.svelte';
import { COMPOSE_DOCK } from '$lib/utils/compose-dock-context';

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
		render(SizeGrip, {
			props: { target: el },
			context: new Map([[COMPOSE_DOCK, { docked: () => true }]])
		});
		await tick();
		drag(-150); // 팝업에서는 손잡이가 위 — 올릴수록 커진다
		expect(el.style.height).toBe('350px');
		expect(el.style.getPropertyValue('--compose-cap')).toBe('350px');
	});

	// BUG-360 다시 고침(admin): 팝업은 제자리 높이로 뜨되 제 최대 높이를 넘지 않는다. 제자리에서 끈 높이까지
	// 상한으로 적으면 그 높이가 팝업의 최대 높이를 이겨 팝업이 제자리만큼 커졌다.
	it('제자리에서 끌면 상한은 안 건드린다', async () => {
		const el = targetEl(200);
		render(SizeGrip, { props: { target: el } });
		await tick();
		drag(150);
		expect(el.style.height).toBe('350px');
		expect(el.style.getPropertyValue('--compose-cap')).toBe('');
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

	// BUG-321(admin): 위아래 간격이 같아야 한다. 붙는 자리마다 줄 간격이 달라서 CSS 에
	// 못 박으면 어디선가 겹친다 — 실제로 작업 기록에서 4.4px 을 파고들었다.
	it('늘어선 자리에서는 줄 간격만큼 당겨 양쪽을 맞춘다', async () => {
		const box = document.createElement('div');
		box.style.display = 'flex';
		box.style.flexDirection = 'column';
		box.style.rowGap = '6.4px';
		document.body.appendChild(box);
		const el = targetEl(200);
		box.appendChild(el);
		// 붙을 자리에 **바로 그려야** 한다 — 그리고 나서 옮기면 이미 잰 뒤다.
		render(SizeGrip, { props: { target: el }, target: box });
		await tick();
		// 2px 만 남기고 줄 간격(6.4)을 당긴다.
		expect(grip().style.marginTop).toBe(`${2 - 6.4}px`);
	});

	// 평범한 블록에서는 여백이 서로 합쳐진다 — 빼면 그대로 파고든다.
	it('평범한 블록에서는 당기지 않는다', async () => {
		const box = document.createElement('div');
		document.body.appendChild(box);
		const el = targetEl(200);
		box.appendChild(el);
		render(SizeGrip, { props: { target: el }, target: box });
		await tick();
		expect(grip().style.marginTop).toBe('2px');
		expect(grip().style.marginBottom).toBe('2px');
	});

	// BUG-323(admin): "아래쪽에 붙이니까 확장시키는게 불편함." 팝업은 화면 바닥에 붙어
	// 있어서, 아래 손잡이를 내려 끌어도 상자는 위로 자란다 — 손과 반대다. 팝업 안에서는
	// 손잡이가 위로 가고 **올릴수록 커진다**.
	it('팝업 안에서는 올릴수록 커진다', async () => {
		const el = targetEl(200);
		render(SizeGrip, {
			props: { target: el },
			context: new Map([[COMPOSE_DOCK, { docked: () => true }]])
		});
		await tick();
		drag(-100);
		expect(el.style.height).toBe('300px');
	});

	it('팝업 안에서는 ↑ 가 키우는 쪽이다', async () => {
		const el = targetEl(200);
		render(SizeGrip, {
			props: { target: el, step: 1.5 },
			context: new Map([[COMPOSE_DOCK, { docked: () => true }]])
		});
		await tick();
		grip().dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
		expect(el.style.height).toBe('224px');
	});

	// 팝업 밖에서는 그대로 아래다 — 제자리 입력칸·작업 기록·메모 상자.
	it('팝업 밖에서는 내릴수록 커진다', async () => {
		const el = targetEl(200);
		render(SizeGrip, {
			props: { target: el },
			context: new Map([[COMPOSE_DOCK, { docked: () => false }]])
		});
		await tick();
		drag(100);
		expect(el.style.height).toBe('300px');
	});

	// 아직 안 그려진 편집기에 붙을 수 있다 — 그때 터져서는 안 된다.
	it('대상이 없어도 안 터진다', async () => {
		render(SizeGrip, { props: {} });
		await tick();
		expect(() => drag(50)).not.toThrow();
	});
});

// DEV-460(admin): 손잡이가 화면 아래쪽이면 마우스가 더 못 내려가 원하는 만큼 못 키운다 — Shift 를 누르고 끌면 2배.
describe('DEV-460 Shift 로 끌면 2배', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
		document.documentElement.style.fontSize = '16px';
	});

	function moves(steps: { y: number; shift?: boolean }[]) {
		const g = grip();
		g.setPointerCapture = () => {};
		g.releasePointerCapture = () => {};
		g.dispatchEvent(new FakePointerEvent('pointerdown', { clientY: 100, bubbles: true }));
		for (const s of steps) {
			g.dispatchEvent(
				new FakePointerEvent('pointermove', { clientY: s.y, shiftKey: !!s.shift, bubbles: true })
			);
		}
		g.dispatchEvent(new FakePointerEvent('pointerup', { bubbles: true }));
	}

	it('Shift 를 누르고 끌면 움직인 거리의 2배', async () => {
		const el = targetEl(200);
		render(SizeGrip, { props: { target: el } });
		await tick();
		moves([{ y: 160, shift: true }]);
		expect(el.style.height).toBe('320px');
	});

	it('끄는 도중 Shift 를 누르거나 떼도 튀지 않는다 — 그 사이 움직인 만큼만 곱한다', async () => {
		const el = targetEl(200);
		render(SizeGrip, { props: { target: el } });
		await tick();
		moves([{ y: 150 }, { y: 150, shift: true }, { y: 200, shift: true }, { y: 220 }]);
		// 50 + 0 + 50×2 + 20
		expect(el.style.height).toBe('370px');
	});

	it('한계에 걸렸다가 되돌리면 곧바로 줄어든다', async () => {
		const el = targetEl(100);
		render(SizeGrip, { props: { target: el, max: 18.75 } }); // 300px
		await tick();
		moves([{ y: 600 }, { y: 580 }]);
		expect(el.style.height).toBe('280px');
	});
});
