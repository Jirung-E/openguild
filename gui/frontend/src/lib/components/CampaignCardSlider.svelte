<!--
  DEV-011 + BUG-024: 캠페인 가로 카드 슬라이드.

  Home 의 "진행 중 캠페인" / "곧 시작되는 캠페인" 두 섹션 공통.
  mode 별 표시 차이:
   - 'active' (진행 중): 슬러그 + 제목 + 기간 (+ 종료 임박 시 카운트다운 빨강) + 진행률.
   - 'upcoming' (곧 시작): 슬러그/진행률 없음. 제목 + "x일 남음 (YYYY-MM-DD)".

  카운트다운: 일/시간/분/초 단위 자동 전환 (formatRemaining).
  매초 갱신은 부모 (Home) 가 `now` prop 매초 update.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import type { CampaignSummary } from '$lib/types';
	import { formatRemaining } from '$lib/utils/datetime';

	let {
		summaries,
		mode,
		now,
		emptyText = '캠페인 없음'
	}: {
		summaries: CampaignSummary[];
		mode: 'active' | 'upcoming';
		/** 매초 갱신되는 부모의 시계. */
		now: number;
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

	function progressText(s: CampaignSummary): string {
		if (s.checklist_total === 0) return '체크리스트 없음';
		const pct = Math.round(s.progress * 100);
		return `${s.checklist_checked}/${s.checklist_total} (${pct}%)`;
	}

	/** 'active' 모드에서 종료가 임박했을 때 표시할 카운트다운. 없으면 빈 문자열. */
	function activeRemainingLabel(s: CampaignSummary): string {
		if (!s.ended_at?.trim()) return '';
		return formatRemaining(s.ended_at, now, 'until-end');
	}

	/** 'upcoming' 모드 — 시작일까지 카운트다운. */
	function upcomingRemainingLabel(s: CampaignSummary): string {
		if (!s.started_at?.trim()) return '';
		return formatRemaining(s.started_at, now, 'until-start');
	}

	function open(slug: string) {
		goto(`/campaigns/${encodeURIComponent(slug)}`);
	}
</script>

<div class="slider {mode}">
	{#if summaries.length === 0}
		<div class="empty">{emptyText}</div>
	{:else}
		{#each summaries as s (s.id)}
			<button class="card" type="button" onclick={() => open(s.campaign_slug)}>
				{#if mode === 'active'}
					<div class="slug">{s.campaign_slug}</div>
					<div class="title">{s.title}</div>
					<div class="meta">
						<div class="period">
							{fmtPeriod(s)}
							{#if activeRemainingLabel(s)}
								<span class="remaining">({activeRemainingLabel(s)})</span>
							{/if}
						</div>
						<div class="progress-row">
							<div class="progress-bar">
								<div
									class="progress-fill"
									style:width={`${Math.round(s.progress * 100)}%`}
								></div>
							</div>
							<div class="progress-text">{progressText(s)}</div>
						</div>
					</div>
				{:else}
					<!-- upcoming: 슬러그 / 진행률 없음, 시작까지 카운트다운만. -->
					<div class="title">{s.title}</div>
					<div class="meta">
						<div class="period">
							<span class="remaining accent">{upcomingRemainingLabel(s)}</span>
							{#if s.started_at?.trim()}
								<span class="start-date">({s.started_at})</span>
							{/if}
						</div>
					</div>
				{/if}
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

	.slider.active .card { width: 280px; }
	.slider.upcoming .card { width: 200px; padding: 0.65rem 0.8rem; gap: 0.35rem; }

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
	.slider.upcoming .title { font-size: 0.85rem; }

	.meta { margin-top: auto; display: flex; flex-direction: column; gap: 0.3rem; }

	.period {
		font-size: 0.75rem;
		color: #8b949e;
		display: flex;
		gap: 0.35rem;
		align-items: center;
		flex-wrap: wrap;
	}
	/* BUG-024: 종료 임박 카운트다운 빨강 강조 (active mode). */
	.remaining { color: #f85149; font-weight: 600; }
	/* BUG-024: upcoming 의 시작 카운트다운은 accent 컬러 (파랑). */
	.remaining.accent { color: #58a6ff; font-weight: 600; }
	.start-date { color: #6e7681; }

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
