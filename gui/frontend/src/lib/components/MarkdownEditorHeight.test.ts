// BUG-360 다시 고침(admin): 댓글 칸은 [M↓] 로 일반 입력칸 ↔ 마크다운 편집기를 오간다. 편집기는 **넘겨받은
// 높이로** 열리고, 바꾼 높이는 그 칸에만 알린다 — 모든 편집기가 같이 쓰는 저장 높이는 건드리지 않는다.
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import MarkdownEditor from './MarkdownEditor.svelte';

const resized: Array<() => void> = [];
vi.stubGlobal(
	'ResizeObserver',
	class {
		constructor(private cb: (e: unknown[]) => void) {
			resized.push(() => this.cb([{}]));
		}
		observe() {}
		unobserve() {}
		disconnect() {}
	}
);

const KEY = 'openguild.questEditorHeight';
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

describe('BUG-360 편집기 높이 넘겨받기', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
		localStorage.clear();
		resized.length = 0;
	});

	it('넘겨받은 높이로 열고, 바꾸면 알리되 저장 높이는 안 건드린다', async () => {
		localStorage.setItem(KEY, '480');
		let h: number | undefined = 75;
		render(MarkdownEditor, {
			props: {
				value: '',
				onError: () => {},
				minHeightRem: 4,
				persistHeight: false,
				get height() {
					return h;
				},
				set height(v: number | undefined) {
					h = v;
				}
			}
		});
		await tick();
		const wrap = document.querySelector('.editor-wrap') as HTMLElement;
		expect(wrap.style.height).toBe('75px'); // 저장된 480 이 아니라 넘겨받은 75
		Object.defineProperty(wrap, 'offsetHeight', { value: 140, configurable: true });
		resized.forEach((f) => f());
		await wait(320);
		expect(h).toBe(140);
		expect(localStorage.getItem(KEY)).toBe('480');
	});

	it('안 넘기면 예전처럼 저장 높이로 열고 바꾼 높이를 저장한다', async () => {
		localStorage.setItem(KEY, '480');
		render(MarkdownEditor, { props: { value: '', onError: () => {} } });
		await tick();
		const wrap = document.querySelector('.editor-wrap') as HTMLElement;
		expect(wrap.style.height).toBe('480px');
		Object.defineProperty(wrap, 'offsetHeight', { value: 520, configurable: true });
		resized.forEach((f) => f());
		await wait(320);
		expect(localStorage.getItem(KEY)).toBe('520');
	});
});
