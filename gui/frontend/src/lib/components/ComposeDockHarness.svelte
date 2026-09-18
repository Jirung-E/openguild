<!-- DEV-416: ComposeDock 은 children 스니펫을 받는다 — 시험용 래퍼. -->
<script lang="ts">
	import ComposeDock from './ComposeDock.svelte';
	let { hasContent = false }: { hasContent?: boolean } = $props();
</script>

<ComposeDock {hasContent}>
	<textarea value={hasContent ? '쓰던 글' : ''}></textarea>
</ComposeDock>
