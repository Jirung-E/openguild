<!--
  REQ-040: 전체 문서 검색 페이지 — /search.

  팔레트는 빨리 가기(제목 · 최근 본 문서), 여기는 찾아보기다. 퀘스트 · 캠페인 · 도서관 · 규칙을 한 번에, 본문 ·
  댓글 · 첨부 이름까지 본다.

  - 검색: 서버의 강화 검색(`/api/search`, REQ-009)을 그대로 쓴다 — 어디서 맞았는지와 발췌가 온다. 메모는 개인
    기록이라 '메모 포함' 을 켤 때만 본다(서버 기본과 같다).
  - 쿼리: 종류마다 키가 달라(REQ-039) 한 종류를 골랐을 때 그 목록 화면의 쿼리를 쓴다. 규칙은 쿼리가 없고,
    '전체' 에서는 쓰지 않는다.
  - 순서: 제목 · 번호에서 맞은 것이 먼저.
  - 줄은 다른 목록과 같이 미리보기 · 새 창 · 이동([[DocLink]]). 본문 등에서만 맞은 문서는 찾기 바를 띄운 채 연다.
  - 주소에 `q` · `kind` · `mode` · `memo` 를 싣는다 — 새로고침 · 공유에 남는다.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { searchApi, type SearchField as Field } from '$lib/api/search';
	import { questsApi } from '$lib/api/quests';
	import { campaignsApi } from '$lib/api/campaigns';
	import { globalCommentsApi } from '$lib/api/comments';
	import { libraryApi } from '$lib/api/library';
	import { locale, t } from '$lib/stores/locale';
	import { LatestQuery } from '$lib/utils/enhanced-search';
	import { termsOfSearch, type HitTerm } from '$lib/utils/highlight';
	import { queryApi } from '$lib/api/query';
	import SearchField from '$lib/components/SearchField.svelte';
	import QueryNotes from '$lib/components/QueryNotes.svelte';
	import DocLink from '$lib/components/DocLink.svelte';
	import HitText from '$lib/components/HitText.svelte';
	import {
		CAMPAIGN_QUERY_KEYS,
		COMMENT_QUERY_KEYS,
		LIBRARY_QUERY_KEYS,
		QUEST_QUERY_KEYS,
		type QueryKeyRow
	} from '$lib/utils/query-keys';
	import {
		SEARCH_TABS,
		canQuery,
		countByTab,
		openHref,
		readSearchState,
		rowsOfTab,
		searchStateParams,
		sortRows,
		splitKindPrefix,
		type QueryKind,
		type SearchKind,
		type SearchRow,
		type SearchTab
	} from '$lib/utils/search-page';

	const initial = readSearchState($page.url.searchParams);
	let search = $state(initial.mode === 'search' ? initial.q : '');
	let query = $state(initial.mode === 'query' ? initial.q : '');
	let mode = $state(initial.mode);
	let tab = $state<SearchTab>(initial.tab);
	let memo = $state(initial.memo);
	let helpOpen = $state(false);

	let rows = $state<SearchRow[]>([]);
	/** REQ-040: 쿼리면 코어가 뽑아 준 칠할 말. */
	let queryTerms = $state<HitTerm[]>([]);
	/** 결과가 이 글의 것인가 — 다른 글의 결과를 보이지 않는다. */
	let rowsFor = $state('');
	let loading = $state(false);
	let error = $state<string | null>(null);

	const term = $derived((mode === 'query' ? query : search).trim());
	/** 제목 · 번호에 칠할 말 — 쿼리는 `키:값` 이라 칠하지 않는다. */
	const hitTerms = $derived<HitTerm[]>(mode === 'search' ? termsOfSearch(term) : queryTerms);

	const QUERY_KEYS_OF: Record<string, { keys: readonly QueryKeyRow[]; rules: string }> = {
		quest: { keys: QUEST_QUERY_KEYS, rules: 'filter.queryHelpRules' },
		campaign: { keys: CAMPAIGN_QUERY_KEYS, rules: 'campaignList.queryHelpRules' },
		book: { keys: LIBRARY_QUERY_KEYS, rules: 'library.queryHelpRules' },
		comment: { keys: COMMENT_QUERY_KEYS, rules: 'comments.queryHelpRules' }
	};

	interface Ask {
		mode: 'search' | 'query';
		term: string;
		kind: QueryKind | null;
		memo: boolean;
	}
	interface Found {
		rows: SearchRow[];
		/** 쿼리면 코어가 뽑아 준 칠할 말. */
		terms: HitTerm[];
	}
	const latest = new LatestQuery<Ask, Found>(async (a) => {
		if (a.mode === 'search') {
			const fields: Field[] = ['body', 'comment', 'attachment'];
			if (a.memo) fields.push('memo');
			const hits = await searchApi.enhanced(a.term, fields);
			return { rows: sortRows(hits), terms: [] };
		}
		const of = a.kind === 'book' ? 'library' : (a.kind ?? 'quest');
		const [rows, terms] = await Promise.all([queryRows(a), queryApi.highlights(a.term, of)]);
		return { rows, terms };
	});
	async function queryRows(a: Ask): Promise<SearchRow[]> {
		const plain = (kind: SearchKind, id: string, title: string): SearchRow => ({
			kind,
			id,
			title,
			matched_in: [],
			excerpt: ''
		});
		switch (a.kind) {
			case 'quest':
				return (await questsApi.matchQuery(a.term)).map((q) => plain('quest', q.quest_id, q.title));
			case 'campaign':
				return (await campaignsApi.matchQuery(a.term)).map((c) =>
					plain('campaign', c.campaign_slug, c.title)
				);
			case 'book':
				return (await libraryApi.matchQuery(a.term)).map((b) => plain('book', b.book_id, b.title));
			case 'comment':
				// 최근 것이 먼저 — 한 문서에 여러 개가 맞아도 한 줄씩이다.
				return (await globalCommentsApi.matchQuery(a.term)).reverse().map((c) => ({
					kind: c.scope,
					id: c.slug,
					title: `#${c.entry_id} · ${c.author} · ${c.ts.slice(0, 10)}`,
					matched_in: ['comment'],
					excerpt: c.body.slice(0, 200),
					comment: c.entry_id
				}));
			default:
				return [];
		}
	}
	onDestroy(() => latest.cancel());

	// 검색어 · 모드 · 종류(쿼리일 때만) · 메모가 바뀌면 다시 묻는다. 검색은 종류와 상관없이 한 번에 받아
	// 탭에서 나눈다 — 탭을 옮겨도 다시 묻지 않고, 탭마다 개수가 보인다.
	$effect(() => {
		const ask: Ask = {
			mode,
			term,
			kind: mode === 'query' && canQuery(tab) ? tab : null,
			memo
		};
		const key = JSON.stringify(ask);
		if (!ask.term) {
			latest.cancel();
			rows = [];
			rowsFor = key;
			loading = false;
			error = null;
			return;
		}
		loading = true;
		latest.run(
			ask,
			(res) => {
				rows = res.rows;
				queryTerms = res.terms;
				rowsFor = key;
				loading = false;
				error = null;
			},
			(e) => {
				rows = [];
				rowsFor = key;
				loading = false;
				error = e instanceof Error ? e.message : String(e);
			}
		);
	});

	// 주소에 싣는다 — 새로고침 · 공유한 링크가 같은 결과를 연다.
	$effect(() => {
		const want = searchStateParams({ q: term, tab, mode, memo }).toString();
		const url = new URL(window.location.href);
		if (url.search.replace(/^\?/, '') === want) return;
		goto(`${url.pathname}${want ? `?${want}` : ''}`, {
			replaceState: true,
			keepFocus: true,
			noScroll: true
		});
	});

	// 팔레트처럼 `library:` · `quest:` 접두로 종류를 고른다 — 접두는 탭으로 옮기고 칸에는 나머지 글만 남긴다(admin).
	$effect(() => {
		if (mode !== 'search') return;
		const s = splitKindPrefix(search);
		if (!s.tab) return;
		tab = s.tab;
		search = s.rest;
	});

	const counts = $derived(countByTab(rows));
	const shown = $derived(rowsOfTab(rows, tab));
	/** 쿼리는 한 종류의 결과라 다른 탭의 개수는 모른다. */
	const countKnown = (k: SearchTab) => mode === 'search' || k === tab;

	/**
	 * 쿼리를 못 쓰는 탭('전체' · 규칙)으로 가면 검색으로 바꾼다 — 쓰던 글은 검색칸으로 옮긴다([쿼리] 단추와
	 * 같은 동작). 예전엔 [쿼리] 가 켜진 채 남아 "여기서는 못 씁니다" 안내만 떴다(admin).
	 */
	function selectTab(k: SearchTab) {
		if (mode === 'query' && !canQuery(k)) {
			search = query;
			query = '';
			mode = 'search';
			helpOpen = false;
		}
		tab = k;
	}

	function tabLabel(k: SearchTab): string {
		if (k === 'all') return t('search.tabAll', $locale);
		return k === 'comment' ? t('search.field.comment', $locale) : t(`kind.${k}`, $locale);
	}
