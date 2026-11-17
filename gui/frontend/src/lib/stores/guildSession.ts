// DEV-207 후속(사용자 보고: "길드를 열었다가 welcome으로 돌아가서 확인했을때"
// 설정 페이지가 여전히 길드가 열려있는 것처럼 표시됨): Rust 의 launch_mode
// 는 로컬 길드가 한 번이라도 열리면 "guild"로 고정되고, Welcome 으로
// 돌아가도 리셋되지 않는다(Nav 로고 → /welcome 이동은 순수 frontend
// 라우팅이라 Rust invoke 자체가 없음) — 원격(remoteServerUrl/
// isRemoteSessionActive)도 마찬가지로 Welcome 방문으로 안 풀린다.
//
// "지금 실제로 어떤 길드가 화면에 표시 중인가"는 Rust 상태가 아니라
// frontend 라우팅 사실(보드가 마지막으로 bounce 없이 마운트됐는가)로
// 판단해야 정확하다. sessionStorage(프로세스 재시작마다 리셋)로 추적 —
// board 의 onMount 가 bounce 안 하면 active 로, welcome 의 onMount 는
// 항상 inactive 로 마크.

import { writable } from 'svelte/store';

const KEY = 'openguild.guildContextActive';

function readFlag(): boolean {
	if (typeof sessionStorage === 'undefined') return false;
	try {
		return sessionStorage.getItem(KEY) === '1';
	} catch {
		return false;
	}
}

/// BUG-136 후속(admin #2): mark* 는 sessionStorage 만 바꿔서 구독자에게
/// 신호가 안 갔음 — Nav 의 길드 이름이 Welcome 복귀 후에도 남는 원인.
/// 반응형 스토어를 함께 갱신해 마운트 순서와 무관하게 구독자가 따라오게.
export const guildContextActive = writable<boolean>(readFlag());

export function markGuildContextActive(): void {
	guildContextActive.set(true);
	if (typeof sessionStorage === 'undefined') return;
	try {
		sessionStorage.setItem(KEY, '1');
	} catch {
		/* 무시 */
	}
}

export function markGuildContextInactive(): void {
	guildContextActive.set(false);
	if (typeof sessionStorage === 'undefined') return;
	try {
		sessionStorage.removeItem(KEY);
	} catch {
		/* 무시 */
	}
}

export function isGuildContextActive(): boolean {
	if (typeof sessionStorage === 'undefined') return false;
	try {
		return sessionStorage.getItem(KEY) === '1';
	} catch {
		return false;
	}
}

/**
 * BUG-399: 히스토리 항목에 남길 "이 화면이 길드 안이었나" 표식.
 *
 * 플래그는 welcome 이 끄고 보드(/) 만 켠다. 그래서 길드 A → 홈 → 길드 B → 뒤로가기로 A 의 보드가 아닌
 * 화면에 돌아오면 길드는 열려 있는데 플래그가 꺼진 채라 타이틀바 ☰ 와 길드 이름이 사라졌다. 항목마다
 * 그때의 상태를 남겨 두고, 뒤로/앞으로 이동으로 그 항목에 닿으면 되살린다.
 * 보드는 마운트가 끝나야 플래그를 켜므로(비동기) 경로로도 판단한다.
 */
export function guildCtxStamp(path: string, active: boolean): boolean {
	return path !== '/welcome' && (active || path === '/');
}

/**
 * BUG-399: 뒤로/앞으로 이동으로 닿은 항목에서 플래그를 켤지.
 * 표식이 없는 옛 항목은 경로로 판단한다 — 설정은 welcome 에서도 갈 수 있어(DEV-207) 켜지 않는다.
 */
export function shouldRestoreGuildCtx(path: string, stamped: boolean | undefined): boolean {
	if (path === '/welcome') return false;
	if (stamped !== undefined) return stamped;
	return path !== '/settings';
}
