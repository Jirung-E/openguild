// BUG-178: '토론만'(DEV-213) 필터를 켠 상태에서 '전체 접기'(DEV-190) 가 먹지 않던 회귀.
//
// 원인은 렌더 조건에 `!discussionOnly &&` 가 붙어 있어서 필터가 켜져 있으면 접힘
// 상태(collapsedRoots)를 아예 무시한 것. 필터(무엇을 보여줄지)와 접기(사용자가
// 누른 동작)는 다른 축이므로 필터가 접기를 덮어써선 안 된다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import QuestCommentsSection from './QuestCommentsSection.svelte';
import { commentsApi } from '$lib/api/comments';
import type { CommentEntry } from '$lib/api/comments';

vi.mock('$app/stores', () => ({
	page: readable({ url: new URL('http://localhost/quests/DEV-001') })
}));

vi.mock('$lib/api/comments', () => {
	const stub = {
		listComments: vi.fn(),
		addComment: vi.fn(),
		updateComment: vi.fn(),
		deleteComment: vi.fn(),
		toggleReaction: vi.fn(),
		toggleDiscussion: vi.fn(),
		toggleResolved: vi.fn(),
		togglePinned: vi.fn(),
		getMemo: vi.fn().mockResolvedValue({ content: null }),
		setMemo: vi.fn()
	};
	return { commentsApi: stub, campaignCommentsApi: stub };
});

function entry(id: number, over: Partial<CommentEntry> = {}): CommentEntry {
	return {
		id,
		ts: '2026-07-30T10:00:00+09:00',
		author: 'admin',
		body: `본문 ${id}`,
		parent_id: null,
		...over
	};
}

// 토론 root(#1) + 그 답글(#2), 일반 root(#3) + 그 답글(#4).
const entries: CommentEntry[] = [
	entry(1, { discussion: true, resolved: false }),
	entry(2, { parent_id: 1 }),
	entry(3),
	entry(4, { parent_id: 3 })
];

/** 렌더 후 목록이 그려질 때까지 기다린다 (listComments 는 async). */
async function renderSection() {
	const view = render(QuestCommentsSection, { props: { slug: 'DEV-001' } });
	await vi.waitFor(() => {
		if (!view.container.querySelector('.entry-list')) throw new Error('목록 미렌더');
	});
	return view;
}

const replyCount = (c: HTMLElement) => c.querySelectorAll('.entry.reply').length;
const threadCount = (c: HTMLElement) => c.querySelectorAll('.thread').length;

describe('QuestCommentsSection — 토론만 + 전체 접기 (BUG-178)', () => {
	beforeEach(() => {
		localStorage.clear();
		vi.mocked(commentsApi.listComments).mockResolvedValue({ entries });
	});

	it('일반 모드에서 전체 접기를 누르면 답글이 접힌다', async () => {
		const { container } = await renderSection();
		expect(replyCount(container)).toBe(2);

		container.querySelector<HTMLButtonElement>('.collapse-all-btn')!.click();
		await vi.waitFor(() => expect(replyCount(container)).toBe(0));
		expect(threadCount(container)).toBe(0);
	});

	it("'토론만' 을 켠 상태에서도 전체 접기가 답글을 접는다", async () => {
		const { container } = await renderSection();

		// 토론 댓글이 1건 있으므로 필터 버튼이 노출된다 (quest scope 전용).
		const filter = container.querySelector<HTMLButtonElement>('.disc-filter-btn');
		expect(filter).not.toBeNull();
		filter!.click();
		// 토론 root(#1) 만 남고, 그 답글(#2) 은 아직 펼쳐진 상태.
		await vi.waitFor(() => expect(replyCount(container)).toBe(1));

		container.querySelector<HTMLButtonElement>('.collapse-all-btn')!.click();
		await vi.waitFor(() => expect(replyCount(container)).toBe(0));
		expect(threadCount(container)).toBe(0);
	});

	it("'토론만' 을 켜기 전에 접어둔 상태가 필터를 켜도 유지된다", async () => {
		const { container } = await renderSection();

		container.querySelector<HTMLButtonElement>('.collapse-all-btn')!.click();
		await vi.waitFor(() => expect(replyCount(container)).toBe(0));

		// 필터를 켠다고 접힘이 풀려서는 안 된다 (BUG-178 의 반대 방향).
		container.querySelector<HTMLButtonElement>('.disc-filter-btn')!.click();
		await vi.waitFor(() => {
			expect(container.querySelectorAll('.entry-card').length).toBe(1);
		});
		expect(replyCount(container)).toBe(0);
	});
});
