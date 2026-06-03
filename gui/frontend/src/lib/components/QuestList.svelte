<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { questsApi } from '$lib/api/quests';
	import { metaApi } from '$lib/api/meta';
	import type { Quest, QuestStatus, QuestType } from '$lib/types';
	import {
		ancestorIdsOf,
		buildTree,
		filterQuests,
		flattenTree,
		includeAncestors
	} from '$lib/utils/quest-list';
	import QuestListFilter from './QuestListFilter.svelte';
	import QuestListItem from './QuestListItem.svelte';

	// DEV-086: New Quest 버튼 — Board toolbar 와 동일 좌표/크기로 우상단 고정.
	// 클릭 시 부모 (+page) 모달 오픈.
	let { onNewQuest }: { onNewQuest?: () => void } = $props();

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
	async function loadData() {
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
	}

	onMount(async () => {
		await loadData();
		// URL → state (초기 로드).
		const params = $page.url.searchParams;
		search = params.get('search') ?? '';
		titleOnly = params.get('title_only') === 'true';
	});

	// DEV-095: Nav 의 Reindex 버튼이 bump 한 store 를 subscribe — 값 변할 때마다
	// loadData() 재호출 → quest 목록 갱신.
	import { reindexBump } from '$lib/stores/reindex';
	let lastBump = $state(0);
	$effect(() => {
		const bump = $reindexBump;
		if (bump !== lastBump && bump > 0) {
			lastBump = bump;
			loading = true;
			loadData();
		}
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
	// DEV-040 후속 버그 수정: 검색이 sub-quest 를 매치해도, 그 부모가 결과에
	// 없으면 buildTree 가 그 sub-quest 에 닿지 못함 → 안 보임. 검색 활성화 시
	// 매치된 항목의 조상을 결과에 포함 + 자동 펼침.
	let flatList = $derived.by(() => {
		const matched = filterQuests(
			quests,
			filterTypeIds,
			filterStatusIds,
			search,
			titleOnly
		);
		const hasSearch = search.trim().length > 0;
		const filtered = hasSearch ? includeAncestors(matched, quests) : matched;
		const effectiveExpanded = hasSearch
			? new Set([...expanded, ...ancestorIdsOf(matched, quests)])
			: expanded;
		const tree = buildTree(filtered, null);
		return flattenTree(tree, effectiveExpanded);
	});

	function toggle(id: number) {
		const next = new Set(expanded);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		expanded = next;
	}
</script>

<div class="quest-list">
	<!-- DEV-086: New Quest — Quest Board toolbar 와 동일 좌표 (top:10px right:14px)
	     + 동일 크기. 페이지 전환 시 버튼이 안 흔들리도록. filter-bar 위에 떠 있되
	     filter-bar 가 우측 130px padding 으로 자리 비워둠. -->
	{#if onNewQuest}
		<button class="qb-new" onclick={onNewQuest} title="새 퀘스트">
			<span class="qb-new-icon">+</span><span>New Quest</span>
		</button>
	{/if}

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
		position: relative; /* DEV-086: New Quest 절대배치 기준. */
	}

	/* DEV-086: New Quest — Quest Board 의 .tb-btn.tb-new 와 px 단위까지 동일
	   (padding 4px 10px / font 0.8rem / radius 6px / 초록). 위치도 동일
	   (top:10px right:14px) — 보드↔리스트 전환 시 안 흔들림. */
	.qb-new {
		position: absolute;
		top: 10px;
		right: 14px;
		z-index: 10;
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 10px;
		background: #238636;
		border: 1px solid #2ea043;
		border-radius: 6px;
		color: #fff;
		font-size: 0.8rem;
		font-weight: 600;
		cursor: pointer;
		transition: background 0.1s, border-color 0.1s;
	}
	.qb-new:hover { background: #2ea043; border-color: #3fb950; }
	.qb-new-icon { font-size: 0.95rem; line-height: 1; }

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
