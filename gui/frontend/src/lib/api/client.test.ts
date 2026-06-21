import { describe, it, expect, vi, beforeEach } from 'vitest';
import { api } from './client';

// fetch 모킹 헬퍼 (실제 Response 와 유사하게: headers, text 포함)
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
			statusText: status === 404 ? 'Not Found' : 'Error'
		})
	);
}

beforeEach(() => {
	vi.unstubAllGlobals();
});

describe('api.get', () => {
	it('returns parsed JSON on success', async () => {
		mockFetch(200, [{ id: 1, prefix: 'DEV' }]);
		const result = await api.get('/api/quest-types');
		expect(result).toEqual([{ id: 1, prefix: 'DEV' }]);
	});

	it('throws on error response', async () => {
		mockFetch(404, { error: 'not found' });
		await expect(api.get('/api/quests/999')).rejects.toThrow('not found');
	});
});

describe('api.post', () => {
	it('sends JSON body and returns response', async () => {
		const created = { id: 1, quest_id: 'DEV-001', title: 'test quest' };
		mockFetch(201, created);

		const result = await api.post('/api/quests', {
			quest_type_id: 1,
			title: 'test quest',
			status_slug: 'open'
		});

		expect(result).toEqual(created);

		const fetchCall = vi.mocked(fetch).mock.calls[0];
		const options = fetchCall[1] as RequestInit;
		expect(options.method).toBe('POST');
		expect(JSON.parse(options.body as string)).toMatchObject({ title: 'test quest' });
	});
});

describe('api.patch', () => {
	it('sends PATCH request', async () => {
		mockFetch(200, { id: 1, status_slug: 'in_progress' });
		await api.patch('/api/quests/1/status', { status_slug: 'in_progress' });

		const options = vi.mocked(fetch).mock.calls[0][1] as RequestInit;
		expect(options.method).toBe('PATCH');
	});
});

describe('api.delete', () => {
	it('sends DELETE and handles 204', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({
				ok: true,
				status: 204,
				headers: new Headers(),
				text: () => Promise.resolve(''),
				json: () => Promise.resolve(null)
			})
		);
		const result = await api.delete('/api/quests/1');
		expect(result).toBeUndefined();
	});
});

describe('empty body handling', () => {
	it('handles 201 Created with empty body (선행퀘 추가 케이스)', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({
				ok: true,
				status: 201,
				headers: new Headers({ 'content-length': '0' }),
				text: () => Promise.resolve(''),
				json: () => Promise.reject(new Error('should not be called'))
			})
		);
		const result = await api.post('/api/quests/1/prerequisites', { prerequisite_id: 2 });
		expect(result).toBeUndefined();
	});

	it('handles 200 with empty text body', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn().mockResolvedValue({
				ok: true,
				status: 200,
				headers: new Headers(),
				text: () => Promise.resolve(''),
				json: () => Promise.reject(new Error('should not be called'))
			})
		);
		const result = await api.get('/api/whatever');
		expect(result).toBeUndefined();
	});
});
