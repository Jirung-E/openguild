<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import QuestBoard from '$lib/components/QuestBoard.svelte';
	import QuestList from '$lib/components/QuestList.svelte';
	import { detectEnvironment } from '$lib/api/transport';

	type View = 'board' | 'list';
	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'board'
	);

	// DEV-052: Tauri 가 인자 없이 시작되면 launch_mode === "welcome".
	// 첫 진입 (root) 일 때만 /welcome 으로 redirect (사용자가 명시적으로
	// 다시 / 로 navigate 한 경우엔 redirect 안 함 — sessionStorage 마커).
	onMount(async () => {
		if (detectEnvironment() !== 'tauri') return;
		if (sessionStorage.getItem('og-launch-handled') === '1') return;
		sessionStorage.setItem('og-launch-handled', '1');
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const mode = await invoke<string>('launch_mode');
			if (mode === 'welcome') {
				goto('/welcome');
			}
		} catch {
			// invoke 실패 시 redirect 생략 (회귀 방지).
		}
	});
</script>

{#if currentView === 'board'}
	<QuestBoard />
{:else}
	<QuestList />
{/if}

<style>
</style>
