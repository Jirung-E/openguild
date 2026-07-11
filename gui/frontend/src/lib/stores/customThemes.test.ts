// DEV-114: customThemes store unit tests — 프리셋 CRUD / 활성화 / palette 연동.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';

async function loadFresh() {
	vi.resetModules();
	const theme = await import('./theme');
	const custom = await import('./customThemes');
	return { theme, custom };
}

describe('customThemes store', () => {
	beforeEach(() => {
		localStorage.clear();
		document.documentElement.removeAttribute('data-theme');
		document.documentElement.removeAttribute('style');
	});

	it('starts empty with no active preset', async () => {
		const { custom } = await loadFresh();
		expect(get(custom.customThemes)).toEqual([]);
		expect(get(custom.activeCustomTheme)).toBeNull();
	});

	it('savePreset persists to localStorage and overwrites same name', async () => {
		const { custom } = await loadFresh();
		custom.savePreset({ name: 'mine', base: 'dark', overrides: { '--bg': '#111111' } });
		custom.savePreset({ name: 'mine', base: 'light', overrides: {} });
		const list = get(custom.customThemes);
		expect(list).toHaveLength(1);
		expect(list[0].base).toBe('light');
		const raw = JSON.parse(localStorage.getItem('openguild.customThemes') ?? '[]');
		expect(raw).toHaveLength(1);
	});

	it('activatePreset sets base theme, applies overrides, merges palette', async () => {
		const { theme, custom } = await loadFresh();
		custom.savePreset({ name: 'neon', base: 'light', overrides: { '--accent': '#ff00ff' } });
		custom.activatePreset('neon');

		expect(get(custom.activeCustomTheme)).toBe('neon');
		expect(get(theme.theme)).toBe('light');
		expect(document.documentElement.style.getPropertyValue('--accent')).toBe('#ff00ff');
		// palette 연동 — CSS var() 못 쓰는 소비처(보드)도 같은 색.
		expect(theme.themePalette('light').accent).toBe('#ff00ff');
		// override 안 한 필드는 base 그대로.
		expect(theme.themePalette('light').danger).toBe('#cf222e');
	});

	it('deactivateCustom clears overrides and palette', async () => {
		const { theme, custom } = await loadFresh();
		custom.savePreset({ name: 'neon', base: 'dark', overrides: { '--accent': '#ff00ff' } });
		custom.activatePreset('neon');
		custom.deactivateCustom();

		expect(get(custom.activeCustomTheme)).toBeNull();
		expect(document.documentElement.style.getPropertyValue('--accent')).toBe('');
		expect(theme.themePalette('dark').accent).toBe('#58a6ff');
	});

	it('setActiveOverride updates preset and applies live', async () => {
		const { custom } = await loadFresh();
		custom.savePreset({ name: 'p', base: 'dark', overrides: {} });
		custom.activatePreset('p');
		custom.setActiveOverride('--danger', '#123456');

		expect(get(custom.customThemes)[0].overrides['--danger']).toBe('#123456');
		expect(document.documentElement.style.getPropertyValue('--danger')).toBe('#123456');

		custom.clearActiveOverride('--danger');
		expect(get(custom.customThemes)[0].overrides['--danger']).toBeUndefined();
		expect(document.documentElement.style.getPropertyValue('--danger')).toBe('');
	});

	it('deletePreset of active preset also deactivates', async () => {
		const { custom } = await loadFresh();
		custom.savePreset({ name: 'p', base: 'dark', overrides: { '--bg': '#000001' } });
		custom.activatePreset('p');
		custom.deletePreset('p');

		expect(get(custom.customThemes)).toEqual([]);
		expect(get(custom.activeCustomTheme)).toBeNull();
		expect(document.documentElement.style.getPropertyValue('--bg')).toBe('');
	});

	it('initCustomTheme restores active preset from storage', async () => {
		localStorage.setItem(
			'openguild.customThemes',
			JSON.stringify([{ name: 'saved', base: 'light', overrides: { '--bg': '#fafafa' } }])
		);
		localStorage.setItem('openguild.activeCustomTheme', 'saved');
		const { custom } = await loadFresh();
		await custom.initCustomTheme();
		expect(document.documentElement.style.getPropertyValue('--bg')).toBe('#fafafa');
	});

	it('initCustomTheme cleans dangling active name', async () => {
		localStorage.setItem('openguild.activeCustomTheme', 'ghost');
		const { custom } = await loadFresh();
		await custom.initCustomTheme();
		expect(get(custom.activeCustomTheme)).toBeNull();
	});

	it('export/import roundtrip; import rejects garbage', async () => {
		const { custom } = await loadFresh();
		custom.savePreset({ name: 'a', base: 'dark', overrides: { '--bg': '#101010' } });
		const json = custom.exportPresetsJson();

		localStorage.clear();
		const { custom: fresh } = await loadFresh();
		expect(fresh.importPresetsJson(json)).toBe(1);
		expect(get(fresh.customThemes)[0].name).toBe('a');

		expect(() => fresh.importPresetsJson('{"nope":true}')).toThrow();
	});

	it('loads only valid presets from storage (corrupt entries dropped)', async () => {
		localStorage.setItem(
			'openguild.customThemes',
			JSON.stringify([
				{ name: 'ok', base: 'dark', overrides: {} },
				{ name: '', base: 'dark', overrides: {} },
				{ name: 'bad-base', base: 'rainbow', overrides: {} },
				'garbage'
			])
		);
		const { custom } = await loadFresh();
		expect(get(custom.customThemes).map((p) => p.name)).toEqual(['ok']);
	});
});
