<!-- DEV-416: ComposeDock 은 children 스니펫을 받는다 — 시험용 래퍼.
     실제 호출부와 같은 모양으로(입력칸 + 아래 버튼 줄 + 머리줄 보내기) 둔다. -->
<script lang="ts">
	import ComposeDock from './ComposeDock.svelte';
	let {
		hasContent = false,
		withSubmit = false,
		onsubmitSpy
	}: { hasContent?: boolean; withSubmit?: boolean; onsubmitSpy?: () => void } = $props();
</script>

<ComposeDock
	{hasContent}
	onsubmit={withSubmit ? (onsubmitSpy ?? (() => {})) : undefined}
	submitTitle="보내기"
	submitDisabled={false}
>
	<textarea value={hasContent ? '쓰던 글' : ''}></textarea>
	<div class="actions">
		<button>보내기</button>
	</div>
</ComposeDock>

<style>
	/* 실제 호출부의 버튼 줄과 같은 모양 — 붙었을 때 접히는 것이 이 줄이다. */
	.actions {
		display: flex;
		gap: 0.5rem;
	}
</style>
