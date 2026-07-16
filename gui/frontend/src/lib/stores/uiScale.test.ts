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

	// BUG-141: DOM 반영이 rAF 로 병합됨(슬라이더 드래그 시 프레임당 1회) —
	// 단언 전에 한 프레임 대기.
	const nextFrame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));

	it('applyUiScaleToDocument writes font-size on documentElement', async () => {
		const m = await loadFreshStore();
		m.applyUiScaleToDocument(1.5);
		await nextFrame();
		const fs = document.documentElement.style.fontSize;
		expect(fs).toMatch(/^24(\.00)?px$/);
		m.applyUiScaleToDocument(0.5);
		await nextFrame();
		expect(document.documentElement.style.fontSize).toMatch(/^8(\.00)?px$/);
	});

	it('applyUiScaleToDocument clamps invalid input', async () => {
		const m = await loadFreshStore();
		m.applyUiScaleToDocument(99);
		await nextFrame();
		// MAX_SCALE = 2.0 → 16 * 2 = 32 px.
		expect(document.documentElement.style.fontSize).toMatch(/^32(\.00)?px$/);
	});

	it('BUG-141: 같은 프레임의 연속 호출은 마지막 값만 반영', async () => {
		const m = await loadFreshStore();
		m.applyUiScaleToDocument(1.1);
		m.applyUiScaleToDocument(1.3);
		m.applyUiScaleToDocument(2.0);
		await nextFrame();
		expect(document.documentElement.style.fontSize).toMatch(/^32(\.00)?px$/);
	});
});
