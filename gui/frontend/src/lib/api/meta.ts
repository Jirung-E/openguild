import { api } from './client';
import { detectEnvironment } from './transport';
import { getRemoteServerUrl } from '$lib/stores/remoteServer';
import type { QuestStatus, QuestType } from '../types';

export interface GuildDisplayInfo {
	name: string;
	/** Tauri 에서 원격 서버에 연결된 상태인지 — Nav 가 "원격" 배지를 붙이는 기준. */
	remote: boolean;
}

export const metaApi = {
	getQuestTypes: () => api.get<QuestType[]>('/api/quest-types'),
	getQuestStatuses: () => api.get<QuestStatus[]>('/api/quest-statuses'),

	/**
	 * DEV-113 후속(사용자 보고: "원격 길드 접속시 제목이 표시 안 되거나
	 * (웹) 잘못 표시됨(GUI 가 로컬 placeholder 이름을 보여줌)") — Nav 의
	 * 길드 이름 조회. Tauri-local(원격 미연결)은 `current_guild_name` invoke
	 * (Rust Store 가 진짜 source of truth), 그 외(브라우저 또는 Tauri+원격)는
	 * `GET /api/guild-info` — transport.ts 의 일반 라우팅을 타지 않고 여기서
	 * 직접 분기하는 이유: invoke 는 plain string, HTTP 는 `{name}` 객체로
	 * 반환 타입이 달라 routeToInvoke 테이블에 넣기 애매함.
	 */
	getGuildDisplayInfo: async (): Promise<GuildDisplayInfo> => {
		const isTauri = detectEnvironment() === 'tauri';
		const remoteUrl = isTauri ? getRemoteServerUrl() : null;
		if (isTauri && !remoteUrl) {
			const { invoke } = await import('@tauri-apps/api/core');
			const name = await invoke<string>('current_guild_name');
			return { name, remote: false };
		}
		const info = await api.get<{ name: string }>('/api/guild-info');
		return { name: info.name, remote: isTauri };
	}
};
