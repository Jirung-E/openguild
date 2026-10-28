// 본문 편집기(CodeMirror) 테마 — 다크/라이트 전환에 라이브 반응.
//
// 기존엔 테마가 바뀌면 editor 를 통째로 재생성해서 (또는 일부 editor 는 아예
// 반응 안 해서 — 캠페인/규칙) 커서/스크롤/undo 가 날아가거나 테마가 안 바뀌는
// 문제가 있었다. Compartment 로 테마 확장만 교체(reconfigure)하면 editor 상태를
// 보존한 채 즉시 전환된다.
//
// 사용:
//   extensions: [ ..., editorThemeCompartment.of(editorThemeExtension($effectiveTheme)) ]
//   $effect(() => { const t = $effectiveTheme; editorView?.dispatch({
//     effects: editorThemeCompartment.reconfigure(editorThemeExtension(t)) }); });
//
// BUG-380: 고른 값(`theme`)이 아니라 **실제 테마**(`effectiveTheme`)를 본다. 앱은 라이트 · 다크를 고르면 창 자체를
// 그 모양으로 고정한다. 라이트에서 '시스템' 으로 바꾸는 순간에는 창이 아직 라이트라 OS 가 다크여도 라이트로
// 읽히고, 창이 다크로 풀린 뒤에도 고른 값은 'system' 그대로라 다시 알려 주지 않는다 — 편집기만 라이트로 남아
// 줄 번호 칸(oneDark 가 칠하는 곳)이 하얗게 남았다. `effectiveTheme` 은 그 뒤의 바뀜도 알린다.

import { Compartment, type Extension } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';
import type { EffectiveTheme } from '$lib/stores/theme';

// 같은 Compartment 인스턴스를 여러 editor 가 공유해도 됨 — reconfigure 는
// 대상 view 의 슬롯만 바꾼다 (각 EditorState 가 독립 보관).
export const editorThemeCompartment = new Compartment();

/** 현재 테마에 맞는 editor 테마 확장. dark=oneDark, light=기본(빈 확장). */
export function editorThemeExtension(theme: EffectiveTheme): Extension {
	return theme === 'dark' ? oneDark : [];
}
