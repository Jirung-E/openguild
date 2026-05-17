import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { detectEnvironment, HttpTransport, TauriTransport } from './transport';

describe('detectEnvironment', () => {
	afterEach(() => {
		const w = window as unknown as Record<string, unknown>;
		delete w.__TAURI__;
		delete w.__TAURI_INTERNALS__;
	});

	it('returns "http" when no Tauri globals present', () => {
		expect(detectEnvironment()).toBe('http');
	});

	it('returns "tauri" when __TAURI_INTERNALS__ is set (Tauri 2.x)', () => {
		(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
		expect(detectEnvironment()).toBe('tauri');
	});

	it('returns "tauri" when __TAURI__ is set (Tauri 1.x compat)', () => {
		(window as unknown as Record<string, unknown>).__TAURI__ = {};
		expect(detectEnvironment()).toBe('tauri');
	});
});

describe('HttpTransport', () => {
	beforeEach(() => {
		vi.unstubAllGlobals();
	});

	function mockFetch(status: number, body: unknown) {
		const text = body === undefined || body === null ? '' : JSON.stringify(body);
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({
				ok: status >= 200 && status < 300,
				status,
				headers: new Headers({ 'content-type': 'application/json' }),
				text: () => Promise.resolve(text),
				json: () => Promise.resolve(body),
				statusText: 'Error'
			})
		);
	}

	it('kind 는 "http"', () => {
		expect(new HttpTransport().kind).toBe('http');
	});

	it('GET 응답 JSON 파싱', async () => {
		mockFetch(200, { hello: 'world' });
		const result = await new HttpTransport().call({ method: 'GET', path: '/api/x' });
		expect(result).toEqual({ hello: 'world' });
	});

	it('POST body 직렬화 + method 정확히', async () => {
		mockFetch(201, { id: 1 });
		await new HttpTransport().call({ method: 'POST', path: '/api/x', body: { foo: 'bar' } });
		const call = vi.mocked(fetch).mock.calls[0];
		const opts = call[1] as RequestInit;
		expect(opts.method).toBe('POST');
		expect(JSON.parse(opts.body as string)).toEqual({ foo: 'bar' });
	});

	it('error 응답은 throw', async () => {
		mockFetch(404, { error: 'not found' });
		await expect(
			new HttpTransport().call({ method: 'GET', path: '/api/missing' })
		).rejects.toThrow('not found');
	});

	it('204 No Content 는 undefined', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({
				ok: true,
				status: 204,
				headers: new Headers(),
				text: () => Promise.resolve('')
			})
		);
		const result = await new HttpTransport().call({ method: 'DELETE', path: '/api/x' });
		expect(result).toBeUndefined();
	});
});

describe('TauriTransport (stub)', () => {
	it('kind 는 "tauri"', () => {
		expect(new TauriTransport().kind).toBe('tauri');
	});

	it('호출 시 명확한 stub 에러 — DEV-004 안내 포함', async () => {
		await expect(
			new TauriTransport().call({ method: 'GET', path: '/api/x' })
		).rejects.toThrow(/DEV-004/);
	});
});
