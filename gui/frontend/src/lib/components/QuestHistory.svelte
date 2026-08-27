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
	// DEV-205: 변경 이력 섹션 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-015: status 표시 이름 — 언어 반응(로컬 statusLabel 헬퍼와 이름 충돌 방지 alias).
	import { statusLabel as localizedStatusLabel } from '$lib/utils/status-label';
	// REQ-004: 늦게 온 응답이 최신 화면을 덮지 않도록.
	import { Generation } from '$lib/utils/latest-only';

	let { questId, statuses = [] }: { questId: number; statuses?: QuestStatus[] } = $props();

	let entries = $state<QuestHistoryEntry[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	// REQ-007: 기본 접힘. 변경 이력은 항상 필요한 정보가 아닌데 상세 페이지
	// 하단을 길게 차지한다. 접기 상태는 영속화하지 않는다 —
	// QuestNoteSection 의 collapsed 와 동일한 정책.
	let collapsed = $state(true);
	function toggleCollapsed() {
		collapsed = !collapsed;
	}

	// statuses 는 부모에서 받은 메타. id → status / slug → status 매핑.
	let statusById = $derived(new Map(statuses.map((s) => [s.id, s])));
	let statusBySlug = $derived(new Map(statuses.map((s) => [s.slug, s])));

	// questId 가 바뀔 때마다 (다른 quest 페이지로 navigate) 다시 로드.
	//
	// REQ-004: 늦게 온 응답이 화면을 덮으면 안 된다 — A 를 열고 응답 전에 B 로
	// 이동하면 나머지는 B 인데 이력만 A 가 된다. 세대 토큰으로 최신만 반영한다.
	const gen = new Generation();
	$effect(() => {
		const id = questId;
		if (id <= 0) return;
		const mine = gen.next();
		loading = true;
		error = null;
		questsApi
			.listHistory(id)
			.then((list) => {
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

	/**
	 * DEV-042: history 값은 slug (예 "open", "testing"). 표시 시 status 이름으로 변환.
	 * DEV-015: 이름은 언어 반응(ko 면 name_ko 우선) — 공용 util 사용.
	 * 폴백: 숫자로 보이면 legacy (DEV-042 이전 기록) — id 로 lookup, 표기에 (legacy) 부착.
	 */
	function statusLabel(value: string | null): string {
		if (!value) return t('history.none', $locale);
		const bySlug = statusBySlug.get(value);
		if (bySlug) return localizedStatusLabel(bySlug, $locale);
		if (/^\d+$/.test(value)) {
			const id = Number(value);
			const s = statusById.get(id);
			if (s) return `${localizedStatusLabel(s, $locale)} (legacy)`;
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
			<span class="qh-count">{entries.length}</span>
		{/if}
	</div>

	{#if !collapsed}
	{#if loading}
		<p class="qh-state">{t('history.loading', $locale)}</p>
	{:else if error}
		<p class="qh-state error">{error}</p>
	{:else if entries.length === 0}
		<p class="qh-state">{t('history.empty', $locale)}</p>
	{:else}
		<ul class="qh-list" data-testid="quest-history-list">
			{#each entries as e (e.id)}
				<li class="qh-item" data-testid="qh-item">
					<time class="qh-ts" datetime={e.ts} title={formatTs(e.ts)}>
						{formatRelative(e.ts, undefined, $locale)}
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
		border-radius: var(--r-xl);
		background: var(--bg-subtle);
	}

	.qh-state {
		font-size: 0.85rem;
		color: var(--text-faint);
		margin: 0;
		padding: 0.6rem 0.8rem;
		background: var(--bg);
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-md);
	}
	.qh-state.error {
		color: var(--danger);
	}

	.qh-list {
		list-style: none;
		padding: 0;
		margin: 0;
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-md);
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
		border-top: var(--bw) solid var(--bg-subtle);
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
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-xl);
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
		border-radius: var(--r-pill);
		font-size: 0.75rem;
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: var(--bw) solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	.qh-arrow {
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
