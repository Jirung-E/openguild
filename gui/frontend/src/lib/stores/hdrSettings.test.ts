// DEV-335: hdrSettings store unit tests.

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';

async function loadFreshStore() {
	vi.resetModules();
	return await import('./hdrSettings');
}

describe('hdrSettings store', () => {
	beforeEach(() => {
		localStorage.clear();
		document.documentElement.style.removeProperty('--hdr-limit');
	});

	it('initial value defaults to no-limit (기존 동작 유지) when no localStorage', async () => {
		const m = await loadFreshStore();
		expect(get(m.hdrLimit)).toBe('no-limit');
		expect(m.DEFAULT_HDR_LIMIT).toBe('no-limit');
	});

	it('loads persisted value from localStorage', async () => {
		localStorage.setItem('openguild.hdrLimit', 'constrained');
		const m = await loadFreshStore();
		expect(get(m.hdrLimit)).toBe('constrained');
	});

	it('ignores garbage in localStorage — falls back to default', async () => {
		localStorage.setItem('openguild.hdrLimit', 'bogus-value');
		const m = await loadFreshStore();
		expect(get(m.hdrLimit)).toBe('no-limit');
	});

	it('setHdrLimit updates the store and persists', async () => {
		const m = await loadFreshStore();
		m.setHdrLimit('standard');
		expect(get(m.hdrLimit)).toBe('standard');
		expect(localStorage.getItem('openguild.hdrLimit')).toBe('standard');
	});

	it('applyHdrLimitToDocument writes the --hdr-limit custom property', async () => {
		const m = await loadFreshStore();
		m.applyHdrLimitToDocument('constrained');
		expect(document.documentElement.style.getPropertyValue('--hdr-limit')).toBe('constrained');
	});
});

describe('isHdrLimitSupported', () => {
	const originalSupports = CSS.supports;

	afterEach(() => {
		CSS.supports = originalSupports;
	});

	it('true when CSS.supports confirms dynamic-range-limit', async () => {
		CSS.supports = vi.fn(() => true) as typeof CSS.supports;
		const m = await loadFreshStore();
		expect(m.isHdrLimitSupported()).toBe(true);
	});

	it('false when CSS.supports rejects dynamic-range-limit (jsdom 기본값)', async () => {
		CSS.supports = vi.fn(() => false) as typeof CSS.supports;
		const m = await loadFreshStore();
		expect(m.isHdrLimitSupported()).toBe(false);
	});
});
