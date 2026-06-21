<!--
  DEV-038: Quest Detail 의 변경이력 섹션.

  현재는 op="change_status" 만 기록되므로 slug 매핑 처리에 집중
  (DEV-042 부터 history 의 old/new_value 는 status slug).
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

	// statuses 는 부모에서 받은 메타. id → status / slug → status 매핑.
	let statusById = $derived(new Map(statuses.map((s) => [s.id, s])));
	let statusBySlug = $derived(new Map(statuses.map((s) => [s.slug, s])));

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

	/**
	 * DEV-042: history 값은 slug (예 "open", "testing"). 표시 시 status name_en 으로 변환.
	 * 폴백: 숫자로 보이면 legacy (DEV-042 이전 기록) — id 로 lookup, 표기에 (legacy) 부착.
	 */
	function statusLabel(value: string | null): string {
		if (!value) return '(없음)';
		const bySlug = statusBySlug.get(value);
		if (bySlug) return bySlug.name_en;
		if (/^\d+$/.test(value)) {
			const id = Number(value);
			const s = statusById.get(id);
			if (s) return `${s.name_en} (legacy)`;
		}
		return value;
	}

	function statusColor(value: string | null): string {
		if (!value) return 'var(--text-faint)';
		const bySlug = statusBySlug.get(value);
		if (bySlug) return bySlug.color;
		if (/^\d+$/.test(value)) {
			const s = statusById.get(Number(value));
			if (s) return s.color;
		}
		return 'var(--text-faint)';
	}

	/**
	 * op 라벨. change_status 는 visual change (old → new pill) 가 의미를 이미
	 * 전달하므로 생략 (DEV-038 후속). 다른 op 만 표시.
	 */
	function opLabel(op: string): string {
		switch (op) {
			case 'change_status':
				return '';
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
					{#if opLabel(e.op)}
						<span class="qh-op">{opLabel(e.op)}</span>
					{:else}
						<!-- grid 컬럼 자리 유지 (op 라벨 없을 때 빈 placeholder) -->
						<span class="qh-op-empty" aria-hidden="true"></span>
					{/if}
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
							{e.old_value ?? '∅'} <span class="qh-arrow">→</span>
							{e.new_value ?? '∅'}
						</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.qh-section {
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
	.qh-count {
		font-size: 0.72rem;
		color: var(--text-faint);
		padding: 0.05rem 0.4rem;
		border-radius: 10px;
		background: var(--bg-subtle);
	}

	.qh-state {
		font-size: 0.85rem;
		color: var(--text-faint);
		margin: 0;
		padding: 0.6rem 0.8rem;
		background: var(--bg);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
	}
	.qh-state.error {
		color: var(--danger);
	}

	.qh-list {
		list-style: none;
		padding: 0;
		margin: 0;
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		overflow: hidden;
	}
	.qh-item {
		display: grid;
		grid-template-columns: auto auto 1fr;
		gap: 0.6rem;
		align-items: baseline;
		padding: 0.55rem 0.85rem;
		font-size: 0.82rem;
		color: var(--text);
	}
	.qh-item + .qh-item {
		border-top: 1px solid var(--bg-subtle);
	}

	.qh-ts {
		font-variant-numeric: tabular-nums;
		color: var(--text-faint);
		font-size: 0.75rem;
		min-width: 5rem;
	}
	.qh-op {
		color: var(--text-muted);
		font-size: 0.72rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		padding: 0.05rem 0.45rem;
		background: var(--bg-elevated);
		border: 1px solid var(--bg-subtle);
		border-radius: 10px;
	}
	.qh-op-empty {
		width: 0;
	}
	.qh-change {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		flex-wrap: wrap;
	}
	.qh-status {
		padding: 0.05rem 0.5rem;
		border-radius: 12px;
		font-size: 0.75rem;
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	.qh-arrow {
		color: var(--text-faint);
	}
</style>
