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
	// DEV-068: tag 필터 — 선택된 tag 모두 가져야 매치 (AND).
	let filterTags = $state(new Set<string>());

	// DEV-037: 검색 — URL ?search= 와 ?title_only= 양방향 동기화.
	let search = $state('');
	let titleOnly = $state(false);

	// DEV-065: 뷰 모드 — 'tree' (부모 그룹 + 들여쓰기, 기본) / 'list' (모든 quest
	// 평면). URL ?mode= 와 localStorage 동시 영속.
	type ViewMode = 'tree' | 'list';
	const VIEW_MODE_KEY = 'openguild.questListMode';
	let viewMode = $state<ViewMode>('tree');

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
		// DEV-065: URL 의 ?mode= 우선, 없으면 localStorage, 없으면 'tree'.
		const urlMode = params.get('mode');
		if (urlMode === 'list' || urlMode === 'tree') {
			viewMode = urlMode;
		} else {
			try {
				const saved = localStorage.getItem(VIEW_MODE_KEY);
				if (saved === 'list' || saved === 'tree') viewMode = saved;
			} catch {
				/* 무시 */
			}
		}
		// DEV-068: URL 의 ?tags=foo,bar → filterTags 초기화 (공유 / bookmark 친화).
		const urlTags = params.get('tags');
		if (urlTags) {
			filterTags = new Set(
				urlTags
					.split(',')
					.map((t) => t.trim())
					.filter((t) => t.length > 0)
			);
		}
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
		// DEV-065: mode 동기화. 'tree' 는 기본이므로 URL 에서 생략.
		if (viewMode === 'list') url.searchParams.set('mode', 'list');
		else url.searchParams.delete('mode');
		// DEV-068: tag filter → URL ?tags=foo,bar. 빈 set 면 키 제거.
		if (filterTags.size > 0) {
			url.searchParams.set('tags', [...filterTags].sort().join(','));
		} else {
			url.searchParams.delete('tags');
		}
		const next = `${url.pathname}${url.search}`;
		const current = `${$page.url.pathname}${$page.url.search}`;
		if (next !== current) {
			goto(next, { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	// DEV-065: mode 변경 시 localStorage 영속.
	$effect(() => {
		if (loading) return;
		try {
			localStorage.setItem(VIEW_MODE_KEY, viewMode);
		} catch {
			/* 무시 */
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
			titleOnly,
			filterTags
		);
		const hasSearch = search.trim().length > 0;
		// DEV-065: 'list' 모드 — 부모 그룹 X. 매칭된 quest 만 평면. ancestor
		// 자동 포함 안 함 (검색 결과 정확).
		if (viewMode === 'list') {
			return matched.map((q) => ({ quest: q, depth: 0, hasChildren: false }));
		}
		// 'tree' 모드 — 기존 동작.
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

	// DEV-068: 모든 quest 의 unique tag 목록 — 필터 chip 옵션.
	let allTagOptions = $derived.by(() => {
		const set = new Set<string>();
		for (const q of quests) {
			for (const t of q.tags ?? []) set.add(t);
		}
		return Array.from(set).sort();
	});
	// DEV-068 후속: 각 tag 별 quest 개수 (현재 filter 무관 — 전체 count).
	let tagCounts = $derived.by(() => {
		const m = new Map<string, number>();
		for (const q of quests) {
			for (const t of q.tags ?? []) m.set(t, (m.get(t) ?? 0) + 1);
		}
		return m;
	});
	function toggleTagFilter(t: string) {
		const next = new Set(filterTags);
		if (next.has(t)) next.delete(t);
		else next.add(t);
		filterTags = next;
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

	<!-- DEV-065 / DEV-068: 뷰 모드 토글 + tag 필터 chip 들 — filter-bar 아래. -->
	<div class="view-toggle-row">
		<div class="view-toggle" role="group" aria-label="뷰 모드">
			<button
				class="vt-btn"
				class:active={viewMode === 'tree'}
				onclick={() => (viewMode = 'tree')}
				title="트리 — 부모 아래로 자식 들여쓰기"
				aria-pressed={viewMode === 'tree'}
			>
				<span class="vt-icon">⇲</span><span>Tree</span>
			</button>
			<button
				class="vt-btn"
				class:active={viewMode === 'list'}
				onclick={() => (viewMode = 'list')}
				title="리스트 — 모든 퀘스트 평면"
				aria-pressed={viewMode === 'list'}
			>
				<span class="vt-icon">≡</span><span>List</span>
			</button>
		</div>
		<!-- DEV-068: 모든 quest 의 unique tag 들. 클릭으로 필터 토글 (AND). -->
		{#if allTagOptions.length > 0}
			<div class="tag-filter-row" aria-label="태그 필터">
				{#each allTagOptions as t (t)}
					<button
						class="tag-filter-chip"
						class:active={filterTags.has(t)}
						onclick={() => toggleTagFilter(t)}
						title={filterTags.has(t) ? `${t} 필터 해제` : `${t} 필터 추가`}
					>
						{t}
						<span class="tag-chip-count">{tagCounts.get(t) ?? 0}</span>
					</button>
				{/each}
				{#if filterTags.size > 0}
					<button class="tag-clear" onclick={() => (filterTags = new Set())} title="태그 필터 모두 해제">
						× 전체 해제
					</button>
				{/if}
			</div>
		{/if}
	</div>

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
		height: calc(100vh - 3.25rem);
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
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.8rem;
		font-weight: 600;
		cursor: pointer;
		transition: background 0.1s, border-color 0.1s;
	}
	.qb-new:hover { background: var(--btn-primary-bg-hover); border-color: var(--btn-primary-border-hover); }
	.qb-new-icon { font-size: 0.95rem; line-height: 1; }

	.list {
		flex: 1;
		overflow-y: auto;
	}

	.state-msg {
		padding: 4rem;
		text-align: center;
		color: var(--text-faint);
		font-size: 0.9rem;
	}

	.state-msg.error {
		color: var(--danger);
	}

	/* DEV-065: 뷰 모드 토글 — segmented 컨트롤. */
	.view-toggle-row {
		display: flex;
		justify-content: flex-start;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.75rem;
		margin: 0.4rem 0 0.75rem;
	}

	/* DEV-068: tag filter chip 들 — view-toggle 옆 inline. */
	.tag-filter-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		align-items: center;
	}
	.tag-filter-chip {
		padding: 0.15rem 0.65rem;
		background: rgba(198, 144, 38, 0.08);
		border: 1px solid rgba(198, 144, 38, 0.3);
		border-radius: 20px;
		color: var(--warning);
		font-size: 0.72rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		cursor: pointer;
		transition: background 0.1s, border-color 0.1s;
	}
	.tag-filter-chip:hover { background: rgba(198, 144, 38, 0.18); }
	.tag-filter-chip.active {
		background: rgba(198, 144, 38, 0.28);
		border-color: rgba(198, 144, 38, 0.7);
		color: color-mix(in srgb, var(--warning) 60%, white);
	}
	.tag-chip-count {
		display: inline-block;
		margin-left: 0.4rem;
		padding: 0 0.4rem;
		min-width: 1.1rem;
		text-align: center;
		font-size: 0.65rem;
		color: var(--text-muted);
		background: var(--bg-subtle);
		border-radius: 10px;
	}
	.tag-clear {
		padding: 0.15rem 0.55rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 20px;
		color: var(--text-muted);
		font-size: 0.7rem;
		cursor: pointer;
	}
	.tag-clear:hover { background: var(--bg-subtle); color: var(--text); }
	.view-toggle {
		display: inline-flex;
		gap: 0;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 2px;
	}
	.vt-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 3px 10px;
		background: transparent;
		border: none;
		border-radius: 4px;
		color: var(--text-muted);
		font-size: 0.8rem;
		cursor: pointer;
		transition: background 0.1s, color 0.1s;
	}
	.vt-btn:hover { color: var(--text); }
	.vt-btn.active {
		background: var(--bg-subtle);
		color: var(--text);
	}
	.vt-icon { font-size: 0.95rem; line-height: 1; }
</style>
