import { describe, it, expect } from 'vitest';
import {
	ancestorIdsOf,
	buildTree,
	filterQuests,
	flattenTree,
	includeAncestors
} from './quest-list';
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
		status_slug: 'open',
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

// --- DEV-040: slug 검색 ---

describe('filterQuests slug search', () => {
	function withSlug(id: number, slug: string, title: string): Quest {
		const [prefix, numStr] = slug.split('-');
		return {
			...q(id),
			quest_id: slug,
			type_prefix: prefix,
			number: Number(numStr),
			title
		};
	}

	const quests: Quest[] = [
		withSlug(1, 'DEV-001', '로그인 구현'),
		withSlug(2, 'DEV-037', 'GUI 검색'),
		withSlug(3, 'BUG-005', '오타 수정'),
		withSlug(4, 'DEV-002', 'API adapter')
	];

	it('전체 slug 매치', () => {
		const r = filterQuests(quests, new Set(), new Set(), 'DEV-037');
		expect(r.map((x) => x.quest_id)).toEqual(['DEV-037']);
	});

	it('부분 number "002" 매치 — zero-padded slug 안에 포함', () => {
		const r = filterQuests(quests, new Set(), new Set(), '002');
		expect(r.map((x) => x.quest_id)).toEqual(['DEV-002']);
	});

	it('prefix "BUG-" → 해당 type 의 quest 들 매치', () => {
		const r = filterQuests(quests, new Set(), new Set(), 'BUG-');
		expect(r.map((x) => x.quest_id)).toEqual(['BUG-005']);
	});

	it('대소문자 무시 — "dev-037" 도 매치', () => {
		const r = filterQuests(quests, new Set(), new Set(), 'dev-037');
		expect(r.map((x) => x.quest_id)).toEqual(['DEV-037']);
	});

	it('titleOnly=true 여도 slug 는 매치 — 메타 정보이므로', () => {
		const r = filterQuests(quests, new Set(), new Set(), 'DEV-037', true);
		expect(r.map((x) => x.quest_id)).toEqual(['DEV-037']);
	});

	it('title + slug 모두 매치할 때 중복 없이 1건', () => {
		// "DEV" 토큰 → title 에 "DEV" 없지만 slug 에 매치 → DEV-* 3건.
		const r = filterQuests(quests, new Set(), new Set(), 'DEV');
		expect(r.map((x) => x.quest_id).sort()).toEqual(['DEV-001', 'DEV-002', 'DEV-037']);
	});
});

// --- DEV-040 follow-up: sub-quest 가 검색에 걸리려면 조상 포함 ---

describe('includeAncestors / ancestorIdsOf', () => {
	// 구조: 1 (root)
	//        ├─ 2
	//        │   └─ 4 (잎)
	//        └─ 3
	const tree: Quest[] = [
		q(1),
		q(2, { parentId: 1 }),
		q(3, { parentId: 1 }),
		q(4, { parentId: 2 })
	];

	it('잎이 매치되면 모든 조상이 포함됨', () => {
		const matched = [tree.find((x) => x.id === 4)!];
		const r = includeAncestors(matched, tree);
		expect(r.map((x) => x.id).sort()).toEqual([1, 2, 4]);
	});

	it('root 가 매치되면 자기 자신만 (조상 없음)', () => {
		const matched = [tree.find((x) => x.id === 1)!];
		const r = includeAncestors(matched, tree);
		expect(r.map((x) => x.id)).toEqual([1]);
	});

	it('여러 매치의 조상 — 중복 없이 union', () => {
		const matched = [tree.find((x) => x.id === 3)!, tree.find((x) => x.id === 4)!];
		const r = includeAncestors(matched, tree);
		expect(r.map((x) => x.id).sort()).toEqual([1, 2, 3, 4]);
	});

	it('all 의 원본 순서 유지', () => {
		const matched = [tree.find((x) => x.id === 4)!];
		const r = includeAncestors(matched, tree);
		// all 은 [1,2,3,4] 순. 결과는 [1,2,4] — 3 빠짐, 순서 유지.
		expect(r.map((x) => x.id)).toEqual([1, 2, 4]);
	});

	it('ancestorIdsOf — 매치된 항목 자신은 제외, 조상만', () => {
		const matched = [tree.find((x) => x.id === 4)!];
		const ids = ancestorIdsOf(matched, tree);
		expect([...ids].sort()).toEqual([1, 2]);
	});

	it('orphan parent_quest_id (all 에 없는 id) 만나면 조용히 중단', () => {
		const orphan: Quest = { ...q(5), parent_quest_id: 999 };
		const r = includeAncestors([orphan], [...tree, orphan]);
		// 5 만 — 999 는 all 에 없으므로 walk 중단.
		expect(r.map((x) => x.id)).toEqual([5]);
	});
});

