// BUG-380(admin): 편집기를 켠 채 라이트 → 시스템(OS 는 다크)으로 바꾸면 줄 번호 칸이 라이트로 남았다.
//
// 앱은 라이트를 고르면 창을 라이트로 고정한다. '시스템' 으로 바꾸는 순간에는 창이 아직 라이트라 라이트로 읽히고,
// 창이 풀려 다크가 된 뒤에는 고른 값('system')이 그대로라 편집기가 다시 보지 않았다. 편집기는 **실제 테마**
// (`effectiveTheme`)를 따라야 한다 — 고른 값이 그대로여도 실제 테마가 바뀌면 바뀐다.
import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import { EditorView } from '@codemirror/view';
import MarkdownEditor from './MarkdownEditor.svelte';
import { theme, effectiveTheme } from '$lib/stores/theme';

vi.stubGlobal(
	'ResizeObserver',
	class {
		observe() {}
		unobserve() {}
		disconnect() {}
	}
);

function dark(): boolean {
	const el = document.querySelector('.cm-editor') as HTMLElement;
	return EditorView.findFromDOM(el)!.state.facet(EditorView.darkTheme);
}

describe('BUG-380 편집기 테마는 실제 테마를 따른다', () => {
	it("고른 값이 'system' 그대로여도, 실제 테마가 다크로 바뀌면 편집기도 다크", async () => {
		theme.set('light');
		effectiveTheme.set('light');
		render(MarkdownEditor, { props: { value: '', onError: () => {} } });
		await tick();
		expect(dark()).toBe(false);
		// 시스템으로 바꾸는 순간 — 창이 아직 라이트라 라이트로 읽힌다.
		theme.set('system');
		effectiveTheme.set('light');
		await tick();
		expect(dark()).toBe(false);
		// 창이 풀려 OS 의 다크가 보인다 — 고른 값은 그대로 'system'.
		effectiveTheme.set('dark');
		await tick();
		expect(dark()).toBe(true);
	});
});
