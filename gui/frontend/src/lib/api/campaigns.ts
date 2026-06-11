import { api } from './client';
import type {
	Campaign,
	CampaignChecklistItem,
	CampaignDetail,
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

	get: (slug: string) => api.get<CampaignDetail>(`/api/campaigns/${encodeURIComponent(slug)}`),

	create: (body: CreateCampaignRequest) => api.post<Campaign>('/api/campaigns', body),

	update: (slug: string, body: UpdateCampaignRequest) =>
		api.patch<Campaign>(`/api/campaigns/${encodeURIComponent(slug)}`, body),

	delete: (slug: string) => api.delete(`/api/campaigns/${encodeURIComponent(slug)}`),

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
		api.post<CampaignChecklistItem>(
			`/api/campaigns/${encodeURIComponent(slug)}/checklist`,
			{ text }
		),

	setChecklist: (slug: string, index: number, checked: boolean) =>
		api.patch<void>(
			`/api/campaigns/${encodeURIComponent(slug)}/checklist/${index}`,
			{ checked }
		),

	removeChecklist: (slug: string, index: number) =>
		api.delete(`/api/campaigns/${encodeURIComponent(slug)}/checklist/${index}`),

	// Home 카드용 summaries
	activeSummaries: () =>
		api.get<CampaignSummary[]>('/api/campaigns/summaries/active'),

	upcomingSummaries: (days = 7) =>
		api.get<CampaignSummary[]>(`/api/campaigns/summaries/upcoming?days=${days}`),

	/** Quest 가 속한 캠페인 목록 — Quest Detail 의 Campaigns 섹션. */
	forQuest: (questId: number) =>
		api.get<Campaign[]>(`/api/quests/${questId}/campaigns`),

	// ─── DEV-087: 배너 이미지 — Tauri 전용 (브라우저 모드는 버튼 숨김) ───
	/** source 파일을 .guild/assets/ 로 복사 + 갱신된 campaign 반환. */
	setBanner: async (slug: string, sourcePath: string): Promise<Campaign> => {
		const { invoke } = await import('@tauri-apps/api/core');
		return await invoke<Campaign>('set_campaign_banner', { slug, sourcePath });
	},
	clearBanner: async (slug: string): Promise<Campaign> => {
		const { invoke } = await import('@tauri-apps/api/core');
		return await invoke<Campaign>('clear_campaign_banner', { slug });
	}
};
