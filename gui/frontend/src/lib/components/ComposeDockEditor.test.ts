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
			// 편집기는 상자의 바깥 높이(offsetHeight)를 적는다 — jsdom 은 늘 0 이라 시험이 넣어 준다.
			resized.push((h) => {
				const wrap = document.querySelector('.editor-wrap');
				if (wrap) Object.defineProperty(wrap, 'offsetHeight', { value: h, configurable: true });
				this.cb([{ contentRect: { height: h - 2 } }]);
			});
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

	it('BUG-360 다시 고침: 팝업에서 손잡이로 끈 높이는 제자리로 돌아오면 저장된 높이로 되돌린다', async () => {
		localStorage.setItem(KEY, '420');
		render(Harness);
		await tick();
		await wait(50);
		const box = document.querySelector('.dock-box') as HTMLElement;
		const wrap = document.querySelector('.editor-wrap') as HTMLElement;
		expect(wrap.style.height).toBe('420px');

		showRatio!(0.4);
		await wait(320);
		await tick();
		expect(box.classList.contains('docked')).toBe(true);
		// 팝업에서 위쪽 손잡이를 키보드로 키운다(↑ = 키우기) — 손잡이가 인라인 높이와 상한을 같이 적는다.
		const grip = document.querySelector('.size-grip') as HTMLElement;
		grip.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
		await tick();
		expect(wrap.style.getPropertyValue('--compose-cap'), '팝업 높이를 정하지 않았다').not.toBe('');
		expect(wrap.style.height).not.toBe('420px');

		showRatio!(1);
		await wait(320);
		await tick();
		expect(box.classList.contains('docked')).toBe(false);
		expect(wrap.style.height, '팝업 높이가 제자리 편집기에 남았다').toBe('420px');
	});
});

// DEV-424: 편집기(CodeMirror)에서도 먹어야 한다. CodeMirror 는 자기 keymap 이 Mod-Enter 에 '빈 줄 넣기' 를
// 걸어 두어, 먼저 받지 않으면 빈 줄이 생긴 뒤에 보내진다.
describe('DEV-424 편집기에서 Ctrl+Enter', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
		localStorage.clear();
	});

	it('보내고, 빈 줄은 안 생긴다', async () => {
		const sent: number[] = [];
		render(Harness, { onsubmitSpy: () => sent.push(1) });
		await tick();
		const content = document.querySelector('.cm-content') as HTMLElement;
		const lines = () => document.querySelectorAll('.cm-line').length;
		const before = lines();
		content.dispatchEvent(
			new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true, cancelable: true })
		);
		await tick();
		expect(sent).toEqual([1]);
		expect(lines(), 'CodeMirror 가 빈 줄을 넣었다').toBe(before);
	});
});
