<!-- BUG-360: 팝업 안의 **마크다운 편집기** — 메모 · 댓글(M↓)이 실제로 이 모양이다. 시험용 래퍼. -->
<script lang="ts">
	import ComposeDock from './ComposeDock.svelte';
	import MarkdownEditor from './MarkdownEditor.svelte';
	let { onsubmitSpy }: { onsubmitSpy?: () => void } = $props();
	let value = $state('쓰던 글');
</script>

<ComposeDock kind="comment" hasContent={true} onsubmit={onsubmitSpy}>
	<MarkdownEditor bind:value onError={() => {}} mediaOnly />
</ComposeDock>
