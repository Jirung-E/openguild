import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
	detectEnvironment,
	HttpTransport,
	TauriTransport,
	transport,
	__test_only
} from './transport';
import { setRemoteServerUrl } from '$lib/stores/remoteServer';

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
		await expect(new HttpTransport().call({ method: 'GET', path: '/api/missing' })).rejects.toThrow(
			'not found'
		);
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
			body: { title: 'x', quest_type_id: 1, status_slug: 'open' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('create_quest', {
			body: { title: 'x', quest_type_id: 1, status_slug: 'open' }
		});
	});

	it('PATCH /api/quests/3/status → change_quest_status', async () => {
		mockInvoke.mockResolvedValue({ id: 3 });
		await new TauriTransport().call({
			method: 'PATCH',
			path: '/api/quests/3/status',
			body: { status_slug: 'in_progress' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('change_quest_status', {
			id: 3,
			body: { status_slug: 'in_progress' }
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

	// DEV-055: quest type 변경.
	it('PATCH /api/quests/42/type → change_quest_type', async () => {
		mockInvoke.mockResolvedValue({});
		await new TauriTransport().call({
			method: 'PATCH',
			path: '/api/quests/42/type',
			body: { new_type_prefix: 'BUG' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('change_quest_type', {
			id: 42,
			body: { new_type_prefix: 'BUG' }
		});
	});

	it('POST /api/admin/reindex → admin_reindex', async () => {
		mockInvoke.mockResolvedValue({});
		await new TauriTransport().call({ method: 'POST', path: '/api/admin/reindex' });
		expect(mockInvoke).toHaveBeenCalledWith('admin_reindex', {});
	});

	// DEV-014: admin meta — types / statuses.
	it('GET /api/admin/types → admin_list_types', async () => {
		mockInvoke.mockResolvedValue([]);
		await new TauriTransport().call({ method: 'GET', path: '/api/admin/types' });
		expect(mockInvoke).toHaveBeenCalledWith('admin_list_types', {});
	});
	it('POST /api/admin/types → admin_create_type', async () => {
		mockInvoke.mockResolvedValue({});
		await new TauriTransport().call({
			method: 'POST',
			path: '/api/admin/types',
			body: { prefix: 'FOO', color: '#000' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('admin_create_type', {
			body: { prefix: 'FOO', color: '#000' }
		});
	});
	it('PATCH /api/admin/types/DEV → admin_update_type', async () => {
		mockInvoke.mockResolvedValue({});
		await new TauriTransport().call({
			method: 'PATCH',
			path: '/api/admin/types/DEV',
			body: { color: '#abc' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('admin_update_type', {
			prefix: 'DEV',
			body: { color: '#abc' }
		});
	});
	it('DELETE /api/admin/types/REQ → admin_delete_type', async () => {
		mockInvoke.mockResolvedValue(undefined);
		await new TauriTransport().call({ method: 'DELETE', path: '/api/admin/types/REQ' });
		expect(mockInvoke).toHaveBeenCalledWith('admin_delete_type', { prefix: 'REQ' });
	});
	it('GET /api/admin/statuses → admin_list_statuses', async () => {
		mockInvoke.mockResolvedValue([]);
		await new TauriTransport().call({ method: 'GET', path: '/api/admin/statuses' });
		expect(mockInvoke).toHaveBeenCalledWith('admin_list_statuses', {});
	});
	it('PATCH /api/admin/statuses/open → admin_update_status', async () => {
		mockInvoke.mockResolvedValue({});
		await new TauriTransport().call({
			method: 'PATCH',
			path: '/api/admin/statuses/open',
			body: { name_ko: '게시' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('admin_update_status', {
			slug: 'open',
			body: { name_ko: '게시' }
		});
	});
	// BUG-018: update 가 prefix/slug rename 도 통합 — body 안에 new_prefix / new_slug.
	it('PATCH /api/admin/types/DEV (with new_prefix) → admin_update_type', async () => {
		mockInvoke.mockResolvedValue({});
		await new TauriTransport().call({
			method: 'PATCH',
			path: '/api/admin/types/DEV',
			body: { new_prefix: 'CORE', color: '#abc' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('admin_update_type', {
			prefix: 'DEV',
			body: { new_prefix: 'CORE', color: '#abc' }
		});
	});

	it('DELETE /api/admin/statuses/on_hold → admin_delete_status', async () => {
		mockInvoke.mockResolvedValue(undefined);
		await new TauriTransport().call({
			method: 'DELETE',
			path: '/api/admin/statuses/on_hold'
		});
		expect(mockInvoke).toHaveBeenCalledWith('admin_delete_status', { slug: 'on_hold' });
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

	// DEV-152: 첨부 업로드 — 브라우저(server) 모드 지원 매핑.
	it('POST /api/attachments → save_attachment', async () => {
		mockInvoke.mockResolvedValue('attachments/abc.png');
		await new TauriTransport().call({
			method: 'POST',
			path: '/api/attachments',
			body: { data_base64: 'QUJD', ext: 'png' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('save_attachment', {
			dataBase64: 'QUJD',
			ext: 'png'
		});
	});

	it('POST /api/quests/by/DEV-001/attachments → add_quest_attachment', async () => {
		mockInvoke.mockResolvedValue([]);
		await new TauriTransport().call({
			method: 'POST',
			path: '/api/quests/by/DEV-001/attachments',
			body: { path: 'attachments/abc.png', name: 'pic.png' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('add_quest_attachment', {
			slug: 'DEV-001',
			path: 'attachments/abc.png',
			name: 'pic.png'
		});
	});

	it('DELETE /api/quests/by/DEV-001/attachments?path=... → remove_quest_attachment', async () => {
		mockInvoke.mockResolvedValue([]);
		await new TauriTransport().call({
			method: 'DELETE',
			path: '/api/quests/by/DEV-001/attachments?path=attachments%2Fabc.png'
		});
		expect(mockInvoke).toHaveBeenCalledWith('remove_quest_attachment', {
			slug: 'DEV-001',
			path: 'attachments/abc.png'
		});
	});

	it('POST /api/campaigns/C-001/attachments → add_campaign_attachment', async () => {
		mockInvoke.mockResolvedValue([]);
		await new TauriTransport().call({
			method: 'POST',
			path: '/api/campaigns/C-001/attachments',
			body: { path: 'attachments/abc.png', name: 'pic.png' }
		});
		expect(mockInvoke).toHaveBeenCalledWith('add_campaign_attachment', {
			slug: 'C-001',
			path: 'attachments/abc.png',
			name: 'pic.png'
		});
	});

	it('DELETE /api/campaigns/C-001/attachments?path=... → remove_campaign_attachment', async () => {
		mockInvoke.mockResolvedValue([]);
		await new TauriTransport().call({
			method: 'DELETE',
			path: '/api/campaigns/C-001/attachments?path=attachments%2Fabc.png'
		});
		expect(mockInvoke).toHaveBeenCalledWith('remove_campaign_attachment', {
			slug: 'C-001',
			path: 'attachments/abc.png'
		});
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

// DEV-113 (MVP): 원격 서버 모드 — `transport` (동적 위임)이 Tauri 환경에서도
// remoteServer 가 설정돼 있으면 invoke 대신 그 URL 로 HTTP 호출하는지.
describe('transport (동적 위임, DEV-113 원격 모드)', () => {
	beforeEach(() => {
		mockInvoke.mockReset();
		setRemoteServerUrl(null); // 매 테스트 로컬(기본)로 리셋.
	});
	afterEach(() => {
		const w = window as unknown as Record<string, unknown>;
		delete w.__TAURI__;
		delete w.__TAURI_INTERNALS__;
		setRemoteServerUrl(null);
		vi.unstubAllGlobals();
	});

	it('Tauri 환경 + 원격 URL 미설정 → invoke 사용(기존 동작 그대로)', async () => {
		(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
		mockInvoke.mockResolvedValue([]);
		expect(transport.kind).toBe('tauri');
		await transport.call({ method: 'GET', path: '/api/quests' });
		expect(mockInvoke).toHaveBeenCalledWith('list_quests', {});
	});

	it('Tauri 환경 + 원격 URL 설정 → invoke 대신 그 URL 로 fetch', async () => {
		(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
		setRemoteServerUrl('http://192.168.1.10:3000');
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({
				ok: true,
				status: 200,
				headers: new Headers({ 'content-type': 'application/json' }),
				text: () => Promise.resolve('[]')
			})
		);
		expect(transport.kind).toBe('http');
		await transport.call({ method: 'GET', path: '/api/quests' });
		expect(mockInvoke).not.toHaveBeenCalled();
		const call = vi.mocked(fetch).mock.calls[0];
		expect(call[0]).toBe('http://192.168.1.10:3000/api/quests');
	});

	it('브라우저 환경에선 원격 URL 설정과 무관하게 항상 HTTP(기존 동작)', async () => {
		// Tauri 글로벌 없음 → detectEnvironment()==='http'.
		setRemoteServerUrl('http://192.168.1.10:3000'); // 영향 없어야.
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({
				ok: true,
				status: 200,
				headers: new Headers({ 'content-type': 'application/json' }),
				text: () => Promise.resolve('[]')
			})
		);
		expect(transport.kind).toBe('http');
		await transport.call({ method: 'GET', path: '/api/quests' });
		// 브라우저 모드는 remoteServerUrl 을 안 보고 기존 base(빈 문자열/VITE_API_URL) 사용.
		const call = vi.mocked(fetch).mock.calls[0];
		expect(call[0]).not.toContain('192.168.1.10');
	});
});
