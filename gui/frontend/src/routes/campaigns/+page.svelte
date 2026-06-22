<!--
  DEV-011: Campaign 목록 페이지 (/campaigns).
   - 정렬: 최근 추가 순 (기본) / 남은 날짜 순 / 수동 (display_order)
   - 어드민이 수동 모드일 때 ↑↓ 버튼으로 순서 변경 (display_order 갱신)
   - 각 카드 클릭 → /campaigns/<slug> detail
   - 우상단 "+ 새 캠페인" 버튼
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { campaignsApi } from '$lib/api/campaigns';
	import type { Campaign } from '$lib/types';
	import { isDateOverdue } from '$lib/utils/datetime';

	// BUG-025: sort 옵션을 localStorage 에 저장 (lib/utils/campaign-sort) →
	// Home 의 카드 정렬도 같은 값 적용.
	import {
		loadCampaignSort,
		saveCampaignSort,
		sortCampaigns,
		type CampaignSortMode
	} from '$lib/utils/campaign-sort';

	let all = $state<Campaign[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let sort = $state<CampaignSortMode>(loadCampaignSort());
	let statusFilter = $state<'all' | 'active' | 'done'>('all');

	// sort 변경마다 localStorage 저장.
	$effect(() => {
		saveCampaignSort(sort);
	});

	onMount(async () => {
		try {
			all = await campaignsApi.list();
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	});

	let filtered = $derived.by(() => {
		const base = statusFilter === 'all' ? all : all.filter((c) => c.status === statusFilter);
		return sortCampaigns(base, sort);
	});

	async function moveOrder(c: Campaign, delta: number) {
		const next = (c.display_order ?? 0) + delta;
		try {
			await campaignsApi.update(c.campaign_slug, { display_order: next });
			all = await campaignsApi.list();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'order 변경 실패');
		}
	}

	function fmtPeriod(c: Campaign): string {
		const a = c.started_at?.trim() || '';
		const b = c.ended_at?.trim() || '';
		if (!a && !b) return '기간 미정';
		if (a && !b) return `${a} ~`;
		if (!a && b) return `~ ${b}`;
		return `${a} ~ ${b}`;
	}
</script>

<div class="page">
	<div class="header">
		<h1>캠페인</h1>
		<button class="btn-primary" onclick={() => goto('/campaigns/new')}>+ 새 캠페인</button>
	</div>

	<div class="controls">
		<label>
			상태
			<select bind:value={statusFilter}>
				<option value="all">전체</option>
				<option value="active">진행 중</option>
				<option value="done">완료</option>
			</select>
		</label>
		<label>
			정렬
			<select bind:value={sort}>
				<option value="recent">최근 추가 순</option>
				<option value="remaining">남은 날짜 순</option>
				<option value="manual">수동 (display_order)</option>
			</select>
		</label>
	</div>

	{#if loading}
		<div class="state">Loading…</div>
	{:else if error}
		<div class="state error">{error}</div>
	{:else if filtered.length === 0}
		<div class="state">캠페인 없음.</div>
	{:else}
		<ul class="list">
			{#each filtered as c (c.id)}
				<li class="row">
					<a class="main" href={`/campaigns/${encodeURIComponent(c.campaign_slug)}`}>
						<span class="slug">{c.campaign_slug}</span>
						<span class="title">{c.title}</span>
						<span class="status status-{c.status}">{c.status}</span>
						<!-- DEV-079: 종료 기한 지났는데 status != done 이면 period 빨강. -->
						<span class="period" class:overdue={isDateOverdue(c.ended_at, c.status)}
							>{fmtPeriod(c)}</span
						>
					</a>
					{#if sort === 'manual'}
						<div class="reorder">
							<button title="up" onclick={() => moveOrder(c, -1)}>↑</button>
							<button title="down" onclick={() => moveOrder(c, 1)}>↓</button>
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.page {
		padding: 1.25rem 1.5rem;
		max-width: var(--content-max-width, 1100px);
		margin: 0 auto;
	}
	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 1rem;
	}
	.header h1 {
		font-size: 1.25rem;
		color: var(--text);
		margin: 0;
	}
	.btn-primary {
		padding: 0.4rem 0.85rem;
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-primary:hover {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}

	.controls {
		display: flex;
		gap: 1rem;
		margin-bottom: 1rem;
	}
	.controls label {
		font-size: 0.825rem;
		color: var(--text-muted);
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}
	.controls select {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 4px;
		padding: 0.25rem 0.5rem;
		font-size: 0.825rem;
	}

	.state {
		color: var(--text-muted);
		padding: 1.5rem 0;
		font-size: 0.875rem;
	}
	.state.error {
		color: var(--danger);
	}

	.list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.row {
		display: flex;
		align-items: stretch;
		gap: 4px;
	}
	.main {
		flex: 1;
		display: grid;
		grid-template-columns: 5rem 1fr auto auto;
		gap: 0.75rem;
		align-items: center;
		padding: 0.6rem 0.85rem;
		background: var(--bg-elevated);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		text-decoration: none;
		color: inherit;
	}
	.main:hover {
		border-color: var(--text-faint);
		background: var(--bg-subtle);
	}

	.slug {
		font-size: 0.75rem;
		color: var(--text-muted);
		font-family: 'JetBrains Mono', ui-monospace, monospace;
	}
	.title {
		color: var(--text);
		font-size: 0.9rem;
	}
	/* BUG-021: Quest List 의 pill 스타일 통일. */
	.status {
		flex-shrink: 0;
		padding: 0.15rem 0.55rem;
		border-radius: 20px;
		font-size: 0.75rem;
		font-weight: 500;
		text-transform: uppercase;
	}
	.status-active {
		--c: var(--success);
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	.status-done {
		--c: var(--text-muted);
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	.period {
		font-size: 0.75rem;
		color: var(--text-muted);
	}
	/* DEV-079: 기한 지남 + status != done — 빨강. */
	.period.overdue {
		color: var(--danger);
		font-weight: 600;
	}

	.reorder {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.reorder button {
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 4px;
		width: 1.8rem;
		font-size: 0.75rem;
		cursor: pointer;
	}
	.reorder button:hover {
		background: var(--bg-subtle);
	}
</style>
