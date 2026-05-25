<!--
  DEV-011: 캠페인 가로 카드 슬라이드.

  Home 의 "진행 중 캠페인" / "곧 시작되는 캠페인" 두 섹션 공통 사용.
  카드: 제목 / 기간 / 체크리스트 완료율. 클릭 시 캠페인 detail 로 이동.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import type { CampaignSummary } from '$lib/types';

	let {
		summaries,
		size = 'lg',
		emptyText = '캠페인 없음'
	}: {
		summaries: CampaignSummary[];
		/** 'lg' = 진행중 메인 카드, 'sm' = 곧 시작 보조 카드. */
		size?: 'lg' | 'sm';
		emptyText?: string;
	} = $props();

	function fmtPeriod(s: CampaignSummary): string {
		const a = s.started_at?.trim() || '';
		const b = s.ended_at?.trim() || '';
		if (!a && !b) return '기간 미정';
		if (a && !b) return `${a} ~`;
		if (!a && b) return `~ ${b}`;
		return `${a} ~ ${b}`;
	}

	function fmtProgress(s: CampaignSummary): string {
		if (s.checklist_total === 0) return '체크리스트 없음';
		const pct = Math.round(s.progress * 100);
		return `${s.checklist_checked}/${s.checklist_total} (${pct}%)`;
	}

	function open(slug: string) {
		goto(`/campaigns/${encodeURIComponent(slug)}`);
	}
</script>

<div class="slider {size}">
	{#if summaries.length === 0}
		<div class="empty">{emptyText}</div>
	{:else}
		{#each summaries as s (s.id)}
			<button class="card" type="button" onclick={() => open(s.campaign_slug)}>
				<div class="slug">{s.campaign_slug}</div>
				<div class="title">{s.title}</div>
				<div class="meta">
					<div class="period">{fmtPeriod(s)}</div>
					<div class="progress-row">
						<div class="progress-bar">
							<div class="progress-fill" style:width={`${Math.round(s.progress * 100)}%`}></div>
						</div>
						<div class="progress-text">{fmtProgress(s)}</div>
					</div>
				</div>
			</button>
		{/each}
	{/if}
</div>

<style>
	.slider {
		display: flex;
		gap: 0.75rem;
		overflow-x: auto;
		padding: 0.5rem 0 0.75rem 0;
		scroll-snap-type: x mandatory;
	}
	.empty {
		color: #6e7681;
		font-size: 0.875rem;
		padding: 1rem 0;
	}

	.card {
		flex: 0 0 auto;
		background: #161b22;
		border: 1px solid #30363d;
		border-radius: 8px;
		padding: 0.85rem 1rem;
		text-align: left;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s;
		scroll-snap-align: start;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		color: inherit;
		font: inherit;
	}
	.card:hover { background: #1c2128; border-color: #484f58; }

	.slider.lg .card { width: 280px; }
	.slider.sm .card { width: 200px; padding: 0.65rem 0.8rem; gap: 0.35rem; }

	.slug {
		font-size: 0.7rem;
		color: #8b949e;
		letter-spacing: 0.04em;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
	}
	.title {
		font-size: 0.95rem;
		font-weight: 600;
		color: #c9d1d9;
		line-height: 1.3;
	}
	.slider.sm .title { font-size: 0.85rem; }

	.meta { margin-top: auto; display: flex; flex-direction: column; gap: 0.3rem; }

	.period { font-size: 0.75rem; color: #8b949e; }

	.progress-row { display: flex; align-items: center; gap: 0.5rem; }
	.progress-bar {
		flex: 1;
		height: 4px;
		background: #21262d;
		border-radius: 2px;
		overflow: hidden;
	}
	.progress-fill {
		height: 100%;
		background: #4a9eff;
		transition: width 0.2s;
	}
	.progress-text {
		font-size: 0.7rem;
		color: #8b949e;
		white-space: nowrap;
	}
</style>
