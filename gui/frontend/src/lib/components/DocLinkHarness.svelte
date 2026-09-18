<!-- DEV-417: DocLink 는 children 스니펫을 받는다 — 시험에서 스니펫을 넘기려면 래퍼가 필요하다. -->
<script lang="ts">
	import DocLink from './DocLink.svelte';
	import type { Kind } from '$lib/stores/questIndex';

	let {
		kind,
		id,
		title,
		href
	}: { kind: Kind; id: string; title: string; href: string } = $props();
</script>

<DocLink {kind} {id} {title} {href}>
	<span>{id}</span>
	<span>{title}</span>
</DocLink>
