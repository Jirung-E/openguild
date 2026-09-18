<!--
  DEV-417: 문서를 가리키는 링크 하나 — 본문의 크로스링크와 **같은 선택지**를 준다.

  퀘스트 상세의 관계 목록(부모·하위·선행·후속·연결된 캠페인)과 캠페인 상세의 퀘스트 목록은
  같은 문서를 가리키는데도 누르면 무조건 이동뿐이었다. 본문에 쓴 `[[DEV-001]]` 에는 호버하면
  미리보기·새 창·이동을 고를 수 있는데, 바로 위 목록에서는 안 되는 것이 말이 안 된다.

  팝업 자체는 [[DEV-256]] 의 `LinkPreviewPopup` 을 그대로 쓴다 — 미리보기 본문·버튼이 한 벌이라
  두 곳이 다르게 보일 일이 없다. 타이밍도 `utils/hover-popup.ts` 를 공유한다.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Kind } from '$lib/stores/questIndex';
	import { HoverTimers } from '$lib/utils/hover-popup';

	let {
		kind,
		id,
		href,
		title = '',
		slug,
		class: className = '',
		children
	}: {
		kind: Kind;
		/** quest/campaign/book 은 대문자 정규 ID, rule 은 slug 와 같다. */
		id: string;
		href: string;
		title?: string;
		slug?: string;
		class?: string;
		children: Snippet;
	} = $props();

	let PopupComp = $state<typeof import('./LinkPreviewPopup.svelte').default | null>(null);
	let open = $state(false);
	let anchorRect = $state({ left: 0, right: 0, top: 0, bottom: 0 });
	let anchor: HTMLAnchorElement | undefined = $state(undefined);
	const timers = new HoverTimers();

	function onEnter() {
		timers.cancelClose();
		timers.scheduleOpen(async () => {
			if (!PopupComp) PopupComp = (await import('./LinkPreviewPopup.svelte')).default;
			if (!anchor) return;
			const r = anchor.getBoundingClientRect();
			anchorRect = { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
			open = true;
		});
	}
	function onLeave() {
		timers.cancelOpen();
		if (open) timers.scheduleClose(() => (open = false));
	}
	// 링크 자체를 눌러 이동하면 팝업이 새 페이지까지 남는다 — 그 자리에서 정리한다.
	function onClick() {
		timers.clear();
		open = false;
	}

	$effect(() => () => timers.clear());
</script>

<a
	bind:this={anchor}
	{href}
	class={className}
	onmouseenter={onEnter}
	onmouseleave={onLeave}
	onclick={onClick}
>
	{@render children()}
</a>

{#if PopupComp && open}
	<PopupComp
		{kind}
		{id}
		{slug}
		{title}
		{href}
		{anchorRect}
		onenter={() => timers.cancelClose()}
		onleave={() => timers.scheduleClose(() => (open = false))}
		onnavigate={() => {
			timers.clear();
			open = false;
		}}
	/>
{/if}
