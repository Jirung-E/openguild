<!--
  DEV-395: 새 캠페인은 이제 목록 위의 **모달**이다(새 퀘스트와 통일). 이 경로는 북마크·에이전트
  링크가 가리킬 수 있어 남긴다 — 목록으로 보내고 모달을 열어 준다.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';

	onMount(() => {
		void goto('/campaigns?new=1', { replaceState: true });
	});
</script>
