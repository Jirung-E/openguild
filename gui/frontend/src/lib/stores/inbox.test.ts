// DEV-478: 알림함 — 길드 알림(파일)과 앱 알림(이 창)을 한 목록으로, 읽음 표시까지.
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';

const inboxApi = vi.hoisted(() => ({
	get: vi.fn(),
	markSeen: vi.fn(async () => ({})),
	dismiss: vi.fn(async () => ({}))
}));
vi.mock('$lib/api/notices', () => ({ noticesApi: inboxApi }));

import {
	dismissItem,
	inboxBadge,
	inboxItems,
	markAllRead,
	recordAppNotice,
	refreshInbox,
	resetInboxForTest,
	MAX_APP_NOTICES
} from './inbox';
import { showToast } from './toast';

describe('알림함', () => {
	beforeEach(() => {
		resetInboxForTest();
		inboxApi.get.mockReset();
		inboxApi.markSeen.mockClear();
		inboxApi.dismiss.mockReset();
		inboxApi.dismiss.mockResolvedValue({});
	});

	it('× 는 그 하나만 지운다 — 길드 알림은 파일에서도, 앱 알림은 목록에서만', async () => {
		inboxApi.get.mockResolvedValue({
			notices: [
				{ at: '2026-11-06T10:00:02.000+09:00', kind: 'problem', text: '실패' },
				{
					at: '2026-11-06T10:00:00.000+09:00',
					kind: 'notify',
					plugin: 'p',
					text: 'a',
					open: 'DEV-1'
				}
			],
			unread: 2,
			unread_problems: 1
		});
		await refreshInbox();
		recordAppNotice('저장 실패', true);
		const items = get(inboxItems);
		await dismissItem(items.find((i) => i.source === 'problem')!);
		expect(inboxApi.dismiss).toHaveBeenCalledWith('2026-11-06T10:00:02.000+09:00', '실패');
		await dismissItem(get(inboxItems).find((i) => i.source === 'app')!);
		expect(inboxApi.dismiss).toHaveBeenCalledTimes(1);
		expect(get(inboxItems).map((i) => i.text)).toEqual(['a']);
		expect(get(inboxBadge)).toEqual({ count: 1, error: false });
	});

	it('못 지우면 파일에 있는 그대로 다시 보인다', async () => {
		const one = {
			notices: [{ at: '2026-11-06T10:00:00.000+09:00', kind: 'notify', text: 'a' }],
			unread: 0,
			unread_problems: 0
		};
		inboxApi.get.mockResolvedValue(one);
		await refreshInbox();
		inboxApi.dismiss.mockRejectedValueOnce(new Error('끊김'));
		await dismissItem(get(inboxItems)[0]);
		expect(get(inboxItems)).toHaveLength(1);
	});

	it('길드 알림과 앱 알림을 최근 것부터 합치고, 갈 곳을 만든다', async () => {
		inboxApi.get.mockResolvedValue({
			notices: [
				{ at: '2026-11-06T10:00:02.000+09:00', kind: 'problem', text: '실패' },
				{
					at: '2026-11-06T10:00:00.000+09:00',
					kind: 'notify',
					plugin: 'p',
					text: '새 퀘스트',
					open: 'DEV-1',
					comment: 2
				}
			],
			unread: 1,
			unread_problems: 1
		});
		await refreshInbox();
		recordAppNotice('저장 실패', true);
		const items = get(inboxItems);
		expect(items.map((i) => [i.source, i.text, i.unread])).toEqual([
			['app', '저장 실패', true],
			['problem', '실패', true],
			['plugin', '새 퀘스트', false]
		]);
		expect(items[2].href).toBe('/quests/DEV-1?comment=2');
		expect(get(inboxBadge)).toEqual({ count: 2, error: true });
	});

	it('모두 읽음은 가장 최근 길드 알림의 시각을 적는다', async () => {
		inboxApi.get.mockResolvedValue({
			notices: [{ at: '2026-11-06T10:00:02.000+09:00', kind: 'notify', plugin: 'p', text: 'a' }],
			unread: 1,
			unread_problems: 0
		});
		await refreshInbox();
		recordAppNotice('x', false);
		await markAllRead();
		expect(inboxApi.markSeen).toHaveBeenCalledWith('2026-11-06T10:00:02.000+09:00');
		expect(get(inboxBadge)).toEqual({ count: 0, error: false });
	});

	it('읽을 것이 없으면 파일을 건드리지 않는다', async () => {
		await markAllRead();
		expect(inboxApi.markSeen).not.toHaveBeenCalled();
	});

	it('못 읽으면 있던 것을 둔다', async () => {
		inboxApi.get.mockResolvedValueOnce({
			notices: [{ at: '2026-11-06T10:00:00.000+09:00', kind: 'notify', text: 'a' }],
			unread: 0,
			unread_problems: 0
		});
		await refreshInbox();
		inboxApi.get.mockRejectedValueOnce(new Error('끊김'));
		await refreshInbox();
		expect(get(inboxItems)).toHaveLength(1);
	});

	it('토스트는 남기고, 성공 알림은 남기지 않는다', () => {
		showToast('저장됨', 'success');
		showToast('저장 실패', 'error', 4000, '/quests/DEV-1');
		showToast('복사함', 'info');
		expect(get(inboxItems).map((i) => [i.text, i.error, i.href])).toEqual([
			['복사함', false, undefined],
			['저장 실패', true, '/quests/DEV-1']
		]);
	});

	it('잇달아 난 같은 앱 알림은 한 줄로 센다', () => {
		recordAppNotice('저장 실패', true);
		recordAppNotice('저장 실패', true);
		recordAppNotice('다른 것', false);
		recordAppNotice('저장 실패', true);
		expect(get(inboxItems).map((i) => [i.text, i.count])).toEqual([
			['저장 실패', 1],
			['다른 것', 1],
			['저장 실패', 2]
		]);
	});

	it('앱 알림은 상한까지만', () => {
		for (let i = 0; i < MAX_APP_NOTICES + 5; i++) recordAppNotice(String(i), false);
		expect(get(inboxItems)).toHaveLength(MAX_APP_NOTICES);
	});
});

