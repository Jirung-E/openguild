// DEV-417: 관계 목록(부모·하위·선행·후속·연결된 캠페인, 캠페인의 퀘스트 목록)의 링크도
// 본문의 크로스링크와 **같은 선택지**를 줘야 한다 — 미리보기 · 새 창 · 이동.
// 같은 문서를 가리키는데 한쪽만 이동뿐인 것이 말이 안 된다.

import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import DocLinkHarness from './DocLinkHarness.svelte';

vi.mock('$lib/api/quests', () => ({
	questsApi: { getBySlug: vi.fn(async () => ({ description: '본문' })) }
}));
vi.mock('$lib/api/campaigns', () => ({ campaignsApi: { get: vi.fn(async () => ({})) } }));
vi.mock('$lib/api/rules', () => ({ rulesApi: { get: vi.fn(async () => ({})) } }));
vi.mock('$lib/api/library', () => ({ libraryApi: { get: vi.fn(async () => ({})) } }));

const gone: string[] = [];
vi.mock('$lib/utils/open-item', () => ({
	openInPage: (href: string) => gone.push(href),
	openInWindow: vi.fn()
}));

describe('DEV-417 관계 목록의 링크', () => {
	it('검색 팔레트와 같은 버튼 셋 — 미리보기 · 새 창 · 이동', async () => {
		const { container } = render(DocLinkHarness, {
			kind: 'quest',
			id: 'DEV-001',
			title: '첫 일',
			href: '/quests/DEV-001'
		});
		const labels = Array.from(container.querySelectorAll('button')).map(
			(b) => b.getAttribute('aria-label') ?? ''
		);
		expect(labels).toEqual(['미리보기', '새 창으로 열기', '페이지로 이동']);
	});

	it('버튼이 아닌 자리는 예전처럼 링크 — 주소가 그대로 있다', async () => {
		const { container } = render(DocLinkHarness, {
			kind: 'quest',
			id: 'DEV-001',
			title: '첫 일',
			href: '/quests/DEV-001'
		});
		const a = container.querySelector('a') as HTMLAnchorElement;
		expect(a.getAttribute('href')).toBe('/quests/DEV-001');
	});

	it('[페이지로 이동] 은 그 주소로 간다', async () => {
		gone.length = 0;
		const { container } = render(DocLinkHarness, {
			kind: 'quest',
			id: 'DEV-002',
			title: '두 번째',
			href: '/quests/DEV-002'
		});
		const go = Array.from(container.querySelectorAll('button')).find(
			(b) => b.getAttribute('aria-label') === '페이지로 이동'
		) as HTMLButtonElement;
		go.click();
		await tick();
		expect(gone).toEqual(['/quests/DEV-002']);
	});

	it('[미리보기] 를 눌러야 팝업이 뜬다 — 호버만으로는 안 뜬다', async () => {
		const { container } = render(DocLinkHarness, {
			kind: 'quest',
			id: 'DEV-003',
			title: '셋째',
			href: '/quests/DEV-003'
		});
		const a = container.querySelector('a') as HTMLAnchorElement;
		a.dispatchEvent(new MouseEvent('mouseenter'));
		await new Promise((r) => setTimeout(r, 400));
		expect(document.querySelector('.lp')).toBeNull();

		const eye = Array.from(container.querySelectorAll('button')).find(
			(b) => b.getAttribute('aria-label') === '미리보기'
		) as HTMLButtonElement;
		eye.click();
		await new Promise((r) => setTimeout(r, 1500));
		await tick();
		await tick();
		expect(document.querySelector('.lp'), '미리보기 팝업이 안 떴다').not.toBeNull();
	});
});
