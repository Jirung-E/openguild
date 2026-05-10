import { api } from './client';
import type {
	CandidateRelation,
	ChangeParentRequest,
	ChangeStatusRequest,
	CreateQuestRequest,
	Quest,
	QuestDependency,
	QuestDetail,
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

	listDependencies: () => api.get<QuestDependency[]>('/api/quest-dependencies')
};
