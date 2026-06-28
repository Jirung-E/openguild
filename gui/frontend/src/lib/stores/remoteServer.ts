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
