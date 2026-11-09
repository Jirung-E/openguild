// 앱 이벤트 → 알림. 이벤트의 모양은 `gui/src/lib.rs` 의 `notice_payload` 시험과 같다.
// 진짜 앱의 이벤트 전달까지는 여기서 못 본다 — 그 앞뒤의 모양과 받는 쪽을 고정한다.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';

type Handler = (e: { payload: unknown }) => void;
const heard = vi.hoisted(() => ({
	handlers: {} as Record<string, unknown>,
	stops: {} as Record<string, () => void>
}));
vi.mock('@tauri-apps/api/event', () => ({
	listen: async (event: string, handler: unknown) => {
		heard.handlers[event] = handler;
		heard.stops[event] = vi.fn();
		return heard.stops[event];
	}
}));
// 화면이 듣기 전에 앱이 모아 둔 알림.
const early = vi.hoisted(() => ({ list: [] as unknown[], calls: 0 }));
vi.mock('@tauri-apps/api/core', () => ({
	invoke: async (cmd: string) => {
		if (cmd !== 'plugin_notices_ready') throw new Error(cmd);
		early.calls++;
		const out = early.list;
		early.list = [];
		return out;
	}
}));
// DEV-478: 알림이 오면 알림함을 다시 읽는다 — 여기서는 부른 것만 본다.
const inboxApi = vi.hoisted(() => ({
	get: vi.fn(async () => ({ notices: [], unread: 0, unread_problems: 0 })),
	markSeen: vi.fn(async () => ({}))
}));
vi.mock('$lib/api/notices', () => ({ noticesApi: inboxApi }));

import { listenPluginNotices, notifications, type ToastNotif } from './toast';
import { inboxItems, resetInboxForTest } from './inbox';

const emit = (payload: unknown, event = 'plugin-notify') =>
	(heard.handlers[event] as Handler)({ payload });
const toasts = () => get(notifications) as ToastNotif[];

describe('listenPluginNotices', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		notifications.set([]);
	});
	afterEach(() => vi.useRealTimers());

	it('앱이 쏘는 이름을 듣고, 멈출 수 있다', async () => {
		const stop = await listenPluginNotices();
		expect(Object.keys(heard.handlers).sort()).toEqual([
			'app-notice',
			'plugin-notify',
			'plugin-problem'
		]);
		stop();
		expect(heard.stops['plugin-notify']).toHaveBeenCalled();
		expect(heard.stops['plugin-problem']).toHaveBeenCalled();
	});

	it('듣기 전에 난 알림도 듣기 시작하면 띄운다 — 시동 직후의 하루 한 번 알림', async () => {
		early.list = [{ plugin: 'daily', text: '오늘 할 일', options: {} }];
		await listenPluginNotices();
		expect(toasts().map((n) => n.message)).toEqual(['daily: 오늘 할 일']);
		await listenPluginNotices();
		expect(toasts()).toHaveLength(1);
	});

	// DEV-478: 길드 알림은 코어가 파일에 적었다 — 화면은 앱 알림으로 또 남기지 않고 알림함을 다시 읽는다.
	it('플러그인 알림은 알림함을 다시 읽고, 앱 알림으로 겹쳐 남지 않는다', async () => {
		resetInboxForTest();
		inboxApi.get.mockClear();
		await listenPluginNotices();
		emit({ plugin: 'p', text: '글', options: {} });
		expect(inboxApi.get).toHaveBeenCalledTimes(1);
		expect(get(inboxItems)).toHaveLength(0);
	});

	// DEV-381: 실패를 토스트로 띄우면 시끄럽다 — 알림함만 다시 읽어 종이 빨개진다.
	it('플러그인 실패는 토스트 없이 알림함을 다시 읽는다', async () => {
		inboxApi.get.mockClear();
		await listenPluginNotices();
		emit({ text: "플러그인 'p' 실패" }, 'plugin-problem');
		expect(toasts()).toHaveLength(0);
		expect(inboxApi.get).toHaveBeenCalledTimes(1);
	});

	it('옵션이 비면 기본 4초, 갈 곳 없음', async () => {
		await listenPluginNotices();
		emit({ plugin: 'p', text: '글', options: {} });
		expect(toasts()[0]).toMatchObject({ message: 'p: 글', href: undefined });
		vi.advanceTimersByTime(3999);
		expect(toasts()).toHaveLength(1);
		vi.advanceTimersByTime(1);
		expect(toasts()).toHaveLength(0);
	});

	it('표시 시간과 갈 곳을 따른다', async () => {
		await listenPluginNotices();
		emit({ plugin: 'p', text: '글', options: { seconds: 10, open: 'DEV-001', comment: 3 } });
		expect(toasts()[0].href).toBe('/quests/DEV-001?comment=3');
		vi.advanceTimersByTime(9999);
		expect(toasts()).toHaveLength(1);
		vi.advanceTimersByTime(1);
		expect(toasts()).toHaveLength(0);
	});

	it('0 초는 닫을 때까지', async () => {
		await listenPluginNotices();
		emit({ plugin: 'p', text: '글', options: { seconds: 0 } });
		vi.advanceTimersByTime(3_600_000);
		expect(toasts()).toHaveLength(1);
	});
});
