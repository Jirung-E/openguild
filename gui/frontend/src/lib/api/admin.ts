import { api } from './client';
import type { DriftReport, RestoreResponse, SnapshotInfo } from '../types';

/**
 * 백업 / drift 등 관리자 작업.
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
	reindex: () => api.post<unknown>('/api/admin/reindex', {})
};
