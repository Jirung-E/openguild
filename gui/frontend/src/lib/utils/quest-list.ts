import type { Quest } from '$lib/types';

export interface QuestNode extends Quest {
	children: QuestNode[];
}

export interface FlatNode {
	quest: Quest;
	depth: number;
	hasChildren: boolean;
}

export function buildTree(items: Quest[], parentId: number | null): QuestNode[] {
	return items
		.filter((q) => q.parent_quest_id === parentId)
		.map((q) => ({ ...q, children: buildTree(items, q.id) }));
}

export function flattenTree(nodes: QuestNode[], expanded: Set<number>, depth = 0): FlatNode[] {
	const result: FlatNode[] = [];
	for (const node of nodes) {
		result.push({ quest: node, depth, hasChildren: node.children.length > 0 });
		if (node.children.length > 0 && expanded.has(node.id)) {
			result.push(...flattenTree(node.children, expanded, depth + 1));
		}
	}
	return result;
}

/**
 * DEV-037: 검색 키워드 — 공백 split 후 AND. 각 토큰은 대소문자 무시 부분 일치.
 * `titleOnly=true` → description 제외, title 만 검사.
 *
 * DEV-040: slug (`quest_id`, 예 `DEV-037`) 도 항상 매치 대상.
 * `titleOnly` 와 무관 — slug 는 메타 정보지 본문이 아니므로.
 * "037" 같은 부분 번호도 매치 (zero-padded slug 안에 포함됨).
 *
 * 빈 문자열 / 공백만 입력 → 검색 필터 무시.
 */
/** DEV-033: 고급 필터 — 모두 AND. 'any' = 필터 미적용. */
export type TriState = 'any' | 'has' | 'none';
export interface ExtraFilters {
	/** urgency 다중 선택 (빈 set = 전체). */
	urgencies?: Set<number>;
	/** 선행 quest 보유 여부. `prereqQuestIds` 필요. */
	prereq?: TriState;
	/** 서브 quest 보유 여부. `parentIds` 필요. */
	sub?: TriState;
	/** 생성/갱신 날짜 범위 — `YYYY-MM-DD` (포함). 빈 문자열 = 미적용. */
	createdAfter?: string;
	createdBefore?: string;
	updatedAfter?: string;
	updatedBefore?: string;
	/** 선행 quest 가 1개 이상인 quest id 집합 (dependencies 에서 산출). */
	prereqQuestIds?: Set<number>;
	/** 자식이 1개 이상인 quest id 집합 (parent_quest_id 역산). */
	parentIds?: Set<number>;
}

/** ISO ts (`2026-06-09T...`) 의 날짜 부분과 `YYYY-MM-DD` 경계 비교 (포함). */
function dateInRange(ts: string, after?: string, before?: string): boolean {
	const d = ts.slice(0, 10);
	if (after && d < after) return false;
	if (before && d > before) return false;
	return true;
}

export function filterQuests(
	quests: Quest[],
	typeIds: Set<number>,
	statusIds: Set<number>,
	search = '',
	titleOnly = false,
	tagFilter: Set<string> = new Set(),
	extra: ExtraFilters = {}
): Quest[] {
	const tokens = search
		.toLowerCase()
		.split(/\s+/)
		.filter((t) => t.length > 0);

	return quests.filter((q) => {
		if (typeIds.size > 0 && !typeIds.has(q.quest_type_id)) return false;
		if (statusIds.size > 0 && !statusIds.has(q.status_id)) return false;
		// DEV-033: urgency 다중 선택.
		if (extra.urgencies && extra.urgencies.size > 0 && !extra.urgencies.has(q.urgency))
			return false;
		// DEV-033: prereq / sub tri-state.
		if (extra.prereq && extra.prereq !== 'any') {
			const has = extra.prereqQuestIds?.has(q.id) ?? false;
			if (extra.prereq === 'has' && !has) return false;
			if (extra.prereq === 'none' && has) return false;
		}
		if (extra.sub && extra.sub !== 'any') {
			const has = extra.parentIds?.has(q.id) ?? false;
			if (extra.sub === 'has' && !has) return false;
			if (extra.sub === 'none' && has) return false;
		}
		// DEV-033: 날짜 범위 (date prefix 비교).
		if (!dateInRange(q.created_at ?? '', extra.createdAfter, extra.createdBefore))
			return false;
		if (!dateInRange(q.updated_at ?? '', extra.updatedAfter, extra.updatedBefore))
			return false;
		// DEV-068: tag 필터 — 선택된 tag 모두 가져야 매치 (AND).
		if (tagFilter.size > 0) {
			const qTags = new Set(q.tags ?? []);
			for (const t of tagFilter) {
				if (!qTags.has(t)) return false;
			}
		}
		if (tokens.length > 0) {
			const title = q.title.toLowerCase();
			const desc = titleOnly ? '' : (q.description ?? '').toLowerCase();
			const slug = q.quest_id.toLowerCase();
			// DEV-068: 검색에 tag 도 포함.
			const tagText = (q.tags ?? []).join(' ').toLowerCase();
			for (const t of tokens) {
				if (
					!title.includes(t) &&
					!desc.includes(t) &&
					!slug.includes(t) &&
					!tagText.includes(t)
				) {
					return false;
				}
			}
		}
		return true;
	});
}