// 통합: search 가 sub-quest 만 매치할 때 buildTree 가 정상 작동하는지 검증.
describe('DEV-040 회귀: sub-quest 검색', () => {
	function quest(id: number, parentId: number | null, title: string): Quest {
		return { ...q(id, { parentId }), title };
	}

	const all: Quest[] = [
		quest(1, null, 'parent root'),
		quest(2, 1, 'child looking for needle'),
		quest(3, null, 'unrelated')
	];

	it('"needle" 매치 → buildTree 가 자식만 받아도 parent 부재로 보이지 않음 (bug)', () => {
		const matched = filterQuests(all, new Set(), new Set(), 'needle');
		// 매치 자체는 성공: id=2.
		expect(matched.map((x) => x.id)).toEqual([2]);
		// 그러나 buildTree(matched, null) 은 parent_quest_id===null 만 →
		// id=2 는 parent_quest_id=1 인데 1 이 없으므로 결과 비어있음.
		const tree = buildTree(matched, null);
		expect(tree).toHaveLength(0);
	});

	it('includeAncestors 적용 후 buildTree → child 가 부모 아래로 보임 (fix)', () => {
		const matched = filterQuests(all, new Set(), new Set(), 'needle');
		const withAncestors = includeAncestors(matched, all);
		const tree = buildTree(withAncestors, null);
		expect(tree).toHaveLength(1);
		expect(tree[0].id).toBe(1);
		expect(tree[0].children).toHaveLength(1);
		expect(tree[0].children[0].id).toBe(2);
	});

	// DEV-068 fix: tag 필터로 child 가 매치돼도 includeAncestors 가 부모 포함해야
	// Tree 모드에서 buildTree 가 child 노드를 표시 가능.
	it('tag 필터의 child 매치 → includeAncestors 후 tree 에서 표시됨', () => {
		const parent: Quest = { ...quest(1, null, 'parent'), tags: ['x'] };
		const child: Quest = { ...quest(2, 1, 'child'), tags: ['target'] };
		const sibling: Quest = { ...quest(3, null, 'sibling'), tags: ['unrelated'] };
		const tagged = [parent, child, sibling];
		const matched = filterQuests(tagged, new Set(), new Set(), '', false, new Set(['target']));
		expect(matched.map((m) => m.id)).toEqual([2]);
		// 직접 buildTree 는 child 못 보임 (parent 가 매치 안 됨).
		expect(buildTree(matched, null)).toHaveLength(0);
		// includeAncestors 적용 후 child 가 부모 아래에 표시.
		const withAncestors = includeAncestors(matched, tagged);
		const tree = buildTree(withAncestors, null);
		expect(tree).toHaveLength(1);
		expect(tree[0].id).toBe(1);
		expect(tree[0].children).toHaveLength(1);
		expect(tree[0].children[0].id).toBe(2);
	});
});

// --- DEV-033: sortQuests ---

import { sortQuests } from './quest-list';

