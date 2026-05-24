import { api } from './client';
import type {
	CandidateRelation,
	ChangeParentRequest,
	ChangeStatusRequest,
	ChangeTypeRequest,
	CreateQuestRequest,
	Quest,
	QuestDependency,
	QuestDetail,
	QuestHistoryEntry,
	QuestPosition,
	UpdatePositionRequest,
	UpdateQuestRequest
} from '../types';

export const questsApi = {
	list: () => api.get<Quest[]>('/api/quests'),

	get: (id: number) => api.get<QuestDetail>(`/api/quests/${id}`),

	getBySlug: (slug: string) => api.get<QuestDetail>(`/api/quests/by/${slug}`),

	create: (body: CreateQuestRequest) => api.post<Quest>('/api/quests', body),

	update: (id: number, body: UpdateQuestRequest) =>
		api.patch<Quest>(`/api/quests/${id}`, body),

	/**
	 * 삭제. cascadeIds 가 주어지면 해당 직계 자식들을 함께 삭제, 나머지는 분리(parent_quest_id=null).
	 */
	delete: (id: number, cascadeIds?: number[]) => {
		const qs = cascadeIds && cascadeIds.length > 0 ? `?cascade=${cascadeIds.join(',')}` : '';
		return api.delete(`/api/quests/${id}${qs}`);
	},

	changeStatus: (id: number, body: ChangeStatusRequest) =>
		api.patch<Quest>(`/api/quests/${id}/status`, body),

	/** 부모 변경 / 분리 (parent_quest_id: null로 분리). */
	changeParent: (id: number, body: ChangeParentRequest) =>
		api.patch<Quest>(`/api/quests/${id}/parent`, body),

	/**
	 * DEV-055: type 변경 (slug 가 바뀜).
	 *
	 * cascade: 본인 파일 rename, frontmatter / DB / history.quest_slug /
	 * positions.quest_slug, 관련 quest 파일들의 auto-block 자동 갱신.
	 * 다른 quest 본문 안 자유 텍스트 mention 은 사용자 책임 (false positive
	 * 방지).
	 */
	changeType: (id: number, body: ChangeTypeRequest) =>
		api.patch<Quest>(`/api/quests/${id}/type`, body),

	/** 후보 조회 — 사이클/자기/이미 부모 있는 것 등 자동 제외. */
	candidates: (id: number, relation: CandidateRelation) =>
		api.get<Quest[]>(`/api/quests/${id}/candidates?relation=${relation}`),

	addPrerequisite: (id: number, prerequisiteId: number) =>
		api.post<void>(`/api/quests/${id}/prerequisites`, { prerequisite_id: prerequisiteId }),

	removePrerequisite: (id: number, prerequisiteId: number) =>
		api.delete(`/api/quests/${id}/prerequisites/${prerequisiteId}`),

	updatePosition: (id: number, body: UpdatePositionRequest) =>
		api.put<QuestPosition>(`/api/quests/${id}/position`, body),

	listPositions: () => api.get<QuestPosition[]>('/api/quest-positions'),

	listDependencies: () => api.get<QuestDependency[]>('/api/quest-dependencies'),

	listHistory: (id: number) => api.get<QuestHistoryEntry[]>(`/api/quests/${id}/history`)
};
