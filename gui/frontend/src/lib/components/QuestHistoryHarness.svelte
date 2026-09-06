<!--
  BUG-262 테스트 전용 래퍼. 제품 코드에서 쓰지 않는다.

  `@testing-library/svelte` 의 `rerender` 는 props 를 **통째로** 무효화해서,
  값이 하나도 안 바뀌어도 이펙트가 다시 돈다(실측). 그 위에서 "reloadToken 이
  바뀌면 다시 읽는다" 를 단언하면 `reloadToken` 을 의존성에서 빼도 통과한다 —
  아무것도 지키지 못하는 테스트가 된다.

  여기서는 부모가 `$state` 로 토큰을 들고 있다가 바꾼다. Svelte 자신의 반응성을
  타므로 의존성이 실제로 걸려 있어야만 다시 읽는다.
-->
<script lang="ts">
	import QuestHistory from './QuestHistory.svelte';
	import type { QuestStatus } from '$lib/types';

	let { questId = 42, statuses = [] }: { questId?: number; statuses?: QuestStatus[] } = $props();

	let token = $state(0);
	export function bumpToken() {
		token += 1;
	}
</script>

<QuestHistory {questId} {statuses} reloadToken={token} />
