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

	// DEV-033: 고급 행 접기. 필터 활성이면 라벨에 표시.
	let advancedOpen = $state(false);
	let advancedActive = $derived(
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
		<button class:active={statusIds.size === 0} onclick={() => (statusIds = new Set())}>All</button>
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

	<div class="divider"></div>

	<!-- DEV-037: 검색 -->
	<div class="search-group">
		<label class="search-input-wrap">
			<span class="sr-only">{t('filter.search', $locale)}</span>
			<input
				type="search"
				class="search-input"
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
	</div>

	<!-- DEV-033: 고급 필터 토글. -->
	<button
		class="adv-toggle"
		class:active={advancedActive}
		onclick={() => (advancedOpen = !advancedOpen)}
		aria-expanded={advancedOpen}>{advancedOpen ? '▾' : '▸'} {t('filter.advanced', $locale)}{advancedActive ? ' ●' : ''}</button
	>
</div>

{#if advancedOpen}
	<div class="adv-bar">
		<!-- urgency 다중 -->
		<div class="filter-group" aria-label={t('filter.urgency', $locale)}>
			{#each [1, 2, 3, 4] as u (u)}
				<button
					class:active={urgencies.has(u)}
					style:--c={urgencyColor(u)}
					onclick={() => (urgencies = toggle(urgencies, u))}>{urgencyLabel(u)}</button
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
			>{t('filter.created', $locale)} <DateField bind:value={createdAfter} /> ~
			<DateField bind:value={createdBefore} /></label
		>
		<label class="date-range"
			>{t('filter.updated', $locale)} <DateField bind:value={updatedAfter} /> ~
			<DateField bind:value={updatedBefore} /></label
		>
		{#if advancedActive}
			<button class="adv-clear" onclick={clearAdvanced} title={t('filter.clearAdvanced', $locale)}>{t('filter.clearBtn', $locale)}</button>
		{/if}
	</div>
{/if}

<style>
	.filter-bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		/* DEV-086: 우측 New Quest 플로팅 버튼 자리 확보 (보드 toolbar 와 동일
		   위치). 필터가 wrap 돼도 버튼 밑으로 안 들어가게 padding-right 예약. */
		padding: 0.75rem 130px 0.75rem 1.5rem;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--bg-subtle);
		flex-wrap: wrap;
	}

	.filter-group {
		display: flex;
		gap: 0.25rem;
		flex-wrap: wrap;
	}

	.divider {
		width: 1px;
		height: 20px;
		background: var(--bg-subtle);
	}

	button {
		padding: 0.25rem 0.65rem;
		border: 1px solid var(--border);
		border-radius: 20px;
		background: transparent;
		color: var(--text-muted);
		font-size: 0.8rem;
		cursor: pointer;
		transition: all 0.15s;
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
		gap: 0.5rem;
		flex-wrap: wrap;
	}
	.search-input-wrap {
		position: relative;
		display: inline-flex;
		align-items: center;
	}
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}
	.search-input {
		padding: 0.3rem 1.8rem 0.3rem 0.7rem;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.8rem;
		min-width: 12.5rem; /* BUG-064 */
		outline: none;
		transition: border-color 0.15s;
	}
	.search-input:focus {
		border-color: var(--accent);
	}
	.search-input::-webkit-search-cancel-button {
		display: none;
	}
	.search-clear {
		position: absolute;
		right: 0.3rem;
		padding: 0 0.4rem;
		border: none;
		border-radius: 12px;
		background: transparent;
		color: var(--text-faint);
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
	}
	.search-clear:hover {
		color: var(--danger);
		background: transparent;
	}
	.search-opt {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.8rem;
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

	/* --- DEV-033: 고급 필터 --- */
	.adv-toggle {
		border-style: dashed;
		color: var(--text-faint);
	}
	.adv-toggle.active {
		color: var(--accent);
		border-color: var(--accent);
		background: transparent;
	}
	.adv-bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
		padding: 0.5rem 130px 0.5rem 1.5rem;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--bg-subtle);
	}
	.adv-bar button {
		padding: 0.25rem 0.65rem;
		border: 1px solid var(--border);
		border-radius: 20px;
		background: transparent;
		color: var(--text-muted);
		font-size: 0.8rem;
		cursor: pointer;
	}
	.adv-bar button.active {
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
		gap: 0.3rem;
		font-size: 0.78rem;
		color: var(--text-muted);
	}
	.adv-clear {
		color: var(--danger);
		border-color: color-mix(in srgb, var(--danger) 35%, transparent);
	}
</style>
