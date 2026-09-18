// DEV-416: 본문을 읽으면서 댓글을 쓰면 글자를 칠 때마다 스크롤이 입력창 쪽으로 끌려 내려갔다.
// 입력창이 화면 아래에 붙어 있으면 그 요소는 이미 화면 안이라, 브라우저가 스크롤할 이유가
// 없어진다. 여기서 지키는 것은 **언제 붙고 언제 떨어지나** 다.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import ComposeDockHarness from './ComposeDockHarness.svelte';

// jsdom 에는 IntersectionObserver 가 없다 — 시험이 직접 "보인다/안 보인다" 를 흘려 넣는다.
let notify: ((visible: boolean) => void) | null = null;
class FakeIO {
	constructor(private cb: (e: Array<{ isIntersecting: boolean }>) => void) {
		notify = (visible: boolean) => this.cb([{ isIntersecting: visible }]);
	}
	observe() {}
	disconnect() {
		notify = null;
	}
}
vi.stubGlobal('IntersectionObserver', FakeIO);

function box() {
	return document.querySelector('.dock-box') as HTMLElement;
}
function isDocked() {
	return box().classList.contains('docked');
}

describe('DEV-416 입력창이 화면 밖으로 밀리면 아래에 붙는다', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
	});

	it('보이는 동안에는 제자리다 — 포커스가 있어도', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: false });
		notify!(true);
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		await tick();
		expect(isDocked()).toBe(false);
	});

	it('화면 밖 + 포커스면 붙는다', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: false });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(false);
		await tick();
		expect(isDocked()).toBe(true);
	});

	it('포커스를 잃어도 쓰던 글이 있으면 그대로 있는다', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: true });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(false);
		await tick();
		expect(isDocked()).toBe(true);
		getByRole('textbox').dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
		await tick();
		expect(isDocked()).toBe(true);
	});

	it('쓰던 글이 없으면 포커스를 잃을 때 떨어진다', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: false });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(false);
		await tick();
		expect(isDocked()).toBe(true);
		getByRole('textbox').dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
		await tick();
		expect(isDocked()).toBe(false);
	});

	it('× 를 누르면 제자리로 — 다시 밀려나도 안 붙는다', async () => {
		const { getByRole, getByLabelText } = render(ComposeDockHarness, { hasContent: true });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(false);
		await tick();
		(getByLabelText(/원래 자리로/) as HTMLButtonElement).click();
		await tick();
		expect(isDocked()).toBe(false);

		// 글은 남아 있고, 스크롤을 더 해도 다시 붙지 않는다.
		expect((getByRole('textbox') as HTMLTextAreaElement).value).toBe('쓰던 글');
		notify!(false);
		await tick();
		expect(isDocked()).toBe(false);
	});

	it('원래 자리가 다시 보이면 닫아 둔 것이 풀린다', async () => {
		const { getByRole, getByLabelText } = render(ComposeDockHarness, { hasContent: true });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(false);
		await tick();
		(getByLabelText(/원래 자리로/) as HTMLButtonElement).click();
		await tick();
		notify!(true); // 스크롤을 올려 제자리가 보였다
		await tick();
		notify!(false); // 다시 밀려나면
		await tick();
		expect(isDocked()).toBe(true);
	});
});
