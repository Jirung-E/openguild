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
	it('호버하면 미리보기와 두 버튼이 뜨고, 누르면 그 주소로 간다', async () => {
		const { container } = render(DocLinkHarness, {
			kind: 'quest',
			id: 'DEV-001',
			title: '첫 일',
			href: '/quests/DEV-001'
		});
		const a = container.querySelector('a') as HTMLAnchorElement;
		expect(a.getAttribute('href')).toBe('/quests/DEV-001');

		a.dispatchEvent(new MouseEvent('mouseenter'));
		await new Promise((r) => setTimeout(r, 1500));
		await tick();
		await tick();
		const buttons = Array.from(document.querySelectorAll('button')).map((b) => b.textContent ?? '');
		expect(buttons.some((t) => t.includes('새 창'))).toBe(true);
		const go = Array.from(document.querySelectorAll('button')).find((b) =>
			b.textContent?.includes('페이지로 이동')
		) as HTMLButtonElement;
		expect(go).toBeTruthy();

		gone.length = 0;
		go.click();
		await tick();
		expect(gone).toEqual(['/quests/DEV-001']);
	});

	it('지나가기만 하면 안 뜬다 — 머물러야 뜬다', async () => {
		const { container } = render(DocLinkHarness, {
			kind: 'quest',
			id: 'DEV-002',
			title: '두 번째',
			href: '/quests/DEV-002'
		});
		const a = container.querySelector('a') as HTMLAnchorElement;
		a.dispatchEvent(new MouseEvent('mouseenter'));
		await new Promise((r) => setTimeout(r, 100));
		a.dispatchEvent(new MouseEvent('mouseleave'));
		await new Promise((r) => setTimeout(r, 400));
		await tick();
		expect(document.querySelector('.lp')).toBeNull();
	});
});
