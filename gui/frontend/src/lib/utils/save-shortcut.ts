/** Ctrl+S / Cmd+S. Shift·Alt 조합은 다른 단축키가 쓸 수 있으므로 제외한다. */
export function isSaveShortcut(
	e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'>
): boolean {
	return (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 's';
}

export interface SaveShortcutOptions {
	disabled?: boolean;
	onSave: () => void;
}

/**
 * 편집 영역 안에서만 저장 단축키를 처리하는 Svelte action.
 * 전역 listener가 아니므로 같은 페이지의 메모·댓글 등 다른 편집기를 함께
 * 저장하지 않고, 포커스가 들어 있는 편집 영역 하나만 저장한다.
 */
export function saveShortcut(node: HTMLElement, initial: SaveShortcutOptions) {
	let options = initial;
	const onKeydown = (e: KeyboardEvent) => {
		if (!isSaveShortcut(e)) return;
		e.preventDefault();
		e.stopPropagation();
		if (!options.disabled) options.onSave();
	};
	node.addEventListener('keydown', onKeydown);
	return {
		update(next: SaveShortcutOptions) {
			options = next;
		},
		destroy() {
			node.removeEventListener('keydown', onKeydown);
		}
	};
}
