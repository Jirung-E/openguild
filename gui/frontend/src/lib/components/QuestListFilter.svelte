<script lang="ts">
	import type { QuestStatus, QuestType } from '$lib/types';
	import { urgencyLabel, urgencyColor } from '$lib/types';
	// DEV-205 모듈3: 필터 문자열 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-015: status 표시 이름 — 언어 반응.
	import { statusLabel } from '$lib/utils/status-label';
	// DEV-205: 언어 반응 날짜 입력(네이티브 date 대체).
	import DateField from './DateField.svelte';
	import type { TriState } from '$lib/utils/quest-list';

	let {
		types,
		statuses,
		typeIds = $bindable(new Set<number>()),
		statusIds = $bindable(new Set<number>()),
		search = $bindable(''),
		titleOnly = $bindable(false),
		// REQ-010: 검색 영역 확장.
		searchComments = $bindable(false),
		searchAttachments = $bindable(false),
		// DEV-447 (REQ-039): 쿼리 문법 — 판정은 부모(QuestList)가 코어에 맡긴다.
		query = $bindable(''),
		queryError = null,
		// DEV-033: 고급 필터.
		urgencies = $bindable(new Set<number>()),
		prereqState = $bindable('any'),
		subState = $bindable('any'),
		createdAfter = $bindable(''),
		createdBefore = $bindable(''),
		updatedAfter = $bindable(''),
		updatedBefore = $bindable('')
	}: {
		types: QuestType[];
		statuses: QuestStatus[];
		typeIds: Set<number>;
		statusIds: Set<number>;
		search?: string;
		titleOnly?: boolean;
		searchComments?: boolean;
		searchAttachments?: boolean;
		query?: string;
		queryError?: string | null;
		urgencies?: Set<number>;
		prereqState?: TriState;
		subState?: TriState;
		createdAfter?: string;
		createdBefore?: string;
		updatedAfter?: string;
		updatedBefore?: string;
	} = $props();

	function toggle(set: Set<number>, id: number): Set<number> {
		const next = new Set(set);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		return next;
	}

	// BUG-194: 좁은 화면에선 타입/상태 칩만으로도 화면 절반을 먹어 목록이 거의
	// 안 보였다(admin 보고). 폭이 좁으면 칩 줄을 접고 '필터' 토글로 여닫는다.
	// 검색은 가장 자주 쓰므로 접힘과 무관하게 항상 보인다.
	let narrow = $state(false);
	let chipsOpen = $state(false);
	$effect(() => {
		if (typeof window === 'undefined' || !window.matchMedia) return;
		const mq = window.matchMedia('(max-width: 640px)');
		const sync = () => (narrow = mq.matches);
		sync();
		mq.addEventListener('change', sync);
		return () => mq.removeEventListener('change', sync);
	});
	/** 접힘 상태에서 보여줄 '몇 개 걸려 있는지'. 0 이면 표시 안 함. */
	const activeChipCount = $derived(typeIds.size + statusIds.size);

	// DEV-447: 쿼리 도움말 — 키 목록. 값의 예는 CLI `quest list -q` 와 같다.
	let queryHelpOpen = $state(false);
	const QUERY_KEYS = [
		['status', 'status:open,in_progress', 'filter.queryHelpStatus'],
		['type', 'type:BUG', 'filter.queryHelpType'],
		['tag', '-tag:wip', 'filter.queryHelpTag'],
		['campaign', 'campaign:C-001', 'filter.queryHelpCampaign'],
		['urgency', 'urgency:<=2', 'filter.queryHelpUrgency'],
		['due', 'due:overdue', 'filter.queryHelpDue'],
		['parent', 'parent:none', 'filter.queryHelpParent'],
		['commented', 'commented:>=2026-10-20', 'filter.queryHelpCommented']
	] as const;

	// DEV-033: 고급 행 접기. 필터 활성이면 라벨에 표시.
	let advancedOpen = $state(false);
	// DEV-447: 쿼리가 틀렸으면 고급을 연다 — 접힌 채면 목록이 왜 안 걸러지는지 알 길이 없다(주소로 받은 쿼리 등).
	$effect(() => {
		if (queryError) advancedOpen = true;
	});
	let advancedActive = $derived(
		query.trim() !== '' ||
			urgencies.size > 0 ||
			prereqState !== 'any' ||
			subState !== 'any' ||
			createdAfter !== '' ||
			createdBefore !== '' ||
			updatedAfter !== '' ||
			updatedBefore !== ''
	);

	const TRI_LABEL: Record<TriState, string> = $derived({
		any: t('filter.triAny', $locale),
		has: t('filter.triHas', $locale),
		none: t('filter.triNone', $locale)
	});
	function cycleTri(cur: TriState): TriState {
		return cur === 'any' ? 'has' : cur === 'has' ? 'none' : 'any';
	}
	function clearAdvanced() {
		query = '';
		urgencies = new Set();
		prereqState = 'any';
		subState = 'any';
		createdAfter = '';
		createdBefore = '';
		updatedAfter = '';
		updatedBefore = '';
	}
