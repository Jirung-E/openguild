/**
 * Recent guild API — Tauri only.
 *
 * `recents.json` 은 사용자 PC 의 user data dir 에 저장. HTTP server 가 그 파일을
 * 모르므로 (server 는 single-guild 모드) HTTP 모드에선 빈 list 반환.
 *
 * Tauri 모드에선 `@tauri-apps/api/core::invoke` 로 backend 호출.
 *
 * backend 구현: `core::recents` + `gui::commands::list_recents/clear_recents`.
 */

import { invoke } from '@tauri-apps/api/core';
import { detectEnvironment } from './transport';

export interface Recent {
	path: string;
	name: string;
	last_opened: string;
}

export const recentsApi = {
	/** 최근 길드 목록 (LRU 순, 최대 10). HTTP 모드면 빈 array. */
	list: async (): Promise<Recent[]> => {
		if (detectEnvironment() !== 'tauri') return [];
		return await invoke<Recent[]>('list_recents');
	},

	/** 전체 비우기. */
	clear: async (): Promise<void> => {
		if (detectEnvironment() !== 'tauri') return;
		await invoke<void>('clear_recents');
	}
};
