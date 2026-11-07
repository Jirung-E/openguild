// DEV-479: 플러그인 확인 창 — 질문을 가져오는 반복. 붙은 곳이 바뀌면 기다리던 요청을 버리고 새 곳에서 바로 다시.
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';

const api = vi.hoisted(() => ({
	wait: vi.fn(),
	answer: vi.fn(async () => ({}))
}));
vi.mock('$lib/api/pluginAsks', () => ({ pluginAsksApi: api }));

import { pluginAsks, startPluginAsks } from './pluginAsks';
import { setRemoteServerUrl } from './remoteServer';

const flush = () => new Promise((r) => setTimeout(r, 0));

describe('startPluginAsks', () => {
	beforeEach(() => {
		api.wait.mockReset();
		pluginAsks.set([]);
		setRemoteServerUrl(null);
	});

	it('원격으로 바꾸면 기다리던 요청을 놓고 곧바로 다시 가져온다', async () => {
		// 첫 요청은 끝나지 않는다(로컬 쪽의 25초 기다림).
		api.wait.mockImplementationOnce(() => new Promise(() => {}));
		const q = { id: 7, plugin: 'asker', text: '만들까요?', options: {} };
		api.wait.mockImplementationOnce(async () => [q]);
		api.wait.mockImplementation(() => new Promise(() => {}));
		const stop = startPluginAsks();
		await flush();
		expect(api.wait).toHaveBeenCalledTimes(1);
		setRemoteServerUrl('http://127.0.0.1:3597');
		await flush();
		await flush();
		// 새 곳에서 곧바로 다시 가져와 질문을 받았다(그 뒤 다음 기다림이 이어진다).
		expect(api.wait.mock.calls.length).toBeGreaterThanOrEqual(2);
		expect(get(pluginAsks)).toEqual([q]);
		stop();
	});
});
