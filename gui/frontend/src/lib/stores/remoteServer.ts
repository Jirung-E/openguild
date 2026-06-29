// DEV-113 (MVP): 원격 서버 모드. Tauri 데스크탑 GUI 가 로컬 .guild 대신 원격
// openguild-server 의 HTTP API 를 쓰도록 — URL 설정 + transport HTTP 분기까지.
//
// 범위 밖(후속): 인증(JWT, DEV-021), 연결 끊김 자동 재시도/오프라인 fallback UX,
// 원격 admin(reindex/drift/snapshot) 전수 점검. 현재는 신뢰된 네트워크에서만
// 사용 권장 — 인증 없이 누구나 API 를 호출할 수 있다.

import { writable, get } from 'svelte/store';

const KEY = 'openguild.remoteServerUrl';

function loadInitial(): string | null {
	if (typeof localStorage === 'undefined') return null;
	try {
		const raw = localStorage.getItem(KEY);
		return raw && raw.trim() ? raw.trim() : null;
	} catch {
		return null;
	}
}

/** null = 로컬 길드(기본, 기존 동작). 값 있으면 그 URL 의 HTTP API 사용. */
export const remoteServerUrl = writable<string | null>(loadInitial());

remoteServerUrl.subscribe((url) => {
	if (typeof localStorage === 'undefined') return;
	try {
		if (url) localStorage.setItem(KEY, url);
		else localStorage.removeItem(KEY);
	} catch {
		/* 무시 */
	}
});

/** trailing slash 제거 + 빈 문자열은 null(로컬 복귀)로 정규화. */
export function setRemoteServerUrl(url: string | null) {
	const trimmed = url?.trim();
	remoteServerUrl.set(trimmed ? trimmed.replace(/\/+$/, '') : null);
}

/** transport.ts 가 매 호출마다 동기적으로 읽는 현재 값. */
export function getRemoteServerUrl(): string | null {
	return get(remoteServerUrl);
}

/** 설정 UI 의 "연결 확인" — `/health` 가 "ok" 를 반환하는지만 확인(인증 없음). */
export async function pingRemoteServer(url: string): Promise<boolean> {
	const base = url.trim().replace(/\/+$/, '');
	const res = await fetch(`${base}/health`);
	if (!res.ok) return false;
	const text = await res.text();
	return text.trim() === 'ok';
}

// BUG-095(사용자 보고: "gui를 처음 열때 이전 원격 길드의 홈으로 열리는 현상"):
// `remoteServerUrl` 은 localStorage(디스크) 영속이라 앱을 완전히 새로 켜도
// 남아있다. local 길드는 DEV-052 설계상 인자 없이 실행하면 항상 Welcome 으로
// 진입(이전 길드 자동 재오픈 X)인데, board(`/`) 의 bounce guard 가
// "remoteServerUrl 이 설정돼 있으면 무조건 건너뛴다"로 되어 있어 원격만
// 콜드 스타트에도 자동 재진입되는 비대칭이 생겼다.
//
// sessionStorage 는 OS 프로세스(=새 WebView 세션)가 재시작되면 항상 빈
// 상태로 시작(localStorage 와의 핵심 차이) — "이번 세션에 사용자가 Welcome
// 에서 실제로 연결을 클릭했는지"를 구분하는 데 쓴다. 콜드 스타트 시점엔
// 이 플래그가 없으므로 board guard 가 정상적으로 Welcome 으로 bounce.
const SESSION_KEY = 'openguild.remoteSessionActive';

/** Welcome 에서 연결(클릭/입력) 성공 시 호출 — 이번 세션에서 활성화됐음을 표시. */
export function markRemoteSessionActive(): void {
	if (typeof sessionStorage === 'undefined') return;
	try {
		sessionStorage.setItem(SESSION_KEY, '1');
	} catch {
		/* 무시 */
	}
}

/** board 의 bounce guard 가 확인 — 이번 세션에서 정말로 연결을 활성화했는지. */
export function isRemoteSessionActive(): boolean {
	if (typeof sessionStorage === 'undefined') return false;
	try {
		return sessionStorage.getItem(SESSION_KEY) === '1';
	} catch {
		return false;
	}
}
