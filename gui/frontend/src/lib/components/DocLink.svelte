<!--
  DEV-417: 문서를 가리키는 목록 한 줄 — 검색 팔레트와 **같은 선택지**를 준다.

  퀘스트 상세의 관계 목록(부모·하위·선행·후속·연결된 캠페인), 캠페인의 퀘스트 목록, 그리고
  연관 문서(백링크)는 전부 "같은 문서를 가리키는 목록" 이다. 팔레트에서는 미리보기·새 창·이동을
  고를 수 있는데 이 목록들에서는 누르면 무조건 이동뿐이었다.

  모양도 팔레트를 따른다(admin) — 줄 오른쪽에 아이콘 버튼 셋([[DocRowActions]]). **버튼이 아닌
  자리를 누르면 예전처럼 페이지 이동**이다.

  미리보기 팝업은 [[DEV-256]] 의 `LinkPreviewPopup` 을 그대로 쓴다 — 본문·버튼이 한 벌이라 두
  화면이 다르게 보일 일이 없다.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Kind } from '$lib/stores/questIndex';
	import { openInWindow, openInPage } from '$lib/utils/open-item';
	import DocRowActions from './DocRowActions.svelte';

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

	const displayName = $derived(title ? `${id} ${title}` : id);

	async function preview() {
		if (!PopupComp) PopupComp = (await import('./LinkPreviewPopup.svelte')).default;
		if (!anchor) return;
		const r = anchor.getBoundingClientRect();
		anchorRect = { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
		open = true;
	}
</script>

<span class="doc-row">
	<a bind:this={anchor} {href} class={className} onclick={() => (open = false)}>
		{@render children()}
	</a>
	<DocRowActions
		onpreview={preview}
		onwindow={() => void openInWindow(href, displayName)}
		onpage={() => {
			open = false;
			openInPage(href);
		}}
	/>
</span>

{#if PopupComp && open}
	<!-- 팝업 밖을 누르면 닫는다 — 목록 줄에는 호버 타이머가 없다(버튼으로 연다). -->
	<div
		class="doc-row-scrim"
		role="presentation"
		onclick={() => (open = false)}
		onkeydown={(e) => e.key === 'Escape' && (open = false)}
	></div>
	<PopupComp
		{kind}
		{id}
		{slug}
		{title}
		{href}
		{anchorRect}
		onenter={() => {}}
		onleave={() => {}}
		onnavigate={() => (open = false)}
	/>
{/if}

<style>
	.doc-row {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		flex: 1;
		min-width: 0;
	}
	/* 줄 내용은 남는 폭을 다 쓰고, 버튼은 오른쪽 끝에 붙는다. */
	.doc-row > :global(a) {
		flex: 1;
		min-width: 0;
	}
	.doc-row-scrim {
		position: fixed;
		inset: 0;
		z-index: 1299; /* 팝업(1300) 바로 아래. */
	}
</style>
