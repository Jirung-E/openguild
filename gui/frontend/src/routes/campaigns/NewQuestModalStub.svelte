<!--
  DEV-396 시험용 대역 — 진짜 `NewQuestModal` 은 타입·상태·템플릿 목록을 스스로 불러온다.
  여기서 확인하려는 것은 "만들어진 뒤 캠페인에 연결되는가" 뿐이라, 그 신호만 낸다.
-->
<script lang="ts">
	let {
		onclose,
		oncreated
	}: {
		onclose: () => void;
		oncreated?: (q: { id: number; quest_id: string }) => void;
	} = $props();
</script>

<div>
	<button
		data-testid="stub-create"
		onclick={() => {
			onclose();
			oncreated?.({ id: 9, quest_id: 'DEV-009' });
		}}>만들기</button
	>
</div>
