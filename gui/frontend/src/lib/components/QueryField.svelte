<!--
  DEV-452 (REQ-039): 쿼리 칸 — 입력 · 지우기 · 틀리면 빨간 칸과 이유 · `?` 쓰는 법.
  퀘스트 목록(DEV-447)에 있던 것을 떼어 보드 · 캠페인 · 도서관이 같이 쓴다. 해석은 코어가 하고
  여기는 글과 오류만 다룬다 — 대상마다 다른 것은 `keys`(쓰는 법에 보일 키와 예)뿐이다.

  뿌리 요소가 셋(줄 · 오류 · 쓰는 법)이라 부모가 flex-wrap 이면 각각 한 줄을 다 쓴다.
-->
<script lang="ts">
	import { locale, t } from '$lib/stores/locale';

	let {
		value = $bindable(''),
		error = null,
		keys,
		rulesKey = 'filter.queryHelpRules',
		placeholderKey = 'filter.queryPlaceholder',
		testid = 'quest-query'
	}: {
		value?: string;
		error?: string | null;
		/** [키, 예, 설명 번역 키] — `?` 를 누르면 이 순서로 보인다. */
		keys: readonly (readonly [string, string, string])[];
		rulesKey?: string;
		placeholderKey?: string;
		/** 시험용 이름 앞부분 — `{testid}-input` · `-clear` · `-error` · `-help`. */
		testid?: string;
	} = $props();

	let helpOpen = $state(false);
</script>

<div class="query-row">
	<label class="query-wrap">
		<span class="query-label">{t('filter.queryLabel', $locale)}</span>
		<input
			type="search"
			class="input query-input"
			class:bad={error}
			placeholder={t(placeholderKey, $locale)}
			bind:value
			spellcheck="false"
			autocomplete="off"
			aria-invalid={error ? 'true' : undefined}
			data-testid="{testid}-input"
		/>
		{#if value}
			<button
				type="button"
				class="query-clear"
				title={t('filter.clearQuery', $locale)}
				onclick={() => (value = '')}
				data-testid="{testid}-clear">×</button
			>
		{/if}
	</label>
	<button
		type="button"
		class="query-help-btn"
		class:active={helpOpen}
		aria-expanded={helpOpen}
		title={t('filter.queryHelpToggle', $locale)}
		onclick={() => (helpOpen = !helpOpen)}>?</button
	>
</div>
{#if error}
	<p class="query-error" role="alert" data-testid="{testid}-error">{error}</p>
{/if}
{#if helpOpen}
	<div class="query-help" data-testid="{testid}-help">
		<p>{t(rulesKey, $locale)}</p>
		<dl>
			{#each keys as [key, example, label] (key)}
				<dt><code>{example}</code></dt>
				<dd>{t(label, $locale)}</dd>
			{/each}
		</dl>
	</div>
{/if}

<style>
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
	/* 목록의 검색칸 지우기 ×와 같은 모양 — 고급 패널 안에서 보이던 크기 그대로. */
	.query-clear {
		position: absolute;
		right: 0.3rem;
		padding: 0 var(--sp-6);
		border: none;
		border-radius: var(--r-pill);
		background: transparent;
		color: var(--text-faint);
		font-size: var(--fs-sm);
		line-height: var(--lh-tight);
		cursor: pointer;
	}
	.query-clear:hover {
		color: var(--danger);
	}
	/* 고급 패널의 알약 버튼과 같은 모양(퀘스트 목록에서 보이던 것). */
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
</style>
