export interface QuestType {
	id: number;
	prefix: string;
	color: string;
	description: string | null;
}

export interface QuestStatus {
	id: number;
	/** DEV-042: stable identifier (예: "open", "testing"). quest_history 와 .md frontmatter 참조용. */
	slug: string;
	name_en: string;
	name_ko: string;
	color: string;
	sort_order: number;
}

export interface Quest {
	id: number;
	quest_id: string; // e.g. "DEV-001"
	quest_type_id: number;
	type_prefix: string;
	type_color: string;
	number: number;
	title: string;
	description: string | null;
	status_id: number;
	/** DEV-046: stable identifier (예: "open", "testing"). status_id 와 달리 reorder 안전. */
	status_slug: string;
	status_name_en: string;
	status_name_ko: string;
	status_color: string;
	urgency: number;
	parent_quest_id: number | null;
	created_at: string;
	updated_at: string;
	/** DEV-076: 희망 기한 (YYYY-MM-DD). null = 미설정. 정보성 (Home 임박 판단 X). */
	desired_due?: string | null;
	/** DEV-076: 필수 기한 (YYYY-MM-DD). null = 미설정. Home "마감 임박" / "Overdue" 기준. */
	required_due?: string | null;
	/**
	 * BUG-034: SQL 계산 필드. 이 퀘스트가 연결된 active 캠페인 중 가장 가까운
	 * ended_at. 파일에는 저장 X. 클라이언트는 `min(required_due, earliest_campaign_due)`
	 * 를 "유효 기한" 으로 표시.
	 */
	earliest_campaign_due?: string | null;
}

export interface QuestDetail extends Quest {
	/** DEV-047: 부모 quest row (slug + 색 + 제목 표시용). 없으면 null/undefined. */
	parent?: Quest | null;
	sub_quests: Quest[];
	prerequisites: Quest[];
	/**
	 * DEV-070: 본 quest 를 선행으로 가지는 quest 들 (= 후속 / successor).
	 * 빈 배열이면 후속 없음 — Quest Detail 의 "후속 퀘스트" 섹션 conditional 표시.
	 * 서버 미배포 환경 대비 optional (undefined → 빈 배열로 fallback).
	 */
	successors?: Quest[];
	position: QuestPosition | null;
}

export interface QuestPosition {
	quest_id: number;
	/** DEV-049: stable identifier — quests.id 재할당 안전. */
	quest_slug?: string | null;
	x: number;
	y: number;
}

// --- 요청 타입 ---

export interface CreateQuestRequest {
	quest_type_id: number;
	title: string;
	description?: string;
	/** DEV-048: stable identifier (예: "open"). */
	status_slug: string;
	urgency?: number;
	parent_quest_id?: number;
}

export interface UpdateQuestRequest {
	title?: string;
	description?: string;
	urgency?: number;
	parent_quest_id?: number;
}

export interface ChangeStatusRequest {
	/** DEV-048: stable identifier (예: "in_progress"). status_id 의 positional 문제 회피. */
	status_slug: string;
}

export interface ChangeParentRequest {
	parent_quest_id: number | null;
}

/** DEV-055: quest 의 type 변경 — slug 가 바뀜. */
export interface ChangeTypeRequest {
	new_type_prefix: string;
}

export type CandidateRelation = 'parent' | 'sub' | 'prereq';

export interface UpdatePositionRequest {
	x: number;
	y: number;
}

// --- 상수 ---

export const URGENCY_LABEL: Record<number, string> = {
	1: 'Critical',
	2: 'High',
	3: 'Medium',
	4: 'Low'
};

export interface QuestDependency {
	quest_id: number;
	prerequisite_id: number;
}

// --- admin (백업 / drift) ---

export interface SnapshotInfo {
	timestamp: string;
	path: string;
	size_bytes: number;
}

export interface DriftReport {
	fresh_files: string[];
	missing_in_index: string[];
	stale_in_index: string[];
}

export interface RestoreResponse {
	restored_to: string;
}

// DEV-013: Quest 변경 이력 한 행.
export interface QuestHistoryEntry {
	id: number;
	quest_id: number;
	/** DEV-049: stable identifier — quests.id 재할당 안전. */
	quest_slug?: string | null;
	ts: string;
	op: string;
	old_value: string | null;
	new_value: string | null;
	actor: string | null;
}

// ─── DEV-011: Campaign ──────────────────────────────────

export type CampaignStatus = 'active' | 'done';

export interface Campaign {
	id: number;
	campaign_slug: string; // "C-001"
	title: string;
	description: string | null;
	status: CampaignStatus | string;
	started_at: string | null;
	ended_at: string | null;
	display_order: number;
	created_at: string;
	updated_at: string;
}

export interface CampaignChecklistItem {
	id: number;
	campaign_id: number;
	text: string;
	checked: boolean;
	order_idx: number;
}

export interface CampaignLinkedQuest {
	id: number;
	quest_id: string;
	title: string;
	type_prefix: string;
	type_color: string;
	status_slug: string;
	status_name_en: string;
	status_color: string;
}

export interface CampaignDetail extends Campaign {
	checklists: CampaignChecklistItem[];
	linked_quests: CampaignLinkedQuest[];
}

export interface CampaignSummary {
	id: number;
	campaign_slug: string;
	title: string;
	status: string;
	started_at: string | null;
	ended_at: string | null;
	display_order: number;
	created_at: string;
	/** 0.0 ~ 1.0 — 체크리스트 완료율. */
	progress: number;
	checklist_total: number;
	checklist_checked: number;
}

export interface CreateCampaignRequest {
	title: string;
	description?: string | null;
	started_at?: string | null;
	ended_at?: string | null;
}

export interface UpdateCampaignRequest {
	title?: string;
	description?: string;
	status?: CampaignStatus | string;
	started_at?: string;
	ended_at?: string;
	display_order?: number;
}

export const URGENCY_COLOR: Record<number, string> = {
	1: '#E94F4F',
	2: '#F5A623',
	3: '#F5D623',
	4: '#8B95A1'
};

export const URGENCY_BG: Record<number, string> = {
	1: '#2d0f0f',
	2: '#2d1800',
	3: '#2a2100',
	4: '#181c22'
};
