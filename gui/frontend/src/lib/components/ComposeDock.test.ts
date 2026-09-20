// DEV-416: 본문을 읽으면서 댓글을 쓰면 글자를 칠 때마다 스크롤이 입력창 쪽으로 끌려 내려갔다.
// 입력창이 화면 아래에 붙어 있으면 그 요소는 이미 화면 안이라, 브라우저가 스크롤할 이유가
// 없어진다. 여기서 지키는 것은 **언제 붙고 언제 떨어지나** 다.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';

/** DEV-416: 붙고 떨어지는 판단에는 머무름(SETTLE_MS)이 있다 — 되먹임으로 떨리지 않게.
 *  **두 번째 전환부터**는 그만큼 늦게 온다. 시험도 기다린다. */
async function settled() {
	await new Promise((r) => setTimeout(r, 320));
	await tick();
}
import ComposeDockHarness from './ComposeDockHarness.svelte';

// jsdom 에는 IntersectionObserver 가 없다 — 시험이 직접 "얼마나 보이나" 를 흘려 넣는다.
// 비율인 것이 중요하다: **한 귀퉁이라도 잘리면** 붙어야 한다(admin).
let notify: ((ratio: number) => void) | null = null;
/** BUG-312: **무엇을** 보고 있는지도 시험한다 — 재는 기준은 입력칸 하나여야 한다. */
let watching: Element[] = [];
class FakeIO {
	constructor(private cb: (e: Array<{ intersectionRatio: number }>) => void) {
		notify = (ratio: number) => this.cb([{ intersectionRatio: ratio }]);
	}
	observe(el: Element) {
		watching.push(el);
	}
	unobserve(el: Element) {
		watching = watching.filter((x) => x !== el);
	}
	disconnect() {
		notify = null;
		watching = [];
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
		// 좁게/넓게는 다시 켜도 남는다 — 시험끼리 새지 않게 지운다.
		localStorage.clear();
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
		await settled();
		expect(isDocked()).toBe(false);
	});

	it('× 를 누르면 제자리로 — 다시 밀려나도 안 붙는다', async () => {
		const { getByRole, getByLabelText } = render(ComposeDockHarness, { hasContent: true });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		(getByLabelText(/원래 자리로/) as HTMLButtonElement).click();
		await settled();
		expect(isDocked()).toBe(false);

		// 글은 남아 있고, 스크롤을 더 해도 다시 붙지 않는다.
		expect((getByRole('textbox') as HTMLTextAreaElement).value).toBe('쓰던 글');
		notify!(0);
		await settled();
		expect(isDocked()).toBe(false);
	});

	it('원래 자리가 다시 보이면 닫아 둔 것이 풀린다', async () => {
		const { getByRole, getByLabelText } = render(ComposeDockHarness, { hasContent: true });
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		(getByLabelText(/원래 자리로/) as HTMLButtonElement).click();
		await settled();
		notify!(1); // 스크롤을 올려 제자리가 보였다
		await settled();
		notify!(0); // 다시 밀려나면
		await settled();
		expect(isDocked()).toBe(true);
	});
});

// admin 반려 반영 — 반쯤 걸친 상태가 제일 불편하다. 완전히 나가야 붙는 것이 아니라
// **한 귀퉁이라도 잘리면** 붙어야 한다.
describe('DEV-416 조금이라도 가려지면 붙는다', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
		// 좁게/넓게는 다시 켜도 남는다 — 시험끼리 새지 않게 지운다.
		localStorage.clear();
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

	// BUG-312(admin): "팝업 기준은 '댓글입력박스'만임. 지금은 '작성자', '댓글추가버튼' 등이
	// 밖으로 나가도 팝업이 되어버린다." — 재는 대상 자체를 좁힌다.
	//
	// BUG-315: 그런데 **입력칸 자체를 보면 안 된다.** 붙으면 상자가 화면 아래로 옮겨 가고
	// 입력칸도 같이 옮겨 가 보이게 되니, "보이니 떨어져라 → 떨어지니 안 보인다" 가 끝없이
	// 돈다(admin: 떴다 내려갔다 반복). 그래서 **제자리에 남는 표식**을 본다.
	it('재는 것은 제자리에 남는 표식이다 — 상자 안의 것이 아니다', async () => {
		render(ComposeDockHarness, { hasContent: false });
		await tick();
		expect(watching.length, '한 곳만 본다').toBe(1);
		const seen = watching[0] as HTMLElement;
		expect(seen.classList.contains('dock-probe'), `본 것: ${seen.className}`).toBe(true);
		// 이것이 핵심이다 — 보는 것이 상자 **밖**이라야 붙어도 안 움직인다.
		expect(document.querySelector('.dock-box')?.contains(seen)).toBe(false);
	});

	// 붙은 뒤에도 보는 것이 그대로 제자리에 있어야 한다 — 옮겨 갔으면 고리가 생긴다.
	it('붙은 뒤에도 표식은 제자리다', async () => {
		const { getByRole } = render(ComposeDockHarness, { hasContent: true });
		const seen = watching[0] as HTMLElement;
		const before = seen.getBoundingClientRect().top;
		getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		expect(box().classList.contains('docked')).toBe(true);
		expect(watching[0], '보던 것이 바뀌면 안 된다').toBe(seen);
		expect(seen.getBoundingClientRect().top).toBe(before);
		expect(document.querySelector('.dock-box')?.contains(seen)).toBe(false);
	});
});

