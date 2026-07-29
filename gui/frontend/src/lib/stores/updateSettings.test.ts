/**
 * DEV-305: 업데이트 자동 확인 on/off.
 *
 * 핵심은 "기본은 켜짐(기존 동작 유지)" 과 "끄면 즉시 반영(앱 재시작 불필요)".
 * 시동/주기 훅은 `isAutoUpdateCheckEnabled()` 를 매번 다시 읽으므로
 * localStorage 왕복이 정확해야 한다.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';

const KEY = 'openguild.autoUpdateCheck';

async function freshModule() {
	vi.resetModules();
	return await import('./updateSettings');
}

beforeEach(() => {
	localStorage.clear();
});

describe('DEV-305 자동 업데이트 확인 설정', () => {
	it('기본값은 켜짐 — 기존 동작을 바꾸지 않는다', async () => {
		const m = await freshModule();
		expect(get(m.autoUpdateCheck)).toBe(true);
		expect(m.isAutoUpdateCheckEnabled()).toBe(true);
	});

	it('끄면 스토어와 즉시조회 양쪽에 반영된다', async () => {
		const m = await freshModule();
		m.setAutoUpdateCheck(false);
		expect(get(m.autoUpdateCheck)).toBe(false);
		// 시동/주기 훅이 쓰는 경로 — 스토어 구독 없이 읽어도 꺼져 있어야 한다.
		expect(m.isAutoUpdateCheckEnabled()).toBe(false);
	});

	it('앱을 다시 켜도 유지된다 (localStorage 영속)', async () => {
		const m1 = await freshModule();
		m1.setAutoUpdateCheck(false);
		const m2 = await freshModule(); // 재시동 시뮬레이션
		expect(get(m2.autoUpdateCheck)).toBe(false);
	});

	it('다시 켜면 복구된다', async () => {
		const m = await freshModule();
		m.setAutoUpdateCheck(false);
		m.setAutoUpdateCheck(true);
		expect(m.isAutoUpdateCheckEnabled()).toBe(true);
		expect(localStorage.getItem(KEY)).toBe('true');
	});

	it('저장값이 손상돼 있어도 기본(켜짐)으로 동작한다', async () => {
		localStorage.setItem(KEY, '{{broken');
		const m = await freshModule();
		// 'false' 가 아닌 값은 전부 켜짐으로 — 확인을 조용히 멈추는 쪽이 더 위험.
		expect(get(m.autoUpdateCheck)).toBe(true);
	});
});
