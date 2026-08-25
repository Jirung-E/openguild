import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { SearchHit } from '$lib/api/search';
import { matchIdsOf, LatestQuery } from './enhanced-search';

function hit(kind: SearchHit['kind'], id: string): SearchHit {
	return { kind, id, title: id, matched_in: ['body'], excerpt: '' };
}

describe('matchIdsOf', () => {
	it('요청한 종류만 추린다', () => {
		const hits = [hit('quest', 'DEV-001'), hit('book', 'BOOK-002'), hit('rule', 'release-process')];
		expect([...matchIdsOf(hits, 'rule')]).toEqual(['release-process']);
		expect([...matchIdsOf(hits, 'book')]).toEqual(['BOOK-002']);
	});

	it('해당 종류가 없으면 빈 집합', () => {
		expect(matchIdsOf([hit('quest', 'DEV-001')], 'rule').size).toBe(0);
	});
});

describe('LatestQuery', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it('디바운스 — 연속 호출 중 마지막 것만 나간다', async () => {
		const fetcher = vi.fn(async (q: string) => `R:${q}`);
		const lq = new LatestQuery(fetcher, { debounceMs: 250 });
		const seen: string[] = [];
		lq.run('수', (r) => seen.push(r));
		lq.run('수달', (r) => seen.push(r));
		lq.run('수달가', (r) => seen.push(r));
		expect(fetcher).not.toHaveBeenCalled();
		await vi.advanceTimersByTimeAsync(250);
		expect(fetcher).toHaveBeenCalledTimes(1);
		expect(fetcher).toHaveBeenCalledWith('수달가');
		expect(seen).toEqual(['R:수달가']);
	});

	/**
	 * 핵심 계약(REQ-004 가 지적한 결함): 먼저 나간 느린 요청이 나중에 도착해도
	 * 최신 결과를 덮어쓰면 안 된다.
	 */
	it('늦게 도착한 이전 응답은 버려진다', async () => {
		const resolvers: Array<() => void> = [];
		const fetcher = vi.fn(
			(q: string) => new Promise<string>((res) => resolvers.push(() => res(`R:${q}`)))
		);
		const lq = new LatestQuery(fetcher, { debounceMs: 0 });
		const seen: string[] = [];

		lq.run('느린질의', (r) => seen.push(r));
		await vi.advanceTimersByTimeAsync(0);
		lq.run('최신질의', (r) => seen.push(r));
		await vi.advanceTimersByTimeAsync(0);
		expect(fetcher).toHaveBeenCalledTimes(2);

		// 최신 → 그 다음에 느린 것이 도착하는 순서로 응답시킨다.
		resolvers[1]();
		await vi.advanceTimersByTimeAsync(0);
		resolvers[0]();
		await vi.advanceTimersByTimeAsync(0);

		expect(seen).toEqual(['R:최신질의']); // 느린 응답이 덮지 않았다
	});

	it('cancel 은 대기 중인 요청을 막는다', async () => {
		const fetcher = vi.fn(async () => 'R');
		const lq = new LatestQuery(fetcher, { debounceMs: 250 });
		lq.run('x', () => expect.unreachable('취소했는데 결과가 왔다'));
		lq.cancel();
		await vi.advanceTimersByTimeAsync(500);
		expect(fetcher).not.toHaveBeenCalled();
	});

	/** 토글을 끈 뒤 이미 나간 요청이 돌아와도 화면을 건드리면 안 된다. */
	it('cancel 은 이미 나간 요청의 응답도 무시한다', async () => {
		let resolve!: (v: string) => void;
		const fetcher = vi.fn(() => new Promise<string>((r) => (resolve = r)));
		const lq = new LatestQuery(fetcher, { debounceMs: 0 });
		lq.run('x', () => expect.unreachable('취소 후 결과가 반영됐다'));
		await vi.advanceTimersByTimeAsync(0);
		expect(fetcher).toHaveBeenCalledTimes(1);
		lq.cancel();
		resolve('R');
		await vi.advanceTimersByTimeAsync(0);
	});

	it('실패는 onError 로 — 호출측이 기존 동작으로 되돌릴 수 있게', async () => {
		const fetcher = vi.fn(async () => {
			throw new Error('네트워크');
		});
		const lq = new LatestQuery(fetcher, { debounceMs: 0 });
		const errs: unknown[] = [];
		lq.run('x', () => expect.unreachable('성공 콜백이 불렸다'), (e) => errs.push(e));
		await vi.advanceTimersByTimeAsync(0);
		expect(errs).toHaveLength(1);
	});

	it('실패해도 stale 이면 onError 를 부르지 않는다', async () => {
		let fail!: (e: unknown) => void;
		const fetcher = vi.fn((q: string) =>
			q === 'old' ? new Promise<string>((_, rej) => (fail = rej)) : Promise.resolve('R')
		);
		const lq = new LatestQuery(fetcher, { debounceMs: 0 });
		const errs: unknown[] = [];
		lq.run('old', () => {}, (e) => errs.push(e));
		await vi.advanceTimersByTimeAsync(0);
		lq.run('new', () => {}, (e) => errs.push(e));
		await vi.advanceTimersByTimeAsync(0);
		fail(new Error('늦은 실패'));
		await vi.advanceTimersByTimeAsync(0);
		expect(errs).toEqual([]);
	});
});
