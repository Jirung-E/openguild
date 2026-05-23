<script lang="ts">
	import { page } from '$app/stores';
	import Nav from '$lib/components/Nav.svelte';
	import '$lib/styles/global.css';

	let { children } = $props();

	// DEV-052 후속: /welcome 라우트에선 Nav (Board/List/Admin/+New Quest) 숨김.
	// 길드 컨텍스트가 없는 상태에서 의미 없는 액션 노출 방지.
	let showNav = $derived($page.url.pathname !== '/welcome');
</script>

{#if showNav}
	<Nav />
{/if}
<main class:no-nav={!showNav}>
	{@render children()}
</main>

<style>
	main {
		min-height: calc(100vh - 52px);
		background: #0d1117;
	}
	main.no-nav {
		min-height: 100vh;
	}
</style>
