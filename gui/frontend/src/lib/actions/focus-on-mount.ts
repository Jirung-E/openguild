// DEV-475: 편집 버튼으로 연 입력칸에 바로 쓸 수 있게 — 나타나는 순간 포커스, 글자는 끝에서 이어 쓴다.
//
// 사용: <textarea use:focusOnMount></textarea> · 경우에 따라서만이면 use:focusOnMount={조건}
//
// 칸이 `{#if}` 로 나타날 때만 돈다. 칸을 없애지 않는 저장 · 재조회는 포커스를 건드리지 않는다.

export function focusOnMount(node: HTMLInputElement | HTMLTextAreaElement, enabled = true) {
	if (!enabled) return;
	node.focus();
	const end = node.value.length;
	// type=color 같은 칸은 글자 위치가 없다.
	if (node.selectionStart !== null) node.setSelectionRange(end, end);
}
