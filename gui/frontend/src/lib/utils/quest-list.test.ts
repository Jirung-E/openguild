import { describe, it, expect } from 'vitest';
import { buildTree, flattenTree, filterQuests } from './quest-list';
import type { Quest } from '$lib/types';

// 테스트용 최소 Quest 생성 헬퍼
function q(
	id: number,
	opts: { typeId?: number; statusId?: number; parentId?: number | null } = {}
): Quest {
	return {
		id,
		quest_id: `DEV-00${id}`,
		quest_type_id: opts.typeId ?? 1,
		type_prefix: 'DEV',
		type_color: '#4A90D9',
		number: id,
		title: `Quest ${id}`,
		description: null,
		status_id: opts.statusId ?? 1,
		status_name_en: 'Open',
		status_name_ko: '게시됨',
		status_color: '#8B95A1',
		urgency: 3,
		parent_quest_id: opts.parentId ?? null,
		created_at: '',
		updated_at: ''
	};
}

// --- buildTree ---

describe('buildTree', () => {
	it('returns top-level quests when no parent', () => {
		const quests = [q(1), q(2), q(3)];
		const tree = buildTree(quests, null);
		expect(tree).toHaveLength(3);
		expect(tree.every((n) => n.children.length === 0)).toBe(true);
	});

	it('nests sub-quests under parent', () => {
		const quests = [q(1), q(2, { parentId: 1 }), q(3, { parentId: 1 })];
		const tree = buildTree(quests, null);
		expect(tree).toHaveLength(1);
		expect(tree[0].children).toHaveLength(2);
	});

	it('handles multi-level nesting', () => {
		const quests = [q(1), q(2, { parentId: 1 }), q(3, { parentId: 2 })];
		const tree = buildTree(quests, null);
		expect(tree[0].children[0].children).toHaveLength(1);
	});

	it('returns empty array when no quests', () => {
		expect(buildTree([], null)).toEqual([]);
	});
});

// --- flattenTree ---

describe('flattenTree', () => {
	it('flattens top-level nodes at depth 0', () => {
		const quests = [q(1), q(2)];
		const tree = buildTree(quests, null);
		const flat = flattenTree(tree, new Set());
		expect(flat).toHaveLength(2);
		expect(flat.every((n) => n.depth === 0)).toBe(true);
	});

	it('hides children when parent is collapsed', () => {
		const quests = [q(1), q(2, { parentId: 1 })];
		const tree = buildTree(quests, null);
		const flat = flattenTree(tree, new Set()); // 1번 collapsed
		expect(flat).toHaveLength(1);
	});

	it('shows children when parent is expanded', () => {
		const quests = [q(1), q(2, { parentId: 1 }), q(3, { parentId: 1 })];
		const tree = buildTree(quests, null);
		const flat = flattenTree(tree, new Set([1])); // 1번 expanded
		expect(flat).toHaveLength(3);
		expect(flat[1].depth).toBe(1);
	});

	it('marks hasChildren correctly', () => {
		const quests = [q(1), q(2, { parentId: 1 })];
		const tree = buildTree(quests, null);
		const flat = flattenTree(tree, new Set([1]));
		expect(flat[0].hasChildren).toBe(true);
		expect(flat[1].hasChildren).toBe(false);
	});
});

// --- filterQuests ---

describe('filterQuests', () => {
	const quests = [
		q(1, { typeId: 1, statusId: 1 }),
		q(2, { typeId: 1, statusId: 2 }),
		q(3, { typeId: 2, statusId: 1 }),
		q(4, { typeId: 2, statusId: 3 })
	];

	it('returns all when filters are empty', () => {
		expect(filterQuests(quests, new Set(), new Set())).toHaveLength(4);
	});

	it('filters by single type', () => {
		const result = filterQuests(quests, new Set([1]), new Set());
		expect(result).toHaveLength(2);
		expect(result.every((q) => q.quest_type_id === 1)).toBe(true);
	});

	it('filters by multiple types', () => {
		const result = filterQuests(quests, new Set([1, 2]), new Set());
		expect(result).toHaveLength(4);
	});

	it('filters by single status', () => {
		const result = filterQuests(quests, new Set(), new Set([1]));
		expect(result).toHaveLength(2);
	});

	it('filters by type and status simultaneously', () => {
		const result = filterQuests(quests, new Set([1]), new Set([2]));
		expect(result).toHaveLength(1);
		expect(result[0].id).toBe(2);
	});

	it('returns empty when no match', () => {
		const result = filterQuests(quests, new Set([99]), new Set());
		expect(result).toHaveLength(0);
	});
});

// --- DEV-037: search + title_only ---

describe('filterQuests search', () => {
	function withDesc(id: number, title: string, description: string | null): Quest {
		return { ...q(id), title, description };
	}

	const quests: Quest[] = [
		withDesc(1, 'Tauri invoke handler', 'Rust 측 commands.rs 작성'),
		withDesc(2, 'Frontend transport adapter', 'HTTP / Tauri 자동 분기'),
		withDesc(3, 'Quest list 검색', 'title / description 부분 일치'),
		withDesc(4, '단순 노트', null)
	];

	it('빈 search → 모든 항목', () => {
		expect(filterQuests(quests, new Set(), new Set(), '')).toHaveLength(4);
	});

	it('title 매치', () => {
		const r = filterQuests(quests, new Set(), new Set(), 'Quest');
		expect(r.map((x) => x.id).sort()).toEqual([3]);
	});

	it('description 만 매치 (default = title+desc)', () => {
		const r = filterQuests(quests, new Set(), new Set(), 'commands.rs');
		expect(r).toHaveLength(1);
		expect(r[0].id).toBe(1);
	});

	it('대소문자 무시', () => {
		const a = filterQuests(quests, new Set(), new Set(), 'TAURI');
		const b = filterQuests(quests, new Set(), new Set(), 'tauri');
		expect(a.length).toBe(b.length);
		expect(a.length).toBe(2); // title 1 + description 1
	});

	it('여러 토큰 AND', () => {
		const r = filterQuests(quests, new Set(), new Set(), 'title description');
		expect(r).toHaveLength(1);
		expect(r[0].id).toBe(3);
	});

	it('description 이 null 이어도 안전', () => {
		const r = filterQuests(quests, new Set(), new Set(), '노트');
		expect(r).toHaveLength(1);
		expect(r[0].id).toBe(4);
	});

	it('titleOnly=true → description 제외', () => {
		// "Tauri" 가 title 에 있는 1건만.
		const r = filterQuests(quests, new Set(), new Set(), 'Tauri', true);
		expect(r).toHaveLength(1);
		expect(r[0].id).toBe(1);
	});

	it('titleOnly=true 에서 description-only 키워드 → 0', () => {
		const r = filterQuests(quests, new Set(), new Set(), 'commands.rs', true);
		expect(r).toHaveLength(0);
	});

	it('search + type 필터 조합', () => {
		// type 1 (id 1,2) AND search "Tauri" (title 또는 desc 매치 → 1, 2)
		const r = filterQuests(quests, new Set([1]), new Set(), 'Tauri');
		expect(r.map((x) => x.id).sort()).toEqual([1, 2]);
	});
});
