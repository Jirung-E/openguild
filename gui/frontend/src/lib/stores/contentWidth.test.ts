// DEV-275: 컨텐츠 폭 — 상한 확대 + 최대값 = "화면 전체"(폭 제한 해제).

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';

// clamp/영속화 상태가 module-level 이라 매 테스트 격리.
async function loadFresh() {
	vi.resetModules();
	return await import('./contentWidth');
}

describe('contentWidth store', () => {
	beforeEach(() => {
		localStorage.clear();
	});

	it('상한은 3200 — 울트라와이드/4K 대응 (DEV-275)', async () => {
		const m = await loadFresh();
		expect(m.MAX_CONTENT_WIDTH).toBe(3200);
		expect(m.MIN_CONTENT_WIDTH).toBe(600);
		expect(m.DEFAULT_CONTENT_WIDTH).toBe(1100);
	});

	it('setContentWidth 는 범위를 벗어난 값을 clamp', async () => {
		const m = await loadFresh();
		m.setContentWidth(99999);
		expect(get(m.contentWidth)).toBe(m.MAX_CONTENT_WIDTH);
		m.setContentWidth(1);
		expect(get(m.contentWidth)).toBe(m.MIN_CONTENT_WIDTH);
	});

	it('isFullWidth — 최대값에서만 true', async () => {
		const m = await loadFresh();
		expect(m.isFullWidth(m.MAX_CONTENT_WIDTH)).toBe(true);
		expect(m.isFullWidth(m.MAX_CONTENT_WIDTH - 5)).toBe(false);
		expect(m.isFullWidth(m.DEFAULT_CONTENT_WIDTH)).toBe(false);
	});

	it('contentWidthCss — 최대값은 none(제한 해제), 그 외는 px', async () => {
		const m = await loadFresh();
		expect(m.contentWidthCss(m.MAX_CONTENT_WIDTH)).toBe('none');
		expect(m.contentWidthCss(1100)).toBe('1100px');
		expect(m.contentWidthCss(m.MIN_CONTENT_WIDTH)).toBe('600px');
	});

	it('예전 상한(1800)으로 저장돼 있던 값도 그대로 유효 — 하위호환', async () => {
		localStorage.setItem('openguild.contentWidth', '1800');
		const m = await loadFresh();
		expect(get(m.contentWidth)).toBe(1800);
		// 새 상한 아래이므로 "전체"가 아니라 고정 폭으로 동작.
		expect(m.isFullWidth(1800)).toBe(false);
	});

	it('resetContentWidth 는 기본값으로', async () => {
		const m = await loadFresh();
		m.setContentWidth(m.MAX_CONTENT_WIDTH);
		m.resetContentWidth();
		expect(get(m.contentWidth)).toBe(m.DEFAULT_CONTENT_WIDTH);
	});
});
