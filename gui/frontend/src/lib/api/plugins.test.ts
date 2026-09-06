// DEV-379: 플러그인 동의는 **이 기계**에 남는다.
//
// 서버에는 일부러 HTTP 를 열지 않았다 — 팀이 공유하는 엔드포인트로 동의를
// 받으면 "누구의 동의인가" 가 흐려지고, 임의 실행(`run`)을 여는 원격 구멍이
// 된다. 그래서 브라우저(http) 경로에서는 아예 못 부르는 것이 계약이다.

import { describe, it, expect, afterEach } from 'vitest';
import { pluginApi, pluginsSupported } from './plugins';

const w = globalThis as unknown as Record<string, unknown>;

afterEach(() => {
	delete w.__TAURI_INTERNALS__;
});

describe('플러그인 관리의 경계', () => {
	it('브라우저(http)에서는 지원하지 않는다', () => {
		expect(pluginsSupported()).toBe(false);
	});

	it('브라우저에서 부르면 조용히 no-op 하지 않고 던진다', async () => {
		// 조용히 아무 일도 안 하면 "허용 눌렀는데 안 도네" 로 이어진다.
		await expect(pluginApi.status()).rejects.toThrow(/desktop-only/);
		await expect(pluginApi.allow('x')).rejects.toThrow(/desktop-only/);
		await expect(pluginApi.revoke('x')).rejects.toThrow(/desktop-only/);
		await expect(pluginApi.trust()).rejects.toThrow(/desktop-only/);
	});

	it('Tauri 환경으로 보이면 지원한다', () => {
		w.__TAURI_INTERNALS__ = {};
		expect(pluginsSupported()).toBe(true);
	});
});
