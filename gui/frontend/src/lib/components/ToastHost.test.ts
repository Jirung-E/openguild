// 갈 곳이 있는 알림은 누르면 그 문서로 가고, 없는 알림은 지금처럼 닫히기만 한다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { tick } from 'svelte';

const goto = vi.fn();
vi.mock('$app/navigation', () => ({ goto: (href: string) => goto(href) }));

import ToastHost from './ToastHost.svelte';
import { notifications, showPluginNotice } from '$lib/stores/toast';

describe('ToastHost — 플러그인 알림 누르기', () => {
	beforeEach(() => {
		goto.mockClear();
		notifications.set([]);
	});

	it('갈 곳이 있으면 그 댓글로 가고 알림은 닫힌다', async () => {
		const { getByRole } = render(ToastHost);
		showPluginNotice('p', '새 댓글', { open: 'DEV-001', comment: 3 });
		await tick();
		await fireEvent.click(getByRole('button', { name: /새 댓글/ }));
		expect(goto).toHaveBeenCalledWith('/quests/DEV-001?comment=3');
		expect(get(notifications)).toEqual([]);
	});

	it('갈 곳이 없으면 닫히기만 한다', async () => {
		const { getByRole } = render(ToastHost);
		showPluginNotice('p', '그냥');
		await tick();
		await fireEvent.click(getByRole('button', { name: /그냥/ }));
		expect(goto).not.toHaveBeenCalled();
		expect(get(notifications)).toEqual([]);
	});
});
