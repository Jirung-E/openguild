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
	/**
	 * BUG-210: `slim` 이면 응답에서 `description` 을 뺀다. 목록·보드·홈·
	 * cross-link 인덱스는 제목과 메타만 쓰는데 본문까지 실려 나와, 퀘스트
	 * 531건 기준 응답이 1.13MB(그중 본문 0.58MB)였다. 필터·검색은 서버가
	 * 하므로 클라이언트가 본문을 들고 있을 이유가 없다.
	 */
	list: (slim = false) => api.get<Quest[]>(`/api/quests${slim ? '?slim=true' : ''}`),

	/**
	 * DEV-277: 최근 갱신순 목록 — 검색 팔레트처럼 "고르는" UI 용.
	 *
	 * `list()` 의 기본 정렬은 `id DESC`(생성 역순)라 방금 만든 퀘스트가 위로
	 * 온다. 무언가를 찾아 고르는 화면에서는 최근에 손댄 것이 위에 있는 편이
	 * 유용해 명시적으로 sort 를 지정한다. CLI `quest list` 의 기본값은 기존
	 * 스크립트 호환을 위해 그대로 두고(필요하면 `--sort updated`), 이 UI 만
	 * 다른 정렬을 요청한다.
	 */
	listRecent: (slim = false) =>
		api.get<Quest[]>(`/api/quests?sort=updated${slim ? '&slim=true' : ''}`),

	get: (id: number) => api.get<QuestDetail>(`/api/quests/${id}`),

	getBySlug: (slug: string) => api.get<QuestDetail>(`/api/quests/by/${slug}`),

	create: (body: CreateQuestRequest) => api.post<Quest>('/api/quests', body),

	update: (id: number, body: UpdateQuestRequest) => api.patch<Quest>(`/api/quests/${id}`, body),

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

	listHistory: (id: number) => api.get<QuestHistoryEntry[]>(`/api/quests/${id}/history`),

	/**
	 * DEV-076: 희망 / 필수 기한 설정 / 해제.
	 *
	 * body 의 키 존재 여부로 변경 의도를 구분:
	 *   { desired_due: "2026-06-15" } → 설정
	 *   { desired_due: null }         → 해제 (NULL 로 UPDATE)
	 *   키 없음                         → 변경 없음 (no-op)
	 *
	 * 두 필드 동시 가능. 유효성 검사 (YYYY-MM-DD) 는 server 가 수행.
	 */
	setDueDates: (id: number, body: { desired_due?: string | null; required_due?: string | null }) =>
		api.patch<Quest>(`/api/quests/${id}/due`, body),

	/**
	 * DEV-068: tag 전체 교체. 정규화 (trim + dedupe + 빈 제거) 는 backend.
	 * 빈 배열 = 전체 삭제.
	 */
	setTags: (id: number, tags: string[]) => api.patch<Quest>(`/api/quests/${id}/tags`, { tags })
};
