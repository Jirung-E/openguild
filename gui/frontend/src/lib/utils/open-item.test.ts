// BUG-352: 새 창으로 연 문서도 "최근 본 문서"에 남는다 — 새 창은 목록을 쌓지 않으므로 여는 창이 기록한다.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';

vi.mock('$lib/api/transport', () => ({ detectEnvironment: () => 'http' }));

import { openInWindow } from './open-item';
import { recentDocs, clearRecentDocs } from '$lib/stores/recentDocs';

beforeEach(() => {
	clearRecentDocs();
	vi.spyOn(window, 'open').mockImplementation(() => null);
});

describe('BUG-352 새 창으로 열기와 최근 문서', () => {
	it('새 창으로 연 문서는 여는 창의 최근 목록에 정규 href 로 올라간다', async () => {
		await openInWindow('/quests/DEV-001?from=board', 'DEV-001');
		expect(window.open).toHaveBeenCalledWith('/quests/DEV-001?from=board', '_blank');
		expect(get(recentDocs).map((d) => d.href)).toEqual(['/quests/DEV-001']);
	});

	it('문서가 아닌 화면은 올리지 않는다', async () => {
		await openInWindow('/settings', '설정');
		expect(get(recentDocs)).toEqual([]);
	});
});
