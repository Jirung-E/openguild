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
import { acceptCompletion } from '@codemirror/autocomplete';
import { EditorSelection, EditorState, type Extension } from '@codemirror/state';
import { keymap, type Command } from '@codemirror/view';
import { get } from 'svelte/store';
import {
	editorSettings,
	nextTabStopSpaces,
	type EditorSettings
} from '$lib/stores/editorSettings';

/** 현재 설정의 들여쓰기 단위 문자열 (tab='\t', space=공백 N칸) — indentUnit 용. */
function unitOf(s: EditorSettings): string {
	return s.tabMode === 'space' ? ' '.repeat(s.indentSize) : '\t';
}

/**
 * Tab: 선택 있으면 indentMore. 없으면 —
 * - tab 모드: 탭 문자 1개 (탭 문자는 그 자체로 정지점 정렬).
 * - space 모드: VSCode 처럼 커서 열에서 다음 탭 정지점까지의 공백만 삽입
 *   (항상 N칸이 아니라 정렬). 커서마다(멀티 커서) 개별 계산.
 */
const insertIndent: Command = (view) => {
	const s = get(editorSettings);
	const { state } = view;
	if (state.selection.ranges.some((r) => !r.empty)) return indentMore(view);
	if (s.tabMode === 'tab') {
		view.dispatch(
			state.update(state.replaceSelection('\t'), {
				scrollIntoView: true,
				userEvent: 'input'
			})
		);
		return true;
	}
	const size = s.indentSize;
	view.dispatch(
		state.update(
			state.changeByRange((range) => {
				const line = state.doc.lineAt(range.head);
				const before = state.sliceDoc(line.from, range.head);
				const insert = ' '.repeat(nextTabStopSpaces(before, size));
				return {
					changes: { from: range.from, to: range.to, insert },
					range: EditorSelection.cursor(range.from + insert.length)
				};
			}),
			{ scrollIntoView: true, userEvent: 'input' }
		)
	);
	return true;
};

/** 현재(또는 주어진) 설정으로 들여쓰기 확장 배열 생성. */
export function indentExtensions(s: EditorSettings = get(editorSettings)): Extension {
	return [
		indentUnit.of(unitOf(s)),
		EditorState.tabSize.of(s.indentSize),
		keymap.of([
			// DEV-140 #9: 자동완성 팝업이 떠 있으면 Tab 으로 선택 적용(댓글과 동일),
			// 없으면 들여쓰기. acceptCompletion 은 활성 완성이 없으면 false 반환.
			{ key: 'Tab', run: (view) => acceptCompletion(view) || insertIndent(view), preventDefault: true },
			{ key: 'Shift-Tab', run: indentLess, preventDefault: true }
		])
	];
}
