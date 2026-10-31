<!--
  검색어에 걸린 글자를 칠한 글 한 줄 — 검색 결과의 제목 · 번호 · 태그에 쓴다(발췌와 같은 노랑, REQ-014).

  예전엔 본문 · 댓글에서 맞았을 때 발췌만 칠하고 제목에서 맞으면 아무것도 안 칠해서, 어느 글자 때문에
  나왔는지 제목을 눈으로 훑어야 했다(admin). 검색어가 비면 글만 그린다.

  `query` 는 검색 글(`/…/` 토막은 정규식), `terms` 는 칠할 말 목록 — 쿼리 모드에서 코어가 뽑아 준 것(REQ-040).
  `terms` 가 있으면 그것을 쓴다.

  `{@html}` 을 쓰지 않는다 — 제목도 사용자가 쓴 글이다([[highlightSegments]] 의 이유와 같다).
-->
<script lang="ts">
	import { highlightTerms, termsOfSearch, type HitTerm } from '$lib/utils/highlight';

	let {
		text,
		query = '',
		terms
	}: { text: string; query?: string; terms?: readonly HitTerm[] } = $props();

	const segs = $derived(highlightTerms(text, terms ?? termsOfSearch(query)));
</script>

{#each segs as seg, i (i)}{#if seg.hit}<mark class="hit">{seg.text}</mark
		>{:else}{seg.text}{/if}{/each}

<style>
	.hit {
		background: var(--search-hit-bg);
		color: var(--search-hit-text);
		border-radius: var(--r-xs);
	}
</style>
