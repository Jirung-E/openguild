<!--
  DEV-452 (REQ-039): 검색칸 하나 — [검색 | 쿼리] 를 골라 쓴다(admin: "쿼리 입력란을 따로 둘 게 아니라
  검색할 때 옵션으로 고르게"). 퀘스트 목록 · 보드 필터 · 캠페인 · 도서관이 같이 쓴다.

  - 검색: 지금까지의 검색. 옵션(제목만 · 댓글 · 첨부 이름)은 `options` 로 받아 이 모드에서만 보인다.
  - 쿼리: `status:open -tag:wip` 같은 `키:값` — 해석은 코어가 한다. 틀리면 칸이 빨개지고, 이유와 쓰는 법은
    [`QueryNotes`] 가 보여 준다(화면마다 놓을 자리가 달라 따로 뒀다).

  모드를 바꾸면 쓰던 글을 그대로 옮기고 다른 쪽은 비운다 — 키 없는 낱말은 쿼리에서도 검색이라 대개 그대로
  통하고, 안 보이는 칸이 몰래 거르는 일이 없다.
-->
<script lang="ts" module>
	export type SearchMode = 'search' | 'query';
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import { locale, t } from '$lib/stores/locale';

	let {
		search = $bindable(''),
		query = $bindable(''),
		mode = $bindable<SearchMode>('search'),
		helpOpen = $bindable(false),
		queryError = null,
		placeholder,
		queryPlaceholderKey = 'filter.queryPlaceholder',
		/** 시험용 이름 앞부분 — 검색은 `{search}-input`, 쿼리는 `{query}-input` 처럼. */
		testids = { search: 'quest-search', query: 'quest-query' },
		/** 한 줄을 다 쓴다(도서관 · 캠페인). 아니면 최소 폭만(퀘스트 목록 필터 줄). */
		grow = false,
		options
	}: {
		search?: string;
		query?: string;
		mode?: SearchMode;
		helpOpen?: boolean;
		queryError?: string | null;
		placeholder?: string;
		queryPlaceholderKey?: string;
		testids?: { search: string; query: string };
		grow?: boolean;
		options?: Snippet;
	} = $props();

	function setMode(next: SearchMode) {
		if (next === mode) return;
		// 쓰던 글을 그대로 옮기고 다른 쪽은 비운다 — 안 보이는 칸이 몰래 거르지 않게(칸은 하나다).
		if (next === 'query') {
			query = search;
			search = '';
		} else {
			search = query;
			query = '';
		}
		mode = next;
		if (next === 'search') helpOpen = false;
	}

	const tid = $derived(mode === 'search' ? testids.search : testids.query);
</script>

<div class="search-field" class:grow>
	<div class="seg" role="radiogroup" aria-label={t('filter.searchModeAria', $locale)}>
		<button
			type="button"
			role="radio"
			aria-checked={mode === 'search'}
			class:on={mode === 'search'}
			onclick={() => setMode('search')}
			data-testid="{testids.search}-mode">{t('filter.modeSearch', $locale)}</button
		>
		<button
			type="button"
			role="radio"
			aria-checked={mode === 'query'}
			class:on={mode === 'query'}
			title={t('filter.modeQueryHint', $locale)}
			onclick={() => setMode('query')}
			data-testid="{testids.query}-mode">{t('filter.modeQuery', $locale)}</button
		>
	</div>
	<label class="search-input-wrap">
		<span class="sr-only"
			>{mode === 'search' ? t('filter.search', $locale) : t('filter.queryLabel', $locale)}</span
		>
		<input
			type="search"
			class="input search-input"
			class:mono={mode === 'query'}
			class:bad={mode === 'query' && queryError}
			placeholder={mode === 'search' ? placeholder : t(queryPlaceholderKey, $locale)}
			bind:value={
				() => (mode === 'search' ? search : query),
				(v) => (mode === 'search' ? (search = v) : (query = v))
			}
			spellcheck="false"
			autocomplete="off"
			aria-invalid={mode === 'query' && queryError ? 'true' : undefined}
			data-testid="{tid}-input"
		/>
		{#if mode === 'search' ? search : query}
			<button
				type="button"
				class="search-clear"
				title={mode === 'search'
					? t('filter.clearSearch', $locale)
					: t('filter.clearQuery', $locale)}
				onclick={() => (mode === 'search' ? (search = '') : (query = ''))}
				data-testid="{tid}-clear">×</button
			>
		{/if}
	</label>
	{#if mode === 'query'}
		<button
			type="button"
			class="query-help-btn"
			class:active={helpOpen}
			aria-expanded={helpOpen}
			title={t('filter.queryHelpToggle', $locale)}
			onclick={() => (helpOpen = !helpOpen)}>?</button
		>
	{:else}
		{@render options?.()}
	{/if}
</div>

<style>
	.search-field {
		display: flex;
		align-items: center;
		gap: var(--sp-8);
		flex-wrap: wrap;
		min-width: 0;
	}
	.search-field.grow {
		flex: 1 1 100%;
	}
	.search-input-wrap {
		position: relative;
		display: inline-flex;
		align-items: center;
		min-width: 0;
	}
	/* 좁은 사이드바에서도 [검색 | 쿼리] 와 같은 줄에 남게 — 옵션은 다음 줄로 넘어간다. */
	.grow .search-input-wrap {
		flex: 1 1 8rem;
	}
	/* 스크린리더 전용 — 화면에는 안 보인다. BUG-256: 이 1px 은 "보이지 않게
	   만드는" 관용구지 치수가 아니라서 배율을 따라갈 이유가 없다(가드의
	   ALLOW 에 등록). */
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: calc(-1 * var(--bw));
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}
	/* `.input` 그대로 — 오른쪽만 지우기 × 자리를 둔다. 위아래 여백을 따로 줄이면 옆의 [검색 | 쿼리] 보다
	   낮아진다(예전 퀘스트 목록 검색칸이 그랬다 — 30px 대 34px). */
	.search-input {
		padding-right: var(--sp-32);
		min-width: 12.5rem;
	}
	.grow .search-input {
		flex: 1 1 auto;
		width: 100%;
		min-width: 0;
	}
	.search-input.mono {
		font-family: var(--font-mono);
	}
	.search-input.bad {
		border-color: var(--danger);
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
	.query-help-btn {
		padding: var(--sp-2) var(--sp-8);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-pill);
		background: transparent;
		color: var(--text-muted);
		font-size: var(--fs-sm);
		font-weight: var(--fw-semibold);
		cursor: pointer;
	}
	.query-help-btn.active {
		background: color-mix(in srgb, var(--accent) 20%, transparent);
		border-color: var(--accent);
		color: var(--accent);
	}
</style>