</script>

<svelte:head>
	<title>{t('search.title', $locale)}</title>
</svelte:head>

<div class="wrap">
	<header>
		<h1>{t('search.title', $locale)}</h1>
	</header>

	<div class="bar">
		<SearchField
			bind:search
			bind:query
			bind:mode
			bind:helpOpen
			queryError={mode === 'query' ? error : null}
			placeholder={t('search.placeholder', $locale)}
			testids={{ search: 'doc-search', query: 'doc-query' }}
			queryDisabled={!canQuery(tab)}
			queryDisabledHint={t('search.queryNeedsKind', $locale)}
			grow
		>
			{#snippet options()}
				<label class="opt" title={t('search.memoHint', $locale)}>
					<input type="checkbox" bind:checked={memo} data-testid="doc-search-memo" />
					{t('search.memo', $locale)}
				</label>
			{/snippet}
		</SearchField>
	</div>
	{#if mode === 'query' && canQuery(tab)}
		<QueryNotes
			{error}
			open={helpOpen}
			keys={QUERY_KEYS_OF[tab].keys}
			rulesKey={QUERY_KEYS_OF[tab].rules}
			testid="doc-query"
		/>
	{/if}

	<div class="tabs" role="tablist" aria-label={t('search.tabsAria', $locale)}>
		{#each SEARCH_TABS as k (k)}
			<button
				type="button"
				role="tab"
				class="tab"
				class:on={tab === k}
				aria-selected={tab === k}
				data-testid="doc-search-tab-{k}"
				onclick={() => selectTab(k)}
			>
				{tabLabel(k)}
				{#if term && countKnown(k) && rowsFor}
					<span class="n">{counts[k]}</span>
				{/if}
			</button>
		{/each}
	</div>

	{#if !term}
		<p class="state">{t('search.hint', $locale)}</p>
	{:else if error && mode === 'search'}
		<p class="state error" role="alert">{error}</p>
	{:else if loading && shown.length === 0}
		<p class="state">{t('search.loading', $locale)}</p>
	{:else if shown.length === 0 && !error}
		<p class="state">{t('search.empty', $locale)}</p>
	{:else}
		<ul class="results" data-testid="doc-search-results" aria-busy={loading}>
			{#each shown as r (`${r.kind}:${r.id}:${r.comment ?? ''}`)}
				<li class="row">
					<DocLink
						kind={r.kind}
						id={r.id}
						slug={r.kind === 'rule' ? r.id : undefined}
						title={r.title}
						href={openHref(r, mode === 'search' ? term : '')}
						class="row-link"
					>
						<!-- 팔레트(SearchPalette)의 줄과 같은 칩 — 종류 칩 + 번호 칩. 규칙은 이름이 곧 번호라 번호
						     칩 없이 이름을 제목 자리에 둔다(팔레트와 같다). -->
						<span class="pills {r.kind}">
							<span class="ptype {r.kind}">{t(`kind.${r.kind}`, $locale)}</span>
							{#if r.kind !== 'rule'}
								<span class="pill mono xs pslug"><HitText text={r.id} terms={hitTerms} /></span>
							{/if}
						</span>
						<span class="title"
							><HitText text={r.kind === 'rule' ? r.id : r.title} terms={hitTerms} /></span
						>
						{#if r.matched_in.length}
							<span class="why">
								{#each r.matched_in as f (f)}
									<span class="f">{t(`search.field.${f}`, $locale)}</span>
								{/each}
							</span>
						{/if}
						<!-- 발췌도 링크 안 — 단추가 제목과 발췌를 합친 높이의 가운데에 온다(팔레트와 같다, admin). -->
						{#if r.excerpt}
							<span class="excerpt">
								<HitText text={r.excerpt} terms={hitTerms} />
							</span>
						{/if}
					</DocLink>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.wrap {
		max-width: var(--content-max-width, 1100px);
		margin: 0 auto;
		padding: var(--sp-24);
	}
	header {
		margin-bottom: var(--sp-16);
	}
	h1 {
		font-size: var(--fs-xl);
		font-weight: var(--fw-bold);
		color: var(--text-strong);
	}
	.bar {
		display: flex;
		gap: var(--sp-12);
		align-items: center;
	}
	.opt {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-4);
		font-size: var(--fs-sm);
		color: var(--text-muted);
		white-space: nowrap;
	}
	.tabs {
		display: flex;
		flex-wrap: wrap;
		gap: var(--sp-4);
		/* 아래 여백 없음 — 탭 밑줄이 첫 결과 줄의 위 구분선이 된다. 있으면 첫 줄만 위가 넓어 가운데가 안 맞아 보였다(admin). */
		margin: var(--sp-16) 0 0;
		border-bottom: var(--bw) solid var(--border);
	}
	.tab {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-6);
		padding: var(--sp-6) var(--sp-12);
		border: none;
		border-bottom: 2px solid transparent;
		background: none;
		color: var(--text-muted);
		font-size: var(--fs-md);
		cursor: pointer;
	}
	.tab.on {
		color: var(--text-strong);
		border-bottom-color: var(--accent);
	}
	.n {
		font-size: var(--fs-xs);
		padding: 0 var(--sp-6);
		border-radius: var(--r-pill, 999px);
		background: var(--bg-subtle);
		color: var(--text-muted);
	}
	.state {
		padding: var(--sp-32);
		text-align: center;
		color: var(--text-muted);
	}
	.state.error {
		color: var(--danger);
	}
	.results {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	/* 줄은 flex — DocLink 가 남는 폭을 다 써서 미리보기 · 새 창 · 이동 단추가 오른쪽 끝에 붙는다(팔레트 · 다른 목록과
	   같다, admin). 예전엔 블록이라 단추가 제목 바로 뒤에 붙었다. 단추는 줄 전체 높이의 가운데(DocLink 의 align-items). */
	.row {
		display: flex;
		padding: var(--sp-8) var(--sp-4);
		border-bottom: var(--bw) solid var(--border-subtle, var(--border));
	}
	.row :global(.row-link) {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--sp-4) var(--sp-8);
		color: var(--text-strong);
		text-decoration: none;
	}
	.row:hover .title {
		text-decoration: underline;
	}
	/* 칩 모양은 SearchPalette 의 .pills · .ptype · .pslug 와 같다 — 같은 검색 결과가 두 곳에서 달라 보이지
	   않게(REQ-040 다시 엶, admin). */
	.pills {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-6);
		flex: none;
		align-self: center;
	}
	.pills.quest {
		--kind-c: var(--accent);
	}
	.pills.campaign {
		--kind-c: var(--hl-pre);
	}
	.pills.rule {
		--kind-c: var(--success);
	}
	.pills.book {
		--kind-c: var(--warning);
	}
	.ptype {
		flex: none;
		min-width: 3.6rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		height: 1.375rem;
		padding: 0 var(--sp-6);
		box-sizing: border-box;
		font-size: var(--fs-sm);
		font-weight: var(--fw-semibold);
		line-height: var(--lh-tight);
		border-radius: var(--r-sm);
		border: var(--bw) solid transparent;
		color: var(--kind-c);
		background: color-mix(in srgb, var(--kind-c) 14%, transparent);
	}
	.pill.pslug {
		--pill-fs: 0.72rem;
		--pill-h: 1.375rem;
		--c: var(--kind-c, var(--text-muted));
		flex: none;
	}
	.title {
		font-weight: var(--fw-medium, 500);
	}
	.why {
		display: inline-flex;
		gap: var(--sp-4);
	}
	.f {
		font-size: var(--fs-xs);
		padding: 0 var(--sp-6);
		border-radius: var(--r-sm);
		background: var(--bg-subtle);
		color: var(--text-muted);
	}
	.excerpt {
		flex-basis: 100%;
		margin: 0;
		font-weight: normal;
		font-size: var(--fs-sm);
		color: var(--text-muted);
		overflow-wrap: anywhere;
	}
</style>
