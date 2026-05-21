<!--
  DEV-038: Quest Detail 의 변경이력 섹션.

  현재는 op="change_status" 만 기록되므로 status_id 매핑 처리에 집중.
  후속 op (update_title / change_parent / add_prereq / ...) 추가 시
  formatChange() 에 case 만 추가.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { questsApi } from '$lib/api/quests';
	import type { QuestHistoryEntry, QuestStatus } from '$lib/types';
	import { formatTs, formatRelative } from '$lib/utils/datetime';

	let { questId, statuses = [] }: { questId: number; statuses?: QuestStatus[] } = $props();

	let entries = $state<QuestHistoryEntry[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// statuses 는 부모에서 받은 메타. id → status 매핑.
	let statusById = $derived(new Map(statuses.map((s) => [s.id, s])));

	// questId 가 바뀔 때마다 (다른 quest 페이지로 navigate) 다시 로드.
	$effect(() => {
		const id = questId;
		if (id <= 0) return;
		loading = true;
		error = null;
		questsApi
			.listHistory(id)
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

	/** status_id 문자열 → 표시명. 매핑 없으면 ID 그대로. */
	function statusLabel(idStr: string | null): string {
		if (!idStr) return '(없음)';
		const id = Number(idStr);
		if (!Number.isFinite(id)) return idStr;
		const s = statusById.get(id);
		return s ? s.name_en : `#${idStr}`;
	}

	function statusColor(idStr: string | null): string {
		if (!idStr) return '#484f58';
		const id = Number(idStr);
		const s = statusById.get(id);
		return s?.color ?? '#484f58';
	}

	/** op 별 변경 표현. 후속 op 추가 시 case 추가. */
	function opLabel(op: string): string {
		switch (op) {
			case 'change_status':
				return '상태';
			default:
				return op;
		}
	}
</script>

<section class="qh-section">
	<div class="section-head">
		<h2 class="section-title">변경 이력</h2>
		{#if entries.length > 0}
			<span class="qh-count">{entries.length}</span>
		{/if}
	</div>

	{#if loading}
		<p class="qh-state">로드 중…</p>
	{:else if error}
		<p class="qh-state error">{error}</p>
	{:else if entries.length === 0}
		<p class="qh-state">변경 이력 없음.</p>
	{:else}
		<ul class="qh-list" data-testid="quest-history-list">
			{#each entries as e (e.id)}
				<li class="qh-item" data-testid="qh-item">
					<time class="qh-ts" datetime={e.ts} title={formatTs(e.ts)}>
						{formatRelative(e.ts)}
					</time>
					<span class="qh-op">{opLabel(e.op)}</span>
					{#if e.op === 'change_status'}
						<span class="qh-change">
							<span class="qh-status" style:--c={statusColor(e.old_value)}>
								{statusLabel(e.old_value)}
							</span>
							<span class="qh-arrow">→</span>
							<span class="qh-status" style:--c={statusColor(e.new_value)}>
								{statusLabel(e.new_value)}
							</span>
						</span>
					{:else}
						<span class="qh-change">
							{e.old_value ?? '∅'} <span class="qh-arrow">→</span> {e.new_value ?? '∅'}
						</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.qh-section { margin-bottom: 1.5rem; }
	.section-head {
		display: flex; align-items: center; gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
	.section-title {
		font-size: 0.8rem; font-weight: 600; color: #8b949e;
		text-transform: uppercase; letter-spacing: 0.05em; margin: 0;
	}
	.qh-count {
		font-size: 0.72rem; color: #484f58;
		padding: 0.05rem 0.4rem; border-radius: 10px;
		background: #21262d;
	}

	.qh-state {
		font-size: 0.85rem; color: #484f58; margin: 0;
		padding: 0.6rem 0.8rem;
		background: #0d1117; border: 1px solid #21262d; border-radius: 6px;
	}
	.qh-state.error { color: #e94f4f; }

	.qh-list {
		list-style: none; padding: 0; margin: 0;
		border: 1px solid #21262d; border-radius: 6px; overflow: hidden;
	}
	.qh-item {
		display: grid;
		grid-template-columns: auto auto 1fr;
		gap: 0.6rem;
		align-items: baseline;
		padding: 0.55rem 0.85rem;
		font-size: 0.82rem;
		color: #c9d1d9;
	}
	.qh-item + .qh-item { border-top: 1px solid #21262d; }

	.qh-ts {
		font-variant-numeric: tabular-nums;
		color: #6e7681; font-size: 0.75rem;
		min-width: 5rem;
	}
	.qh-op {
		color: #8b949e; font-size: 0.72rem;
		text-transform: uppercase; letter-spacing: 0.04em;
		padding: 0.05rem 0.45rem;
		background: #161b22; border: 1px solid #21262d;
		border-radius: 10px;
	}
	.qh-change {
		display: inline-flex; align-items: center; gap: 0.45rem;
		flex-wrap: wrap;
	}
	.qh-status {
		padding: 0.05rem 0.5rem; border-radius: 12px;
		font-size: 0.75rem;
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	.qh-arrow { color: #484f58; }
</style>
