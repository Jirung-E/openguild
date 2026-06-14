<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	// DEV-135: mount 시 필터 복원 (Board 와 공유 store).
	import {
		questFilters,
		serializeFilter,
		deserializeFilter,
		FILTER_STORAGE_SUFFIX,
		type QuestFilterState
	} from '$lib/stores/quest-filter';
	// DEV-033 #2: 필터를 길드별 localStorage 에 영속 — Ctrl+R / 앱 재시작 후에도 유지.
	import { resolveGuildKeyPrefix, guildKey } from '$lib/utils/guild-storage';
	import { questsApi } from '$lib/api/quests';
	import { metaApi } from '$lib/api/meta';
	import type { Quest, QuestStatus, QuestType } from '$lib/types';
	import {
		ancestorIdsOf,
		buildTree,
		filterQuests,
		flattenTree,
		includeAncestors,
		sortQuests,
		type SortKey,
		type TriState
	} from '$lib/utils/quest-list';
	import QuestListFilter from './QuestListFilter.svelte';
	import QuestListItem from './QuestListItem.svelte';
	// DEV-074 fix14: 내부 스크롤 컨테이너용 overlay 스크롤바.
	import OverlayScrollbar from './OverlayScrollbar.svelte';

	// DEV-086: New Quest 버튼 — Board toolbar 와 동일 좌표/크기로 우상단 고정.
	// 클릭 시 부모 (+page) 모달 오픈.
	let { onNewQuest }: { onNewQuest?: () => void } = $props();

	// --- 상태 ---
	let quests = $state<Quest[]>([]);
	let types = $state<QuestType[]>([]);
	let statuses = $state<QuestStatus[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	// DEV-033 fix: 초기 복원 완료 플래그 — true 가 되기 전엔 영속 effect 가
	// 저장하지 않음 (기본값으로 덮어쓰는 race 방지).
	let initialized = $state(false);
	// DEV-074 fix14: 내부 .list 의 ref — OverlayScrollbar target 으로 전달.
	let listEl: HTMLDivElement | undefined = $state(undefined);

	// DEV-126 fix2: 퀘스트 리스트는 window 가 아니라 내부 .list 컨테이너가
	// 스크롤한다 (overflow-y:auto). layout 의 window.scrollY 복원은 항상 0 이라
	// 효과 없음 → 컨테이너 scrollTop 을 path 별로 직접 저장/복원.
	const LIST_SCROLL_KEY = 'openguild.listScroll.';
	function listScrollKey(): string {
		return LIST_SCROLL_KEY + $page.url.pathname + $page.url.search;
	}
	function saveListScroll() {
		if (!listEl) return;
		try {
			sessionStorage.setItem(listScrollKey(), String(listEl.scrollTop));
		} catch {
			/* quota / disabled — 무시 */
		}
	}
	function restoreListScroll() {
		if (!listEl) return;
		let raw: string | null;
		try {
			raw = sessionStorage.getItem(listScrollKey());
		} catch {
			return;
		}
		if (raw === null) return;
		const y = parseInt(raw, 10);
		if (!Number.isFinite(y) || y <= 0) return;
		// 트리/목록은 loadData 후 비동기로 자라남 — 컨테이너가 y 에 도달 가능할
		// 때까지 (또는 ~1.2초) 재시도 (layout 의 window 복원과 동일 패턴).
		let tries = 0;
		const attempt = () => {
			if (!listEl) return;
			listEl.scrollTop = y;
			tries += 1;
			const reached = Math.abs(listEl.scrollTop - y) <= 2;
			const tall = listEl.scrollHeight - listEl.clientHeight >= y;
			if (reached || tall || tries >= 40) return;
			setTimeout(attempt, 30);
		};
		requestAnimationFrame(() => requestAnimationFrame(attempt));
	}

	let filterTypeIds = $state(new Set<number>());
	let filterStatusIds = $state(new Set<number>());
	let expanded = $state(new Set<number>());
	// DEV-068: tag 필터 — 선택된 tag 모두 가져야 매치 (AND).
	let filterTags = $state(new Set<string>());
	// DEV-033: 고급 필터.
	let filterUrgencies = $state(new Set<number>());
	let filterPrereq = $state<TriState>('any');
	let filterSub = $state<TriState>('any');
	let createdAfter = $state('');
	let createdBefore = $state('');
	let updatedAfter = $state('');
	let updatedBefore = $state('');
	// 선행 quest 가 있는 quest id 들 (dependencies 산출) — tri-state 용.
	let prereqQuestIds = $state(new Set<number>());
	// 자식이 있는 quest id 들 — quests 의 parent_quest_id 역산.
	let parentIds = $derived(
		new Set(quests.map((q) => q.parent_quest_id).filter((p): p is number => p != null))
	);

	// DEV-037: 검색 — URL ?search= 와 ?title_only= 양방향 동기화.
	let search = $state('');
	let titleOnly = $state(false);

	// DEV-065: 뷰 모드 — 'tree' (부모 그룹 + 들여쓰기, 기본) / 'list' (모든 quest
	// 평면). URL ?mode= 와 localStorage 동시 영속.
	type ViewMode = 'tree' | 'list';
	const VIEW_MODE_KEY = 'openguild.questListMode';
	let viewMode = $state<ViewMode>('tree');

	// DEV-033: 정렬 — CLI --sort 와 1:1. URL ?sort= / ?desc=1 + localStorage 영속.
	const SORT_KEY = 'openguild.questListSort';
	const SORT_KEYS: SortKey[] = ['id', 'urgency', 'status', 'updated', 'created'];
	const SORT_LABELS: Record<SortKey, string> = {
		id: 'ID (생성 순)',
		urgency: '긴급도',
		status: '상태',
		updated: '갱신 시각',
		created: '생성 시각'
	};
	let sortKey = $state<SortKey>('id');
	let sortDesc = $state(false);

	// DEV-033 #2: 필터 영속 (길드별) — Ctrl+R / 앱 재시작 후에도 유지.
	// questFilters store 는 in-memory 라 전체 리로드 시 날아감 → localStorage 병행.
	// 타입/상태 ID 는 길드마다 달라 guildKey 로 namespace 분리.
	let filterKeyPrefix = $state('');
	function filterKey(): string {
		return guildKey(filterKeyPrefix, FILTER_STORAGE_SUFFIX);
	}
	function snapshotFilter(): QuestFilterState {
		return {
			typeIds: filterTypeIds,
			statusIds: filterStatusIds,
			search,
			titleOnly,
			tags: filterTags,
			urgencies: filterUrgencies,
			prereq: filterPrereq,
			sub: filterSub,
			createdAfter,
			createdBefore,
			updatedAfter,
			updatedBefore
		};
	}
	function applyFilter(f: QuestFilterState) {
		filterTypeIds = new Set(f.typeIds);
		filterStatusIds = new Set(f.statusIds);
		search = f.search;
		titleOnly = f.titleOnly;
		filterTags = new Set(f.tags);
		filterUrgencies = new Set(f.urgencies);
		filterPrereq = f.prereq;
		filterSub = f.sub;
		createdAfter = f.createdAfter;
		createdBefore = f.createdBefore;
		updatedAfter = f.updatedAfter;
		updatedBefore = f.updatedBefore;
	}
	function saveFilterToStorage() {
		try {
			localStorage.setItem(filterKey(), serializeFilter(snapshotFilter()));
		} catch {
			/* 무시 */
		}
	}
	function loadFilterFromStorage(): QuestFilterState | null {
		try {
			return deserializeFilter(localStorage.getItem(filterKey()));
		} catch {
			return null;
		}
	}
	// status 정렬용 — status_id → sort_order 맵.
	let statusOrder = $derived(new Map(statuses.map((s) => [s.id, s.sort_order])));

	// --- 데이터 ---
	async function loadData() {
		try {
			const [q, t, s, deps] = await Promise.all([
				questsApi.list(),
				metaApi.getQuestTypes(),
				metaApi.getQuestStatuses(),
				// DEV-033: 선행 tri-state 용. 실패해도 목록 자체는 OK.
				questsApi.listDependencies().catch(() => [])
			]);
			quests = q;
			types = t;
			statuses = s;
			prereqQuestIds = new Set(deps.map((d) => d.quest_id));
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	}

	onMount(async () => {
		await loadData();
		// DEV-033 #2: 길드별 필터 키 prefix 확정 (localStorage load/save 전).
		filterKeyPrefix = await resolveGuildKeyPrefix();
		// DEV-135: 공유 store → state 복원 (view 전환 시 in-memory 일관성).
		applyFilter(get(questFilters));
		// DEV-033 #2: 영속된 필터가 있으면 우선 적용 — 전체 리로드(Ctrl+R)/앱
		// 재시작 시 store 는 비어 있으므로 localStorage 가 진짜 복원원.
		const savedFilter = loadFilterFromStorage();
		if (savedFilter) applyFilter(savedFilter);
		// URL → state (초기 로드). DEV-135 #4 fix: param 이 '있을 때만' 덮어씀.
		// 이전엔 ?? '' 로 무조건 덮어써, /?view=list 처럼 param 없는 nav 후
		// localStorage 에서 복원한 검색어가 매번 지워졌다 (다른 필터는 유지되는데
		// 검색만 풀리는 비일관). tags 처럼 '있을 때만' 적용으로 통일.
		const params = $page.url.searchParams;
		const urlSearch = params.get('search');
		if (urlSearch !== null) search = urlSearch;
		const urlTitleOnly = params.get('title_only');
		if (urlTitleOnly !== null) titleOnly = urlTitleOnly === 'true';
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
		// DEV-033: URL ?sort= 우선, 없으면 localStorage.
		const urlSort = params.get('sort');
		if (urlSort && (SORT_KEYS as string[]).includes(urlSort)) {
			sortKey = urlSort as SortKey;
			sortDesc = params.get('desc') === '1';
		} else {
			try {
				const saved = localStorage.getItem(SORT_KEY);
				if (saved) {
					const [k, d] = saved.split(':');
					if ((SORT_KEYS as string[]).includes(k)) {
						sortKey = k as SortKey;
						sortDesc = d === 'desc';
					}
				}
			} catch {
				/* 무시 */
			}
		}
		// DEV-126 fix2: 데이터 로드 후 컨테이너 스크롤 위치 복원 (reload 대비).
		restoreListScroll();
		// DEV-033 fix: 모든 복원(URL/localStorage)이 끝난 뒤에야 영속 effect 가
		// 저장하도록 — loading 만으로 막으면 loadData 의 finally 에서 loading 이
		// false 가 되는 시점(아직 복원 전)에 effect 가 기본값(id:asc)으로 덮어써
		// 저장된 정렬이 매번 날아갔다.
		initialized = true;
	});

	// DEV-126 fix2: .list 컨테이너 스크롤 저장 — throttle + 페이지 떠나기 직전.
	$effect(() => {
		const el = listEl;
		if (!el) return;
		let last = 0;
		const onScroll = () => {
			const now = Date.now();
			if (now - last < 200) return;
			last = now;
			saveListScroll();
		};
		const onLeave = () => saveListScroll();
		el.addEventListener('scroll', onScroll, { passive: true });
		window.addEventListener('beforeunload', onLeave);
		window.addEventListener('pagehide', onLeave);
		return () => {
			el.removeEventListener('scroll', onScroll);
			window.removeEventListener('beforeunload', onLeave);
			window.removeEventListener('pagehide', onLeave);
		};
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
		// DEV-033 fix: 복원 완료 전엔 무시 (기본값을 URL 에 쓰지 않도록).
		if (!initialized) return;
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
		// DEV-033: 정렬 → URL. 기본 (id asc) 은 생략.
		if (sortKey !== 'id' || sortDesc) {
			url.searchParams.set('sort', sortKey);
			if (sortDesc) url.searchParams.set('desc', '1');
			else url.searchParams.delete('desc');
		} else {
			url.searchParams.delete('sort');
			url.searchParams.delete('desc');
		}
		const next = `${url.pathname}${url.search}`;
		const current = `${$page.url.pathname}${$page.url.search}`;
		if (next !== current) {
			goto(next, { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	// DEV-033: 필터 상태를 Board 공유 store 로 mirror (List 가 truth).
	// (import 는 상단 — DEV-135 의 mount 복원과 공유.)
	// DEV-033 fix: 복원 완료 전엔 mirror 안 함 — onMount 의 store→state 복원을
	// 기본값으로 덮어쓰는 race 방지.
	$effect(() => {
		if (!initialized) return;
		const snap = snapshotFilter();
		questFilters.set(snap);
		// DEV-033 #2: 동일 변경을 길드별 localStorage 에도 — 전체 리로드 후 복원.
		saveFilterToStorage();
	});

	// DEV-033: 정렬 선택 localStorage 영속. (DEV-033 fix: loading → initialized)
	$effect(() => {
		if (!initialized) return;
		try {
			localStorage.setItem(SORT_KEY, `${sortKey}:${sortDesc ? 'desc' : 'asc'}`);
		} catch {
			/* 무시 */
		}
	});

	// DEV-065: mode 변경 시 localStorage 영속.
	$effect(() => {
		if (!initialized) return;
		try {
			localStorage.setItem(VIEW_MODE_KEY, viewMode);
		} catch {
			/* 무시 */
		}
	});

	// --- 필터 + 트리 ---
	// DEV-040 후속 버그 수정: filter (검색 / tag / type / status) 가 sub-quest 를
	// 매치해도, 그 부모가 결과에 없으면 buildTree 가 그 sub-quest 로 닿지 못함
	// → 안 보임. filter 활성화 시 매치된 항목의 조상을 결과에 포함 + 자동 펼침.
	// DEV-068 fix: 이전엔 검색만 처리했지만 tag / type / status 필터도 같은 문제 발생.
	let flatList = $derived.by(() => {
		const matched = filterQuests(
			quests,
			filterTypeIds,
			filterStatusIds,
			search,
			titleOnly,
			filterTags,
			// DEV-033: 고급 필터.
			{
				urgencies: filterUrgencies,
				prereq: filterPrereq,
				sub: filterSub,
				createdAfter,
				createdBefore,
				updatedAfter,
				updatedBefore,
				prereqQuestIds,
				parentIds
			}
		);
		// DEV-065: 'list' 모드 — 부모 그룹 X. 매칭된 quest 만 평면. ancestor
		// 자동 포함 안 함 (검색 결과 정확).
		// DEV-033: 정렬 — list 모드는 평면 그대로, tree 모드는 buildTree 가
		// 입력 배열 순서를 sibling 순서로 보존하므로 정렬 후 build.
		if (viewMode === 'list') {
			return sortQuests(matched, sortKey, sortDesc, statusOrder).map((q) => ({
				quest: q,
				depth: 0,
				hasChildren: false
			}));
		}
		// 'tree' 모드.
		const hasFilters =
			search.trim().length > 0 ||
			filterTags.size > 0 ||
			filterTypeIds.size > 0 ||
			filterStatusIds.size > 0 ||
			// DEV-033: 고급 필터도 ancestor 포함 트리거.
			filterUrgencies.size > 0 ||
			filterPrereq !== 'any' ||
			filterSub !== 'any' ||
			createdAfter !== '' ||
			createdBefore !== '' ||
			updatedAfter !== '' ||
			updatedBefore !== '';
		const filtered = hasFilters ? includeAncestors(matched, quests) : matched;
		const effectiveExpanded = hasFilters
			? new Set([...expanded, ...ancestorIdsOf(matched, quests)])
			: expanded;
		const tree = buildTree(sortQuests(filtered, sortKey, sortDesc, statusOrder), null);
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
		bind:urgencies={filterUrgencies}
		bind:prereqState={filterPrereq}
		bind:subState={filterSub}
		bind:createdAfter
		bind:createdBefore
		bind:updatedAfter
		bind:updatedBefore
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
		<!-- DEV-033: 정렬 — CLI --sort 와 1:1. 방향 토글 = --reverse. -->
		<div class="sort-group" aria-label="정렬">
			<select class="sort-sel" bind:value={sortKey} title="정렬 기준">
				{#each SORT_KEYS as k (k)}
					<option value={k}>{SORT_LABELS[k]}</option>
				{/each}
			</select>
			<button
				class="sort-dir"
				onclick={() => (sortDesc = !sortDesc)}
				title={sortDesc ? '내림차순 — 클릭 시 오름차순' : '오름차순 — 클릭 시 내림차순'}
				aria-label="정렬 방향"
			>{sortDesc ? '↓' : '↑'}</button>
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
		<div class="list" bind:this={listEl}>
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
	<!-- DEV-074 fix14: 내부 overflow 컨테이너에도 overlay scrollbar. -->
	{#if listEl}
		<OverlayScrollbar target={listEl} />
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
		/* DEV-074 fix14: native scrollbar 숨김 — OverlayScrollbar 가 대신 그림. */
		scrollbar-width: none;
	}
	.list::-webkit-scrollbar {
		display: none;
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

	/* DEV-065: 뷰 모드 토글 — segmented 컨트롤.
	   DEV-065 fix: padding 추가 — filter-bar (`padding-left: 1.5rem`) /
	   QuestListItem (`padding-left: 1rem+depth`) 와 시각 정렬. */
	.view-toggle-row {
		display: flex;
		justify-content: flex-start;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.75rem;
		margin: 0.4rem 0 0.75rem;
		padding: 0 1.5rem;
	}

	/* DEV-068: tag filter chip 들 — view-toggle 옆 inline. */
	.tag-filter-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		align-items: center;
	}
	/* DEV-033: 정렬 select + 방향 토글. */
	.sort-group {
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}
	.sort-sel {
		padding: 0.25rem 0.5rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.78rem;
		cursor: pointer;
	}
	.sort-dir {
		width: 1.7rem;
		height: 1.7rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.85rem;
		cursor: pointer;
	}
	.sort-dir:hover { color: var(--text); border-color: var(--text-faint); }

	.tag-filter-chip {
		padding: 0.15rem 0.65rem;
		background: color-mix(in srgb, var(--warning) 8%, transparent);
		border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
		border-radius: 20px;
		color: var(--warning);
		font-size: 0.72rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		cursor: pointer;
		transition: background 0.1s, border-color 0.1s;
	}
	.tag-filter-chip:hover { background: color-mix(in srgb, var(--warning) 18%, transparent); }
	.tag-filter-chip.active {
		background: color-mix(in srgb, var(--warning) 28%, transparent);
		border-color: color-mix(in srgb, var(--warning) 70%, transparent);
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
