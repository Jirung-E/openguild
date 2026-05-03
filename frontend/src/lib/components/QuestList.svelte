<script lang="ts">
	import { onMount } from 'svelte';
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
	});

	// --- 필터 + 트리 ---
	let flatList = $derived.by(() => {
		const filtered = filterQuests(quests, filterTypeIds, filterStatusIds);
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
	<QuestListFilter {types} {statuses} bind:typeIds={filterTypeIds} bind:statusIds={filterStatusIds} />

	{#if loading}
		<div class="state-msg">Loading...</div>
	{:else if error}
		<div class="state-msg error">{error}</div>
	{:else if flatList.length === 0}
		<div class="state-msg">No quests found.</div>
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
