<!--
  DEV-452: 쿼리의 오류 한 줄과 쓰는 법 — [`SearchField`] 가 쿼리 모드일 때. 화면마다 놓을 자리가 달라
  (퀘스트 목록은 필터 줄 아래 한 줄, 도서관은 검색 줄 아래) 칸과 따로 둔다. 둘 다 없으면 아무것도 안 그린다.
-->
<script lang="ts">
	import { locale, t } from '$lib/stores/locale';

	let {
		error = null,
		open = false,
		keys,
		rulesKey = 'filter.queryHelpRules',
		testid = 'quest-query'
	}: {
		error?: string | null;
		open?: boolean;
		/** [키, 예, 설명 번역 키] — 쓰는 법에 이 순서로 보인다. */
		keys: readonly (readonly [string, string, string])[];
		rulesKey?: string;
		testid?: string;
	} = $props();
</script>

{#if error || open}
	<div class="query-notes">
		{#if error}
			<p class="query-error" role="alert" data-testid="{testid}-error">{error}</p>
		{/if}
		{#if open}
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
	</div>
{/if}

<style>
	.query-notes {
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
		flex: 1 1 100%;
		min-width: 0;
	}
	.query-error {
		margin: 0;
		font-size: var(--fs-sm);
		color: var(--danger);
	}
	.query-help {
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
