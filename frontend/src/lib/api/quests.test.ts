import { describe, it, expect, vi, beforeEach } from 'vitest';
import { questsApi } from './quests';

function mockFetch(status: number, body: unknown) {
	const text = body === undefined || body === null ? '' : JSON.stringify(body);
	const fn = vi.fn().mockResolvedValue({
		ok: status >= 200 && status < 300,
		status,
		headers: new Headers({ 'content-type': 'application/json' }),
		text: () => Promise.resolve(text),
		json: () => Promise.resolve(body),
		statusText: 'OK'
	});
	vi.stubGlobal('fetch', fn);
	return fn;
}

beforeEach(() => {
	vi.unstubAllGlobals();
});

describe('questsApi.delete', () => {
	it('without cascade — no query string', async () => {
		const fn = mockFetch(204, null);
		await questsApi.delete(42);
		const url = fn.mock.calls[0][0] as string;
		expect(url).toContain('/api/quests/42');
		expect(url).not.toContain('cascade=');
	});

	it('with cascade — appends comma-separated query', async () => {
		const fn = mockFetch(204, null);
		await questsApi.delete(42, [1, 2, 3]);
		const url = fn.mock.calls[0][0] as string;
		expect(url).toContain('/api/quests/42?cascade=1,2,3');
	});

	it('empty cascade array — no query', async () => {
		const fn = mockFetch(204, null);
		await questsApi.delete(42, []);
		const url = fn.mock.calls[0][0] as string;
		expect(url).not.toContain('cascade=');
	});

	it('uses DELETE method', async () => {
		const fn = mockFetch(204, null);
		await questsApi.delete(42);
		const opts = fn.mock.calls[0][1] as RequestInit;
		expect(opts.method).toBe('DELETE');
	});
});

describe('questsApi.changeParent', () => {
	it('PATCH /parent with parent_quest_id', async () => {
		const fn = mockFetch(200, { id: 1, parent_quest_id: 5 });
		await questsApi.changeParent(1, { parent_quest_id: 5 });
		const url = fn.mock.calls[0][0] as string;
		const opts = fn.mock.calls[0][1] as RequestInit;
		expect(url).toContain('/api/quests/1/parent');
		expect(opts.method).toBe('PATCH');
		expect(JSON.parse(opts.body as string)).toEqual({ parent_quest_id: 5 });
	});

	it('detach — explicit null in body', async () => {
		const fn = mockFetch(200, { id: 1, parent_quest_id: null });
		await questsApi.changeParent(1, { parent_quest_id: null });
		const opts = fn.mock.calls[0][1] as RequestInit;
		expect(JSON.parse(opts.body as string)).toEqual({ parent_quest_id: null });
	});
});

describe('questsApi.candidates', () => {
	it('parent relation', async () => {
		const fn = mockFetch(200, []);
		await questsApi.candidates(7, 'parent');
		const url = fn.mock.calls[0][0] as string;
		expect(url).toContain('/api/quests/7/candidates?relation=parent');
	});

	it('sub relation', async () => {
		const fn = mockFetch(200, []);
		await questsApi.candidates(7, 'sub');
		const url = fn.mock.calls[0][0] as string;
		expect(url).toContain('relation=sub');
	});

	it('prereq relation', async () => {
		const fn = mockFetch(200, []);
		await questsApi.candidates(7, 'prereq');
		const url = fn.mock.calls[0][0] as string;
		expect(url).toContain('relation=prereq');
	});
});

describe('questsApi.addPrerequisite', () => {
	it('handles 201 Created with empty body without throwing', async () => {
		// 백엔드는 201 + 빈 body 반환 — client.ts 에서 처리되어야 함
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
		await expect(questsApi.addPrerequisite(1, 2)).resolves.toBeUndefined();
	});
});
