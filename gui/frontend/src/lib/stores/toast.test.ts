// DEV-266: 알림 스토어 동시 다발 처리 정책 테스트 — 중복 억제 / 표시 상한
// / 우선순위 / persistent 정렬을 스토어(순수 함수) 단위로 검증.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import type { Notif, ToastNotif } from './toast';

// showToast 의 자동 소멸 타이머/seq 가 module-level 이라 매 테스트 격리.
async function loadFreshStore() {
	vi.resetModules();
	return await import('./toast');
}

describe('toast store — DEV-266 동시 다발 알림 정책', () => {
	beforeEach(() => {
		vi.useFakeTimers();
	});

	it('showToast 는 도착순으로 쌓인다', async () => {
		const m = await loadFreshStore();
		m.showToast('a');
		m.showToast('b');
		const list = get(m.notifications);
		expect(list.map((n) => (n as ToastNotif).message)).toEqual(['a', 'b']);
	});

	it('중복 억제 — 같은 message+variant 는 새 카드 대신 count 증가 + 수명 리셋', async () => {
		const m = await loadFreshStore();
		const id1 = m.showToast('dup', 'info', 1000);
		vi.advanceTimersByTime(800); // 소멸 직전
		const id2 = m.showToast('dup', 'info', 1000); // 재발생 — 리셋
		expect(id2).toBe(id1);
		const list = get(m.notifications);
		expect(list).toHaveLength(1);
		expect((list[0] as ToastNotif).count).toBe(2);
		// 리셋됐으므로 원래 소멸 시점(+200ms)엔 살아있고, 새 수명(+1000ms) 후 소멸.
		vi.advanceTimersByTime(300);
		expect(get(m.notifications)).toHaveLength(1);
		vi.advanceTimersByTime(800);
		expect(get(m.notifications)).toHaveLength(0);
	});

	it('variant 가 다르면 별개 카드', async () => {
		const m = await loadFreshStore();
		m.showToast('same', 'info');
		m.showToast('same', 'error');
		expect(get(m.notifications)).toHaveLength(2);
	});

	it('durationMs<=0 은 자동 소멸 없음, dismissNotif 로 수동 제거', async () => {
		const m = await loadFreshStore();
		const id = m.showToast('sticky', 'info', 0);
		vi.advanceTimersByTime(60_000);
		expect(get(m.notifications)).toHaveLength(1);
		m.dismissNotif(id);
		expect(get(m.notifications)).toHaveLength(0);
	});

	it('computeVisible — 상한 이하면 전부 표시, persistent 는 코너쪽(뒤)으로 정렬', async () => {
		const m = await loadFreshStore();
		const list: Notif[] = [
			{ id: 'update', kind: 'update' },
			{ id: 't1', kind: 'toast', message: 'a', variant: 'info', count: 1 },
			{ id: 't2', kind: 'toast', message: 'b', variant: 'info', count: 1 }
		];
		const { visible, hidden } = m.computeVisible(list, false);
		expect(hidden).toBe(0);
		// 배열 = 위→아래(코너): 토스트 도착순 → persistent 맨 뒤.
		expect(visible.map((n) => n.id)).toEqual(['t1', 't2', 'update']);
	});

	it('computeVisible — 상한 초과 시 오래된 non-error 토스트부터 숨기고 hidden 집계', async () => {
		const m = await loadFreshStore();
		const list: Notif[] = [];
		for (let i = 1; i <= 7; i++) {
			list.push({ id: `t${i}`, kind: 'toast', message: `m${i}`, variant: 'info', count: 1 });
		}
		const { visible, hidden } = m.computeVisible(list, false);
		expect(m.MAX_VISIBLE_NOTIFS).toBe(5);
		expect(hidden).toBe(2);
		// 오래된 t1/t2 가 숨고 t3~t7 표시.
		expect(visible.map((n) => n.id)).toEqual(['t3', 't4', 't5', 't6', 't7']);
	});

	it('computeVisible — error 토스트와 persistent 는 상한 초과에도 숨기지 않는다', async () => {
		const m = await loadFreshStore();
		const list: Notif[] = [
			{ id: 'e1', kind: 'toast', message: 'err', variant: 'error', count: 1 },
			{ id: 'schema', kind: 'schema', binaryVersion: '1', aheadVersions: [9], latestKnown: 1 }
		];
		for (let i = 1; i <= 6; i++) {
			list.push({ id: `t${i}`, kind: 'toast', message: `m${i}`, variant: 'info', count: 1 });
		}
		const { visible, hidden } = m.computeVisible(list, false);
		expect(hidden).toBe(3); // 총 8개 − 상한 5 = 3개 숨김(전부 info)
		expect(visible.some((n) => n.id === 'e1')).toBe(true);
		expect(visible.some((n) => n.id === 'schema')).toBe(true);
		// persistent 는 항상 맨 뒤(코너).
		expect(visible[visible.length - 1].id).toBe('schema');
	});

	it('computeVisible — expanded 면 상한 무시하고 전부 표시', async () => {
		const m = await loadFreshStore();
		const list: Notif[] = [];
		for (let i = 1; i <= 9; i++) {
			list.push({ id: `t${i}`, kind: 'toast', message: `m${i}`, variant: 'info', count: 1 });
		}
		const { visible, hidden } = m.computeVisible(list, true);
		expect(hidden).toBe(0);
		expect(visible).toHaveLength(9);
	});

	it('숨김 후보가 전부 error 면 상한을 넘겨도 그대로 표시 (숨김 0)', async () => {
		const m = await loadFreshStore();
		const list: Notif[] = [];
		for (let i = 1; i <= 7; i++) {
			list.push({ id: `e${i}`, kind: 'toast', message: `err${i}`, variant: 'error', count: 1 });
		}
		const { visible, hidden } = m.computeVisible(list, false);
		expect(hidden).toBe(0);
		expect(visible).toHaveLength(7);
	});

	it('upsertNotif — 같은 id 는 제자리 교체(순서 유지)', async () => {
		const m = await loadFreshStore();
		m.upsertNotif({ id: 'update', kind: 'update' });
		m.showToast('after');
		m.upsertNotif({ id: 'update', kind: 'update' }); // 상태 갱신 시뮬레이션
		const list = get(m.notifications);
		expect(list.map((n) => n.id)[0]).toBe('update'); // 여전히 처음 위치
		expect(list).toHaveLength(2);
	});
});
