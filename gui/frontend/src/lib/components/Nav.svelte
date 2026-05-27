<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import NewQuestModal from './NewQuestModal.svelte';
	import { flashQuestId } from '$lib/stores';
	import type { Quest } from '$lib/types';

	// DEV-011: Home 탭 추가. URL `/` 가 ?view 없으면 home 기본.
	type View = 'home' | 'board' | 'list';

	let currentView: View = $derived(
		($page.url.searchParams.get('view') as View | null) ?? 'home'
	);

	let onAdminPath = $derived($page.url.pathname.startsWith('/admin'));
	let onRootPath = $derived($page.url.pathname === '/');
	// BUG-022: + New Quest 버튼은 Quest Board / Quest List 컨텍스트에서만.
	// Home / Admin / Campaigns / Quest Detail 등에서는 숨김.
	let showNewQuestButton = $derived(
		onRootPath && (currentView === 'board' || currentView === 'list')
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
		<a href="/" class:active={onRootPath && currentView === 'home'}>Home</a>
		<a href="/?view=board" class:active={onRootPath && currentView === 'board'}>Quest Board</a>
		<a href="/?view=list" class:active={onRootPath && currentView === 'list'}>Quest List</a>
		<a href="/admin" class:active={onAdminPath}>Admin</a>
	</nav>

	<div class="nav-right">
		{#if showNewQuestButton}
			<button class="btn-new" onclick={() => (showNewQuest = true)}>+ New Quest</button>
		{/if}
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
