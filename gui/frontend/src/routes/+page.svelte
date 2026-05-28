<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import Home from '$lib/components/Home.svelte';
	import QuestBoard from '$lib/components/QuestBoard.svelte';
	import QuestList from '$lib/components/QuestList.svelte';
	import NewQuestModal from '$lib/components/NewQuestModal.svelte';
	import { flashQuestId } from '$lib/stores';
	import type { Quest } from '$lib/types';
	import { detectEnvironment } from '$lib/api/transport';

	// DEV-011: Home 추가. ?view 없으면 home 기본.
	type View = 'home' | 'board' | 'list';
	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'home'
	);

	// DEV-084: New Quest 를 상단 nav 에서 각 뷰의 기존 상단 바/툴바 우측 끝으로
	// 이동 (QuestBoard toolbar / QuestList filter-bar). 모달 + 생성 로직은 여기
	// (+page) 에서 소유, 콜백으로 버튼 클릭만 위임.
	let showNewQuest = $state(false);

	async function onCreated(quest: Quest) {
		// 보드 뷰가 아니면 보드로 이동 후 펄스 (생성 결과 위치 확인 편의).
		if (currentView !== 'board') {
			await goto('/?view=board');
		}
		flashQuestId.set(quest.id);
		showNewQuest = false;
	}

	// DEV-052: Tauri 가 인자 없이 시작되면 launch_mode === "welcome".
	// 길드 컨텍스트가 없으므로 / (board) 진입 시 항상 /welcome 으로 bounce.
	// (이전에 sessionStorage 마커로 첫 회만 redirect 했지만, Nav 로고 클릭 등
	// 으로 다시 / 진입 시 빈 보드가 노출되는 버그가 있어서 제거.)
	onMount(async () => {
		if (detectEnvironment() !== 'tauri') return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			// DEV-052 후속 (3회차): launch_mode 가 string → { mode, uninit_path }
			// 객체로 바뀌었음. 이전 string 비교 (mode === 'welcome') 는 영원히
			// false 라 redirect 가 안 되는 버그가 있었음.
			const info = await invoke<{ mode: string; uninit_path: string | null }>(
				'launch_mode'
			);
			if (info.mode === 'welcome' || info.mode === 'uninit') {
				goto('/welcome');
			}
		} catch {
			// invoke 실패 시 redirect 생략 (회귀 방지).
		}
	});
</script>

{#if currentView === 'board'}
	<QuestBoard onNewQuest={() => (showNewQuest = true)} />
{:else if currentView === 'list'}
	<QuestList onNewQuest={() => (showNewQuest = true)} />
{:else}
	<Home />
{/if}

{#if showNewQuest}
	<NewQuestModal onclose={() => (showNewQuest = false)} oncreated={onCreated} />
{/if}
