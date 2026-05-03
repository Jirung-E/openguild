import { api } from './client';
import type {
	ChangeStatusRequest,
	CreateQuestRequest,
	Quest,
	QuestDetail,
	QuestPosition,
	UpdatePositionRequest,
	UpdateQuestRequest
} from '../types';

export const questsApi = {
	list: () => api.get<Quest[]>('/api/quests'),

	get: (id: number) => api.get<QuestDetail>(`/api/quests/${id}`),

	create: (body: CreateQuestRequest) => api.post<Quest>('/api/quests', body),

	update: (id: number, body: UpdateQuestRequest) =>
		api.patch<Quest>(`/api/quests/${id}`, body),

	delete: (id: number) => api.delete(`/api/quests/${id}`),

	changeStatus: (id: number, body: ChangeStatusRequest) =>
		api.patch<Quest>(`/api/quests/${id}/status`, body),

	addPrerequisite: (id: number, prerequisiteId: number) =>
		api.post<void>(`/api/quests/${id}/prerequisites`, { prerequisite_id: prerequisiteId }),

	removePrerequisite: (id: number, prerequisiteId: number) =>
		api.delete(`/api/quests/${id}/prerequisites/${prerequisiteId}`),

	updatePosition: (id: number, body: UpdatePositionRequest) =>
		api.put<QuestPosition>(`/api/quests/${id}/position`, body)
};