describe('알림함 열고 닫기 · 모두 지우기', () => {
	it('닫을 때 모두 읽음, 모두 지우기는 파일도 비운다', async () => {
		const { openInbox, closeInbox, inboxOpen, clearAllNotices } = await import('./inbox');
		inboxApi.get.mockResolvedValue({
			notices: [{ at: '2026-11-06T10:00:02.000+09:00', kind: 'notify', plugin: 'p', text: 'a' }],
			unread: 1,
			unread_problems: 0
		});
		openInbox();
		expect(get(inboxOpen)).toBe(true);
		await refreshInbox();
		closeInbox();
		expect(get(inboxOpen)).toBe(false);
		await Promise.resolve();
		expect(inboxApi.markSeen).toHaveBeenCalledWith('2026-11-06T10:00:02.000+09:00');
		(inboxApi as unknown as { clear: ReturnType<typeof vi.fn> }).clear = vi.fn(async () => ({}));
		recordAppNotice('x', true);
		await clearAllNotices();
		expect(get(inboxItems)).toEqual([]);
	});
});

describe('길드 알림 줄의 key', () => {
	it('앞의 줄을 지워도 나머지 줄의 key 는 그대로다', async () => {
		const n = (s: number, text: string) => ({
			at: `2026-11-07T10:00:0${s}.000+09:00`,
			kind: 'notify' as const,
			plugin: 'p',
			text
		});
		inboxApi.get.mockResolvedValueOnce({
			notices: [n(3, 'c'), n(2, 'b'), n(1, 'a')],
			unread: 0,
			unread_problems: 0
		});
		await refreshInbox();
		const before = new Map(get(inboxItems).map((i) => [i.text, i.key]));
		inboxApi.get.mockResolvedValueOnce({
			notices: [n(4, 'd'), n(3, 'c'), n(1, 'a')],
			unread: 0,
			unread_problems: 0
		});
		await refreshInbox();
		const after = new Map(get(inboxItems).map((i) => [i.text, i.key]));
		expect(after.get('a')).toBe(before.get('a'));
		expect(after.get('c')).toBe(before.get('c'));
	});
});
