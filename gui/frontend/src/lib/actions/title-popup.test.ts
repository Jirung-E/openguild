import { describe, it, expect, afterEach } from 'vitest';
import { titlePopup, showTitlePopupNow, hideTitlePopupNow } from './title-popup';

function node(): HTMLElement {
	const el = document.createElement('button');
	document.body.appendChild(el);
	return el;
}
function popupEl(): HTMLElement | null {
	return document.querySelector('[role="tooltip"]');
}

afterEach(() => {
	hideTitlePopupNow();
	document.body.innerHTML = '';
});

describe('titlePopup — 싱글턴 하이재킹 (REQ-004)', () => {
	it('자기 노드의 label 변경은 떠 있는 팝업에 반영된다', () => {
		const a = node();
		const act = titlePopup(a, 'A 원래');
		showTitlePopupNow(a, 'A 원래');
		expect(popupEl()?.textContent).toBe('A 원래');
		act?.update?.('A 바뀜');
		expect(popupEl()?.textContent).toBe('A 바뀜');
		act?.destroy?.();
	});

	/**
	 * 핵심: A 의 툴팁이 떠 있을 때 **무관한** B 의 label 이 바뀌어도
	 * 팝업이 B 로 끌려가면 안 된다. 예전엔 `if (current)` 만 봐서 끌려갔다.
	 */
	it('다른 노드의 label 변경은 떠 있는 팝업을 건드리지 않는다', () => {
		const a = node();
		const b = node();
		const actA = titlePopup(a, 'A 툴팁');
		const actB = titlePopup(b, 'B 원래');
		showTitlePopupNow(a, 'A 툴팁');
		expect(popupEl()?.textContent).toBe('A 툴팁');

		actB?.update?.('B 바뀜'); // 무관한 노드
		expect(popupEl()?.textContent).toBe('A 툴팁');

		actA?.destroy?.();
		actB?.destroy?.();
	});

	it('팝업이 안 떠 있으면 update 가 팝업을 만들지 않는다', () => {
		const a = node();
		const act = titlePopup(a, '처음');
		act?.update?.('바뀜');
		expect(popupEl()).toBeNull();
		act?.destroy?.();
	});
});
