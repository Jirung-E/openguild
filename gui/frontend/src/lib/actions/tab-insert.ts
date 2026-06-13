// DEV-130: 편집 textarea 에서 Tab 키 — focus 이동 대신 들여쓰기 삽입.
//
// 사용: <textarea use:tabInsert></textarea>
//
// - 삽입 문자열은 editorSettings 를 따름 (CodeMirror 편집기와 동일): tab 모드
//   '\t', space 모드 공백 N칸. (DEV-130 #2 — 설정대로 동작하지 않던 버그.)
// - execCommand('insertText') 우선 — 브라우저 undo 스택 보존 (Ctrl+Z 동작).
//   WebView2(Chromium) 에서 동작. deprecated 지만 대체 API (beforeinput
//   주입) 가 더 번거로워 fallback 과 병행.
// - Escape 후 Tab 으로 focus 빠져나가는 표준 패턴은 유지 — Tab 만 가로챔.

import { get } from 'svelte/store';
import { editorSettings } from '$lib/stores/editorSettings';

export function tabInsert(node: HTMLTextAreaElement) {
	function onKeydown(e: KeyboardEvent) {
		if (e.key !== 'Tab' || e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
		e.preventDefault();
		const s = get(editorSettings);
		const unit = s.tabMode === 'space' ? ' '.repeat(s.indentSize) : '\t';
		// undo 스택 보존 경로.
		if (document.execCommand && document.execCommand('insertText', false, unit)) {
			return;
		}
		// fallback — 수동 삽입 (undo 스택은 끊김).
		const start = node.selectionStart;
		const end = node.selectionEnd;
		node.value = node.value.slice(0, start) + unit + node.value.slice(end);
		node.selectionStart = node.selectionEnd = start + unit.length;
		// Svelte bind:value 가 인지하도록 input 이벤트 발화.
		node.dispatchEvent(new Event('input', { bubbles: true }));
	}
	node.addEventListener('keydown', onKeydown);
	return {
		destroy() {
			node.removeEventListener('keydown', onKeydown);
		}
	};
}
