/**
 * BUG-215: 편집기 기본 확장 세트 — 터치 기기용 변형.
 *
 * CodeMirror 의 `basicSetup` 에는 `drawSelection()` 이 들어 있다. 이 확장은
 * **네이티브 선택과 캐럿을 숨기고**(`.cm-content { caret-color: transparent }`)
 * 자체 레이어로 선택을 그린다. 데스크톱에서는 마우스로 끌어 선택하니 문제가
 * 없지만, 모바일의 "길게 눌러 선택" 은 OS 가 네이티브 선택 위에 핸들과
 * 확대경을 띄우는 방식이라, 네이티브 선택이 숨겨지면 선택 자체를 할 수단이
 * 사라진다(admin 보고).
 *
 * 끼운 뒤 되돌릴 수도 없다 — `drawSelection` 이 넣는
 * `EditorView.nativeSelectionHidden` 팩싯의 combine 이 `values.some(x => x)` 라
 * 나중에 false 를 넣어도 true 가 이긴다. 그래서 터치 환경에서는 **처음부터
 * 빼고** 구성한다.
 *
 * 함께 빼는 것들은 전부 마우스 전용이라 터치에서 의미가 없다:
 * `rectangularSelection`(Alt+드래그), `crosshairCursor`(Alt 커서),
 * `dropCursor`(드래그&드랍 위치 표시).
 *
 * 나머지(줄 번호, 접기, 괄호 매칭, 자동완성, 들여쓰기, 문법 강조, 히스토리)는
 * basicSetup 과 같게 유지한다. CodeMirror 문서가 "basicSetup 은 import 와 배열
 * 리터럴일 뿐이니 필요하면 복사해서 고쳐 쓰라" 고 안내하는 그대로다.
 */
import type { Extension } from '@codemirror/state';
import { EditorState } from '@codemirror/state';
import {
	lineNumbers,
	highlightActiveLineGutter,
	highlightSpecialChars,
	highlightActiveLine,
	keymap
} from '@codemirror/view';
import { history, defaultKeymap, historyKeymap } from '@codemirror/commands';
import {
	foldGutter,
	foldKeymap,
	indentOnInput,
	bracketMatching,
	syntaxHighlighting,
	defaultHighlightStyle
} from '@codemirror/language';
import {
	closeBrackets,
	closeBracketsKeymap,
	autocompletion,
	completionKeymap
} from '@codemirror/autocomplete';

/** 터치 기기 판정 — 앱의 다른 곳(모바일 분기)과 같은 기준. */
export function isCoarsePointer(): boolean {
	return typeof window !== 'undefined' && window.matchMedia?.('(pointer: coarse)').matches === true;
}

/**
 * 터치용 기본 확장 — basicSetup 에서 drawSelection 과 마우스 전용 확장만 뺀 것.
 * 데스크톱은 이 함수를 쓰지 않고 `basicSetup` 을 그대로 둔다(동작 변화 없음).
 */
export function touchSetup(): Extension {
	return [
		lineNumbers(),
		highlightActiveLineGutter(),
		highlightSpecialChars(),
		history(),
		foldGutter(),
		// drawSelection() — 제외. 이게 이 파일의 존재 이유다.
		// dropCursor() / rectangularSelection() / crosshairCursor() — 마우스 전용.
		EditorState.allowMultipleSelections.of(true),
		indentOnInput(),
		syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
		bracketMatching(),
		closeBrackets(),
		autocompletion(),
		highlightActiveLine(),
		keymap.of([
			...closeBracketsKeymap,
			...defaultKeymap,
			...historyKeymap,
			...foldKeymap,
			...completionKeymap
		])
	];
}
