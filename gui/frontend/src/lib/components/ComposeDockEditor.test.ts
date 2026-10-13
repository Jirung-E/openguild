// BUG-360: 팝업으로 붙은 마크다운 편집기가 위쪽만 보이고 아래가 하얗던 것.
//
// 고침은 둘이다 — 붙었을 때 바깥 상자의 높이를 푸는 것(CSS, 브라우저에서 확인)과, 그렇게 줄어든 높이를
// "사용자가 정한 편집기 높이" 로 **저장하지 않는 것**. 여기서는 뒤의 것을 지킨다: 저장해 버리면 팝업을
// 닫은 뒤 여는 편집기가 엉뚱하게 작아진다.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import Harness from './ComposeDockEditorHarness.svelte';

let showRatio: ((ratio: number) => void) | null = null;
vi.stubGlobal(
	'IntersectionObserver',
	class {
		constructor(private cb: (e: Array<{ intersectionRatio: number }>) => void) {
			showRatio = (ratio: number) => this.cb([{ intersectionRatio: ratio }]);
		}
		observe() {}
		unobserve() {}
		disconnect() {}
	}
);
/** 편집기 상자의 크기 변화를 시험이 직접 흘린다. */
const resized: Array<(h: number) => void> = [];
vi.stubGlobal(
	'ResizeObserver',
	class {
		constructor(private cb: (e: Array<{ contentRect: { height: number } }>) => void) {
			resized.push((h) => this.cb([{ contentRect: { height: h } }]));
		}
		observe() {}
		unobserve() {}
		disconnect() {}
	}
);

const KEY = 'openguild.questEditorHeight';
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

describe('BUG-360 팝업 안의 마크다운 편집기', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
		localStorage.clear();
		resized.length = 0;
	});

	it('붙어 있는 동안 줄어든 높이는 편집기 높이로 저장하지 않는다', async () => {
		render(Harness);
		await tick();
		await wait(50);
		const box = document.querySelector('.dock-box') as HTMLElement;
		// 제자리에서 손으로 정한 높이 — 저장된다.
		resized.forEach((f) => f(420));
		await wait(320);
		expect(localStorage.getItem(KEY)).toBe('420');

		// 입력칸이 반쯤 가려지면(쓰던 글이 있으니) 팝업으로 붙는다.
		showRatio!(0.4);
		await wait(320);
		await tick();
		expect(box.classList.contains('docked')).toBe(true);
		resized.forEach((f) => f(74));
		await wait(320);
		expect(localStorage.getItem(KEY), '팝업 크기를 저장했다').toBe('420');

		// 다시 보이면 제자리 — 그때부터는 다시 저장한다.
		showRatio!(1);
		await wait(320);
		await tick();
		expect(box.classList.contains('docked')).toBe(false);
		resized.forEach((f) => f(380));
		await wait(320);
		expect(localStorage.getItem(KEY)).toBe('380');
	});
});
