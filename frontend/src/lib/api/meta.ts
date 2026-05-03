import { api } from './client';
import type { QuestStatus, QuestType } from '../types';

export const metaApi = {
	getQuestTypes: () => api.get<QuestType[]>('/api/quest-types'),
	getQuestStatuses: () => api.get<QuestStatus[]>('/api/quest-statuses')
};
