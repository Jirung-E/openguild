<!--
  DEV-226: Campaign Detail 의 변경이력 섹션 — QuestHistory.svelte(DEV-038) 와
  동일 패턴. campaign status 는 quest 처럼 별도 테이블이 아니라 "active"/"done"
  리터럴이라 statusLabel/Color 는 훨씬 단순.
-->
<script lang="ts">
	import { campaignsApi } from '$lib/api/campaigns';
	import type { CampaignHistoryEntry } from '$lib/types';
	import { formatTs, formatRelative } from '$lib/utils/datetime';

	let { campaignSlug }: { campaignSlug: string } = $props();

	let entries = $state<CampaignHistoryEntry[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	$effect(() => {
		const slug = campaignSlug;
		if (!slug) return;
		loading = true;
		error = null;
		campaignsApi
			.listHistory(slug)
			.then((list) => {
				entries = list;
			})
			.catch((e) => {
				error = e instanceof Error ? e.message : '이력 로드 실패';
			})
			.finally(() => {
				loading = false;
			});
	});

	function statusLabel(value: string | null): string {
		if (!value) return '(없음)';
		if (value === 'active') return 'Active';
		if (value === 'done') return 'Done';
		return value;
	}

	function statusColor(value: string | null): string {
		if (value === 'active') return 'var(--success)';
		if (value === 'done') return 'var(--text-faint)';
		return 'var(--text-faint)';
	}

	function opLabel(op: string): string {
		switch (op) {
			case 'change_status':
				return '';
			default:
				return op;
		}
	}
</script>

<section class="ch-section">
	<div class="section-head">
		<h2 class="section-title">변경 이력</h2>
		{#if entries.length > 0}
			<span class="ch-count">{entries.length}</span>
		{/if}
	</div>

	{#if loading}
		<p class="ch-state">로드 중…</p>
	{:else if error}
		<p class="ch-state error">{error}</p>
	{:else if entries.length === 0}
		<p class="ch-state">변경 이력 없음.</p>
	{:else}
		<ul class="ch-list" data-testid="campaign-history-list">
			{#each entries as e (e.id)}
				<li class="ch-item" data-testid="ch-item">
					<time class="ch-ts" datetime={e.ts} title={formatTs(e.ts)}>
						{formatRelative(e.ts)}
					</time>
					{#if opLabel(e.op)}
						<span class="ch-op">{opLabel(e.op)}</span>
					{:else}
						<span class="ch-op-empty" aria-hidden="true"></span>
					{/if}
					{#if e.op === 'change_status'}
						<span class="ch-change">
							<span class="ch-status" style:--c={statusColor(e.old_value)}>
								{statusLabel(e.old_value)}
							</span>
							<span class="ch-arrow">→</span>
							<span class="ch-status" style:--c={statusColor(e.new_value)}>
								{statusLabel(e.new_value)}
							</span>
						</span>
					{:else}
						<span class="ch-change">
							{e.old_value ?? '∅'} <span class="ch-arrow">→</span>
							{e.new_value ?? '∅'}
						</span>
					{/if}
				</li>
			{/each}
		</ul>
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
	.ch-status {
		padding: 0.05rem 0.5rem;
		border-radius: 12px;
		font-size: 0.75rem;
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	.ch-arrow {
		color: var(--text-faint);
	}
</style>