/**
 * DEV-040 후속 버그 수정: filterQuests 가 sub-quest 를 매치해도, 그 부모가
 * 매치 결과에 없으면 buildTree(filtered, null) 가 그 sub-quest 로 닿지 못함
 * (parent_quest_id 가 결과에 없는 id 를 가리키므로). 결과: 사용자에게
 * sub-quest 가 안 보임.
 *
 * 해결: 매치된 항목들의 모든 조상을 결과에 포함 — 트리 구조 유지하면서
 * sub-quest 도 노출.
 *
 * 매개변수 `matched` 의 항목 순서는 보존; 조상이 새로 추가되면 `all` 의 원본
 * 순서를 따름 (sort 가 외부에서 이미 적용된 가정).
 */
export function includeAncestors(matched: Quest[], all: Quest[]): Quest[] {
	const matchedIds = new Set(matched.map((q) => q.id));
	const byId = new Map(all.map((q) => [q.id, q]));

	for (const m of matched) {
		let pid: number | null = m.parent_quest_id;
		while (pid != null) {
			if (matchedIds.has(pid)) break; // 이미 포함됨 → 더 위는 처리됐을 것.
			matchedIds.add(pid);
			const p = byId.get(pid);
			if (!p) break;
			pid = p.parent_quest_id;
		}
	}

	return all.filter((q) => matchedIds.has(q.id));
}

/**
 * 검색 시 매치된 항목의 부모들을 자동 펼침. 검색 결과의 sub-quest 가 펼침
 * 상태와 무관하게 보이도록.
 */
export function ancestorIdsOf(matched: Quest[], all: Quest[]): Set<number> {
	const byId = new Map(all.map((q) => [q.id, q]));
	const ancestors = new Set<number>();
	for (const m of matched) {
		let pid: number | null = m.parent_quest_id;
		while (pid != null && !ancestors.has(pid)) {
			ancestors.add(pid);
			const p = byId.get(pid);
			if (!p) break;
			pid = p.parent_quest_id;
		}
	}
	return ancestors;
}

// ── DEV-033: 정렬 ────────────────────────────────────────────

export type SortKey = 'id' | 'urgency' | 'status' | 'updated' | 'created';

/**
 * DEV-033: quest 배열 정렬 — CLI `--sort` 와 1:1 의미.
 *
 * - `id`: 생성 순 (id asc 가 기본 방향).
 * - `urgency`: 높은 순 (1=Critical 이 위) 이 기본 방향.
 * - `status`: statusOrder (slug sort_order) 순. 미지정 status 는 뒤로.
 * - `updated` / `created`: 최신이 위 (desc 가 기본 방향 아님 — 호출자가
 *   desc 토글로 제어. 여기선 ISO 문자열 asc).
 *
 * 같은 키 값일 땐 id asc 로 안정화. `desc=true` 면 전체 방향 반전
 * (CLI `--reverse` 와 동일).
 */
export function sortQuests(
	quests: Quest[],
	key: SortKey,
	desc = false,
	statusOrder?: Map<number, number>
): Quest[] {
	const cmp = (a: Quest, b: Quest): number => {
		let c = 0;
		switch (key) {
			case 'id':
				c = a.id - b.id;
				break;
			case 'urgency':
				c = a.urgency - b.urgency; // 1 (Critical) 이 먼저
				break;
			case 'status': {
				const oa = statusOrder?.get(a.status_id) ?? Number.MAX_SAFE_INTEGER;
				const ob = statusOrder?.get(b.status_id) ?? Number.MAX_SAFE_INTEGER;
				c = oa - ob;
				break;
			}
			case 'updated':
				c = (a.updated_at ?? '').localeCompare(b.updated_at ?? '');
				break;
			case 'created':
				c = (a.created_at ?? '').localeCompare(b.created_at ?? '');
				break;
		}
		if (c === 0) c = a.id - b.id; // tie-break 안정화
		return c;
	};
	const sorted = [...quests].sort(cmp);
	return desc ? sorted.reverse() : sorted;
}
