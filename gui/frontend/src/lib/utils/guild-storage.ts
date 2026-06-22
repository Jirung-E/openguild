// 길드별 localStorage 키 namespace (BUG-019 패턴 공용화).
//
// 길드 데이터에 종속된 상태(타입/상태 ID 필터, 보드 좌표 등)는 길드마다 키를
// 분리해야 한다 — 길드 A 설정이 B 로 누수되면 안 됨. 활성 길드 경로의 FNV-1a
// 32-bit 해시(8 hex)를 prefix 로 사용. Tauri 아님(web)이면 prefix '' (단일 namespace).

import { detectEnvironment } from '$lib/api/transport';

export function fnv1a32(s: string): string {
	let h = 0x811c9dc5;
	for (let i = 0; i < s.length; i++) {
		h ^= s.charCodeAt(i);
		h = (h + ((h << 1) + (h << 4) + (h << 7) + (h << 8) + (h << 24))) >>> 0;
	}
	return h.toString(16).padStart(8, '0');
}

/** 현재 길드 경로 → prefix. 실패/web 이면 ''. (길드 swap 대비 캐시 안 함.) */
export async function resolveGuildKeyPrefix(): Promise<string> {
	try {
		if (detectEnvironment() === 'tauri') {
			const { invoke } = await import('@tauri-apps/api/core');
			const path = await invoke<string>('current_guild_path');
			if (path) return fnv1a32(path);
		}
	} catch {
		/* fallback '' */
	}
	return '';
}

/** prefix + suffix → 최종 localStorage 키. */
export function guildKey(prefix: string, suffix: string): string {
	return prefix ? `openguild.${prefix}.${suffix}` : `openguild.${suffix}`;
}
