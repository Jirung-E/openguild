// DEV-416: 본문을 읽으면서 댓글을 쓰면 글자를 칠 때마다 스크롤이 입력창 쪽으로 끌려 내려갔다.
// 입력창이 화면 아래에 붙어 있으면 그 요소는 이미 화면 안이라, 브라우저가 스크롤할 이유가
// 없어진다. 여기서 지키는 것은 **언제 붙고 언제 떨어지나** 다.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import ComposeDockHarness from './ComposeDockHarness.svelte';

// jsdom 에는 IntersectionObserver 가 없다 — 시험이 직접 "얼마나 보이나" 를 흘려 넣는다.
// 비율인 것이 중요하다: **한 귀퉁이라도 잘리면** 붙어야 한다(admin).
let notify: ((ratio: number) => void) | null = null;
class FakeIO {
	constructor(private cb: (e: Array<{ intersectionRatio: number }>) => void) {
		notify = (ratio: number) => this.cb([{ intersectionRatio: ratio }]);
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
		notify!(1);
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		await tick();
		expect(isDocked()).toBe(false);
	});

	it('화면 밖 + 포커스면 붙는다', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: false });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		expect(isDocked()).toBe(true);
	});

	it('포커스를 잃어도 쓰던 글이 있으면 그대로 있는다', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: true });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		expect(isDocked()).toBe(true);
		getByRole('textbox').dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
		await tick();
		expect(isDocked()).toBe(true);
	});

	it('쓰던 글이 없으면 포커스를 잃을 때 떨어진다', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: false });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		expect(isDocked()).toBe(true);
		getByRole('textbox').dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
		await tick();
		expect(isDocked()).toBe(false);
	});

	it('× 를 누르면 제자리로 — 다시 밀려나도 안 붙는다', async () => {
		const { getByRole, getByLabelText } = render(ComposeDockHarness, { hasContent: true });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		(getByLabelText(/원래 자리로/) as HTMLButtonElement).click();
		await tick();
		expect(isDocked()).toBe(false);

		// 글은 남아 있고, 스크롤을 더 해도 다시 붙지 않는다.
		expect((getByRole('textbox') as HTMLTextAreaElement).value).toBe('쓰던 글');
		notify!(0);
		await tick();
		expect(isDocked()).toBe(false);
	});

	it('원래 자리가 다시 보이면 닫아 둔 것이 풀린다', async () => {
		const { getByRole, getByLabelText } = render(ComposeDockHarness, { hasContent: true });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		(getByLabelText(/원래 자리로/) as HTMLButtonElement).click();
		await tick();
		notify!(1); // 스크롤을 올려 제자리가 보였다
		await tick();
		notify!(0); // 다시 밀려나면
		await tick();
		expect(isDocked()).toBe(true);
	});
});

// admin 반려 반영 — 반쯤 걸친 상태가 제일 불편하다. 완전히 나가야 붙는 것이 아니라
// **한 귀퉁이라도 잘리면** 붙어야 한다.
describe('DEV-416 조금이라도 가려지면 붙는다', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
	});

	it('절반쯤 걸쳐도 붙는다', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: false });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0.5);
		await tick();
		expect(isDocked()).toBe(true);
	});

	it('온전히 보이면 제자리다', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: false });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(1);
		await tick();
		expect(isDocked()).toBe(false);
	});
});

// admin 반려 반영 — 아래쪽 빈 자리(버튼 한 줄)가 화면을 먹었다. 보내기를 머리줄로 올리고,
// 팝업 자체에는 스크롤이 생기면 안 된다(버튼이 밀려 올라간다).
describe('DEV-416 붙었을 때의 모양', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
	});

	async function dock(props: Record<string, unknown> = {}) {
		const r = render(ComposeDockHarness, { hasContent: true, ...props });
		r.getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		return r;
	}

	it('오른쪽 위에 동그란 버튼 셋 — 보내기 · 컴팩트 · 닫기', async () => {
		await dock({ withSubmit: true });
		const labels = Array.from(document.querySelectorAll('.dock-btn')).map(
			(b) => b.getAttribute('aria-label') ?? ''
		);
		expect(labels).toEqual(['보내기', '좁게 보기', '원래 자리로 (글은 남습니다)']);
	});

	it('컴팩트를 켜면 좁게 — 다시 누르면 넓게', async () => {
		await dock({ withSubmit: true });
		const btn = Array.from(document.querySelectorAll('.dock-btn')).find(
			(b) => b.getAttribute('aria-label') === '좁게 보기'
		) as HTMLButtonElement;
		btn.click();
		await tick();
		expect(box().classList.contains('compact')).toBe(true);
		const back = document.querySelector('[aria-label="넓게 보기"]') as HTMLButtonElement;
		expect(back).toBeTruthy();
		back.click();
		await tick();
		expect(box().classList.contains('compact')).toBe(false);
	});

	it('머리줄의 보내기가 실제로 보낸다', async () => {
		const sent: number[] = [];
		await dock({ withSubmit: true, onsubmitSpy: () => sent.push(1) });
		const send = document.querySelector('[aria-label="보내기"]') as HTMLButtonElement;
		send.click();
		await tick();
		expect(sent).toEqual([1]);
	});
});
