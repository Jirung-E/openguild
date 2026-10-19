import { beforeEach, describe, expect, it, vi } from 'vitest';

const lists = vi.hoisted(() => ({
	quests: [{ quest_id: 'DEV-001', title: '퀘스트' }],
	campaigns: [{ campaign_slug: 'C-001', title: '캠페인' }],
	rules: { entries: [{ slug: '코딩 규칙', content: '# 코딩 규칙' }] },
	books: [{ book_id: 'BOOK-003', title: '책', path: 'a.md' }]
}));
vi.mock('$lib/api/quests', () => ({ questsApi: { list: async () => lists.quests } }));
vi.mock('$lib/api/campaigns', () => ({ campaignsApi: { list: async () => lists.campaigns } }));
vi.mock('$lib/api/rules', () => ({ rulesApi: { list: async () => lists.rules } }));
vi.mock('$lib/api/library', () => ({ libraryApi: { list: async () => lists.books } }));

import { launchDocHref } from './launch-doc';

describe('launchDocHref', () => {
	beforeEach(() => vi.clearAllMocks());

	it('퀘스트 · 캠페인 · 도서관 · 규칙을 링크와 같은 규칙으로 푼다', async () => {
		expect(await launchDocHref('dev-001')).toBe('/quests/DEV-001');
		expect(await launchDocHref('C-001')).toBe('/campaigns/C-001');
		expect(await launchDocHref('BOOK-003')).toBe('/library?id=BOOK-003');
		expect(await launchDocHref('코딩 규칙')).toBe(`/rules?slug=${encodeURIComponent('코딩 규칙')}`);
		expect(await launchDocHref('rule:코딩 규칙')).toBe(
			`/rules?slug=${encodeURIComponent('코딩 규칙')}`
		);
		expect(await launchDocHref(' quest:DEV-001 ')).toBe('/quests/DEV-001');
	});

	it('없는 번호는 null', async () => {
		expect(await launchDocHref('DEV-999')).toBeNull();
		expect(await launchDocHref('campaign:DEV-001')).toBeNull();
	});
});
