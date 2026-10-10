// BUG-272 반려 후: 모든 모달이 쓰는 포커스 부품. ConfirmDialog.test.ts 가 확인 창 하나를 보고, 여기서는 부품 자체를 본다.

import { describe, it, expect, vi, afterEach } from 'vitest';
import { modalFocus } from './modal-focus';

afterEach(() => {
	document.body.innerHTML = '';
});

function dialog(labels: string[]): HTMLElement {
	const d = document.createElement('div');
	d.setAttribute('role', 'dialog');
	d.tabIndex = -1;
	for (const l of labels) {
		const b = document.createElement('button');
		b.textContent = l;
		d.appendChild(b);
	}
	document.body.appendChild(d);
	return d;
}

const active = () => document.activeElement?.textContent;

function tab(shift = false): KeyboardEvent {
	const e = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: shift, bubbles: true, cancelable: true });
	(document.activeElement ?? document.body).dispatchEvent(e);
	return e;
}

describe('BUG-272 modalFocus', () => {
	it('Tab 을 언제나 직접 옮긴다 — 버튼만 있는 창에서도 브라우저에 맡기지 않는다', () => {
		const d = dialog(['취소', '확인']);
		const a = modalFocus(d);
		expect(active()).toBe('취소');
		// WKWebView 는 기본 설정에서 Tab 이 버튼을 건너뛴다 — 맡기면 뒤 페이지로 나갔다.
		const e = tab();
		expect(e.defaultPrevented).toBe(true);
		expect(active()).toBe('확인');
		tab();
		expect(active()).toBe('취소'); // 끝에서 처음으로
		tab(true);
		expect(active()).toBe('확인'); // 처음에서 끝으로
		a.destroy();
	});

	it('안쪽에서 이미 쓴 Tab(편집기 들여쓰기 등)은 건드리지 않는다', () => {
		const d = dialog(['가', '나']);
		const a = modalFocus(d);
		const b = d.querySelector('button')!;
		b.addEventListener('keydown', (ev) => ev.preventDefault());
		tab();
		expect(active()).toBe('가'); // 옮기지 않았다
		a.destroy();
	});

	it('모달 위에 모달 — 맨 위 것만 포커스를 지키고, 닫으면 아래 것으로 돌아간다', () => {
		const lower = dialog(['아래 버튼']);
		const a = modalFocus(lower);
		const upper = dialog(['위 취소', '위 확인']);
		const b = modalFocus(upper);
		// 예전엔 아래 모달이 "밖으로 나갔다" 며 포커스를 도로 끌어갔다.
		expect(active()).toBe('위 취소');
		tab();
		expect(active()).toBe('위 확인');
		b.destroy();
		expect(active()).toBe('아래 버튼');
		a.destroy();
	});

	it('onEscape 를 주면 Esc 로 닫고, 밖으로 안 올려 보낸다', () => {
		const d = dialog(['취소']);
		const onEscape = vi.fn();
		const outer = vi.fn();
		window.addEventListener('keydown', outer);
		const a = modalFocus(d, { onEscape });
		document.activeElement!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
		expect(onEscape).toHaveBeenCalledTimes(1);
		expect(outer).not.toHaveBeenCalled();
		window.removeEventListener('keydown', outer);
		a.destroy();
	});

	it('닫히면 열기 전 요소로 돌아간다', () => {
		const back = document.createElement('button');
		back.textContent = '뒤';
		document.body.appendChild(back);
		back.focus();
		const a = modalFocus(dialog(['확인']));
		expect(active()).toBe('확인');
		a.destroy();
		expect(active()).toBe('뒤');
	});
});
