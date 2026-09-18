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
	/** 모든 줄의 이벤트 패턴(바뀌기 전 단계는 `pre:` 가 붙는다). */
	on: string[];
	scope: string[];
	/** DEV-403: 줄 목록 — 언제 → 무엇, 적힌 순서대로. */
	handlers: PluginHandler[];
	/** 내보내는 곳 전부 — 무엇에 동의하는지의 핵심. */
	actions: PluginActionView[];
	scripts: string[];
	/** DEV-406: 스크립트가 길드에 시킬 수 있는 일(알림·백업). */
	permissions: string[];
	/**
	 * BUG-279: `run` 훅이 파일을 쓰는 자리. `post` 면 null 이다.
	 *
	 * 훅의 작업 디렉터리는 플러그인 폴더가 **아니다** — 거기 쓰면 동의 지문이
	 * 바뀌어 스스로 꺼진다. 대신 어디에 쌓이는지 사용자가 알아야 한다.
	 */
	data_dir: string | null;
	/** REQ-021: 이 플러그인이 사용자에게 받아야 하는 값들 — 선언 + 지금 상태. */
	inputs: PluginInput[];
	/** 스크립트 원문. 이걸 안 보여주면 동의가 형식만 남는다. */
	script_src: string | null;
	granted: boolean;
	/** 이 컴포넌트에서 도는가. */
	runs_here: boolean;
	/**
	 * DEV-399/400: 어디서 왔나 — null 이면 이 길드의 `.guild/plugins/`, 문자열이면 그 소스.
	 * 한 목록에 섞여 나오므로 화면이 출처를 보여 줘야 한다.
	 */
	source?: string | null;
	/** 플러그인 폴더의 실제 경로 — 소스에서 온 것은 길드 밖이다. */
	dir?: string;
}

/** DEV-400: 소스가 내놓는 플러그인 하나. */
export interface AvailablePlugin {
	/** 정의의 이름 — 사람이 부르는 이름. */
	name: string;
	/** 소스 안의 폴더 이름. 소스 자체가 플러그인이면 `.` */
	folder: string;
	dir: string;
	/** 이 길드에서 쓰고 있나. */
	used: boolean;
}

/** DEV-400: 이 기계에 등록된 소스(플러그인을 가져다 쓸 폴더) 하나. */
export interface PluginSource {
	name: string;
	path: string;
	/** 폴더가 사라졌거나 플러그인이 없으면 그 이유. 목록에서 빼지 않는다. */
	problem: string | null;
	plugins: AvailablePlugin[];
}

/** 폴더를 더한 결과 — 하나면 바로 쓰고, 여럿이면 등록만 한다(고르는 것은 사람). */
export type AddFolderOutcome =
	| { kind: 'used'; source: string; folder: string; name: string }
	| { kind: 'registered'; source: string; plugins: number };

/** DEV-403: 핸들러 한 줄. */
export interface PluginHandler {
	/** `id`, 없으면 "N번째 줄". */
	label: string;
	stage: 'pre' | 'post';
	events: string[];
	/** 부르는 스크립트 함수. */
	call: string | null;
	/** DEV-405: 함수가 받는 연결 데이터(읽는 것). */
	with: string[];
	/** REQ-025: 이 줄이 불릴 조건 — 사람이 읽을 한 줄씩. */
	when: string[];
	/** DEV-407: 이 줄을 기다리나. `pre` 줄은 언제나 기다린다. */
	wait: boolean;
	/** 이름 붙인 동작을 가리키면 그 이름. */
	action: string | null;
	/** 줄에 바로 적은 동작이면 그 종류와 대상. */
	action_kind: string | null;
	action_target: string | null;
}

/** 이름 붙인 동작 하나. */
export interface PluginActionView {
	name: string;
	/** 'post' | 'run' */
	kind: string;
	/** post 의 목적지 또는 run 의 명령(이 기계에서 띄울 것). */
	target: string;
}

export interface PluginInputOption {
	value: string;
	label: string;
}

