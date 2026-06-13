// DEV-130: editorSettings → CodeMirror 들여쓰기 확장.
//
// - indentUnit: indentMore/indentWithTab 가 삽입하는 단위. tab 모드면 '\t',
//   space 모드면 공백 N칸.
// - EditorState.tabSize: 탭 문자의 표시 폭 (tab 모드의 시각적 칸수).
//
// quest / campaign 상세 editor 의 initEditor 가 매 생성 시 호출 (editorSettings
// 의 현재 값으로). 설정 변경 후 editor 를 재생성하면 반영.

import { indentUnit } from '@codemirror/language';
import { EditorState, type Extension } from '@codemirror/state';
import { get } from 'svelte/store';
import { editorSettings, type EditorSettings } from '$lib/stores/editorSettings';

/** 현재(또는 주어진) 설정으로 들여쓰기 확장 배열 생성. */
export function indentExtensions(s: EditorSettings = get(editorSettings)): Extension {
	const unit = s.tabMode === 'space' ? ' '.repeat(s.indentSize) : '\t';
	return [indentUnit.of(unit), EditorState.tabSize.of(s.indentSize)];
}
