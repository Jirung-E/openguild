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
