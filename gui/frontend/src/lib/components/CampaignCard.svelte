<!--
  BUG-025: 캠페인 단일 카드 — CampaignCarousel / CampaignConveyor 가 공유.

  mode:
   - 'active' (진행 중): 슬러그 + 제목 + 기간 + 종료 임박 카운트다운 + 진행률.
                         완료 시 progress bar 초록 + ✓ 표시.
   - 'upcoming' (곧 시작): 슬러그/진행률 제거. 제목 + "n일 남음 (YYYY-MM-DD)".

  width 는 부모가 결정 (carousel = 100%, conveyor = 200px 등).
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import type { CampaignSummary } from '$lib/types';
	import { formatRemaining } from '$lib/utils/datetime';

	let {
		summary,
		mode,
		now
	}: {
		summary: CampaignSummary;
		mode: 'active' | 'upcoming';
		now: number;
	} = $props();

	let completed = $derived(
		summary.checklist_total > 0 &&
			summary.checklist_checked === summary.checklist_total
	);

	function fmtPeriod(): string {
		const a = summary.started_at?.trim() || '';
		const b = summary.ended_at?.trim() || '';
		if (!a && !b) return '기간 미정';
		if (a && !b) return `${a} ~`;
		if (!a && b) return `~ ${b}`;
		return `${a} ~ ${b}`;
	}

	function progressText(): string {
		if (summary.checklist_total === 0) return '체크리스트 없음';
		const pct = Math.round(summary.progress * 100);
		return `${summary.checklist_checked}/${summary.checklist_total} (${pct}%)`;
	}

	function activeRemainingLabel(): string {
		if (!summary.ended_at?.trim()) return '';
		return formatRemaining(summary.ended_at, now, 'until-end');
	}

	// BUG-031: '진행 중' 카드의 남은 기간을 빨강 강조하는 임계값 (≤ 7일).
	// 그 이상 남았으면 평이한 회색 — 한참 남은 캠페인에도 빨강 표시되는 게
	// 피로감을 줌. 7일 이내가 되는 시점부터 시각 경고.
	const ACTIVE_URGENT_DAYS = 7;
	let activeRemainingIsUrgent = $derived.by(() => {
		const e = summary.ended_at?.trim();
		if (!e) return false;
		const endMs = new Date(`${e}T23:59:59`).getTime();
		if (Number.isNaN(endMs)) return false;
		const remaining = endMs - now;
		return remaining > 0 && remaining <= ACTIVE_URGENT_DAYS * 24 * 60 * 60 * 1000;
	});

	function upcomingRemainingLabel(): string {
		if (!summary.started_at?.trim()) return '';
		return formatRemaining(summary.started_at, now, 'until-start');
	}

	function open() {
		goto(`/campaigns/${encodeURIComponent(summary.campaign_slug)}`);
	}
</script>

<button class="card {mode}" class:completed type="button" onclick={open}>
	{#if mode === 'active'}
		<div class="head">
			<span class="slug">{summary.campaign_slug}</span>
			{#if completed}<span class="done-mark" title="완료">✓ 완료</span>{/if}
		</div>
		<div class="title">{summary.title}</div>
		<div class="meta">
			<div class="period">
				{fmtPeriod()}
				{#if activeRemainingLabel()}
					<!-- BUG-031: 7일 이내만 빨간 강조. 그 외는 평이한 회색. -->
					<span class="remaining" class:urgent={activeRemainingIsUrgent}
						>({activeRemainingLabel()})</span
					>
				{/if}
			</div>
			<div class="progress-row">
				<div class="progress-bar">
					<div
						class="progress-fill"
						class:done={completed}
						style:width={`${Math.round(summary.progress * 100)}%`}
					></div>
				</div>
				<div class="progress-text" class:done-text={completed}>
					{progressText()}
				</div>
			</div>
		</div>
	{:else}
		<div class="title small">{summary.title}</div>
		<div class="meta">
			<div class="period">
				<span class="remaining accent">{upcomingRemainingLabel()}</span>
				{#if summary.started_at?.trim()}
					<span class="start-date">({summary.started_at})</span>
				{/if}
			</div>
		</div>
	{/if}
</button>

<style>
	.card {
		width: 100%;
		background: #161b22;
		border: 1px solid #30363d;
		border-radius: 8px;
		padding: 0.85rem 1rem;
		text-align: left;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		color: inherit;
		font: inherit;
	}
	.card:hover { background: #1c2128; border-color: #484f58; }
	/* BUG-027: active 카드 세로 길이 늘림 (사용자 피드백 — 너무 짧음). */
	.card.active {
		min-height: 180px;
		padding: 1.1rem 1.4rem;
		gap: 0.85rem;
	}
	.card.upcoming { padding: 0.65rem 0.8rem; gap: 0.35rem; }
	/* BUG-025: 100% 달성 카드 — 초록 border 강조 */
	.card.completed {
		border-color: #2ea043;
		background: linear-gradient(180deg, #102a18 0%, #161b22 60%);
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.slug {
		font-size: 0.7rem;
		color: #8b949e;
		letter-spacing: 0.04em;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
	}
	.done-mark {
		font-size: 0.7rem;
		font-weight: 700;
		color: #56d364;
		letter-spacing: 0.02em;
	}
	.title { font-size: 0.95rem; font-weight: 600; color: #c9d1d9; line-height: 1.3; }
	.title.small { font-size: 0.85rem; }
	/* BUG-027: active 카드의 title 도 더 크게. */
	.card.active .title { font-size: 1.1rem; line-height: 1.4; }

	.meta { margin-top: auto; display: flex; flex-direction: column; gap: 0.3rem; }
	.period {
		font-size: 0.75rem;
		color: #8b949e;
		display: flex;
		gap: 0.35rem;
		align-items: center;
		flex-wrap: wrap;
	}
	/* BUG-031: 기본은 회색 (한참 남은 캠페인에도 빨강 X). 7일 이내만 urgent
	   modifier 로 빨강. upcoming 의 accent 는 종전대로 파랑. */
	.remaining { color: #8b949e; font-weight: 500; }
	.remaining.urgent { color: #f85149; font-weight: 600; }
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
		transition: width 0.2s, background 0.2s;
	}
	/* BUG-025: 100% 시 초록 */
	.progress-fill.done { background: #2ea043; }
	.progress-text { font-size: 0.7rem; color: #8b949e; white-space: nowrap; }
	.progress-text.done-text { color: #56d364; font-weight: 600; }
</style>
