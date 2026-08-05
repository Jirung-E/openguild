/**
 * BUG-215: 터치 구성이 네이티브 선택을 살려두는지.
 *
 * 이게 깨지면 모바일에서 "길게 눌러 선택" 이 다시 안 된다 — 화면상으로는
 * 아무 에러도 없고 그냥 선택이 안 잡히는 조용한 회귀라 테스트로 묶어둔다.
 *
 * `EditorView.nativeSelectionHidden` 팩싯은 public export 가 아니라 직접 읽을
 * 수 없다. 대신 **관측 가능한 결과**로 판정한다: drawSelection 은 자체 선택
 * 레이어(`.cm-selectionLayer`)를 만들고 캐럿을 투명하게 하므로, 그 레이어의
 * 유무가 곧 네이티브 선택이 살아 있는지 여부다.
 */
import { describe, it, expect, afterEach } from 'vitest';
import { EditorView } from '@codemirror/view';
import { EditorSelection } from '@codemirror/state';
import { basicSetup } from 'codemirror';
import { touchSetup, desktopSetup, markdownEditorExtensions } from './editor-setup';

let view: EditorView | null = null;

afterEach(() => {
	view?.destroy();
	view = null;
});

function mount(extensions: unknown, doc = 'hello\nworld') {
	const parent = document.createElement('div');
	document.body.appendChild(parent);
	view = new EditorView({
		doc,
		extensions: extensions as never,
		parent
	});
	return parent;
}

/** CodeMirror 는 실제 keydown DOM 이벤트를 contentDOM 에서 처리한다 —
 * jsdom 에서도 동일 경로로 keymap 이 반응한다. */
function pressEnter() {
	view!.contentDOM.dispatchEvent(
		new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true })
	);
}

describe('BUG-215 터치 편집기 구성', () => {
	it('터치 구성은 자체 선택 레이어를 만들지 않는다 (= 네이티브 선택 유지)', () => {
		const parent = mount(touchSetup());
		expect(parent.querySelector('.cm-selectionLayer')).toBeNull();
	});

	it('basicSetup 은 자체 선택 레이어를 만든다 (이 버그의 원인 — 대조군)', () => {
		const parent = mount(basicSetup);
		expect(parent.querySelector('.cm-selectionLayer')).not.toBeNull();
	});

	it('터치 구성도 편집 기능은 유지한다', () => {
		mount(touchSetup());
		expect(view!.state.doc.lines).toBe(2);
		// 줄 번호 거터는 basicSetup 과 동일하게 있어야 한다.
		expect(view!.dom.querySelector('.cm-gutters')).not.toBeNull();
	});
});

describe('DEV-336 자동 서식 끄기', () => {
	it('markdownEditorExtensions(autoFormat: true) — "- " 다음 줄에 이어짐 (기본 동작)', () => {
		mount(markdownEditorExtensions({ touch: false, autoFormat: true }), '- item');
		view!.dispatch({ selection: EditorSelection.cursor(view!.state.doc.length) });
		pressEnter();
		expect(view!.state.doc.toString()).toBe('- item\n- ');
	});

	it('markdownEditorExtensions(autoFormat: false) — "- " 다음 줄에 안 이어짐', () => {
		mount(markdownEditorExtensions({ touch: false, autoFormat: false }), '- item');
		view!.dispatch({ selection: EditorSelection.cursor(view!.state.doc.length) });
		pressEnter();
		expect(view!.state.doc.toString()).toBe('- item\n');
	});

	it('markdownEditorExtensions(autoFormat: false, touch: true) — 터치 구성에서도 동일', () => {
		mount(markdownEditorExtensions({ touch: true, autoFormat: false }), '- item');
		view!.dispatch({ selection: EditorSelection.cursor(view!.state.doc.length) });
		pressEnter();
		expect(view!.state.doc.toString()).toBe('- item\n');
	});

	it('desktopSetup({ indentOnInput: false }) 도 basicSetup 과 동일하게 줄 번호 거터를 유지한다', () => {
		mount(desktopSetup({ indentOnInput: false }));
		expect(view!.dom.querySelector('.cm-gutters')).not.toBeNull();
		expect(view!.state.doc.lines).toBe(2);
	});
});
