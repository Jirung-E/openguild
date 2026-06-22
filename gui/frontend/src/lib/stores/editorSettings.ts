// DEV-130: 본문 편집기(CodeMirror) 들여쓰기 설정 — 코드 편집기처럼.
//
// - tabMode: 'tab' = Tab 키가 탭 문자(\t) 삽입 / 'space' = 공백 N칸 삽입.
// - indentSize: 들여쓰기 칸수 (2 또는 4). space 모드의 공백 개수 + tab 모드의
//   표시 폭(tabSize) 양쪽에 적용.
//
// 영속화: localStorage. quest / campaign 상세 편집기가 initEditor 에서 구독.

import { writable } from 'svelte/store';

export type TabMode = 'tab' | 'space';
export type IndentSize = 2 | 4;

export interface EditorSettings {
	tabMode: TabMode;
	indentSize: IndentSize;
}

const KEY = 'openguild.editorSettings';
export const DEFAULT_EDITOR_SETTINGS: EditorSettings = { tabMode: 'tab', indentSize: 4 };

function loadInitial(): EditorSettings {
	if (typeof localStorage === 'undefined') return { ...DEFAULT_EDITOR_SETTINGS };
	try {
		const raw = localStorage.getItem(KEY);
		if (!raw) return { ...DEFAULT_EDITOR_SETTINGS };
		const parsed = JSON.parse(raw) as Partial<EditorSettings>;
		const tabMode: TabMode = parsed.tabMode === 'space' ? 'space' : 'tab';
		const indentSize: IndentSize = parsed.indentSize === 2 ? 2 : 4;
		return { tabMode, indentSize };
	} catch {
		return { ...DEFAULT_EDITOR_SETTINGS };
	}
}

export const editorSettings = writable<EditorSettings>(loadInitial());

editorSettings.subscribe((s) => {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(KEY, JSON.stringify(s));
	} catch {
		/* 무시 */
	}
});

export function setTabMode(mode: TabMode) {
	editorSettings.update((s) => ({ ...s, tabMode: mode }));
}

export function setIndentSize(size: IndentSize) {
	editorSettings.update((s) => ({ ...s, indentSize: size }));
}

/**
 * DEV-130: VSCode 식 탭 정지점 — `beforeText`(줄 시작 ~ 커서) 의 시각적 열을
 * 계산해, 다음 탭 정지점(indentSize 의 배수)까지 필요한 공백 수를 반환.
 * 즉 항상 indentSize 칸이 아니라 "현재 열에서 다음 정지점까지" 만큼만.
 * 탭 문자(\t)는 indentSize 폭으로 환산. 결과는 1..=indentSize.
 */
export function nextTabStopSpaces(beforeText: string, indentSize: number): number {
	let col = 0;
	for (const ch of beforeText) {
		if (ch === '\t') col += indentSize - (col % indentSize);
		else col += 1;
	}
	return indentSize - (col % indentSize);
}
