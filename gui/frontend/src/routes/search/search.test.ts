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
const commentQuery = vi.fn();
vi.mock('$lib/api/comments', () => ({
	globalCommentsApi: { matchQuery: (q: string) => commentQuery(q) }
}));
const highlights = vi.fn();
vi.mock('$lib/api/query', () => ({
	queryApi: { highlights: (q: string, of: string) => highlights(q, of) }
}));

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
		highlights.mockReset().mockResolvedValue([{ text: '열린' }]);
		commentQuery.mockReset().mockResolvedValue([
			{
				scope: 'quest',
				slug: 'DEV-001',
				entry_id: 1,
				ts: '2026-11-01T10:00:00',
				author: 'a',
				body: '옛 댓글'
			},
			{
				scope: 'quest',
				slug: 'DEV-001',
				entry_id: 2,
				ts: '2026-11-02T10:00:00',
				author: 'b',
				body: '새 댓글'
			},
			{
				scope: 'campaign',
				slug: 'C-001',
				entry_id: 1,
				ts: '2026-11-03T10:00:00',
				author: 'a',
				body: '캠페인 댓글'
			}
		]);
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

	it('댓글 탭 — 쿼리는 맞는 댓글을 최근 것부터 한 줄씩, 그 댓글로 간다', async () => {
		await open('http://localhost/search?q=author:a&kind=comment&mode=query');
		const list = await screen.findByTestId('doc-search-results');
		expect(commentQuery).toHaveBeenCalledWith('author:a');
		expect(highlights).toHaveBeenCalledWith('author:a', 'comment');
		const hrefs = [...list.querySelectorAll('a.row-link, li a[href]')].map((a) =>
			a.getAttribute('href')
		);
		expect(hrefs).toContain('/campaigns/C-001?comment=1');
		expect(hrefs).toContain('/quests/DEV-001?comment=2');
		expect(list.querySelectorAll('li')).toHaveLength(3);
		expect(list.querySelector('li')?.textContent).toContain('캠페인 댓글');
		expect(screen.getByTestId('doc-search-tab-comment').textContent).toContain('3');
	});

	it('댓글 탭 — 검색이면 댓글에서 맞은 문서만', async () => {
		enhanced.mockResolvedValue([
			{ kind: 'quest', id: 'DEV-005', title: '가', matched_in: ['comment'], excerpt: '코끼리' },
			{ kind: 'quest', id: 'DEV-006', title: '나', matched_in: ['body'], excerpt: '코끼리' }
		]);
		await open('http://localhost/search?q=코끼리&kind=comment');
		const list = await screen.findByTestId('doc-search-results');
		expect(list.querySelectorAll('li')).toHaveLength(1);
		expect(list.textContent).toContain('DEV-005');
	});

	it('쿼리는 고른 종류의 쿼리로, 못 쓰는 탭으로 가면 검색으로 바뀐다', async () => {
		await open('http://localhost/search?q=status:open&kind=quest&mode=query');
		const list = await screen.findByTestId('doc-search-results');
		expect(questQuery).toHaveBeenCalledWith('status:open');
		expect(list.textContent).toContain('DEV-001');
		// REQ-040: 쿼리 결과도 코어가 뽑아 준 말로 칠한다.
		expect(highlights).toHaveBeenCalledWith('status:open', 'quest');
		await vi.waitFor(() =>
			expect([...list.querySelectorAll('mark')].map((m) => m.textContent)).toEqual(['열린'])
		);
		// '전체' 로 가면 [쿼리] 가 꺼지고 검색으로 — 쓰던 글은 검색칸으로 옮긴다.
		fireEvent.click(screen.getByTestId('doc-search-tab-all'));
		await vi.waitFor(() =>
			expect(screen.getByTestId('doc-query-mode').getAttribute('aria-checked')).toBe('false')
		);
		expect(screen.getByTestId('doc-search-mode').getAttribute('aria-checked')).toBe('true');
		expect((screen.getByTestId('doc-query-mode') as HTMLButtonElement).disabled).toBe(true);
		expect((screen.getByTestId('doc-search-input') as HTMLInputElement).value).toBe('status:open');
		await vi.waitFor(() => expect(enhanced).toHaveBeenCalledWith('status:open', expect.anything()));
		// 규칙 탭도 같다.
		fireEvent.click(screen.getByTestId('doc-search-tab-quest'));
		fireEvent.click(screen.getByTestId('doc-query-mode'));
		fireEvent.click(screen.getByTestId('doc-search-tab-rule'));
		await vi.waitFor(() =>
			expect(screen.getByTestId('doc-search-mode').getAttribute('aria-checked')).toBe('true')
		);
	});
	it('검색칸에 library: 를 쓰면 도서관 탭으로 옮기고 나머지 글로 찾는다(admin)', async () => {
		await open('http://localhost/search');
		const box = screen.getByTestId('doc-search-input') as HTMLInputElement;
		await fireEvent.input(box, { target: { value: 'library:코끼리' } });
		await vi.waitFor(() =>
			expect(screen.getByTestId('doc-search-tab-book').getAttribute('aria-selected')).toBe('true')
		);
		expect((screen.getByTestId('doc-search-input') as HTMLInputElement).value).toBe('코끼리');
		await vi.waitFor(() =>
			expect(enhanced).toHaveBeenCalledWith('코끼리', ['body', 'comment', 'attachment'])
		);
	});
});
