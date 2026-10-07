/**
 * BUG-272: 모달이 떠 있는 동안 포커스를 **모달 안에** 둔다 — 대화상자 노드에 거는 액션.
 *
 * 예전 확인 대화상자는 열려도 포커스가 뒤 페이지에 남았다. Tab 은 뒤 페이지를 돌았고, 키 처리가 window 에 걸려 있어
 * 뒤 페이지의 다른 버튼에 포커스가 있어도 스친 Enter 가 "확인"(삭제 · 전부 허용)을 눌렀다.
 *
 * - 열릴 때: `[data-autofocus]` 요소에, 없으면 첫 포커스 가능 요소에, 그것도 없으면 노드 자체에 포커스.
 * - Tab / Shift+Tab: 노드 안에서만 돈다(끝에서 처음으로, 처음에서 끝으로).
 * - 닫힐 때: 열기 전에 포커스가 있던 요소로 돌려준다(아직 문서에 있으면).
 *
 * 오버레이처럼 열림/닫힘과 수명이 같은 노드에 건다: `<div role="dialog" use:modalFocus>`.
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

export function modalFocus(node: HTMLElement) {
	const before = document.activeElement instanceof HTMLElement ? document.activeElement : null;

	const first =
		node.querySelector<HTMLElement>('[data-autofocus]') ?? focusables(node)[0] ?? node;
	first.focus();

	function onkeydown(e: KeyboardEvent) {
		if (e.key !== 'Tab') return;
		const list = focusables(node);
		if (list.length === 0) {
			e.preventDefault();
			node.focus();
			return;
		}
		const head = list[0];
		const tail = list[list.length - 1];
		const at = document.activeElement;
		const inside = at instanceof Node && node.contains(at);
		if (e.shiftKey && (at === head || !inside)) {
			e.preventDefault();
			tail.focus();
		} else if (!e.shiftKey && (at === tail || !inside)) {
			e.preventDefault();
			head.focus();
		}
	}

	// 포커스가 어떤 이유로든(클릭 등) 밖으로 나가면 다시 안으로.
	function onfocusin(e: FocusEvent) {
		if (e.target instanceof Node && !node.contains(e.target)) {
			(focusables(node)[0] ?? node).focus();
		}
	}

	node.addEventListener('keydown', onkeydown);
	document.addEventListener('focusin', onfocusin);
	return {
		destroy() {
			node.removeEventListener('keydown', onkeydown);
			document.removeEventListener('focusin', onfocusin);
			if (before && before.isConnected) before.focus();
		}
	};
}