function qs(
	id: number,
	opts: { urgency?: number; statusId?: number; created?: string; updated?: string } = {}
): Quest {
	return {
		...q(id),
		urgency: opts.urgency ?? 3,
		status_id: opts.statusId ?? 1,
		created_at: opts.created ?? '',
		updated_at: opts.updated ?? ''
	};
}

describe('sortQuests', () => {
	it('id 기본 — asc, desc 토글로 반전', () => {
		const list = [qs(3), qs(1), qs(2)];
		expect(sortQuests(list, 'id').map((x) => x.id)).toEqual([1, 2, 3]);
		expect(sortQuests(list, 'id', true).map((x) => x.id)).toEqual([3, 2, 1]);
	});

	it('urgency — 1 (Critical) 이 먼저, tie 는 id asc', () => {
		const list = [qs(1, { urgency: 4 }), qs(2, { urgency: 1 }), qs(3, { urgency: 1 })];
		expect(sortQuests(list, 'urgency').map((x) => x.id)).toEqual([2, 3, 1]);
	});

	it('status — statusOrder 의 sort_order 순, 미지정 status 는 뒤로', () => {
		const order = new Map([
			[10, 1],
			[20, 2]
		]);
		const list = [qs(1, { statusId: 20 }), qs(2, { statusId: 99 }), qs(3, { statusId: 10 })];
		expect(sortQuests(list, 'status', false, order).map((x) => x.id)).toEqual([3, 1, 2]);
	});

	it('updated / created — ISO 문자열 asc', () => {
		const list = [
			qs(1, { updated: '2026-06-09T10:00:00+09:00' }),
			qs(2, { updated: '2026-06-08T10:00:00+09:00' })
		];
		expect(sortQuests(list, 'updated').map((x) => x.id)).toEqual([2, 1]);
		expect(sortQuests(list, 'updated', true).map((x) => x.id)).toEqual([1, 2]);
	});

	it('원본 배열 비파괴', () => {
		const list = [qs(2), qs(1)];
		sortQuests(list, 'id');
		expect(list.map((x) => x.id)).toEqual([2, 1]);
	});
});

// --- DEV-033: ExtraFilters ---

describe('filterQuests extra', () => {
	const none = new Set<number>();
	it('urgency 다중 선택', () => {
		const list = [qs(1, { urgency: 1 }), qs(2, { urgency: 3 }), qs(3, { urgency: 4 })];
		const out = filterQuests(list, none, none, '', false, new Set(), {
			urgencies: new Set([1, 4])
		});
		expect(out.map((x) => x.id)).toEqual([1, 3]);
	});

	it('prereq tri-state — has / none', () => {
		const list = [qs(1), qs(2), qs(3)];
		const prereqQuestIds = new Set([2]);
		expect(
			filterQuests(list, none, none, '', false, new Set(), { prereq: 'has', prereqQuestIds }).map((x) => x.id)
		).toEqual([2]);
		expect(
			filterQuests(list, none, none, '', false, new Set(), { prereq: 'none', prereqQuestIds }).map((x) => x.id)
		).toEqual([1, 3]);
		// any = 미적용.
		expect(
			filterQuests(list, none, none, '', false, new Set(), { prereq: 'any', prereqQuestIds })
		).toHaveLength(3);
	});

	it('sub tri-state', () => {
		const list = [qs(1), qs(2)];
		const parentIds = new Set([1]);
		expect(
			filterQuests(list, none, none, '', false, new Set(), { sub: 'has', parentIds }).map((x) => x.id)
		).toEqual([1]);
	});

	it('날짜 범위 — created (포함 경계)', () => {
		const list = [
			qs(1, { created: '2026-06-01T10:00:00+09:00' }),
			qs(2, { created: '2026-06-05T10:00:00+09:00' }),
			qs(3, { created: '2026-06-09T10:00:00+09:00' })
		];
		const out = filterQuests(list, none, none, '', false, new Set(), {
			createdAfter: '2026-06-05',
			createdBefore: '2026-06-09'
		});
		expect(out.map((x) => x.id)).toEqual([2, 3]);
	});
});
