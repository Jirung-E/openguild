// DEV-130: 편집 textarea 에서 Tab 키 — focus 이동 대신 tab 문자 삽입.
//
// 사용: <textarea use:tabInsert></textarea>
//
// - execCommand('insertText') 우선 — 브라우저 undo 스택 보존 (Ctrl+Z 동작).
//   WebView2(Chromium) 에서 동작. deprecated 지만 대체 API (beforeinput
//   주입) 가 더 번거로워 fallback 과 병행.
// - Escape 후 Tab 으로 focus 빠져나가는 표준 패턴은 유지 — Tab 만 가로챔.

export function tabInsert(node: HTMLTextAreaElement) {
	function onKeydown(e: KeyboardEvent) {
		if (e.key !== 'Tab' || e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
		e.preventDefault();
		// undo 스택 보존 경로.
		if (document.execCommand && document.execCommand('insertText', false, '\t')) {
			return;
		}
		// fallback — 수동 삽입 (undo 스택은 끊김).
		const start = node.selectionStart;
		const end = node.selectionEnd;
		node.value = node.value.slice(0, start) + '\t' + node.value.slice(end);
		node.selectionStart = node.selectionEnd = start + 1;
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
