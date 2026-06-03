/**
 * DEV-095: 페이지 ↔ Nav 의 reindex 신호 store.
 *
 * 페이지들 (Home / QuestList / QuestBoard / +page.svelte 등) 은 `onMount` 안에서
 * 직접 fetch 하는 패턴이라 SvelteKit 의 `invalidateAll()` 이 트리거할 `load()`
 * 함수가 없음. → reindex 후 데이터 갱신이 안 됨.
 *
 * 해결: Nav 가 reindex 성공 시 본 store 의 값을 bump (`+1`). 페이지들은
 * `$effect(() => { $reindexBump; loadData(); })` 로 subscribe — 값이 바뀌면
 * 자기 fetch 함수를 재호출.
 */
import { writable } from 'svelte/store';

/** 값이 변할 때마다 페이지가 reload 트리거. 초기 0. */
export const reindexBump = writable(0);

/** Nav 등에서 reindex 완료 후 호출. */
export function bumpReindex(): void {
	reindexBump.update((n) => n + 1);
}