// admin 반려 반영 — 아래쪽 빈 자리(버튼 한 줄)가 화면을 먹었다. 보내기를 머리줄로 올리고,
// 팝업 자체에는 스크롤이 생기면 안 된다(버튼이 밀려 올라간다).
describe('DEV-416 붙었을 때의 모양', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
		// 좁게/넓게는 다시 켜도 남는다 — 시험끼리 새지 않게 지운다.
		localStorage.clear();
	});

	async function dock(props: Record<string, unknown> = {}) {
		const r = render(ComposeDockHarness, { hasContent: true, ...props });
		r.getByRole('textbox').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
		notify!(0);
		await tick();
		return r;
	}

	it('오른쪽 위에 동그란 버튼 셋 — 보내기 · 좁게/넓게 · 닫기', async () => {
		await dock({ withSubmit: true });
		const labels = Array.from(document.querySelectorAll('.dock-btn')).map(
			(b) => b.getAttribute('aria-label') ?? ''
		);
		// 좁게가 기본이므로 가운데 버튼은 '넓게 보기' 다.
		expect(labels).toEqual(['보내기', '넓게 보기', '원래 자리로 (글은 남습니다)']);
	});

	// BUG-312(admin): "기본적으로 컴팩트 모드로 떠야한다."
	it('처음부터 좁게 뜬다', async () => {
		await dock({ withSubmit: true });
		expect(box().classList.contains('compact')).toBe(true);
	});

	// BUG-312(admin): "배경 없이 오직 댓글입력박스와 동그라미 버튼들만" — '작성 중' 글자는
	// 그 모양을 해친다.
	it('좁게일 때는 이름표가 없다 — 넓히면 나온다', async () => {
		await dock({ withSubmit: true });
		expect(document.querySelector('.dock-label')).toBeNull();
		(document.querySelector('[aria-label="넓게 보기"]') as HTMLButtonElement).click();
		await tick();
		expect(document.querySelector('.dock-label')).toBeTruthy();
	});

	it('넓혔다 다시 좁게', async () => {
		await dock({ withSubmit: true });
		const wide = document.querySelector('[aria-label="넓게 보기"]') as HTMLButtonElement;
		wide.click();
		await tick();
		expect(box().classList.contains('compact')).toBe(false);
		const narrow = document.querySelector('[aria-label="좁게 보기"]') as HTMLButtonElement;
		expect(narrow).toBeTruthy();
		narrow.click();
		await tick();
		expect(box().classList.contains('compact')).toBe(true);
	});

	// BUG-312(admin): "'컴팩트모드 토글 상태'는 앱을 다시시작해도 유지되어야함."
	it('넓게로 바꿔 두면 다시 켜도 넓다', async () => {
		await dock({ withSubmit: true });
		(document.querySelector('[aria-label="넓게 보기"]') as HTMLButtonElement).click();
		await tick();

		// 앱을 다시 켠 셈 — 화면을 지우고 처음부터 그린다.
		document.body.innerHTML = '';
		await dock({ withSubmit: true });
		expect(box().classList.contains('compact')).toBe(false);
	});

	// 동그라미 안의 그림이 기준선만큼 치우쳐 보이던 것(admin) — 글자가 아니라 그림으로 그린다.
	it('버튼 안은 글자가 아니라 그림이다', async () => {
		await dock({ withSubmit: true });
		for (const b of Array.from(document.querySelectorAll('.dock-btn'))) {
			expect(b.querySelector('svg'), `${b.getAttribute('aria-label')} 에 그림이 없다`).toBeTruthy();
			expect(b.textContent?.trim(), `${b.getAttribute('aria-label')} 에 글자가 남아 있다`).toBe('');
		}
	});

	// admin: [좁게 보기] 를 누르면 팝업이 그냥 사라졌다. 버튼을 누르는 순간 입력칸이 포커스를
	// 잃는데, 쓴 글이 없으면 붙어 있을 이유가 사라지기 때문이었다.
	it('머리줄 버튼을 눌러도 팝업이 안 사라진다 — 상자 안으로 가는 포커스는 잃은 것이 아니다', async () => {
		const r = await dock({ withSubmit: true });
		const compact = document.querySelector('[aria-label="넓게 보기"]') as HTMLButtonElement;
		// 실제 순서대로: 입력칸이 포커스를 잃고 → 버튼으로 간다.
		r.getByRole('textbox').dispatchEvent(
			new FocusEvent('focusout', { bubbles: true, relatedTarget: compact })
		);
		compact.click();
		await settled();
		expect(isDocked(), '버튼을 눌렀다고 사라지면 안 된다').toBe(true);
		expect(box().classList.contains('compact')).toBe(false);
	});

	it('보내기는 원본 버튼과 같은 색(초록) 이다', async () => {
		await dock({ withSubmit: true });
		const send = document.querySelector('.dock-btn.send');
		expect(send, '보내기 버튼에 send 표시가 있어야 색이 붙는다').toBeTruthy();
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
