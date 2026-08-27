import { api } from './client';
import { isLocalTauri, postWithUploadProgress } from './transport';
import type {
	Campaign,
	CampaignChecklistItem,
	CampaignDetail,
	CampaignHistoryEntry,
	CampaignStatus,
	CampaignSummary,
	CreateCampaignRequest,
	UpdateCampaignRequest
} from '../types';

export const campaignsApi = {
	list: (status?: CampaignStatus | string) => {
		const qs = status ? `?status=${encodeURIComponent(status)}` : '';
		return api.get<Campaign[]>(`/api/campaigns${qs}`);
	},

	/**
	 * 목록 화면용 — 전체 캠페인 summary(진행도 포함, admin 요청).
	 * `list()` 는 진행도가 없는 원본 행이라 목록에서 진행바를 못 그린다.
	 */
	listSummaries: () => api.get<CampaignSummary[]>('/api/campaigns/summaries'),

	get: (slug: string) => api.get<CampaignDetail>(`/api/campaigns/${encodeURIComponent(slug)}`),

	create: (body: CreateCampaignRequest) => api.post<Campaign>('/api/campaigns', body),

	update: (slug: string, body: UpdateCampaignRequest) =>
		api.patch<Campaign>(`/api/campaigns/${encodeURIComponent(slug)}`, body),

	delete: (slug: string) => api.delete(`/api/campaigns/${encodeURIComponent(slug)}`),

	// DEV-226: 변경 이력 — quest listHistory 와 대칭.
	listHistory: (slug: string) =>
		api.get<CampaignHistoryEntry[]>(`/api/campaigns/${encodeURIComponent(slug)}/history`),

	// Quest 연결
	linkQuest: (slug: string, questSlug: string) =>
		api.post<void>(`/api/campaigns/${encodeURIComponent(slug)}/quests`, {
			quest_slug: questSlug
		}),

	unlinkQuest: (slug: string, questSlug: string) =>
		api.delete(
			`/api/campaigns/${encodeURIComponent(slug)}/quests/${encodeURIComponent(questSlug)}`
		),

	// 체크리스트
	addChecklist: (slug: string, text: string) =>
		api.post<CampaignChecklistItem>(`/api/campaigns/${encodeURIComponent(slug)}/checklist`, {
			text
		}),

	setChecklist: (slug: string, index: number, checked: boolean) =>
		api.patch<void>(`/api/campaigns/${encodeURIComponent(slug)}/checklist/${index}`, { checked }),

	removeChecklist: (slug: string, index: number) =>
		api.delete(`/api/campaigns/${encodeURIComponent(slug)}/checklist/${index}`),

	// Home 카드용 summaries
	activeSummaries: () => api.get<CampaignSummary[]>('/api/campaigns/summaries/active'),

	upcomingSummaries: (days = 7) =>
		api.get<CampaignSummary[]>(`/api/campaigns/summaries/upcoming?days=${days}`),

	/** Quest 가 속한 캠페인 목록 — Quest Detail 의 Campaigns 섹션. */
	forQuest: (questId: number) => api.get<Campaign[]>(`/api/quests/${questId}/campaigns`),

	// ─── DEV-087 / BUG-255: 배너 이미지 ───
	//
	// 예전엔 이 둘이 곧장 `invoke` 라 **로컬 파일 경로**가 있어야만 동작했고,
	// 그래서 브라우저·원격에서는 버튼 자체를 숨겼다(보기만 되고 쓰기는 불가).
	// 첨부(BUG-168)와 같은 모양으로 환경별 분기를 넣는다 — 로컬 데스크톱은
	// 경로, 그 외는 bytes 를 서버로 스트리밍.

	/** 로컬 Tauri 전용 — 네이티브 다이얼로그가 준 **경로**로 설정. */
	setBannerFromPath: async (slug: string, sourcePath: string): Promise<Campaign> => {
		const { invoke } = await import('@tauri-apps/api/core');
		return await invoke<Campaign>('set_campaign_banner', { slug, sourcePath });
	},

	/**
	 * 브라우저/원격 — 고른 파일을 그대로 POST.
	 *
	 * body 가 파일 원문이라 확장자는 쿼리로 보낸다(서버가 저장 파일명을 정한다).
	 * 진행률이 필요해질 수 있어 `postWithUploadProgress` 를 쓰되 콜백은 비운다 —
	 * 배너는 보통 작아서 표시할 것이 없다.
	 */
	setBannerFromFile: async (slug: string, file: File): Promise<Campaign> => {
		const ext = (file.name.split('.').pop() ?? '').toLowerCase();
		return await postWithUploadProgress<Campaign>(
			`/api/campaigns/${encodeURIComponent(slug)}/banner?ext=${encodeURIComponent(ext)}`,
			file,
			() => {}
		);
	},

	clearBanner: async (slug: string): Promise<Campaign> => {
		if (isLocalTauri()) {
			const { invoke } = await import('@tauri-apps/api/core');
			return await invoke<Campaign>('clear_campaign_banner', { slug });
		}
		// 제거는 파일 선택이 없는데도 설정 버튼과 같이 묶여 가려져 있었다.
		return await api.delete<Campaign>(`/api/campaigns/${encodeURIComponent(slug)}/banner`);
	}
};