export interface PluginInput {
	key: string;
	/** 선언에 label 이 없으면 코어가 key 로 채워 보낸다. */
	label: string;
	type: 'text' | 'checkbox' | 'select' | 'number';
	help: string | null;
	secret: boolean;
	options: PluginInputOption[];
	/**
	 * 지금 값. **`secret` 이면 언제나 null 이다** — 코어가 아예 안 싣는다.
	 * 값이 있는지는 `has_value` 로 안다.
	 */
	value: string | number | boolean | null;
	has_value: boolean;
	/** 값이 어디서 왔나 — 덮어쓸지 사용자가 판단하는 근거다. */
	source: 'stored' | 'env' | 'default' | 'missing';
}

export interface PluginStatus {
	plugins: PluginView[];
	errors: [string, string][];
	/** BUG-288: 자동 허용 — 새로 오거나 바뀐 플러그인도 묻지 않고 돈다(직접 철회한 것은 빼고). */
	auto_allow: boolean;
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

async function manage<T = void>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	if (!pluginsManageable()) {
		// 조용히 no-op 하면 "허용 눌렀는데 안 도네" 로 이어진다.
		throw new Error('plugin management requires a local guild on the desktop app');
	}
	const { invoke } = await import('@tauri-apps/api/core');
	return await invoke<T>(cmd, args);
}

export const pluginApi = {
	/** 읽기 — 어디서든. 화면이 보고 있는 길드의 것을 돌려준다. */
	status: () => api.get<PluginStatus>('/api/plugins'),
	allow: (name: string) => manage('plugin_allow', { name }),
	revoke: (name: string) => manage('plugin_revoke', { name }),
	/** BUG-288: 전체 허용·전체 해제는 모드가 아니다 — 지금 있는 것들의 개별 상태를 한 번에 바꾼다. */
	allowAll: () => manage('plugin_allow_all'),
	revokeAll: () => manage('plugin_revoke_all'),
	/** BUG-288: 자동 허용 켜기/끄기. 끄면 지금 돌던 것은 그대로 돈다. */
	setAutoAllow: (on: boolean) => manage('plugin_set_auto_allow', { on }),
	/**
	 * REQ-021: 설정값 저장. `null` 이면 지운다 — 정의의 기본값이나 환경변수로
	 * 되돌아간다("비우기" 와 "빈 문자열을 저장" 은 다른 일이다).
	 *
	 * 허용/철회와 같은 경로를 탄다: 이 값은 이 기계의 것이고, 원격 길드를 보고
	 * 있을 때 저장하면 보고 있지 않은 길드의 값을 고치게 된다.
	 */
	setValue: (name: string, key: string, value: string | number | boolean | null) =>
		manage('plugin_set_value', { name, key, value }),

	// ─── DEV-400: 길드 밖 플러그인(소스) ───
	// 소스·사용 기록은 이 기계의 것이라 동의와 같은 경로(로컬 데스크톱)만 탄다 — 원격 길드를
	// 보면서 더하면 **보고 있지 않은** 로컬 길드에 붙는다.
	sources: () => manage<PluginSource[]>('plugin_sources'),
	/** 폴더를 더한다. 하나짜리는 바로 쓰고, 여럿이면 등록만 한다. */
	addFolder: (path: string) => manage<AddFolderOutcome>('plugin_add_folder', { path }),
	use: (source: string, folder: string) => manage('plugin_use', { source, folder }),
	/** 이 길드에서 안 쓴다 — 파일은 그대로. */
	stopUsing: (source: string, folder: string) =>
		manage('plugin_stop_using', { source, folder }),
	/** 소스 등록 해제 — 그 소스에서 쓰던 것은 모든 길드에서 빠진다. */
	removeSource: (name: string) => manage('plugin_source_remove', { name }),
	/**
	 * DEV-394: 디스크의 정의로 다시 꽂는다. 자동 감지는 없다 — 손보는 중인 정의가 저장되는
	 * 순간 돌면 안 되므로 사람이 누른다. 바뀐 정의는 동의가 풀려 멈춘다.
	 */
	reload: () => manage<PluginStatus>('plugin_reload')
};
