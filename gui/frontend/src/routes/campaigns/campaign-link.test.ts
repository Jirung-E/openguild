// DEV-396: 캠페인에서 **새 퀘스트를 만들어 바로 연결**한다.
//
// 퀘스트 상세는 DEV-278 로 이미 "새로 만들어 연관 맺기" 가 있는데 캠페인 쪽만 고르는 것뿐이라,
// 캠페인을 짜다 떠오른 일은 퀘스트를 따로 만들고 다시 돌아와 연결해야 했다.
//
// 모달이 만드는 것은 퀘스트뿐이고 연결은 그다음이다 — 연결만 실패하면 퀘스트는 이미 있다.
// 그 경우를 조용히 넘기면 "만들었는데 목록에 없다" 가 되므로 함께 본다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';

const linkQuest = vi.fn();
const getCampaign = vi.fn();
vi.mock('$lib/api/campaigns', () => ({
	campaignsApi: {
		get: (slug: string) => getCampaign(slug),
		linkQuest: (slug: string, q: string) => linkQuest(slug, q),
		unlinkQuest: vi.fn(),
		update: vi.fn(),
		listHistory: () => Promise.resolve([])
	}
}));
vi.mock('$lib/api/quests', () => ({
	questsApi: { list: () => Promise.resolve([]), candidates: () => Promise.resolve([]) }
}));
const toasts: string[] = [];
vi.mock('$lib/stores/toast', () => ({
	showToast: (m: string) => toasts.push(m)
}));
// 모달은 자기 몫의 API 를 많이 부른다 — 여기서는 "만들어졌다" 신호만 필요하다.
vi.mock('$lib/components/NewQuestModal.svelte', async () => {
	const { default: Stub } = await import('./NewQuestModalStub.svelte');
	return { default: Stub };
});
vi.mock('$app/stores', () => ({
	page: {
		subscribe: (f: (v: unknown) => void) => (
			f({ url: new URL('http://x/campaigns/C-001'), params: { slug: 'C-001' } }), () => {}
		)
	}
}));
vi.mock('$app/navigation', () => ({ goto: vi.fn() }));

const campaign = () => ({
	id: 1,
	campaign_slug: 'C-001',
	title: '베타',
	status: 'active',
	description: '',
	linked_quests: [],
	checklists: [],
	attachments: [],
	start_date: null,
	end_date: null,
	image_path: null,
	created_at: 't',
	updated_at: 't'
});

async function open() {
	const { default: Page } = await import('./[slug]/+page.svelte');
	render(Page);
	await screen.findByText('베타');
}

describe('DEV-396 캠페인에서 새 퀘스트 만들기', () => {
	beforeEach(() => {
		toasts.length = 0;
		linkQuest.mockReset().mockResolvedValue(undefined);
		getCampaign.mockReset().mockImplementation(() => Promise.resolve(campaign()));
	});

	it('만든 퀘스트를 그 캠페인에 연결한다', async () => {
		await open();
		await fireEvent.click(screen.getByText('+ 새 퀘스트'));
		await fireEvent.click(await screen.findByTestId('stub-create'));
		await waitFor(() => expect(linkQuest).toHaveBeenCalledWith('C-001', 'DEV-009'));
		expect(toasts).toEqual([]);
	});

	it('연결만 실패하면 만들어진 퀘스트 번호와 함께 알린다', async () => {
		linkQuest.mockRejectedValue(new Error('boom'));
		await open();
		await fireEvent.click(screen.getByText('+ 새 퀘스트'));
		await fireEvent.click(await screen.findByTestId('stub-create'));
		await waitFor(() => expect(toasts.length).toBe(1));
		expect(toasts[0]).toContain('DEV-009');
		expect(toasts[0]).toContain('boom');
	});
});
