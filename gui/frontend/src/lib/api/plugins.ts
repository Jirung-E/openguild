/**
 * DEV-379/380: 플러그인 — **이 기계에서** 남의 코드를 돌릴지 정한다.
 *
 * `.guild/plugins/` 는 git 으로 공유되므로 동료가 만든 정의가 내 기계로 온다.
 * 정의가 왔다는 것과 도는 것은 다른 문제고, 그 사이에 동의가 있다.
 *
 * # 조회와 관리는 갈라져 있다
 *
 * **조회**는 어디서든 된다. 기존 transport 를 그대로 타므로 브라우저는 서버에게,
 * 데스크톱은 로컬 Store 에게, 데스크톱+원격은 원격 서버에게 묻는다 — 화면이
 * 보고 있는 길드와 목록이 항상 같은 길드다.
 *
 * **관리**(허용/철회/신뢰)는 로컬 데스크톱에서만 된다. 서버에는 일부러 HTTP 를
 * 열지 않았다 — 팀이 공유하는 주소로 동의를 받으면 "누구의 동의인가" 가
 * 흐려지고 `run` 을 여는 원격 구멍이 된다. 그리고 데스크톱이라도 **원격 길드에
 * 접속 중이면 안 된다**: `invoke` 는 그때도 로컬 Store 를 보므로, 원격 길드를
 * 띄워놓고 로컬 길드의 동의를 고치게 된다(BUG-255 와 같은 함정이라
 * `isLocalTauri()` 로 막는다).
 */

import { api } from './client';
import { isLocalTauri } from './transport';

export interface PluginView {
	name: string;
	/**
	 * REQ-020: 정의(`plugin.json`)가 적어 둔 사람 말 설명. 선택이라 없을 수 있다.
	 *
	 * **플러그인 작성자가 쓴 문장이라 번역하지 않는다.** UI 문구와 달리 이건
	 * 데이터다 — `t()` 를 태울 대상이 아니고, 태울 수도 없다.
	 */
	description: string | null;
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
	/** 이 컴포넌트에서 도는가. */
	runs_here: boolean;
}

export interface PluginStatus {
	plugins: PluginView[];
	errors: [string, string][];
	trusted: boolean;
	/** 이 경로로 허용/철회까지 되나. HTTP 로 받은 답이면 항상 false. */
	manageable: boolean;
	/** 아직 길드를 안 열었나. 문구는 프런트가 자기 언어로 만든다. */
	no_guild: boolean;
	/** 전달 중 쌓인 문제. 비어 있는 것이 정상이다. */
	problems: string[];
}

/** 허용/철회를 할 수 있는 상태인가 — 로컬 길드를 연 데스크톱에서만. */
export function pluginsManageable(): boolean {
	return isLocalTauri();
}

async function manage(cmd: string, args?: Record<string, unknown>): Promise<void> {
	if (!pluginsManageable()) {
		// 조용히 no-op 하면 "허용 눌렀는데 안 도네" 로 이어진다.
		throw new Error('plugin management requires a local guild on the desktop app');
	}
	const { invoke } = await import('@tauri-apps/api/core');
	await invoke<void>(cmd, args);
}

export const pluginApi = {
	/** 읽기 — 어디서든. 화면이 보고 있는 길드의 것을 돌려준다. */
	status: () => api.get<PluginStatus>('/api/plugins'),
	allow: (name: string) => manage('plugin_allow', { name }),
	revoke: (name: string) => manage('plugin_revoke', { name }),
	setTrusted: (on: boolean) => manage('plugin_trust', { on })
};
