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
}

export interface QuestDetail extends Quest {
	/** DEV-047: 부모 quest row (slug + 색 + 제목 표시용). 없으면 null/undefined. */
	parent?: Quest | null;
	sub_quests: Quest[];
	prerequisites: Quest[];
	position: QuestPosition | null;
}

export interface QuestPosition {
	quest_id: number;
	x: number;
	y: number;
}

// --- 요청 타입 ---

export interface CreateQuestRequest {
	quest_type_id: number;
	title: string;
	description?: string;
	status_id: number;
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
	status_id: number;
}

export interface ChangeParentRequest {
	parent_quest_id: number | null;
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
	ts: string;
	op: string;
	old_value: string | null;
	new_value: string | null;
	actor: string | null;
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
