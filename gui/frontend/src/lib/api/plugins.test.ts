// DEV-380: 조회와 관리의 경계.
//
// 조회는 어디서든 되어야 한다 — "이 공유 서버에 어떤 훅이 걸려 있나" 는 보여야
// 하는 정보고, 정의는 어차피 길드 파일이다.
//
// 관리는 다르다. 서버에는 일부러 HTTP 를 안 열었고(팀 공유 주소로 동의를 받으면
// "누구의 동의인가" 가 흐려지고 `run` 을 여는 원격 구멍이 된다), **데스크톱이라도
// 원격 길드에 접속 중이면 막아야 한다** — `invoke` 는 그때도 로컬 Store 를 보므로
// 원격 길드를 띄워놓고 로컬 길드의 동의를 고치게 된다(BUG-255 와 같은 함정).

import { describe, it, expect, afterEach, vi } from 'vitest';
import { pluginApi, pluginsManageable } from './plugins';
import { setRemoteServerUrl } from '../stores/remoteServer';

const w = globalThis as unknown as Record<string, unknown>;

afterEach(() => {
	delete w.__TAURI_INTERNALS__;
	setRemoteServerUrl(null);
	vi.restoreAllMocks();
});

describe('플러그인 조회/관리의 경계', () => {
	it('브라우저에서는 관리 불가', () => {
		expect(pluginsManageable()).toBe(false);
	});

	it('로컬 데스크톱에서는 관리 가능', () => {
		w.__TAURI_INTERNALS__ = {};
		expect(pluginsManageable()).toBe(true);
	});

	// BUG-255 계열: 데스크톱이지만 화면은 원격 길드다. invoke 는 로컬 Store 를
	// 보므로 여기서 허용하면 **보고 있지 않은 길드**의 동의를 고치게 된다.
	it('데스크톱이라도 원격 길드에 접속 중이면 관리 불가', () => {
		w.__TAURI_INTERNALS__ = {};
		setRemoteServerUrl('http://box:3000');
		expect(pluginsManageable()).toBe(false);
	});

	it('관리가 불가한 상태에서 부르면 조용히 넘어가지 않고 던진다', async () => {
		// 조용히 no-op 이면 "허용 눌렀는데 안 도네" 로 이어진다.
		await expect(pluginApi.allow('x')).rejects.toThrow(/local guild/);
		await expect(pluginApi.revoke('x')).rejects.toThrow(/local guild/);
		await expect(pluginApi.setTrusted(true)).rejects.toThrow(/local guild/);

		w.__TAURI_INTERNALS__ = {};
		setRemoteServerUrl('http://box:3000');
		await expect(pluginApi.allow('x')).rejects.toThrow(/local guild/);
	});

	// 조회는 transport 를 그대로 탄다 — 그래야 화면이 보고 있는 길드와 목록이
	// 같은 길드다. 브라우저에서도 막히면 안 된다.
	it('조회는 브라우저에서도 서버로 나간다', async () => {
		const body = {
			plugins: [],
			errors: [],
			trusted: false,
			manageable: false,
			notes: []
		};
		const fetchMock = vi.fn(async () =>
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
		vi.stubGlobal('fetch', fetchMock);
		await expect(pluginApi.status()).resolves.toEqual(body);
		const urls = fetchMock.mock.calls.map((c: unknown[]) => String(c[0]));
		expect(urls.some((u) => u.includes('/api/plugins'))).toBe(true);
		vi.unstubAllGlobals();
	});
});
