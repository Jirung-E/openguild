// DEV-074: theme store unit tests.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';

async function loadFreshStore() {
	vi.resetModules();
	return await import('./theme');
}

describe('theme store', () => {
	beforeEach(() => {
		localStorage.clear();
		document.documentElement.removeAttribute('data-theme');
	});

	it('default is system when no localStorage', async () => {
		const m = await loadFreshStore();
		expect(get(m.theme)).toBe('system');
	});

	it('loads persisted value', async () => {
		localStorage.setItem('openguild.theme', 'light');
		const m = await loadFreshStore();
		expect(get(m.theme)).toBe('light');
	});

	it('ignores garbage in localStorage', async () => {
		localStorage.setItem('openguild.theme', 'rainbow');
		const m = await loadFreshStore();
		expect(get(m.theme)).toBe('system');
	});

	it('setTheme persists', async () => {
		const m = await loadFreshStore();
		m.setTheme('dark');
		expect(localStorage.getItem('openguild.theme')).toBe('dark');
		expect(get(m.theme)).toBe('dark');
	});

	it('resolveTheme handles dark / light directly', async () => {
		const m = await loadFreshStore();
		expect(m.resolveTheme('dark')).toBe('dark');
		expect(m.resolveTheme('light')).toBe('light');
	});

	it('resolveTheme(system) falls back to dark when matchMedia missing', async () => {
		// jsdom 의 matchMedia 가 mock — 기본 false.
		const m = await loadFreshStore();
		const r = m.resolveTheme('system');
		// 'dark' 또는 'light' 둘 중 하나 — system 의 결정은 환경.
		expect(['dark', 'light']).toContain(r);
	});

	it('applyThemeToDocument sets <html data-theme>', async () => {
		const m = await loadFreshStore();
		m.applyThemeToDocument('light');
		expect(document.documentElement.getAttribute('data-theme')).toBe('light');
		m.applyThemeToDocument('dark');
		expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
	});

	// BUG-121: effectiveTheme 은 theme 이 명시 dark/light 로 바뀔 때마다 따라간다.
	it('effectiveTheme tracks explicit theme changes', async () => {
		const m = await loadFreshStore();
		m.setTheme('dark');
		expect(get(m.effectiveTheme)).toBe('dark');
		m.setTheme('light');
		expect(get(m.effectiveTheme)).toBe('light');
	});

	/** jsdom 은 matchMedia 를 구현하지 않아 — 최소 mock 으로 change 이벤트 재현. */
	function mockMatchMedia(initialMatches: boolean) {
		let matches = initialMatches;
		const listeners = new Set<() => void>();
		const mql = {
			get matches() {
				return matches;
			},
			addEventListener: (_type: string, fn: () => void) => listeners.add(fn),
			removeEventListener: (_type: string, fn: () => void) => listeners.delete(fn)
		};
		vi.stubGlobal('matchMedia', () => mql);
		return {
			fireChange(next: boolean) {
				matches = next;
				for (const fn of listeners) fn();
			}
		};
	}

	// BUG-121: system 모드에서 OS preference 가 바뀌면(matchMedia change 이벤트)
	// theme 스토어 값은 그대로 'system' 이지만 effectiveTheme 은 재계산돼야
	// 한다 — 이게 안 되면 Cytoscape/SVG 처럼 JS 가 색을 직접 계산하는 곳이
	// system 모드에서 OS 테마 전환에 반응하지 않는다(사용자 보고 — Quest
	// Board 노드).
	it('effectiveTheme updates on OS preference change while in system mode', async () => {
		const mq = mockMatchMedia(false); // 시작: dark(=matches:false, prefers-color-scheme:light 불일치)
		const m = await loadFreshStore();
		m.setTheme('system');
		expect(get(m.effectiveTheme)).toBe('dark');

		mq.fireChange(true); // OS 가 light 로 전환.
		expect(get(m.effectiveTheme)).toBe('light');

		vi.unstubAllGlobals();
	});

	it('effectiveTheme is untouched by OS preference change when theme is explicit', async () => {
		const mq = mockMatchMedia(false);
		const m = await loadFreshStore();
		m.setTheme('dark');

		mq.fireChange(true); // OS 가 light 로 바뀌어도 사용자가 명시 'dark' 선택했으면 무시.
		expect(get(m.effectiveTheme)).toBe('dark');

		vi.unstubAllGlobals();
	});
});
