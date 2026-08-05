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
	drawSelection,
	dropCursor,
	rectangularSelection,
	crosshairCursor,
	keymap
} from '@codemirror/view';
import { history, defaultKeymap, historyKeymap, insertNewline } from '@codemirror/commands';
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
import { markdown } from '@codemirror/lang-markdown';
import { basicSetup } from 'codemirror';

/** 터치 기기 판정 — 앱의 다른 곳(모바일 분기)과 같은 기준. */
export function isCoarsePointer(): boolean {
	return typeof window !== 'undefined' && window.matchMedia?.('(pointer: coarse)').matches === true;
}

/**
 * 터치용 기본 확장 — basicSetup 에서 drawSelection 과 마우스 전용 확장만 뺀 것.
 * 데스크톱은 이 함수를 쓰지 않고 `basicSetup` 을 그대로 둔다(동작 변화 없음).
 *
 * DEV-336: `indentOnInput` 도 옵션으로 뺄 수 있다 — 편집기 "자동 서식" 설정이
 * 꺼져 있으면 타이핑 중 재들여쓰기를 원치 않는다.
 */
export function touchSetup(opts: { indentOnInput?: boolean } = {}): Extension {
	const { indentOnInput: withIndentOnInput = true } = opts;
	return [
		lineNumbers(),
		highlightActiveLineGutter(),
		highlightSpecialChars(),
		history(),
		foldGutter(),
		// drawSelection() — 제외. 이게 이 파일의 존재 이유다.
		// dropCursor() / rectangularSelection() / crosshairCursor() — 마우스 전용.
		EditorState.allowMultipleSelections.of(true),
		...(withIndentOnInput ? [indentOnInput()] : []),
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

/**
 * DEV-336: 데스크톱용 `basicSetup` 동등 구성 — `indentOnInput` 을 뺄 수 있게
 * 재구성한 버전. `indentOnInput: true`(기본)일 때는 `codemirror` 패키지의
 * `basicSetup` 과 구성이 동일해야 한다(아래 값은 `codemirror` v6.0.2 의
 * `basicSetup` 소스를 그대로 옮긴 것 — 패키지 업데이트 시 대조 필요).
 * `autoFormat` 이 켜져 있는(기본) 흔한 경로는 여전히 `basicSetup` 원본을
 * 직접 쓴다 — 이 함수는 껐을 때만 쓰인다.
 */
export function desktopSetup(opts: { indentOnInput?: boolean } = {}): Extension {
	const { indentOnInput: withIndentOnInput = true } = opts;
	return [
		lineNumbers(),
		highlightActiveLineGutter(),
		highlightSpecialChars(),
		history(),
		foldGutter(),
		drawSelection(),
		dropCursor(),
		EditorState.allowMultipleSelections.of(true),
		...(withIndentOnInput ? [indentOnInput()] : []),
		syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
		bracketMatching(),
		closeBrackets(),
		autocompletion(),
		rectangularSelection(),
		crosshairCursor(),
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

/**
 * DEV-336: quest/campaign/memo/rules/library 공통 마크다운 편집기가 쓰는
 * 기본 확장 조립 지점 — 터치 여부 + "자동 서식" 설정(editorSettings.autoFormat)
 * 을 한 번에 반영한다.
 *
 * autoFormat 이 꺼지면 세 가지를 동시에 끈다(사용자가 "자동으로 뭐가
 * 입력되는 게 싫다"는 단일 의도로 켜고 끄는 성격이라 하나로 묶음):
 * 1. 목록/인용 이어쓰기 + Backspace 마크업 정리 — `markdown({ addKeymap: false })`.
 * 2. 새 줄 자동 들여쓰기 — `insertNewlineAndIndent`(defaultKeymap 의 Enter)
 *    보다 높은 우선순위로 평범한 `insertNewline` 을 Enter 에 바인딩(배열
 *    앞쪽에 둔 keymap 이 이긴다).
 * 3. 타이핑 중 재들여쓰기 — `indentOnInput()` 자체를 기본 확장에서 제외.
 */
export function markdownEditorExtensions(opts: { touch: boolean; autoFormat: boolean }): Extension {
	const { touch, autoFormat } = opts;
	// 흔한 경로(autoFormat 켜짐, 기본값)의 데스크톱은 `basicSetup` 원본을
	// 그대로 쓴다 — 동작 변화 없음. 꺼졌을 때만 indentOnInput 뺀 재구성 사용.
	const base = touch
		? touchSetup({ indentOnInput: autoFormat })
		: autoFormat
			? basicSetup
			: desktopSetup({ indentOnInput: false });
	return [
		...(autoFormat
			? []
			: [keymap.of([{ key: 'Enter', run: insertNewline, preventDefault: true }])]),
		base,
		markdown({ addKeymap: autoFormat })
	];
}
