// DEV-101: uiScale store unit tests.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';

// 매 테스트마다 localStorage 초기화 + module 재import (writable 의 internal
// state 가 module-level 이라 같은 module 공유 시 누적).
async function loadFreshStore() {
	vi.resetModules();
	return await import('./uiScale');
}

describe('uiScale store', () => {
	beforeEach(() => {
		localStorage.clear();
	});

	it('initial value defaults to 1.0 when no localStorage', async () => {
		const m = await loadFreshStore();
		expect(get(m.uiScale)).toBe(m.DEFAULT_SCALE);
		expect(m.DEFAULT_SCALE).toBe(1.0);
	});

	it('loads persisted value from localStorage', async () => {
		localStorage.setItem('openguild.uiScale', '1.4');
		const m = await loadFreshStore();
		expect(get(m.uiScale)).toBeCloseTo(1.4);
	});

	it('clamps out-of-range persisted value', async () => {
		localStorage.setItem('openguild.uiScale', '5.0');
		const m = await loadFreshStore();
		expect(get(m.uiScale)).toBe(m.MAX_SCALE);
	});

	it('ignores garbage in localStorage', async () => {
		localStorage.setItem('openguild.uiScale', 'nope');
		const m = await loadFreshStore();
		expect(get(m.uiScale)).toBe(m.DEFAULT_SCALE);
	});

	it('setUiScale clamps to [MIN, MAX]', async () => {
		const m = await loadFreshStore();
		m.setUiScale(0.1);
		expect(get(m.uiScale)).toBe(m.MIN_SCALE);
		m.setUiScale(99);
		expect(get(m.uiScale)).toBe(m.MAX_SCALE);
		m.setUiScale(1.2);
		expect(get(m.uiScale)).toBeCloseTo(1.2);
	});

	it('setUiScale persists to localStorage', async () => {
		const m = await loadFreshStore();
		m.setUiScale(1.5);
		expect(localStorage.getItem('openguild.uiScale')).toBe('1.5');
	});

	it('resetUiScale returns to DEFAULT_SCALE', async () => {
		const m = await loadFreshStore();
		m.setUiScale(1.8);
		expect(get(m.uiScale)).toBeCloseTo(1.8);
		m.resetUiScale();
		expect(get(m.uiScale)).toBe(m.DEFAULT_SCALE);
	});

	it('applyUiScaleToDocument writes font-size on documentElement', async () => {
		const m = await loadFreshStore();
		m.applyUiScaleToDocument(1.5);
		const fs = document.documentElement.style.fontSize;
		expect(fs).toMatch(/^24(\.00)?px$/);
		m.applyUiScaleToDocument(0.5);
		expect(document.documentElement.style.fontSize).toMatch(/^8(\.00)?px$/);
	});

	it('applyUiScaleToDocument clamps invalid input', async () => {
		const m = await loadFreshStore();
		m.applyUiScaleToDocument(99);
		// MAX_SCALE = 2.0 → 16 * 2 = 32 px.
		expect(document.documentElement.style.fontSize).toMatch(/^32(\.00)?px$/);
	});
});
