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

export function filterQuests(
	quests: Quest[],
	typeIds: Set<number>,
	statusIds: Set<number>
): Quest[] {
	return quests.filter((q) => {
		if (typeIds.size > 0 && !typeIds.has(q.quest_type_id)) return false;
		if (statusIds.size > 0 && !statusIds.has(q.status_id)) return false;
		return true;
	});
}
