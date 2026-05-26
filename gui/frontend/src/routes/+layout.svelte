<script lang="ts">
	import { page } from '$app/stores';
	import { onMount } from 'svelte';
	import Nav from '$lib/components/Nav.svelte';
	import { detectEnvironment } from '$lib/api/transport';
	import '$lib/styles/global.css';

	let { children } = $props();

	// DEV-052 후속: /welcome 라우트에선 Nav (Board/List/Admin/+New Quest) 숨김.
	// 길드 컨텍스트가 없는 상태에서 의미 없는 액션 노출 방지.
	let showNav = $derived($page.url.pathname !== '/welcome');

	// BUG-031: Tauri 데스크탑 앱에서 웹 기본 우클릭 메뉴 (Inspect / Reload /
	// Back / Forward) 노출 차단. 데스크탑 앱답게 동작.
	// 브라우저 (mode === 'http') 에서는 dev 편의를 위해 그대로 둠.
	onMount(() => {
		if (detectEnvironment() !== 'tauri') return;
		const block = (e: MouseEvent) => e.preventDefault();
		document.addEventListener('contextmenu', block);
		return () => document.removeEventListener('contextmenu', block);
	});
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
