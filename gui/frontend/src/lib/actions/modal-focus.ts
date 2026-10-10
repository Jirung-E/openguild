/**
 * BUG-272: 모달이 떠 있는 동안 포커스를 **모달 안에** 둔다 — 대화상자 노드에 거는 액션.
 *
 * 예전 확인 대화상자는 열려도 포커스가 뒤 페이지에 남았다. Tab 은 뒤 페이지를 돌았고, 키 처리가 window 에 걸려 있어
 * 뒤 페이지의 다른 버튼에 포커스가 있어도 스친 Enter 가 "확인"(삭제 · 전부 허용)을 눌렀다.
 *
 * - 열릴 때: `[data-autofocus]` 요소에, 없으면 첫 포커스 가능 요소에, 그것도 없으면 노드 자체에 포커스.
 * - Tab / Shift+Tab: 노드 안에서만 돈다(끝에서 처음으로, 처음에서 끝으로).
 * - 닫힐 때: 열기 전에 포커스가 있던 요소로 돌려준다(아직 문서에 있으면).
 * - `onEscape` 를 주면 Esc 로 닫는다 — 아래 깔린 모달까지 같은 Esc 로 닫히지 않게 밖으로 안 올려 보낸다.
 *
 * 오버레이처럼 열림/닫힘과 수명이 같은 노드에 건다: `<div role="dialog" use:modalFocus={{ onEscape: close }}>`.
 *
 * 반려 후(2026-10-11, admin "아직 동일한 문제가 있다"):
 * - 처음엔 공통 확인 창(`ConfirmDialog`)에만 걸었다. 퀘스트 삭제 · 타입 변경, 캠페인, 보드의 "변경" 확인, 관리,
 *   새 퀘스트 · 새 캠페인 창은 각자 만든 창이라 그대로였다 — 지금은 모든 모달에 건다.
 * - **Tab 을 언제나 직접 옮긴다.** 맥 앱(WKWebView)은 기본 설정에서 Tab 이 버튼을 건너뛰고 글 입력칸만 돈다.
 *   브라우저에 맡기면 버튼만 있는 확인 창에서 Tab 이 곧장 뒤 페이지의 입력칸으로 나갔다.
 * - **맨 위 모달만** 지킨다. 모달 위에 확인 창이 뜨면 예전엔 둘이 서로 포커스를 빼앗았다.
 */
const FOCUSABLE = [
	'a[href]',
	'button:not([disabled])',
	'input:not([disabled]):not([type="hidden"])',
	'select:not([disabled])',
	'textarea:not([disabled])',
	'[tabindex]:not([tabindex="-1"])'
].join(',');

function focusables(node: HTMLElement): HTMLElement[] {
	return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE));
}

export interface ModalFocusOptions {
	/** Esc 로 닫기. 없으면 Esc 를 건드리지 않는다(모달이 스스로 받는다). */
	onEscape?: () => void;
}

/** 지금 떠 있는 모달들 — 나중에 뜬 것이 위다. 맨 위 하나만 포커스를 지킨다. */
const stack: HTMLElement[] = [];
const isTop = (node: HTMLElement) => stack[stack.length - 1] === node;

export function modalFocus(node: HTMLElement, options: ModalFocusOptions = {}) {
	let opts = options;
	const before = document.activeElement instanceof HTMLElement ? document.activeElement : null;
	stack.push(node);

	const first =
		node.querySelector<HTMLElement>('[data-autofocus]') ?? focusables(node)[0] ?? node;
	first.focus();

	function onkeydown(e: KeyboardEvent) {
		// 안쪽에서 이미 쓴 키(편집기의 Tab 들여쓰기, 자동완성의 Esc 등)는 건드리지 않는다.
		if (!isTop(node) || e.defaultPrevented) return;
		if (e.key === 'Escape' && opts.onEscape) {
			e.preventDefault();
			e.stopPropagation();
			opts.onEscape();
			return;
		}
		if (e.key !== 'Tab') return;
		// 브라우저에 맡기지 않는다 — WKWebView 는 기본 설정에서 버튼을 건너뛴다(위 설명).
		e.preventDefault();
		const list = focusables(node);
		if (list.length === 0) {
			node.focus();
			return;
		}
		const at = list.indexOf(document.activeElement as HTMLElement);
		const step = e.shiftKey ? -1 : 1;
		const next = at < 0 ? (e.shiftKey ? list.length - 1 : 0) : (at + step + list.length) % list.length;
		list[next].focus();
	}

	// 포커스가 어떤 이유로든(클릭 등) 밖으로 나가면 다시 안으로.
	function onfocusin(e: FocusEvent) {
		if (!isTop(node)) return;
		if (e.target instanceof Node && !node.contains(e.target)) {
			(focusables(node)[0] ?? node).focus();
		}
	}

	// 키는 문서에서 받는다 — 포커스가 잠깐 밖에 있어도(열린 직후 등) Tab 이 새지 않게. 맨 위 모달만 반응한다.
	// 캡처가 아니라 버블로 — 안쪽 요소가 먼저 쓸 기회를 갖는다.
	document.addEventListener('keydown', onkeydown);
	document.addEventListener('focusin', onfocusin);
	return {
		update(next: ModalFocusOptions = {}) {
			opts = next;
		},
		destroy() {
			document.removeEventListener('keydown', onkeydown);
			document.removeEventListener('focusin', onfocusin);
			const i = stack.lastIndexOf(node);
			if (i >= 0) stack.splice(i, 1);
			if (before && before.isConnected) before.focus();
		}
	};
}
