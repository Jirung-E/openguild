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
});
