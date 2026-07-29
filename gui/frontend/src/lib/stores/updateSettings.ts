// DEV-305: 업데이트 **자동** 확인 on/off.
//
// 지금까지는 시동 시 + 주기적으로 무조건 updater endpoint(latest.json)를
// 확인했다. 오프라인/사내망이거나 버전을 고정해 쓰는 경우 매번 외부 요청이
// 나가는 것을 막을 방법이 없었다.
//
// 끄더라도 **수동 확인은 남긴다** — 설정 화면의 '지금 확인'은 이 설정과
// 무관하게 동작한다(사용자가 명시적으로 누른 것이므로).
//
// 영속화: localStorage (다른 GUI 표시 설정과 동일 규칙).

import { writable } from 'svelte/store';

const KEY = 'openguild.autoUpdateCheck';
/** 기본값 on — 기존 동작 유지(끄는 것은 사용자의 명시적 선택). */
export const DEFAULT_AUTO_UPDATE_CHECK = true;

function loadInitial(): boolean {
	if (typeof localStorage === 'undefined') return DEFAULT_AUTO_UPDATE_CHECK;
	try {
		const raw = localStorage.getItem(KEY);
		if (raw === null) return DEFAULT_AUTO_UPDATE_CHECK;
		return raw !== 'false';
	} catch {
		return DEFAULT_AUTO_UPDATE_CHECK;
	}
}

export const autoUpdateCheck = writable<boolean>(loadInitial());

export function setAutoUpdateCheck(on: boolean): void {
	autoUpdateCheck.set(on);
	try {
		localStorage.setItem(KEY, on ? 'true' : 'false');
	} catch {
		/* 저장 실패는 무시 — 이번 세션에는 반영된다 */
	}
}

/**
 * 자동 확인이 켜져 있는지 — 스토어 구독 없이 즉시 읽어야 하는 곳
 * (시동 훅, setInterval 콜백)에서 사용.
 */
export function isAutoUpdateCheckEnabled(): boolean {
	return loadInitial();
}