</script>

<div class="filter-bar">
	<!-- BUG-194: 좁은 화면 — 칩 줄을 접고 토글로. -->
	{#if narrow}
		<button
			class="xfilter-toggle"
			class:active={activeChipCount > 0}
			onclick={() => (chipsOpen = !chipsOpen)}
			aria-expanded={chipsOpen}
			>{chipsOpen ? '▾' : '▸'}
			{t('filter.chips', $locale)}{activeChipCount > 0 ? ` (${activeChipCount})` : ''}</button
		>
	{/if}
	{#if !narrow || chipsOpen}
		<div class="filter-group">
			<button class:active={typeIds.size === 0} onclick={() => (typeIds = new Set())}>All</button>
			{#each types as t}
				<button
					class:active={typeIds.has(t.id)}
					style:--c={t.color}
					onclick={() => (typeIds = toggle(typeIds, t.id))}
				>
					{t.prefix}
				</button>
			{/each}
		</div>

		<div class="divider"></div>

		<div class="filter-group">
			<button class:active={statusIds.size === 0} onclick={() => (statusIds = new Set())}
				>All</button
			>
			{#each statuses as s}
				<button
					class:active={statusIds.has(s.id)}
					style:--c={s.color}
					onclick={() => (statusIds = toggle(statusIds, s.id))}
				>
					{statusLabel(s, $locale)}
				</button>
			{/each}
		</div>
	{/if}

	<div class="divider"></div>

	<!-- DEV-037: 검색 -->
	<div class="search-group">
		<label class="search-input-wrap">
			<span class="sr-only">{t('filter.search', $locale)}</span>
			<input
				type="search"
				class="input search-input"
				placeholder={t('filter.searchPlaceholder', $locale)}
				bind:value={search}
				data-testid="quest-search-input"
			/>
			{#if search}
				<button
					type="button"
					class="search-clear"
					title={t('filter.clearSearch', $locale)}
					onclick={() => (search = '')}
					data-testid="quest-search-clear">×</button
				>
			{/if}
		</label>
		<label class="search-opt">
			<input type="checkbox" bind:checked={titleOnly} data-testid="quest-search-title-only" />
			<span>{t('filter.titleOnly', $locale)}</span>
		</label>
		<!-- REQ-010: 검색 영역을 넓히는 옵션. titleOnly 가 좁히는 것과 방향이 반대라
		     나란히 두되, 켜지 않으면 예전과 완전히 같은 결과가 나온다. -->
		<label class="search-opt">
			<input
				type="checkbox"
				bind:checked={searchComments}
				data-testid="quest-search-comments"
			/>
			<span>{t('filter.searchComments', $locale)}</span>
		</label>
		<label class="search-opt">
			<input
				type="checkbox"
				bind:checked={searchAttachments}
				data-testid="quest-search-attachments"
			/>
			<span>{t('filter.searchAttachments', $locale)}</span>
		</label>
	</div>

	<!-- DEV-033: 고급 필터 토글. -->
	<button
		class="xfilter-toggle"
		class:active={advancedActive}
		onclick={() => (advancedOpen = !advancedOpen)}
		aria-expanded={advancedOpen}
		>{advancedOpen ? '▾' : '▸'} {t('filter.advanced', $locale)}{advancedActive ? ' ●' : ''}</button
	>
</div>

{#if advancedOpen}
	<div class="xfilter-panel">
		<!-- urgency 다중 -->
		<div class="filter-group" aria-label={t('filter.urgency', $locale)}>
			{#each [1, 2, 3, 4] as u (u)}
				<button
					class:active={urgencies.has(u)}
					style:--c={urgencyColor(u)}
					onclick={() => (urgencies = toggle(urgencies, u))}>{urgencyLabel(u, $locale)}</button
				>
			{/each}
		</div>
		<div class="divider"></div>
		<!-- prereq / sub tri-state -->
		<button
			class="tri"
			class:active={prereqState !== 'any'}
			onclick={() => (prereqState = cycleTri(prereqState))}
			title={t('filter.prereqTitle', $locale)}
		>
			{t('filter.prereqLabel', $locale)}: {TRI_LABEL[prereqState]}
		</button>
		<button
			class="tri"
			class:active={subState !== 'any'}
			onclick={() => (subState = cycleTri(subState))}
			title={t('filter.subTitle', $locale)}
		>
			{t('filter.subLabel', $locale)}: {TRI_LABEL[subState]}
		</button>
		<div class="divider"></div>
		<!-- 날짜 범위 -->
		<label class="date-range"
			>{t('filter.created', $locale)}
			<DateField bind:value={createdAfter} /> ~
			<DateField bind:value={createdBefore} /></label
		>
		<label class="date-range"
			>{t('filter.updated', $locale)}
			<DateField bind:value={updatedAfter} /> ~
			<DateField bind:value={updatedBefore} /></label
		>
		{#if advancedActive}
			<button class="xfilter-clear" onclick={clearAdvanced} title={t('filter.clearAdvanced', $locale)}
				>{t('filter.clearBtn', $locale)}</button
			>
		{/if}
		<!-- DEV-447 (REQ-039): 쿼리 — 고급 안에서 줄 하나를 다 쓴다(값이 길다; admin: 고급에 넣는다). -->
		<div class="query-row">
			<label class="query-wrap">
				<span class="query-label">{t('filter.queryLabel', $locale)}</span>
				<input
					type="search"
					class="input query-input"
					class:bad={queryError}
					placeholder={t('filter.queryPlaceholder', $locale)}
					bind:value={query}
					spellcheck="false"
					autocomplete="off"
					aria-invalid={queryError ? 'true' : undefined}
					data-testid="quest-query-input"
				/>
				{#if query}
					<button
						type="button"
						class="search-clear"
						title={t('filter.clearQuery', $locale)}
						onclick={() => (query = '')}
						data-testid="quest-query-clear">×</button
					>
				{/if}
			</label>
			<button
				type="button"
				class="query-help-btn"
				class:active={queryHelpOpen}
				aria-expanded={queryHelpOpen}
				title={t('filter.queryHelpToggle', $locale)}
				onclick={() => (queryHelpOpen = !queryHelpOpen)}>?</button
			>
		</div>
		{#if queryError}
			<p class="query-error" role="alert" data-testid="quest-query-error">{queryError}</p>
		{/if}
		{#if queryHelpOpen}
			<div class="query-help" data-testid="quest-query-help">
				<p>{t('filter.queryHelpRules', $locale)}</p>
				<dl>
					{#each QUERY_KEYS as [key, example, label] (key)}
						<dt><code>{example}</code></dt>
						<dd>{t(label, $locale)}</dd>
					{/each}
				</dl>
			</div>
		{/if}
	</div>
{/if}

<style>
	.filter-bar {
		display: flex;
		align-items: center;
		gap: var(--sp-12);
		/* DEV-086: 우측 New Quest 플로팅 버튼 자리 확보 (보드 toolbar 와 동일
		   위치). 필터가 wrap 돼도 버튼 밑으로 안 들어가게 padding-right 예약. */
		padding: var(--sp-12) 8.125rem var(--sp-12) var(--sp-24);
		background: var(--bg-elevated);
		border-bottom: var(--bw) solid var(--bg-subtle);
		flex-wrap: wrap;
	}
	/* BUG-194: 좁은 화면에선 세로 공간이 귀하다 — 여백을 줄이고 검색은 한 줄 전체. */
	@media (max-width: 640px) {
		.filter-bar {
			gap: var(--sp-8);
			padding: var(--sp-8) var(--sp-12);
		}
		.filter-bar .search-group {
			flex: 1 1 100%;
		}
		.filter-bar .divider {
			display: none;
		}
	}

	.filter-group {
		display: flex;
		gap: var(--sp-4);
		flex-wrap: wrap;
	}

	.divider {
		width: 1px;
		height: 1.25rem;
		background: var(--bg-subtle);
	}

	button {
		padding: var(--sp-4) var(--sp-12);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-pill);
		background: transparent;
		color: var(--text-muted);
		font-size: var(--fs-sm);
		cursor: pointer;
		transition: all var(--dur-fast);
	}

	button:hover {
		border-color: var(--text-muted);
		color: var(--text);
	}

	button.active {
		background: color-mix(in srgb, var(--c, var(--accent)) 20%, transparent);
		border-color: var(--c, var(--accent));
		color: var(--c, var(--accent));
	}

	/* --- 검색 영역 --- */
	.search-group {
		display: flex;
		align-items: center;
		gap: var(--sp-8);
		flex-wrap: wrap;
	}
	.search-input-wrap {
		position: relative;
		display: inline-flex;
		align-items: center;
	}
	/* 스크린리더 전용 — 화면에는 안 보인다. BUG-256: 이 1px 은 "보이지 않게
	   만드는" 관용구지 치수가 아니라서 배율을 따라갈 이유가 없다(가드의
	   ALLOW 에 등록). */
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		/* BUG-256: 테두리 겹침 보정이라 rem 이 아니라 **테두리 두께 토큰**이
		   맞다 — 상쇄해야 할 대상이 `--bw` 이기 때문이다. rem 으로 두면
		   배율에 따라 상쇄량이 테두리와 어긋난다. */
		margin: calc(-1 * var(--bw));
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}
	.search-input {
		padding: var(--sp-4) var(--sp-32) var(--sp-4) var(--sp-12);
		min-width: 12.5rem;
		transition: border-color var(--dur-fast);
	}
	.search-input::-webkit-search-cancel-button {
		display: none;
	}
	.search-clear {
		position: absolute;
		right: 0.3rem;
		padding: 0 var(--sp-6);
		border: none;
		border-radius: var(--r-pill);
		background: transparent;
		color: var(--text-faint);
		font-size: 1rem; /* token-ok: × 글리프 크기 */
		line-height: var(--lh-tight);
		cursor: pointer;
	}
	.search-clear:hover {
		color: var(--danger);
		background: transparent;
	}
	.search-opt {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-4);
		font-size: var(--fs-sm);
		color: var(--text-muted);
		cursor: pointer;
		user-select: none;
	}
	.search-opt input {
		cursor: pointer;
	}
	.search-opt:hover {
		color: var(--text);
	}

	/* --- DEV-447: 쿼리 --- */
	.query-row {
		flex: 1 1 100%;
		display: flex;
		align-items: center;
		gap: var(--sp-8);
	}
	.query-wrap {
		position: relative;
		flex: 1 1 auto;
		display: flex;
		align-items: center;
		gap: var(--sp-8);
		min-width: 0;
	}
	.query-label {
		font-size: var(--fs-sm);
		color: var(--text-muted);
		white-space: nowrap;
	}
	.query-input {
		flex: 1 1 auto;
		min-width: 0;
		padding: var(--sp-4) var(--sp-32) var(--sp-4) var(--sp-12);
		font-family: var(--font-mono);
		font-size: var(--fs-sm);
		transition: border-color var(--dur-fast);
	}
	.query-input::-webkit-search-cancel-button {
		display: none;
	}
	.query-input.bad {
		border-color: var(--danger);
	}
	/* 고급 패널의 알약 버튼 규칙(.xfilter-panel button)보다 세게 — 지우기 ×는 테두리 없이. */
	.xfilter-panel .search-clear {
		padding: 0 var(--sp-6);
		border: none;
		border-radius: var(--r-pill);
		background: transparent;
		color: var(--text-faint);
	}
	.xfilter-panel .search-clear:hover {
		color: var(--danger);
	}
	.xfilter-panel .query-help-btn {
		padding: var(--sp-2) var(--sp-8);
		font-weight: var(--fw-semibold);
	}
	.query-error {
		flex: 1 1 100%;
		margin: 0;
		font-size: var(--fs-sm);
		color: var(--danger);
	}
	.query-help {
		flex: 1 1 100%;
		font-size: var(--fs-sm);
		color: var(--text-muted);
	}
	.query-help p {
		margin: 0 0 var(--sp-4);
	}
	.query-help dl {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: var(--sp-2) var(--sp-12);
		margin: 0;
	}
	.query-help dd {
		margin: 0;
	}
	.query-help code {
		font-family: var(--font-mono);
		color: var(--text);
	}

	/* --- DEV-033: 고급 필터 --- */
	.xfilter-toggle {
		border-style: dashed;
		color: var(--text-faint);
	}
	.xfilter-toggle.active {
		color: var(--accent);
		border-color: var(--accent);
		background: transparent;
	}
	.xfilter-panel {
		display: flex;
		align-items: center;
		gap: var(--sp-12);
		flex-wrap: wrap;
		padding: var(--sp-8) 8.125rem var(--sp-8) var(--sp-24);
		background: var(--bg-elevated);
		border-bottom: var(--bw) solid var(--bg-subtle);
	}
	.xfilter-panel button {
		padding: var(--sp-4) var(--sp-12);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-pill);
		background: transparent;
		color: var(--text-muted);
		font-size: var(--fs-sm);
		cursor: pointer;
	}
	.xfilter-panel button.active {
		background: color-mix(in srgb, var(--c, var(--accent)) 20%, transparent);
		border-color: var(--c, var(--accent));
		color: var(--c, var(--accent));
	}
	.tri.active {
		--c: var(--accent);
	}
	.date-range {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-4);
		font-size: var(--fs-sm);
		color: var(--text-muted);
	}
	.xfilter-clear {
		color: var(--danger);
		border-color: color-mix(in srgb, var(--danger) 35%, transparent);
	}

	/* BUG: .filter-bar 는 위쪽에 좁은 화면용 padding 축소 쿼리가 있는데 .xfilter-panel 는
	   빠져 있었음 — New Quest 버튼용 130px 우측 padding이 좁은 화면에서도 그대로
	   남아 실사용 폭이 거의 0으로 줄어 고급 필터 행이 사실상 안 보이는 원인이었다.
	   (base .xfilter-panel 규칙보다 반드시 뒤에 와야 캐스케이드에서 이김.) */
	@media (max-width: 640px) {
		.xfilter-panel {
			padding: var(--sp-8) var(--sp-12);
		}
	}
</style>
