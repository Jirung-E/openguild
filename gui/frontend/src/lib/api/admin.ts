import { api } from './client';
import type {
	DriftReport,
	QuestStatus,
	QuestTagDef,
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

/** reindex 중 건너뛴 파일 (파싱 실패 / 정의되지 않은 status 등). */
export interface SkippedFile {
	path: string;
	reason: string;
}
/** reindex 결과 — 적재 카운트 + 건너뛴 파일 상세. */
export interface ReindexResult {
	quests_loaded: number;
	skipped: SkippedFile[];
}

/** DEV-162: VACUUM 결과 — index.db 크기 변화. */
export interface VacuumReport {
	before_bytes: number;
	after_bytes: number;
	saved_bytes: number;
}
/** DEV-162: journal.db(AOF) 한 op. */
export interface JournalOp {
	id: number;
	ts: string;
	op: string;
	args: string;
	result: string | null;
}
/** DEV-162: journal tail — 전체 op 수 + 최근 N(오래된→최신). */
export interface JournalTail {
	total: number;
	rows: JournalOp[];
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
	reindex: () => api.post<ReindexResult>('/api/admin/reindex', {}),

	/** DEV-162: index.db VACUUM (런타임 정비). */
	vacuum: () => api.post<VacuumReport>('/api/admin/vacuum', {}),

	/** DEV-162: journal.db(AOF) 최근 op. */
	journalTail: (count = 50) =>
		api.get<JournalTail>(`/api/admin/journal?count=${encodeURIComponent(count)}`),

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
			/** DEV-093: 캠페인 진행도용 "완료" 카운트 토글. */
			counts_as_done?: boolean;
		}
	) => api.patch<QuestStatus>(`/api/admin/statuses/${encodeURIComponent(slug)}`, body),
	deleteStatus: (slug: string) =>
		api.delete(`/api/admin/statuses/${encodeURIComponent(slug)}`),

	// ─── DEV-068: tag defs (`.guild/tags/{slug}.toml`) ───
	/** 모든 tag 정의 (slug ASC). */
	listTagDefs: () => api.get<QuestTagDef[]>('/api/tag-defs'),
	/** upsert — 같은 slug 면 갱신. color = '#RRGGBB' 또는 빈 문자열. */
	upsertTagDef: (body: { slug: string; color?: string; description?: string }) =>
		api.post<QuestTagDef>('/api/tag-defs', body),
	/** 정의만 삭제 — quest frontmatter 의 tag 사용은 보존 (fallback 색으로 표시). */
	deleteTagDef: (slug: string) =>
		api.delete(`/api/tag-defs/${encodeURIComponent(slug)}`)
};
