<!--
  DEV-290: 규칙/BOOK 변경이력 섹션 — CampaignHistory.svelte(DEV-226) 와 동일 톤.
  이력은 DB 테이블이 아니라 `.guild/history/{id}.jsonl` 사이드카에서 직접 읽으며
  (로컬은 Tauri invoke, 원격은 GET /api/{rules|library}/{id}/history), op 은
  create/update/delete/rename. rename 만 old→new 를 가진다.
-->
<script lang="ts">
	import { rulesApi } from '$lib/api/rules';
	import { libraryApi } from '$lib/api/library';
	import type { SidecarHistoryEntry } from '$lib/types';
	import { formatTs, formatRelative } from '$lib/utils/datetime';
	import { locale, t } from '$lib/stores/locale';
	// REQ-004: 늦게 온 응답이 최신 화면을 덮지 않도록.
	import { Generation } from '$lib/utils/latest-only';

	let { kind, id }: { kind: 'rule' | 'book'; id: string } = $props();

	let entries = $state<SidecarHistoryEntry[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	// REQ-007: 기본 접힘. 변경 이력은 항상 필요한 정보가 아닌데 상세 페이지
	// 하단을 길게 차지한다. 접기 상태는 영속화하지 않는다 —
	// QuestNoteSection 의 collapsed 와 동일한 정책.
	let collapsed = $state(true);
	function toggleCollapsed() {
		collapsed = !collapsed;
	}

	// REQ-004: 늦게 온 응답이 화면을 덮지 않도록 세대 토큰으로 최신만 반영.
	const gen = new Generation();
	$effect(() => {
		const cur = id;
		if (!cur) return;
		const mine = gen.next();
		loading = true;
		error = null;
		const p = kind === 'rule' ? rulesApi.history(cur) : libraryApi.history(cur);
		p.then((list) => {
			if (!gen.isCurrent(mine)) return;
			entries = list;
		})
			.catch((e) => {
				if (!gen.isCurrent(mine)) return;
				error = e instanceof Error ? e.message : t('history.loadFailed', $locale);
			})
			.finally(() => {
				if (!gen.isCurrent(mine)) return;
				loading = false;
			});
	});
</script>

<section class="ch-section">
	<div class="section-head">
		<button
			type="button"
			class="section-toggle"
			onclick={toggleCollapsed}
			aria-expanded={!collapsed}
			title={collapsed ? t('history.expand', $locale) : t('history.collapse', $locale)}
		>
			<span class="toggle-icon" class:collapsed>▼</span>
			<h2 class="section-title">{t('history.title', $locale)}</h2>
		</button>
		{#if entries.length > 0}
			<span class="ch-count">{entries.length}</span>
		{/if}
	</div>

	{#if !collapsed}
	{#if loading}
		<p class="ch-state">{t('history.loading', $locale)}</p>
	{:else if error}
		<p class="ch-state error">{error}</p>
	{:else if entries.length === 0}
		<p class="ch-state">{t('history.empty', $locale)}</p>
	{:else}
		<ul class="ch-list" data-testid="sidecar-history-list">
			{#each entries as e, i (i)}
				<li class="ch-item" data-testid="ch-item">
					<time class="ch-ts" datetime={e.ts} title={formatTs(e.ts)}>
						{formatRelative(e.ts, undefined, $locale)}
					</time>
					<span class="ch-op">{e.op}</span>
					{#if e.old != null || e.new != null}
						<span class="ch-change">
							{e.old ?? '∅'} <span class="ch-arrow">→</span>
							{e.new ?? '∅'}
						</span>
					{:else}
						<span class="ch-op-empty" aria-hidden="true"></span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	{/if}
</section>

<style>
	.ch-section {
		margin-bottom: 1.5rem;
	}
	.section-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
	.section-title {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin: 0;
	}
	.ch-count {
		font-size: 0.72rem;
		color: var(--text-faint);
		padding: 0.05rem 0.4rem;
		border-radius: 10px;
		background: var(--bg-subtle);
	}
	.ch-state {
		font-size: 0.85rem;
		color: var(--text-faint);
		margin: 0;
		padding: 0.6rem 0.8rem;
		background: var(--bg);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
	}
	.ch-state.error {
		color: var(--danger);
	}
	.ch-list {
		list-style: none;
		padding: 0;
		margin: 0;
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		overflow: hidden;
	}
	.ch-item {
		display: grid;
		grid-template-columns: auto auto 1fr;
		gap: 0.6rem;
		align-items: baseline;
		padding: 0.55rem 0.85rem;
		font-size: 0.82rem;
		color: var(--text);
	}
	.ch-item + .ch-item {
		border-top: 1px solid var(--bg-subtle);
	}
	.ch-ts {
		font-variant-numeric: tabular-nums;
		color: var(--text-faint);
		font-size: 0.75rem;
		min-width: 5rem;
	}
	.ch-op {
		color: var(--text-muted);
		font-size: 0.72rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		padding: 0.05rem 0.45rem;
		background: var(--bg-elevated);
		border: 1px solid var(--bg-subtle);
		border-radius: 10px;
	}
	.ch-op-empty {
		width: 0;
	}
	.ch-change {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		flex-wrap: wrap;
	}
	.ch-arrow {
		color: var(--text-faint);
	}
	/* REQ-007: 섹션 접기 토글. QuestNoteSection(DEV-107)의 패턴을 그대로 따른다
	   — 같은 상세 페이지의 형제 섹션이라 조작감이 달라지면 안 된다. */
	.section-toggle {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		color: inherit;
		font: inherit;
	}
	.toggle-icon {
		font-size: 0.65rem;
		color: var(--text-muted);
		transition: transform 0.12s;
		display: inline-block;
	}
	.toggle-icon.collapsed {
		transform: rotate(-90deg);
	}
</style>
