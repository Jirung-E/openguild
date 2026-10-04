// BUG-274: 데스크톱 앱에서 언어를 바꾸면 Rust 쪽(~/.openguild/locale.json)에도 알린다 — 시동 때 한 번, 바꿀 때마다.
// 웹(서버)에서는 알리지 않는다(서버는 자기 설정을 따른다).

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const invoke = vi.fn(() => Promise.resolve());
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...(a as [])) }));

const flush = () => new Promise((r) => setTimeout(r, 0));

describe('BUG-274 언어를 Rust 쪽에도', () => {
	beforeEach(() => {
		vi.resetModules();
		invoke.mockClear();
		localStorage.clear();
	});
	afterEach(() => {
		delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
	});

	it('데스크톱이면 시동 때와 바꿀 때 알린다', async () => {
		(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
		const { setLocale } = await import('./locale');
		await flush();
		expect(invoke).toHaveBeenCalledWith('set_locale', { locale: 'ko' });
		setLocale('en');
		await flush();
		expect(invoke).toHaveBeenLastCalledWith('set_locale', { locale: 'en' });
	});

	it('웹이면 알리지 않는다', async () => {
		const { setLocale } = await import('./locale');
		setLocale('en');
		await flush();
		expect(invoke).not.toHaveBeenCalled();
	});
});
