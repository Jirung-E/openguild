// 본문 편집기(CodeMirror) 테마 — 다크/라이트 전환에 라이브 반응.
//
// 기존엔 테마가 바뀌면 editor 를 통째로 재생성해서 (또는 일부 editor 는 아예
// 반응 안 해서 — 캠페인/규칙) 커서/스크롤/undo 가 날아가거나 테마가 안 바뀌는
// 문제가 있었다. Compartment 로 테마 확장만 교체(reconfigure)하면 editor 상태를
// 보존한 채 즉시 전환된다.
//
// 사용:
//   extensions: [ ..., editorThemeCompartment.of(editorThemeExtension($theme)) ]
//   $effect(() => { const t = $theme; editorView?.dispatch({
//     effects: editorThemeCompartment.reconfigure(editorThemeExtension(t)) }); });

import { Compartment, type Extension } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import { resolveTheme, type ThemeChoice } from '$lib/stores/theme';

// 같은 Compartment 인스턴스를 여러 editor 가 공유해도 됨 — reconfigure 는
// 대상 view 의 슬롯만 바꾼다 (각 EditorState 가 독립 보관).
export const editorThemeCompartment = new Compartment();

/** 현재 테마에 맞는 editor 테마 확장. dark=oneDark, light=기본(빈 확장). */
export function editorThemeExtension(theme: ThemeChoice): Extension {
	return resolveTheme(theme) === 'dark' ? oneDark : [];
}
