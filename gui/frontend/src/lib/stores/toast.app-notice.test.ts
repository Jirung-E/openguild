// 다른 창의 앱 알림을 알림함에 모은다 — 자식 창(새 창으로 연 문서)엔 알림함이 없어, 거기서 난 알림이 사라졌다.
// 진짜 창 사이 전달은 여기서 못 본다 — 보내는 모양과 받는 쪽을 고정한다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get, writable } from 'svelte/store';

type Handler = (e: { payload: unknown }) => void;
const bus = vi.hoisted(() => ({
	handlers: {} as Record<string, unknown>,
	sent: [] as { event: string; payload: unknown }[]
}));
vi.mock('@tauri-apps/api/event', () => ({
	listen: async (event: string, handler: unknown) => {
		bus.handlers[event] = handler;
		return () => {};
	},
	emit: async (event: string, payload: unknown) => {
		bus.sent.push({ event, payload });
	}
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => [] }));
vi.mock('$lib/api/notices', () => ({
	noticesApi: { get: vi.fn(async () => ({ notices: [], unread: 0, unread_problems: 0 })) }
}));
const win = vi.hoisted(() => ({ label: 'main' as string | null }));
vi.mock('./windowKind', async () => {
	const child = writable(false);
	return {
		readLabel: () => win.label,
		isChildWindow: { subscribe: child.subscribe, set: child.set }
	};
});

import { listenPluginNotices, showToast } from './toast';
import { inboxItems, resetInboxForTest } from './inbox';
import { isChildWindow } from './windowKind';

const deliver = (payload: unknown) => (bus.handlers['app-notice'] as Handler)({ payload });

describe('창 사이 앱 알림', () => {
	beforeEach(() => {
		resetInboxForTest();
		bus.sent = [];
		win.label = 'main';
		(isChildWindow as unknown as { set: (v: boolean) => void }).set(false);
	});

	it('자식 창의 알림을 보내고, 알림함이 있는 창이 받아 둔다', async () => {
		win.label = 'item-DEV-1';
		showToast('저장 실패', 'error', 0, '/quests/DEV-1');
		await vi.waitFor(() => expect(bus.sent).toHaveLength(1));
		expect(bus.sent[0]).toEqual({
			event: 'app-notice',
			payload: { from: 'item-DEV-1', text: '저장 실패', error: true, href: '/quests/DEV-1' }
		});

		resetInboxForTest();
		win.label = 'main';
		await listenPluginNotices();
		deliver(bus.sent[0].payload);
		const items = get(inboxItems);
		expect(items).toHaveLength(1);
		expect(items[0]).toMatchObject({ text: '저장 실패', error: true, href: '/quests/DEV-1' });
	});

	it('자기가 보낸 것 · 자식 창 · 성공 알림은 받거나 보내지 않는다', async () => {
		await listenPluginNotices();
		deliver({ from: 'main', text: '내 것', error: false });
		expect(get(inboxItems)).toHaveLength(0);

		(isChildWindow as unknown as { set: (v: boolean) => void }).set(true);
		deliver({ from: 'main', text: '다른 창', error: false });
		expect(get(inboxItems)).toHaveLength(0);

		showToast('저장됨', 'success');
		await Promise.resolve();
		expect(bus.sent).toHaveLength(0);
	});

	it('앱이 아니면(창 이름이 없으면) 보내지 않는다', async () => {
		win.label = null;
		showToast('웹 알림', 'info');
		await new Promise((r) => setTimeout(r, 0));
		expect(bus.sent).toHaveLength(0);
	});
});
