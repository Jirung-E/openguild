import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import QuestHistory from './QuestHistory.svelte';
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

	beforeEach(() => {
		mockListHistory.mockReset();
	});

	afterEach(() => {
		vi.clearAllMocks();
	});

	it('빈 이력 → "변경 이력 없음" 표시', async () => {
		mockListHistory.mockResolvedValue([]);
		render(QuestHistory, { props: { questId: 42, statuses } });
		await waitFor(() => {
			expect(screen.getByText(/변경 이력 없음/)).toBeInTheDocument();
		});
		expect(mockListHistory).toHaveBeenCalledWith(42);
	});

	it('change_status (slug) → name_en 으로 렌더', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'change_status', 'open', 'in_progress')]);
		render(QuestHistory, { props: { questId: 42, statuses } });
		await waitFor(() => {
			expect(screen.getByText('Open')).toBeInTheDocument();
			expect(screen.getByText('In Progress')).toBeInTheDocument();
		});
		const items = screen.getAllByTestId('qh-item');
		expect(items).toHaveLength(1);
	});

	it('DEV-042 legacy: 숫자 status_id → name_en + "(legacy)" 부착', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'change_status', '1', '2')]);
		render(QuestHistory, { props: { questId: 42, statuses } });
		await waitFor(() => {
			expect(screen.getByText('Open (legacy)')).toBeInTheDocument();
			expect(screen.getByText('In Progress (legacy)')).toBeInTheDocument();
		});
	});

	it('알 수 없는 slug → 그대로 표시', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'change_status', 'unknown_slug', 'other_slug')]);
		render(QuestHistory, { props: { questId: 42, statuses } });
		await waitFor(() => {
			expect(screen.getByText('unknown_slug')).toBeInTheDocument();
			expect(screen.getByText('other_slug')).toBeInTheDocument();
		});
	});

	it('old_value 가 null (최초 생성 시점) → "(없음)" 표시', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'change_status', null, 'open')]);
		render(QuestHistory, { props: { questId: 42, statuses } });
		await waitFor(() => {
			expect(screen.getByText('(없음)')).toBeInTheDocument();
			expect(screen.getByText('Open')).toBeInTheDocument();
		});
	});

	it('API 에러 → 에러 메시지 표시', async () => {
		mockListHistory.mockRejectedValue(new Error('not found'));
		render(QuestHistory, { props: { questId: 42, statuses } });
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
		render(QuestHistory, { props: { questId: 42, statuses } });
		await waitFor(() => {
			expect(screen.getAllByTestId('qh-item')).toHaveLength(3);
		});
	});

	it('알 수 없는 op → 그대로 표시', async () => {
		mockListHistory.mockResolvedValue([entry(1, 'update_title', 'OLDVAL', 'NEWVAL')]);
		render(QuestHistory, { props: { questId: 42, statuses } });
		await waitFor(() => {
			expect(screen.getByText('update_title')).toBeInTheDocument();
		});
		const change = screen.getAllByTestId('qh-item')[0].querySelector('.qh-change');
		expect(change?.textContent).toContain('OLDVAL');
		expect(change?.textContent).toContain('NEWVAL');
		expect(change?.textContent).toContain('→');
	});
});
