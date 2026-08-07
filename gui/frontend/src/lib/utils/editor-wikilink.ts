/**
 * DEV-172: CodeMirror 쪽 cross-link 삽입 — textarea-wikilink.ts 의
 * applyWikiLink/applyWikiPrefix 와 동일한 의미론을 CM Transaction API 로 구현
 * (매칭 로직인 wikiMatch 는 프레임워크 무관이라 그대로 공유, 삽입만 API 가 다름).
 */
import type { EditorView } from '@codemirror/view';

/** [from,to) 토큰을 `[[id]]` 로 치환. closeBrackets 가 이미 `]]` 를 넣어뒀으면 그걸 삼킨다. */
export function applyWikiLinkCM(view: EditorView, from: number, to: number, id: string): void {
	const to2 = view.state.sliceDoc(to, to + 2) === ']]' ? to + 2 : to;
	const text = `[[${id}]]`;
	view.dispatch({
		changes: { from, to: to2, insert: text },
		selection: { anchor: from + text.length }
	});
	view.focus();
}

/** `[[` 부터 caret 까지를 `[[{prefix}` 로 치환 — `]]` 는 열어둔 채로(이어서 ID 타이핑). */
export function applyWikiPrefixCM(
	view: EditorView,
	from: number,
	to: number,
	prefix: string
): void {
	const text = `[[${prefix}`;
	view.dispatch({
		changes: { from, to, insert: text },
		selection: { anchor: from + text.length }
	});
	view.focus();
}
