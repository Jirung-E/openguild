// DEV-468: 자식 창에서 본 문서를 메인 창의 최근 목록으로.
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';

const handlers = new Map<string, (e: { payload: unknown }) => void>();
const emitTo = vi.fn(async (target: string, event: string, payload: unknown) => {
	if (target === 'main') handlers.get(event)?.({ payload });
});
vi.mock('@tauri-apps/api/event', () => ({
	emitTo: (...a: [string, string, unknown]) => emitTo(...a),
	listen: async (event: string, cb: (e: { payload: unknown }) => void) => {
		handlers.set(event, cb);
		return () => handlers.delete(event);
	}
}));

import { forwardRecentDoc, listenRecentDocs, RECENT_DOC_EVENT } from './recent-doc-relay';
import { recentDocs } from '$lib/stores/recentDocs';

describe('DEV-468 최근 본 문서 — 자식 창 → 메인 창', () => {
	beforeEach(() => {
		recentDocs.set([]);
		handlers.clear();
		emitTo.mockClear();
	});
	afterEach(() => {
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		delete (window as any).__TAURI_INTERNALS__;
	});

	it('앱에서는 자식 창이 알린 문서가 메인 창 목록에 정규 주소로 오른다', async () => {
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		(window as any).__TAURI_INTERNALS__ = {};
		const stop = await listenRecentDocs();
		await forwardRecentDoc('/quests/DEV-002?from=board');
		expect(emitTo).toHaveBeenCalledWith('main', RECENT_DOC_EVENT, '/quests/DEV-002?from=board');
		expect(get(recentDocs).map((d) => d.href)).toEqual(['/quests/DEV-002']);

		// 문서가 아닌 화면 · 이상한 값은 올리지 않는다.
		await forwardRecentDoc('/board');
		handlers.get(RECENT_DOC_EVENT)?.({ payload: 42 });
		expect(get(recentDocs)).toHaveLength(1);

		stop();
		await forwardRecentDoc('/quests/DEV-003');
		expect(get(recentDocs)).toHaveLength(1);
	});

	it('웹(브라우저)에서는 아무것도 보내거나 듣지 않는다', async () => {
		await listenRecentDocs();
		await forwardRecentDoc('/quests/DEV-002');
		expect(emitTo).not.toHaveBeenCalled();
		expect(handlers.size).toBe(0);
	});
});
