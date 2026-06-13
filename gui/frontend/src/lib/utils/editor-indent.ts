// DEV-130: editorSettings → CodeMirror 들여쓰기 확장.
//
// - indentUnit: indentMore/indentLess 가 삽입/제거하는 단위. tab 모드면 '\t',
//   space 모드면 공백 N칸.
// - EditorState.tabSize: 탭 문자의 표시 폭.
// - Tab 키맵: 선택이 있으면 indentMore, 없으면 설정 단위를 직접 삽입.
//   (@codemirror/commands 의 insertTab 은 빈 선택일 때 무조건 '\t' 를 넣어
//   space 모드가 동작하지 않으므로 — DEV-130 #2 버그 — 커스텀 커맨드로 대체.)
//
// quest / campaign 상세 editor 의 initEditor 가 매 생성 시 호출.

import { indentUnit } from '@codemirror/language';
import { indentMore, indentLess } from '@codemirror/commands';
import { EditorState, type Extension } from '@codemirror/state';
import { keymap, type Command } from '@codemirror/view';
import { get } from 'svelte/store';
import { editorSettings, type EditorSettings } from '$lib/stores/editorSettings';

/** 현재 설정의 들여쓰기 단위 문자열 (tab='\t', space=공백 N칸). */
function unitOf(s: EditorSettings): string {
	return s.tabMode === 'space' ? ' '.repeat(s.indentSize) : '\t';
}

/** Tab: 선택 있으면 indentMore, 없으면 설정 단위 삽입 (실시간 store 값 사용). */
const insertIndent: Command = (view) => {
	const s = get(editorSettings);
	const { state } = view;
	if (state.selection.ranges.some((r) => !r.empty)) return indentMore(view);
	view.dispatch(
		state.update(state.replaceSelection(unitOf(s)), {
			scrollIntoView: true,
			userEvent: 'input'
		})
	);
	return true;
};

/** 현재(또는 주어진) 설정으로 들여쓰기 확장 배열 생성. */
export function indentExtensions(s: EditorSettings = get(editorSettings)): Extension {
	return [
		indentUnit.of(unitOf(s)),
		EditorState.tabSize.of(s.indentSize),
		keymap.of([
			{ key: 'Tab', run: insertIndent, preventDefault: true },
			{ key: 'Shift-Tab', run: indentLess, preventDefault: true }
		])
	];
}
