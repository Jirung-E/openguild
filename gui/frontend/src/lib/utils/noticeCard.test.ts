// DEV-478: 알림 카드 — 토스트와 알림함 줄이 같은 카드가 되고, 알림함을 열면 떠 있는 카드는 제자리에 남는다.
import { describe, it, expect } from 'vitest';
import type { InboxItem } from '$lib/stores/inbox';
import type { ToastNotif } from '$lib/stores/toast';
import { cardOfItem, stackCards, targetOf } from './noticeCard';

const item = (over: Partial<InboxItem>): InboxItem => ({
	key: 'k',
	at: '2026-11-07T10:00:00.000+09:00',
	source: 'plugin',
	text: '',
	error: false,
	unread: false,
	count: 1,
	...over
});
const toast = (over: Partial<ToastNotif>): ToastNotif => ({
	id: 'toast-1',
	kind: 'toast',
	message: '',
	variant: 'info',
	count: 1,
	...over
});

describe('알림 카드', () => {
	it('실패 문장을 보낸 플러그인 · 줄 · 이유로 나눈다', () => {
		const c = cardOfItem(
			item({ source: 'problem', text: "플러그인 'hand' 생성 — `backup()` 는 권한에 없습니다" })
		);
		expect([c.tone, c.kind, c.who, c.failedLine, c.body]).toEqual([
			'problem',
			'problem',
			'hand',
			'생성',
			'`backup()` 는 권한에 없습니다'
		]);
		// 줄 이름이 아니라 문장이면 그대로 제목.
		const s = cardOfItem(
			item({ source: 'problem', text: "플러그인 'p' 시간 기록을 못 적었습니다 — 디스크" })
		);
		expect([s.title, s.failedLine]).toEqual(['시간 기록을 못 적었습니다', undefined]);
	});

	it('첫 줄은 제목, 나머지는 본문, 갈 곳은 번호로', () => {
		const c = cardOfItem(
			item({ plugin: 'daily', text: '오늘의 알림\n세 건', href: '/quests/DEV-1?comment=2' })
		);
		expect([c.who, c.title, c.body, c.target]).toEqual(['daily', '오늘의 알림', '세 건', 'DEV-1']);
		expect(targetOf('/library?id=BOOK-2')).toBe('BOOK-2');
		expect(targetOf('/campaigns/C-3')).toBe('C-3');
		expect(targetOf(undefined)).toBeUndefined();
	});

	it('닫혀 있으면 떠 있는 토스트만, 열면 지나간 줄이 시각 순으로 끼고 떠 있는 것은 같은 key', () => {
		const items = [
			item({
				key: 'b',
				at: '2026-11-07T10:00:02.000+09:00',
				plugin: 'p',
				text: '둘',
				unread: true
			}),
			item({ key: 'a', at: '2026-11-07T10:00:01.000+09:00', plugin: 'p', text: '하나' }),
			item({
				key: 'app',
				at: '2026-11-07T10:00:03.000+09:00',
				source: 'app',
				text: '저장 실패',
				error: true
			})
		];
		const shown = [
			toast({
				id: 'toast-9',
				plugin: 'p',
				text: '둘',
				message: 'p: 둘',
				at: '2026-11-07T01:00:02.100Z'
			})
		];
		const closed = stackCards(shown, items, false);
		expect(closed.map((c) => [c.key, c.unread])).toEqual([['toast-9', true]]);
		const open = stackCards(shown, items, true);
		expect(open.map((c) => c.key)).toEqual(['i-a', 'toast-9', 'i-app']);
		expect(open[2].tone).toBe('problem');
	});

	it('앱 토스트는 알림함 key 로 짝짓고, 성공 알림은 짝 없이 그대로 남는다', () => {
		const items = [item({ key: 'app-1', source: 'app', text: '복사 실패', error: true })];
		const shown = [
			toast({ id: 't1', message: '복사 실패', variant: 'error', inboxKey: 'app-1' }),
			toast({ id: 't2', message: '저장됨', variant: 'success', at: '2026-11-07T01:00:05.000Z' })
		];
		const open = stackCards(shown, items, true);
		expect(open.map((c) => c.key)).toEqual(['t1', 't2']);
		expect(open[0].item?.key).toBe('app-1');
		expect(open[1].item).toBeUndefined();
	});
});

describe('알림함을 연 채 토스트가 사라져도', () => {
	it('같은 알림은 같은 key 로 남는다', () => {
		const items = [item({ key: 'a', plugin: 'p', text: '하나' })];
		const seen = new Map<string, string>();
		const t = toast({ id: 'toast-3', plugin: 'p', text: '하나', message: 'p: 하나' });
		expect(stackCards([t], items, true, seen).map((c) => c.key)).toEqual(['toast-3']);
		expect(stackCards([], items, true, seen).map((c) => c.key)).toEqual(['toast-3']);
	});
});

describe('플러그인이 고른 아이콘', () => {
	it('icon: created 면 무엇이 생긴 카드', () => {
		expect(cardOfItem(item({ plugin: 'p', text: '새 퀘스트', icon: 'created' })).tone).toBe(
			'created'
		);
		expect(cardOfItem(item({ plugin: 'p', text: '그냥' })).tone).toBe('notify');
		const t = toast({ plugin: 'p', text: '새 퀘스트', message: 'p: 새 퀘스트', icon: 'created' });
		expect(stackCards([t], [], false)[0].tone).toBe('created');
	});
});

describe('같은 밀리초에 온 알림', () => {
	it('알림함에 적힌 순서(오래된 것 위)로 놓인다', () => {
		const at = '2026-11-07T10:00:00.000+09:00';
		// 알림함 목록은 최근 것이 먼저 — 셋 → 둘 → 하나 순으로 적혔다.
		const items = ['셋', '둘', '하나'].map((text, k) =>
			item({ key: `k${k}`, at, plugin: 'p', text })
		);
		expect(stackCards([], items, true).map((c) => c.title)).toEqual(['하나', '둘', '셋']);
	});
});
