<script lang="ts">
	import { page } from '$app/stores';
	import { onMount } from 'svelte';
	import Nav from '$lib/components/Nav.svelte';
	import UpdateBanner from '$lib/components/UpdateBanner.svelte';
	import SchemaAheadBanner from '$lib/components/SchemaAheadBanner.svelte';
	import { detectEnvironment } from '$lib/api/transport';
	import '$lib/styles/global.css';

	let { children } = $props();

	// DEV-052 후속: /welcome 라우트에선 Nav (Board/List/Admin/+New Quest) 숨김.
	// 길드 컨텍스트가 없는 상태에서 의미 없는 액션 노출 방지.
	let showNav = $derived($page.url.pathname !== '/welcome');

	// BUG-031 / BUG-033: Tauri 데스크탑 앱에서 웹 기본 우클릭 메뉴
	// (Inspect / Reload / Back / Forward) 노출 차단.
	// - capture phase 로 등록해 다른 핸들러가 먼저 e.preventDefault() 를 호출
	//   할 가능성 없이 가장 먼저 받음.
	// - document 와 window 둘 다 등록 (WebView2 환경별 fallback).
	// - 브라우저 (`http`) 에서는 dev 편의를 위해 그대로 둠.
	onMount(() => {
		if (detectEnvironment() !== 'tauri') return;
		const block = (e: MouseEvent) => {
			e.preventDefault();
			e.stopPropagation();
			return false;
		};
		// capture: true — bubbling 단계가 아닌 capture 단계에서 즉시 차단.
		document.addEventListener('contextmenu', block, { capture: true });
		window.addEventListener('contextmenu', block, { capture: true });
		return () => {
			document.removeEventListener('contextmenu', block, { capture: true });
			window.removeEventListener('contextmenu', block, { capture: true });
		};
	});
</script>

<!-- BUG-041: DB schema 가 binary 보다 새로운 경우 알림 (Tauri 만). 항상 최상단. -->
<SchemaAheadBanner />
<!-- DEV-063: 업데이트 배너 — Nav 아래, 새 버전 있을 때만 노출. 모든 라우트
     (welcome 포함) 공통. -->
<UpdateBanner />
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
