import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
	detectEnvironment,
	HttpTransport,
	TauriTransport,
	__test_only
} from './transport';

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn()
}));
import { invoke } from '@tauri-apps/api/core';
const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

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

describe('TauriTransport', () => {
	beforeEach(() => {
		mockInvoke.mockReset();
	});

	it('kind 는 "tauri"', () => {
		expect(new TauriTransport().kind).toBe('tauri');
	});

	it('GET /api/quests → list_quests invoke', async () => {
		mockInvoke.mockResolvedValue([{ id: 1 }]);
		const result = await new TauriTransport().call({ method: 'GET', path: '/api/quests' });
		expect(mockInvoke).toHaveBeenCalledWith('list_quests', {});
		expect(result).toEqual([{ id: 1 }]);
	});

	it('GET /api/quests/42 → get_quest with id', async () => {
		mockInvoke.mockResolvedValue({ id: 42 });
		await new TauriTransport().call({ method: 'GET', path: '/api/quests/42' });
		expect(mockInvoke).toHaveBeenCalledWith('get_quest', { id: 42 });
	});

	it('POST /api/quests → create_quest with body', async () => {
		mockInvoke.mockResolvedValue({ id: 7 });
		await new TauriTransport().call({
			method: 'POST',
			path: '/api/quests',
			body: { title: 'x', quest_type_id: 1, status_id: 1 }
		});
		expect(mockInvoke).toHaveBeenCalledWith('create_quest', {
			body: { title: 'x', quest_type_id: 1, status_id: 1 }
		});
	});

	it('PATCH /api/quests/3/status → change_quest_status', async () => {
		mockInvoke.mockResolvedValue({ id: 3 });
		await new TauriTransport().call({
			method: 'PATCH',
			path: '/api/quests/3/status',
			body: { status_id: 2 }
		});
		expect(mockInvoke).toHaveBeenCalledWith('change_quest_status', {
			id: 3,
			body: { status_id: 2 }
		});
	});

	it('DELETE /api/quests/5?cascade=6,7 → delete_quest with cascade', async () => {
		mockInvoke.mockResolvedValue(null);
		await new TauriTransport().call({ method: 'DELETE', path: '/api/quests/5?cascade=6,7' });
		expect(mockInvoke).toHaveBeenCalledWith('delete_quest', { id: 5, cascade: [6, 7] });
	});

	it('PUT /api/quests/1/position → update_quest_position', async () => {
		mockInvoke.mockResolvedValue({ quest_id: 1, x: 10, y: 20 });
		await new TauriTransport().call({
			method: 'PUT',
			path: '/api/quests/1/position',
			body: { x: 10, y: 20 }
		});
		expect(mockInvoke).toHaveBeenCalledWith('update_quest_position', {
			id: 1,
			body: { x: 10, y: 20 }
		});
	});

	it('GET /api/quest-positions → list_quest_positions', async () => {
		mockInvoke.mockResolvedValue([]);
		await new TauriTransport().call({ method: 'GET', path: '/api/quest-positions' });
		expect(mockInvoke).toHaveBeenCalledWith('list_quest_positions', {});
	});

	it('GET /api/quests/by/DEV-001 → get_quest_by_slug', async () => {
		mockInvoke.mockResolvedValue({ id: 1 });
		await new TauriTransport().call({ method: 'GET', path: '/api/quests/by/DEV-001' });
		expect(mockInvoke).toHaveBeenCalledWith('get_quest_by_slug', { slug: 'DEV-001' });
	});

	it('GET /api/quests/2/candidates?relation=parent → list_quest_candidates', async () => {
		mockInvoke.mockResolvedValue([]);
		await new TauriTransport().call({
			method: 'GET',
			path: '/api/quests/2/candidates?relation=parent'
		});
		expect(mockInvoke).toHaveBeenCalledWith('list_quest_candidates', {
			id: 2,
			relation: 'parent'
		});
	});

	it('POST /api/admin/reindex → admin_reindex', async () => {
		mockInvoke.mockResolvedValue({});
		await new TauriTransport().call({ method: 'POST', path: '/api/admin/reindex' });
		expect(mockInvoke).toHaveBeenCalledWith('admin_reindex', {});
	});

	it('미매핑 path 는 명확한 에러', async () => {
		await expect(
			new TauriTransport().call({ method: 'GET', path: '/api/no-such-thing' })
		).rejects.toThrow(/매핑된 invoke 핸들러 없음/);
	});

	it('invoke 가 string 에러 throw → Error 로 감싸짐', async () => {
		mockInvoke.mockRejectedValue('quest not found');
		await expect(
			new TauriTransport().call({ method: 'GET', path: '/api/quests/999' })
		).rejects.toThrow('quest not found');
	});

	it('routeToInvoke — meta 양쪽 매핑', () => {
		expect(__test_only.routeToInvoke({ method: 'GET', path: '/api/quest-types' })).toEqual({
			cmd: 'list_quest_types',
			args: {}
		});
		expect(__test_only.routeToInvoke({ method: 'GET', path: '/api/quest-statuses' })).toEqual({
			cmd: 'list_quest_statuses',
			args: {}
		});
	});
});
