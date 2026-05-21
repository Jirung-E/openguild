<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { questsApi } from '$lib/api/quests';
	import { metaApi } from '$lib/api/meta';
	import type { Quest, QuestStatus, QuestType } from '$lib/types';
	import { buildTree, filterQuests, flattenTree } from '$lib/utils/quest-list';
	import QuestListFilter from './QuestListFilter.svelte';
	import QuestListItem from './QuestListItem.svelte';

	// --- 상태 ---
	let quests = $state<Quest[]>([]);
	let types = $state<QuestType[]>([]);
	let statuses = $state<QuestStatus[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let filterTypeIds = $state(new Set<number>());
	let filterStatusIds = $state(new Set<number>());
	let expanded = $state(new Set<number>());

	// DEV-037: 검색 — URL ?search= 와 ?title_only= 양방향 동기화.
	let search = $state('');
	let titleOnly = $state(false);

	// --- 데이터 ---
	onMount(async () => {
		try {
			[quests, types, statuses] = await Promise.all([
				questsApi.list(),
				metaApi.getQuestTypes(),
				metaApi.getQuestStatuses()
			]);
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}

		// URL → state (초기 로드).
		const params = $page.url.searchParams;
		search = params.get('search') ?? '';
		titleOnly = params.get('title_only') === 'true';
	});

	// state → URL (변경 시).
	// `replaceState=true` 로 history 폭증 방지.
	$effect(() => {
		// 최초 onMount 전에는 무시.
		if (loading) return;
		const url = new URL($page.url);
		if (search.trim()) url.searchParams.set('search', search.trim());
		else url.searchParams.delete('search');
		if (titleOnly) url.searchParams.set('title_only', 'true');
		else url.searchParams.delete('title_only');
		const next = `${url.pathname}${url.search}`;
		const current = `${$page.url.pathname}${$page.url.search}`;
		if (next !== current) {
			goto(next, { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	// --- 필터 + 트리 ---
	let flatList = $derived.by(() => {
		const filtered = filterQuests(
			quests,
			filterTypeIds,
			filterStatusIds,
			search,
			titleOnly
		);
		const tree = buildTree(filtered, null);
		return flattenTree(tree, expanded);
	});

	function toggle(id: number) {
		const next = new Set(expanded);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		expanded = next;
	}
</script>

<div class="quest-list">
	<QuestListFilter
		{types}
		{statuses}
		bind:typeIds={filterTypeIds}
		bind:statusIds={filterStatusIds}
		bind:search
		bind:titleOnly
	/>

	{#if loading}
		<div class="state-msg">Loading...</div>
	{:else if error}
		<div class="state-msg error">{error}</div>
	{:else if flatList.length === 0}
		<div class="state-msg">
			{#if search.trim()}
				"{search}" 와 일치하는 퀘스트가 없습니다.
			{:else}
				No quests found.
			{/if}
		</div>
	{:else}
		<div class="list">
			{#each flatList as node (node.quest.id)}
				<QuestListItem
					quest={node.quest}
					depth={node.depth}
					hasChildren={node.hasChildren}
					expanded={expanded.has(node.quest.id)}
					ontoggle={() => toggle(node.quest.id)}
				/>
			{/each}
		</div>
	{/if}
</div>

<style>
	.quest-list {
		display: flex;
		flex-direction: column;
		height: calc(100vh - 52px);
	}

	.list {
		flex: 1;
		overflow-y: auto;
	}

	.state-msg {
		padding: 4rem;
		text-align: center;
		color: #484f58;
		font-size: 0.9rem;
	}

	.state-msg.error {
		color: #e94f4f;
	}
</style>
