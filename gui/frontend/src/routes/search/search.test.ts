// REQ-040: 검색 페이지 — 강화 검색을 탭으로 나누고, 쿼리는 한 종류에서만, 주소에서 시작 상태를 읽는다.
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { readable } from 'svelte/store';

let url = new URL('http://localhost/search?q=코끼리');
vi.mock('$app/stores', () => ({
	page: { subscribe: (fn: (v: unknown) => void) => readable({ url }).subscribe(fn) }
}));
vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
const enhanced = vi.fn();
vi.mock('$lib/api/search', () => ({
	searchApi: { enhanced: (...a: unknown[]) => enhanced(...a) }
}));
const questQuery = vi.fn();
vi.mock('$lib/api/quests', () => ({ questsApi: { matchQuery: (q: string) => questQuery(q) } }));
vi.mock('$lib/api/campaigns', () => ({ campaignsApi: { matchQuery: () => Promise.resolve([]) } }));
vi.mock('$lib/api/library', () => ({ libraryApi: { matchQuery: () => Promise.resolve([]) } }));

const HITS = [
	{ kind: 'quest', id: 'DEV-002', title: '둘째', matched_in: ['body'], excerpt: '맨 아래 코끼리' },
	{ kind: 'campaign', id: 'C-001', title: '코끼리 원정', matched_in: ['title'], excerpt: '' },
	{ kind: 'rule', id: 'zoo', title: 'zoo', matched_in: ['body'], excerpt: '코끼리 규칙' }
];

async function open(href: string) {
	url = new URL(href);
	const { default: Page } = await import('./+page.svelte');
	render(Page);
}

describe('REQ-040 검색 페이지', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
		enhanced.mockReset().mockResolvedValue(HITS);
		questQuery.mockReset().mockResolvedValue([{ quest_id: 'DEV-001', title: '열린 것' }]);
	});

	it('강화 검색 결과를 제목에서 맞은 것부터, 탭마다 개수와 함께', async () => {
		await open('http://localhost/search?q=코끼리');
		const list = await screen.findByTestId('doc-search-results');
		const ids = [...list.querySelectorAll('li')].map((li) => li.textContent ?? '');
		expect(ids[0]).toContain('C-001');
		expect(enhanced).toHaveBeenCalledWith('코끼리', ['body', 'comment', 'attachment']);
		expect(screen.getByTestId('doc-search-tab-quest').textContent).toContain('1');
		expect(screen.getByTestId('doc-search-tab-all').textContent).toContain('3');
		// 제목에서 맞아도 걸린 글자를 칠한다.
		expect([...list.querySelectorAll('mark')].some((m) => m.closest('.title'))).toBe(true);
		// 본문에서만 맞은 문서는 찾기 바를 띄운 채 연다.
		const a = list.querySelector('a[href^="/quests/DEV-002"]') as HTMLAnchorElement;
		expect(a.getAttribute('href')).toContain('find=');
		fireEvent.click(screen.getByTestId('doc-search-tab-rule'));
		await vi.waitFor(() => expect(list.querySelectorAll('li')).toHaveLength(1));
	});

	it('메모 포함을 켜면 메모도 찾는다', async () => {
		await open('http://localhost/search?q=코끼리&memo=1');
		await screen.findByTestId('doc-search-results');
		expect(enhanced).toHaveBeenCalledWith('코끼리', ['body', 'comment', 'attachment', 'memo']);
	});

	it('쿼리는 고른 종류의 쿼리로, 전체 탭에서는 막는다', async () => {
		await open('http://localhost/search?q=status:open&kind=quest&mode=query');
		const list = await screen.findByTestId('doc-search-results');
		expect(questQuery).toHaveBeenCalledWith('status:open');
		expect(list.textContent).toContain('DEV-001');
		fireEvent.click(screen.getByTestId('doc-search-tab-all'));
		expect(await screen.findByTestId('doc-query-blocked')).toBeTruthy();
		// 쿼리를 못 쓰는 탭에서는 쓰는 법(?)도 꺼진다.
		expect((screen.getByTestId('doc-query-help-toggle') as HTMLButtonElement).disabled).toBe(true);
		fireEvent.click(screen.getByTestId('doc-search-tab-campaign'));
		await vi.waitFor(() =>
			expect((screen.getByTestId('doc-query-help-toggle') as HTMLButtonElement).disabled).toBe(
				false
			)
		);
	});
});
