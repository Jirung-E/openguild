/**
 * DEV-379: 플러그인 — **이 기계에서** 남의 코드를 돌릴지 정한다.
 *
 * `.guild/plugins/` 는 git 으로 공유되므로 동료가 만든 정의가 내 기계로 온다.
 * 정의가 왔다는 것과 도는 것은 다른 문제고, 그 사이에 동의가 있다.
 *
 * **데스크톱 전용.** 서버에는 일부러 HTTP 를 열지 않았다 — 팀이 공유하는
 * 엔드포인트로 동의를 받으면 "누구의 동의인가" 가 흐려지고, 임의 실행(`run`)을
 * 열어 주는 원격 구멍이 된다. 서버 쪽 동의는 그 기계에서 CLI 로 한다.
 * 그래서 `api/client` 의 path 추상화를 안 쓰고 invoke 를 직접 부른다.
 */

import { detectEnvironment } from './transport';

export interface PluginView {
	name: string;
	on: string[];
	scope: string[];
	/** 'post' | 'run' */
	action: string;
	/** post 의 목적지 또는 run 의 명령 — 무엇에 동의하는지의 핵심. */
	target: string;
	script: string | null;
	/** 스크립트 원문. 이걸 안 보여주면 동의가 형식만 남는다. */
	script_src: string | null;
	granted: boolean;
	/** 이 컴포넌트(gui)에서 도는가. */
	runs_here: boolean;
}

export interface PluginStatus {
	plugins: PluginView[];
	errors: [string, string][];
	trusted: boolean;
	problems: string[];
}

export function pluginsSupported(): boolean {
	return detectEnvironment() === 'tauri';
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	if (!pluginsSupported()) {
		throw new Error('plugins are desktop-only');
	}
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke<T>(cmd, args);
}

export const pluginApi = {
	status: () => call<PluginStatus>('plugin_status'),
	allow: (name: string) => call<void>('plugin_allow', { name }),
	revoke: (name: string) => call<void>('plugin_revoke', { name }),
	trust: () => call<void>('plugin_trust')
};
