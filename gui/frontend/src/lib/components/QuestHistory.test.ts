import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import QuestHistory from './QuestHistory.svelte';
import QuestHistoryHarness from './QuestHistoryHarness.svelte';
import { questsApi } from '$lib/api/quests';
import type { QuestHistoryEntry, QuestStatus } from '$lib/types';

vi.mock('$lib/api/quests', () => ({
	questsApi: {
		listHistory: vi.fn()
	}
}));

const statuses: QuestStatus[] = [
	{ id: 1, slug: 'open', name_en: 'Open', name_ko: '게시됨', color: '#8B95A1', sort_order: 1 },
	{
		id: 2,
		slug: 'in_progress',
		name_en: 'In Progress',
		name_ko: '진행중',
		color: '#F5A623',
		sort_order: 2
	},
	{ id: 3, slug: 'testing', name_en: 'Testing', name_ko: '테스트', color: '#79c0ff', sort_order: 3 }
];

function entry(
	id: number,
	op: string,
	old_value: string | null,
	new_value: string | null
): QuestHistoryEntry {
	return {
		id,
		quest_id: 42,
		ts: '2026-05-20 10:00:00',
		op,
		old_value,
		new_value,
		actor: null
	};
}

describe('QuestHistory', () => {
	const mockListHistory = vi.mocked(questsApi.listHistory);

	// REQ-007: 변경 이력 섹션은 **기본 접힘**이다. 내용을 검증하는 테스트는
	// 먼저 펼쳐야 한다 — 접힌 동안 목록은 DOM 에 아예 없다(`{#if !collapsed}`).
	async function renderExpanded(props: { questId: number; statuses: QuestStatus[] }) {
		const r = render(QuestHistory, { props });
		await fireEvent.click(screen.getByRole('button', { name: /변경 이력/ }));
		return r;
	}

	beforeEach(() => {
		mockListHistory.mockReset();
	});

	afterEach(() => {
		vi.clearAllMocks();
	});

	// BUG-262: 상태를 바꿔도 이 컴포넌트가 **재마운트되지 않는다.**
	//
	// 예전엔 부모가 `{#key ...historyVersion}` 로 통째로 다시 만들었다. 그
	// 방식은 상태 변경 한 번에 파괴·재생성이 따라붙어 느렸고, 같은 key 안에
	// 있던 BacklinkSection 까지 상태와 무관하게 다시 읽었으며, **펼쳐 둔
	// 접힘 상태가 매번 초기화됐다.** 이제 `reloadToken` 이 바뀌면 제자리에서
	// 다시 읽는다.
	describe('BUG-262: reloadToken — 제자리 재조회', () => {
		// **`rerender` 로 검증하면 안 된다.** 그것은 props 를 통째로 무효화해서
		// 값이 안 바뀌어도 이펙트가 다시 돈다 — `reloadToken` 을 의존성에서
		// 빼도 통과한다(실측으로 확인했다). 부모가 `$state` 로 토큰을 들고
		// 바꾸는 래퍼를 거쳐 Svelte 자신의 반응성을 탄다.
		it('토큰이 바뀌면 다시 읽는다', async () => {
			mockListHistory.mockResolvedValue([entry(1, 'change_status', 'open', 'in_progress')]);
			const { component } = render(QuestHistoryHarness, { props: { questId: 42, statuses } });
			await waitFor(() => expect(mockListHistory).toHaveBeenCalledTimes(1));
			mockListHistory.mockResolvedValue([
				entry(1, 'change_status', 'open', 'in_progress'),
				entry(2, 'change_status', 'in_progress', 'testing')
			]);
			(component as unknown as { bumpToken: () => void }).bumpToken();
			await waitFor(() => expect(mockListHistory).toHaveBeenCalledTimes(2));
		});

		// **"토큰이 그대로면 다시 읽지 않는다" 는 여기서 검증할 수 없다.**
		// testing-library 의 `rerender` 는 props 를 통째로 무효화해서, 값이
		// 하나도 안 바뀌어도(같은 배열 참조를 그대로 넘겨도) 이펙트가 다시
		// 돈다 — 실측으로 확인했다. 컴포넌트 동작이 아니라 하니스의 성질이라
		// 단언으로 옮기면 거짓을 고정하게 된다. 불필요한 재조회가 걱정된다면
		// 실제 페이지에서 네트워크를 보는 편이 맞다.

		it('펼쳐 둔 상태가 재조회 뒤에도 유지된다 — 재마운트였다면 도로 접힌다', async () => {
			mockListHistory.mockResolvedValue([entry(1, 'change_status', 'open', 'in_progress')]);
			const { component } = render(QuestHistoryHarness, { props: { questId: 42, statuses } });
			const toggle = screen.getByRole('button', { name: /변경 이력/ });
			await fireEvent.click(toggle);
			await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));
			mockListHistory.mockResolvedValue([
				entry(1, 'change_status', 'open', 'in_progress'),
				entry(2, 'change_status', 'in_progress', 'testing')
			]);
			(component as unknown as { bumpToken: () => void }).bumpToken();
			await waitFor(() => expect(screen.getAllByTestId('qh-item')).toHaveLength(2));
			expect(toggle).toHaveAttribute('aria-expanded', 'true');
		});

		it('questId 가 바뀌면 토큰과 무관하게 다시 읽는다 — 기존 동작', async () => {
			mockListHistory.mockResolvedValue([]);
			const { rerender } = render(QuestHistory, {
				props: { questId: 42, statuses, reloadToken: 0 }
			});
			await waitFor(() => expect(mockListHistory).toHaveBeenCalledTimes(1));
			await rerender({ questId: 43, statuses, reloadToken: 0 });
			await waitFor(() => expect(mockListHistory).toHaveBeenCalledTimes(2));
			expect(mockListHistory).toHaveBeenLastCalledWith(43);
		});
	});

	// REQ-007: 기본 접힘 — 펼치기 전에는 목록도 상태 문구도 렌더되지 않는다.
	it('REQ-007: 기본은 접힘 — 펼쳐야 내용이 보인다', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'change_status', 'open', 'in_progress')]);
		render(QuestHistory, { props: { questId: 42, statuses } });
		const toggle = screen.getByRole('button', { name: /변경 이력/ });
		expect(toggle).toHaveAttribute('aria-expanded', 'false');
		// 접힌 동안에는 항목이 DOM 에 없다.
		expect(screen.queryAllByTestId('qh-item')).toHaveLength(0);
		await fireEvent.click(toggle);
		await waitFor(() => {
			expect(screen.getAllByTestId('qh-item')).toHaveLength(1);
		});
		expect(toggle).toHaveAttribute('aria-expanded', 'true');
	});

	it('빈 이력 → "변경 이력 없음" 표시', async () => {
		mockListHistory.mockResolvedValue([]);
		await renderExpanded({ questId: 42, statuses });
		await waitFor(() => {
			expect(screen.getByText(/변경 이력 없음/)).toBeInTheDocument();
		});
		expect(mockListHistory).toHaveBeenCalledWith(42);
	});

	// DEV-015: status 표시 이름이 언어 반응 — 테스트 기본 locale(ko, localStorage
	// 빈 상태 기본값) 에선 name_ko 로 렌더된다.
	it('change_status (slug) → 언어 반응 이름(기본 ko = name_ko) 으로 렌더', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'change_status', 'open', 'in_progress')]);
		await renderExpanded({ questId: 42, statuses });
		await waitFor(() => {
			expect(screen.getByText('게시됨')).toBeInTheDocument();
			expect(screen.getByText('진행중')).toBeInTheDocument();
		});
		const items = screen.getAllByTestId('qh-item');
		expect(items).toHaveLength(1);
	});

	it('DEV-042 legacy: 숫자 status_id → 이름 + "(legacy)" 부착', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'change_status', '1', '2')]);
		await renderExpanded({ questId: 42, statuses });
		await waitFor(() => {
			expect(screen.getByText('게시됨 (legacy)')).toBeInTheDocument();
			expect(screen.getByText('진행중 (legacy)')).toBeInTheDocument();
		});
	});

	it('알 수 없는 slug → 그대로 표시', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'change_status', 'unknown_slug', 'other_slug')]);
		await renderExpanded({ questId: 42, statuses });
		await waitFor(() => {
			expect(screen.getByText('unknown_slug')).toBeInTheDocument();
			expect(screen.getByText('other_slug')).toBeInTheDocument();
		});
	});

	it('old_value 가 null (최초 생성 시점) → "(없음)" 표시', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'change_status', null, 'open')]);
		await renderExpanded({ questId: 42, statuses });
		await waitFor(() => {
			expect(screen.getByText('(없음)')).toBeInTheDocument();
			expect(screen.getByText('게시됨')).toBeInTheDocument();
		});
	});

	it('API 에러 → 에러 메시지 표시', async () => {
		mockListHistory.mockRejectedValue(new Error('not found'));
		await renderExpanded({ questId: 42, statuses });
		await waitFor(() => {
			expect(screen.getByText('not found')).toBeInTheDocument();
		});
	});

	it('count 뱃지 — 항목 수 표시', async () => {
		mockListHistory.mockResolvedValue([
			entry(1, 'change_status', 'open', 'in_progress'),
			entry(2, 'change_status', 'in_progress', 'testing'),
			entry(3, 'change_status', 'testing', 'open')
		]);
		await renderExpanded({ questId: 42, statuses });
		await waitFor(() => {
			expect(screen.getAllByTestId('qh-item')).toHaveLength(3);
		});
	});

	it('알 수 없는 op → 그대로 표시', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'update_title', 'OLDVAL', 'NEWVAL')]);
		await renderExpanded({ questId: 42, statuses });
		await waitFor(() => {
			expect(screen.getByText('update_title')).toBeInTheDocument();
		});
		const change = screen.getAllByTestId('qh-item')[0].querySelector('.qh-change');
		expect(change?.textContent).toContain('OLDVAL');
		expect(change?.textContent).toContain('NEWVAL');
		expect(change?.textContent).toContain('→');
	});
});
