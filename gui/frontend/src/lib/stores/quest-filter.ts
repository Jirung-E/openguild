// DEV-033: Quest List 의 필터 상태를 Quest Board 와 공유.
//
// List 가 자기 local state 를 한 방향으로 mirror (List 가 source of truth).
// Board 는 subscribe 해서 매치 안 되는 노드를 dim 처리.
// 두 컴포넌트는 같은 `/?view=` 라우트의 형제 — props drilling 대신 store.

import { writable } from 'svelte/store';
import type { TriState } from '$lib/utils/quest-list';

export interface QuestFilterState {
	typeIds: Set<number>;
	statusIds: Set<number>;
	search: string;
	titleOnly: boolean;
	tags: Set<string>;
	urgencies: Set<number>;
	prereq: TriState;
	sub: TriState;
	createdAfter: string;
	createdBefore: string;
	updatedAfter: string;
	updatedBefore: string;
}

export const EMPTY_FILTER: QuestFilterState = {
	typeIds: new Set(),
	statusIds: new Set(),
	search: '',
	titleOnly: false,
	tags: new Set(),
	urgencies: new Set(),
	prereq: 'any',
	sub: 'any',
	createdAfter: '',
	createdBefore: '',
	updatedAfter: '',
	updatedBefore: ''
};

export function isFilterActive(f: QuestFilterState): boolean {
	return (
		f.typeIds.size > 0 ||
		f.statusIds.size > 0 ||
		f.search.trim().length > 0 ||
		f.tags.size > 0 ||
		f.urgencies.size > 0 ||
		f.prereq !== 'any' ||
		f.sub !== 'any' ||
		f.createdAfter !== '' ||
		f.createdBefore !== '' ||
		f.updatedAfter !== '' ||
		f.updatedBefore !== ''
	);
}

export const questFilters = writable<QuestFilterState>(EMPTY_FILTER);

// ── DEV-033 #2 / DEV-135: localStorage 영속용 직렬화 (List / Board 공용) ──
// Set 은 JSON 직렬화 불가라 배열로. TriState 등은 복원 시 검증.

export function serializeFilter(f: QuestFilterState): string {
	return JSON.stringify({
		typeIds: [...f.typeIds],
		statusIds: [...f.statusIds],
		search: f.search,
		titleOnly: f.titleOnly,
		tags: [...f.tags],
		urgencies: [...f.urgencies],
		prereq: f.prereq,
		sub: f.sub,
		createdAfter: f.createdAfter,
		createdBefore: f.createdBefore,
		updatedAfter: f.updatedAfter,
		updatedBefore: f.updatedBefore
	});
}

export function deserializeFilter(raw: string | null): QuestFilterState | null {
	if (!raw) return null;
	try {
		const o = JSON.parse(raw);
		const tri = (v: unknown): TriState => (v === 'has' || v === 'none' ? v : 'any');
		const str = (v: unknown): string => (typeof v === 'string' ? v : '');
		return {
			typeIds: new Set<number>(Array.isArray(o.typeIds) ? o.typeIds : []),
			statusIds: new Set<number>(Array.isArray(o.statusIds) ? o.statusIds : []),
			search: str(o.search),
			titleOnly: o.titleOnly === true,
			tags: new Set<string>(Array.isArray(o.tags) ? o.tags : []),
			urgencies: new Set<number>(Array.isArray(o.urgencies) ? o.urgencies : []),
			prereq: tri(o.prereq),
			sub: tri(o.sub),
			createdAfter: str(o.createdAfter),
			createdBefore: str(o.createdBefore),
			updatedAfter: str(o.updatedAfter),
			updatedBefore: str(o.updatedBefore)
		};
	} catch {
		return null;
	}
}

/** localStorage 키 suffix (guildKey 와 조합). List / Board 동일 키 공유. */
export const FILTER_STORAGE_SUFFIX = 'questListFilter';
