// 앱 이벤트 → 알림. 이벤트의 모양은 `gui/src/lib.rs` 의 `notice_payload` 시험과 같다.
// 진짜 앱의 이벤트 전달까지는 여기서 못 본다 — 그 앞뒤의 모양과 받는 쪽을 고정한다.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';

type Handler = (e: { payload: unknown }) => void;
const heard = vi.hoisted(() => ({ event: '', handler: undefined as unknown, stop: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({
	listen: async (event: string, handler: unknown) => {
		heard.event = event;
		heard.handler = handler;
		return heard.stop;
	}
}));

import { listenPluginNotices, notifications, type ToastNotif } from './toast';

const emit = (payload: unknown) => (heard.handler as Handler)({ payload });
const toasts = () => get(notifications) as ToastNotif[];

describe('listenPluginNotices', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		notifications.set([]);
	});
	afterEach(() => vi.useRealTimers());

	it('앱이 쏘는 이름을 듣고, 멈출 수 있다', async () => {
		const stop = await listenPluginNotices();
		expect(heard.event).toBe('plugin-notify');
		expect(stop).toBe(heard.stop);
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
