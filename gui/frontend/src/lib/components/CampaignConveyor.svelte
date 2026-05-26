<!--
  BUG-025: 곧 시작 캠페인 컨베이어 — 멈춤 없이 흐르는 marquee.

  - 카드들이 좌→우 방향으로 무한 스크롤.
  - hover 시 일시정지.
  - 카드 < 4 개면 단순 flex (스크롤 안 함 — 화면에 모두 들어옴).
  - 무한 효과: 카드 목록을 한 번 더 복제 후 50% 시점에서 0% 로 리셋.
-->
<script lang="ts">
	import type { CampaignSummary } from '$lib/types';
	import CampaignCard from './CampaignCard.svelte';

	let {
		summaries,
		now,
		emptyText = '곧 시작 예정인 캠페인이 없습니다.',
		/** 카드 < 이 수면 스크롤 없이 단순 flex. */
		conveyorThreshold = 4,
		/** 한 바퀴 도는 시간 (초). 카드 수에 비례. */
		secondsPerCard = 6
	}: {
		summaries: CampaignSummary[];
		now: number;
		emptyText?: string;
		conveyorThreshold?: number;
		secondsPerCard?: number;
	} = $props();

	// 컨베이어 효과 켜는 조건
	let isConveyor = $derived(summaries.length >= conveyorThreshold);
	// 한 바퀴 (translateX -50%) 도는 시간 — 카드 수 × secondsPerCard
	let duration = $derived(summaries.length * secondsPerCard);
</script>

{#if summaries.length === 0}
	<div class="empty">{emptyText}</div>
{:else if !isConveyor}
	<!-- 단순 flex — 카드가 모두 화면 안에 들어옴 -->
	<div class="row">
		{#each summaries as s (s.id)}
			<div class="slot">
				<CampaignCard summary={s} mode="upcoming" {now} />
			</div>
		{/each}
	</div>
{:else}
	<!-- 무한 흐름 marquee — 목록 두 번 복제, animation 50% 지점에서 시각 동일. -->
	<div class="conveyor">
		<div class="track" style:animation-duration={`${duration}s`}>
			{#each summaries as s (s.id)}
				<div class="slot">
					<CampaignCard summary={s} mode="upcoming" {now} />
				</div>
			{/each}
			<!-- 두 번째 복제 (무한 효과). aria-hidden 으로 a11y 중복 회피. -->
			{#each summaries as s, i (`dup-${i}`)}
				<div class="slot" aria-hidden="true">
					<CampaignCard summary={s} mode="upcoming" {now} />
				</div>
			{/each}
		</div>
	</div>
{/if}

<style>
	.empty { color: #6e7681; font-size: 0.875rem; padding: 1rem 0; }
	.row {
		display: flex;
		gap: 0.75rem;
		padding: 0.25rem 0 0.5rem 0;
	}
	.row .slot { flex: 0 0 200px; }

	.conveyor {
		overflow: hidden;
		padding: 0.25rem 0 0.5rem 0;
		/* 양쪽 fade mask — 카드가 가장자리에서 자연스럽게 사라지도록. */
		-webkit-mask-image: linear-gradient(
			90deg,
			transparent 0,
			#000 32px,
			#000 calc(100% - 32px),
			transparent 100%
		);
		mask-image: linear-gradient(
			90deg,
			transparent 0,
			#000 32px,
			#000 calc(100% - 32px),
			transparent 100%
		);
	}
	.track {
		display: flex;
		gap: 0.75rem;
		width: max-content;
		animation: scroll-x linear infinite;
	}
	.track:hover { animation-play-state: paused; }
	.conveyor .slot { flex: 0 0 200px; }

	@keyframes scroll-x {
		0% { transform: translateX(0); }
		100% { transform: translateX(calc(-50% - 0.375rem)); }
	}
</style>
