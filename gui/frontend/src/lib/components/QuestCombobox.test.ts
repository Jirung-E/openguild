import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import QuestCombobox from './QuestCombobox.svelte';
import type { Quest } from '$lib/types';

function q(id: number, slug: string, title: string): Quest {
	return {
		id,
		quest_id: slug,
		quest_type_id: 1,
		type_prefix: slug.split('-')[0],
		type_color: '#4A90D9',
		number: id,
		title,
		description: null,
		status_id: 1,
		status_name_en: 'Open',
		status_name_ko: '게시됨',
		status_color: '#8B95A1',
		urgency: 3,
		parent_quest_id: null,
		created_at: '',
		updated_at: ''
	};
}

describe('QuestCombobox', () => {
	const sample: Quest[] = [
		q(1, 'DEV-001', 'implement login'),
		q(2, 'DEV-002', 'fix race condition'),
		q(3, 'BUG-001', 'crash on logout')
	];

	it('renders all candidates initially', () => {
		render(QuestCombobox, {
			props: { quests: sample, onselect: () => {}, oncancel: () => {} }
		});
		const options = screen.getAllByTestId('quest-combobox-option');
		expect(options).toHaveLength(3);
	});

	it('filters by quest_id substring', async () => {
		render(QuestCombobox, {
			props: { quests: sample, onselect: () => {}, oncancel: () => {} }
		});
		const input = screen.getByTestId('quest-combobox-input') as HTMLInputElement;
		await fireEvent.input(input, { target: { value: 'BUG' } });

		const options = screen.getAllByTestId('quest-combobox-option');
		expect(options).toHaveLength(1);
		expect(options[0].textContent).toContain('BUG-001');
	});

	it('filters by title substring (case-insensitive)', async () => {
		render(QuestCombobox, {
			props: { quests: sample, onselect: () => {}, oncancel: () => {} }
		});
		const input = screen.getByTestId('quest-combobox-input') as HTMLInputElement;
		await fireEvent.input(input, { target: { value: 'LOGIN' } });

		const options = screen.getAllByTestId('quest-combobox-option');
		expect(options).toHaveLength(1);
		expect(options[0].textContent?.toLowerCase()).toContain('login');
	});

	it('shows empty state when no match', async () => {
		render(QuestCombobox, {
			props: { quests: sample, onselect: () => {}, oncancel: () => {} }
		});
		const input = screen.getByTestId('quest-combobox-input') as HTMLInputElement;
		await fireEvent.input(input, { target: { value: 'zzz' } });

		expect(screen.getByText('결과 없음')).toBeInTheDocument();
		expect(screen.queryAllByTestId('quest-combobox-option')).toHaveLength(0);
	});

	it('calls onselect on option click', async () => {
		const onselect = vi.fn();
		render(QuestCombobox, {
			props: { quests: sample, onselect, oncancel: () => {} }
		});
		const options = screen.getAllByTestId('quest-combobox-option');
		await fireEvent.click(options[1]);
		expect(onselect).toHaveBeenCalledWith(2);
	});

	it('calls oncancel on Escape', async () => {
		const oncancel = vi.fn();
		render(QuestCombobox, {
			props: { quests: sample, onselect: () => {}, oncancel }
		});
		const input = screen.getByTestId('quest-combobox-input');
		await fireEvent.keyDown(input, { key: 'Escape' });
		expect(oncancel).toHaveBeenCalled();
	});

	it('navigates with ArrowDown / ArrowUp and selects with Enter', async () => {
		const onselect = vi.fn();
		render(QuestCombobox, {
			props: { quests: sample, onselect, oncancel: () => {} }
		});
		const input = screen.getByTestId('quest-combobox-input');

		// 처음 highlight = 0 (DEV-001). ↓ 두 번 → idx 2 (BUG-001), Enter → id 3
		await fireEvent.keyDown(input, { key: 'ArrowDown' });
		await fireEvent.keyDown(input, { key: 'ArrowDown' });
		await fireEvent.keyDown(input, { key: 'Enter' });
		expect(onselect).toHaveBeenCalledWith(3);
	});

	it('ArrowUp does not go below 0', async () => {
		const onselect = vi.fn();
		render(QuestCombobox, {
			props: { quests: sample, onselect, oncancel: () => {} }
		});
		const input = screen.getByTestId('quest-combobox-input');
		// ↑ 여러 번 (clamp)
		await fireEvent.keyDown(input, { key: 'ArrowUp' });
		await fireEvent.keyDown(input, { key: 'ArrowUp' });
		await fireEvent.keyDown(input, { key: 'Enter' });
		expect(onselect).toHaveBeenCalledWith(1); // 첫 번째
	});

	it('ArrowDown does not exceed last index', async () => {
		const onselect = vi.fn();
		render(QuestCombobox, {
			props: { quests: sample, onselect, oncancel: () => {} }
		});
		const input = screen.getByTestId('quest-combobox-input');
		// ↓ 너무 많이 — clamp 되어 마지막
		for (let i = 0; i < 10; i++) {
			await fireEvent.keyDown(input, { key: 'ArrowDown' });
		}
		await fireEvent.keyDown(input, { key: 'Enter' });
		expect(onselect).toHaveBeenCalledWith(3);
	});

	it('Enter on empty result does nothing', async () => {
		const onselect = vi.fn();
		render(QuestCombobox, {
			props: { quests: sample, onselect, oncancel: () => {} }
		});
		const input = screen.getByTestId('quest-combobox-input') as HTMLInputElement;
		await fireEvent.input(input, { target: { value: 'zzz' } });
		await fireEvent.keyDown(input, { key: 'Enter' });
		expect(onselect).not.toHaveBeenCalled();
	});
});
