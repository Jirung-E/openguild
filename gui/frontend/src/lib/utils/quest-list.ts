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
 * 빈 문자열 / 공백만 입력 → 검색 필터 무시.
 */
export function filterQuests(
	quests: Quest[],
	typeIds: Set<number>,
	statusIds: Set<number>,
	search = '',
	titleOnly = false
): Quest[] {
	const tokens = search
		.toLowerCase()
		.split(/\s+/)
		.filter((t) => t.length > 0);

	return quests.filter((q) => {
		if (typeIds.size > 0 && !typeIds.has(q.quest_type_id)) return false;
		if (statusIds.size > 0 && !statusIds.has(q.status_id)) return false;
		if (tokens.length > 0) {
			const title = q.title.toLowerCase();
			const desc = titleOnly ? '' : (q.description ?? '').toLowerCase();
			for (const t of tokens) {
				if (!title.includes(t) && !desc.includes(t)) return false;
			}
		}
		return true;
	});
}
