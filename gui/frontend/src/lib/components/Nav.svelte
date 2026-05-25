<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import NewQuestModal from './NewQuestModal.svelte';
	import { flashQuestId } from '$lib/stores';
	import type { Quest } from '$lib/types';

	type View = 'board' | 'list';

	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'board'
	);

	let showNewQuest = $state(false);

	async function onCreated(quest: Quest) {
		// 보드 뷰가 아니면(목록 뷰 등) 보드로 이동시킨 뒤 펄스
		if ($page.url.pathname !== '/' || currentView !== 'board') {
			await goto('/?view=board');
		}
		// QuestBoard 가 store 구독해서 해당 노드로 panTo + 펄스
		flashQuestId.set(quest.id);
	}
</script>

<header>
	<!-- DEV-052 후속 (4회차): 로고 클릭 → Welcome (다른 길드로 전환 / recent 관리). -->
	<a href="/welcome" class="logo">openguild</a>

	<nav>
		<a href="/?view=board" class:active={currentView === 'board'}>Quest Board</a>
		<a href="/?view=list" class:active={currentView === 'list'}>Quest List</a>
		<a href="/admin" class:active={$page.url.pathname.startsWith('/admin')}>Admin</a>
	</nav>

	<div class="nav-right">
		<button class="btn-new" onclick={() => (showNewQuest = true)}>+ New Quest</button>
	</div>
</header>

{#if showNewQuest}
	<NewQuestModal
		onclose={() => (showNewQuest = false)}
		oncreated={onCreated}
	/>
{/if}

<style>
	header {
		display: flex;
		align-items: center;
		gap: 2rem;
		padding: 0 1.5rem;
		height: 52px;
		background: #1a1a2e;
		border-bottom: 1px solid #2a2a4a;
		position: sticky;
		top: 0;
		z-index: 100;
	}

	.logo {
		font-size: 1.1rem;
		font-weight: 700;
		color: #c9d1d9;
		text-decoration: none;
		letter-spacing: 0.02em;
	}

	nav {
		display: flex;
		gap: 0.25rem;
		flex: 1;
	}

	nav a {
		padding: 0.35rem 0.85rem;
		border-radius: 6px;
		font-size: 0.875rem;
		color: #8b949e;
		text-decoration: none;
		transition: background 0.15s, color 0.15s;
	}

	nav a:hover {
		background: #2a2a4a;
		color: #c9d1d9;
	}

	nav a.active {
		background: #2a2a4a;
		color: #ffffff;
		font-weight: 500;
	}

	.nav-right {
		display: flex;
		align-items: center;
		margin-left: auto;
	}

	.btn-new {
		padding: 0.35rem 1rem;
		background: #238636;
		border: 1px solid #2ea043;
		border-radius: 6px;
		color: #fff;
		font-size: 0.825rem;
		font-weight: 500;
		cursor: pointer;
		transition: background 0.15s;
		white-space: nowrap;
	}
	.btn-new:hover { background: #2ea043; }
</style>
