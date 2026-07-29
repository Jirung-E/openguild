// BUG-176: "지금 열려 있는 길드가 무엇인지" 를 히스토리 항목과 비교하기 위한 식별자.
//
// 길드 전환은 URL 을 바꾸지 않는다 — Rust 의 `Store` 를 통째로 갈아끼우고
// `goto('/')` 할 뿐이다. 그래서 히스토리에 남은 이전 길드의 항목(`/`,
// `/quests/DEV-033` …)으로 뒤로가기를 하면, **URL 은 그때 그대로인데 내용은
// 지금 열린 길드**가 그려진다(사용자 보고: A → Welcome → B 후 뒤로가기가
// B 로 돌아옴).
//
// 이를 감지하려면 "이 히스토리 항목이 어느 길드의 것인지" 를 남겨둬야 한다.
// 이 모듈은 그 비교에 쓸 식별자를 만든다.
//
// 로컬(길드 폴더 경로)과 원격(서버 URL)은 축이 다르므로 kind 로 구분한다.

import { detectEnvironment } from '$lib/api/transport';
import { getRemoteServerUrl, isRemoteSessionActive } from '$lib/stores/remoteServer';

export type GuildId = { kind: 'local'; path: string } | { kind: 'remote'; url: string };

/** 같은 길드인지 — kind 가 다르면 무조건 다른 길드. */
export function sameGuild(a: GuildId | null | undefined, b: GuildId | null | undefined): boolean {
	if (!a || !b) return false;
	if (a.kind !== b.kind) return false;
	return a.kind === 'local' && b.kind === 'local' ? a.path === b.path : a.kind === 'remote' && b.kind === 'remote' ? a.url === b.url : false;
}

/**
 * 지금 열려 있는 길드.
 *
 * - 원격 세션이 활성이면 그 URL (BUG-095/099 와 같은 기준 — `remoteServerUrl`
 *   만 보면 이전 세션의 잔존값과 구분되지 않는다).
 * - 아니면 Rust 가 들고 있는 로컬 길드 경로.
 * - 브라우저 모드는 서버가 길드 하나에 바인딩돼 있어 전환 개념 자체가 없다 →
 *   `null` 을 돌려 호출부가 아무것도 하지 않게 한다.
 *
 * 경로 조회는 invoke 라 매 네비게이션마다 호출하지 않도록 캐시한다. 길드가
 * 바뀌는 시점(웰컴에서 열기)은 `invalidateCurrentGuild()` 로 명시적으로 비운다.
 */
let cached: GuildId | null = null;

export function invalidateCurrentGuild(): void {
	cached = null;
}

export async function currentGuildId(): Promise<GuildId | null> {
	if (detectEnvironment() !== 'tauri') return null;
	const remote = getRemoteServerUrl();
	if (remote && isRemoteSessionActive()) return { kind: 'remote', url: remote };
	if (cached) return cached;
	try {
		const { invoke } = await import('@tauri-apps/api/core');
		const path = await invoke<string>('current_guild_path');
		if (!path) return null;
		cached = { kind: 'local', path };
		return cached;
	} catch {
		// 아직 길드가 없거나(웰컴) 구 backend — 비교를 건너뛴다.
		return null;
	}
}

/**
 * BUG-176 후속: 이 환경에서 "길드 전환" 이 일어날 수 있는가.
 *
 * 데스크탑(Tauri)만 프로세스 안에서 길드를 갈아끼운다. 브라우저 배포는 서버가
 * 길드 하나에 바인딩돼 있어 전환 자체가 없으므로 히스토리 가드도 불필요하다.
 *
 * `currentGuildId()` 와 달리 **동기** — `beforeNavigate` 처럼 즉시 판단해야
 * 하는 곳에서 쓴다(비동기로 판단하면 이미 렌더가 시작돼 깜빡인다).
 */
export function guildSwitchingPossible(): boolean {
	return detectEnvironment() === 'tauri';
}
