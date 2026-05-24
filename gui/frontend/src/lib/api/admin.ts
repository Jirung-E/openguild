import { api } from './client';
import type {
	DriftReport,
	QuestStatus,
	QuestType,
	RestoreResponse,
	SnapshotInfo
} from '../types';

/** DEV-014: count flatten — UI 에서 "삭제 가능?" 판단. */
export interface QuestTypeWithCount extends QuestType {
	quest_count: number;
}
export interface QuestStatusWithCount extends QuestStatus {
	quest_count: number;
}

/**
 * 백업 / drift / type-status 관리.
 * 인증 없음 — 향후 멀티유저 단계에서 토큰 / role 가드 추가.
 */
export const adminApi = {
	/** 즉시 snapshot 생성. */
	createSnapshot: () => api.post<SnapshotInfo>('/api/admin/snapshot', {}),

	/** 사용 가능 snapshot 목록 (오래된 순). */
	listSnapshots: () => api.get<SnapshotInfo[]>('/api/admin/snapshots'),

	/** snapshot 으로 index.db 복원. to 미지정 시 최신 사용. */
	restore: (to?: string) =>
		api.post<RestoreResponse>('/api/admin/restore', to ? { to } : {}),

	/** 파일 vs index.db drift 검사. */
	checkDrift: () => api.get<DriftReport>('/api/admin/drift'),

	/** 파일 → index.db 재구축. */
	reindex: () => api.post<unknown>('/api/admin/reindex', {}),

	// ─── DEV-014: types ───
	listTypes: () => api.get<QuestTypeWithCount[]>('/api/admin/types'),
	createType: (body: { prefix: string; color: string; description?: string | null }) =>
		api.post<QuestType>('/api/admin/types', body),
	/**
	 * BUG-018: update 가 prefix rename 도 통합.
	 * `new_prefix` 지정 시 그 type 의 모든 quest 의 slug cascade —
	 * 파일명 / frontmatter / DB history.quest_slug / positions.quest_slug 갱신
	 * + 관련 다른 quest 의 auto-block 재생성. 본문 안 자유 텍스트 mention 은
	 * 사용자 책임.
	 */
	updateType: (
		prefix: string,
		body: { new_prefix?: string; color?: string; description?: string | null }
	) => api.patch<QuestType>(`/api/admin/types/${encodeURIComponent(prefix)}`, body),
	deleteType: (prefix: string) =>
		api.delete(`/api/admin/types/${encodeURIComponent(prefix)}`),

	// ─── DEV-014: statuses ───
	listStatuses: () => api.get<QuestStatusWithCount[]>('/api/admin/statuses'),
	createStatus: (body: {
		name_en: string;
		name_ko: string;
		color: string;
		sort_order?: number;
	}) => api.post<QuestStatus>('/api/admin/statuses', body),
	/**
	 * BUG-018: update 가 slug rename 도 통합.
	 * `new_slug` 지정 시 quest_history + 모든 quest .md frontmatter cascade.
	 */
	updateStatus: (
		slug: string,
		body: {
			new_slug?: string;
			name_en?: string;
			name_ko?: string;
			color?: string;
			sort_order?: number;
		}
	) => api.patch<QuestStatus>(`/api/admin/statuses/${encodeURIComponent(slug)}`, body),
	deleteStatus: (slug: string) =>
		api.delete(`/api/admin/statuses/${encodeURIComponent(slug)}`)
};
