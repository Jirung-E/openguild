<!--
  BUG-026: 곧 시작 캠페인 — marquee 폐기, finite carousel 재작성.

  사용자 피드백 (BUG-025 직후):
   - 적은 양에도 돌아 산만함
   - 끝 인지 어려움 (무한 회전이라)
   - 지나간 카드 다시 보려면 기다림
   - 회전 중 첫 카드 갑자기 사라졌다 나타남 (복제 boundary)

  새 동작:
   - 화면에 모두 들어가면 단순 flex (overflow 측정).
   - overflow 면 좌/우 화살표 페이지네이션. 1 카드씩 슬라이드.
   - 양 끝 도달 시 그 방향 화살표 disabled — "끝" 명시.
   - 자동 회전 없음 — 사용자 컨트롤만.
-->
<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	import type { CampaignSummary } from '$lib/types';
	import CampaignCard from './CampaignCard.svelte';

	let {
		summaries,
		now,
		emptyText = '곧 시작 예정인 캠페인이 없습니다.'
	}: {
		summaries: CampaignSummary[];
		now: number;
		emptyText?: string;
	} = $props();

	let viewportEl: HTMLDivElement | undefined = $state(undefined);
	let trackEl: HTMLDivElement | undefined = $state(undefined);
	let viewportW = $state(0);
	let trackW = $state(0);
	let idx = $state(0); // 첫 보이는 카드 index
	let cardW = 200; // CSS .slot width 와 일치
	let gapPx = 12; // 0.75rem
	let resizeObs: ResizeObserver | null = null;

	let overflow = $derived(trackW > viewportW + 1);
	// 화면에 동시에 보이는 카드 수 (floor)
	let visibleCount = $derived(
		viewportW > 0 ? Math.max(1, Math.floor((viewportW + gapPx) / (cardW + gapPx))) : 1
	);
	let maxIdx = $derived(Math.max(0, summaries.length - visibleCount));
	let canPrev = $derived(idx > 0);
	let canNext = $derived(idx < maxIdx);

	// idx 가 범위 초과하면 clamp.
	$effect(() => {
		if (idx > maxIdx) idx = maxIdx;
	});

	function prev() {
		if (canPrev) idx -= 1;
	}
	function next() {
		if (canNext) idx += 1;
	}

	async function measure() {
		await tick();
		if (viewportEl) viewportW = viewportEl.clientWidth;
		if (trackEl) trackW = trackEl.scrollWidth;
	}

	onMount(() => {
		measure();
		if (typeof ResizeObserver !== 'undefined') {
			resizeObs = new ResizeObserver(() => measure());
			if (viewportEl) resizeObs.observe(viewportEl);
		}
	});
	$effect(() => {
		// summaries 길이 / 변동 시 다시 측정.
		void summaries.length;
		measure();
	});
	onDestroy(() => {
		resizeObs?.disconnect();
	});

	// translateX 계산 — 카드 폭 + gap 단위.
	let offsetX = $derived(idx * (cardW + gapPx));
</script>

{#if summaries.length === 0}
	<div class="empty">{emptyText}</div>
{:else}
	<div class="wrap">
		<button
			class="arrow"
			type="button"
			onclick={prev}
			disabled={!canPrev}
			aria-label="이전"
		>‹</button>

		<div class="viewport" bind:this={viewportEl}>
			<div
				class="track"
				bind:this={trackEl}
				style:transform={overflow ? `translateX(-${offsetX}px)` : ''}
			>
				{#each summaries as s (s.id)}
					<div class="slot">
						<CampaignCard summary={s} mode="upcoming" {now} />
					</div>
				{/each}
			</div>
		</div>

		<button
			class="arrow"
			type="button"
			onclick={next}
			disabled={!canNext}
			aria-label="다음"
		>›</button>
	</div>
{/if}

<style>
	.empty { color: #6e7681; font-size: 0.875rem; padding: 1rem 0; }

	.wrap {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.25rem 0 0.5rem 0;
	}

	.viewport {
		flex: 1;
		overflow: hidden;
		min-width: 0;
	}
	.track {
		display: flex;
		gap: 0.75rem;
		width: max-content;
		transition: transform 0.35s cubic-bezier(0.4, 0, 0.2, 1);
	}
	.slot { flex: 0 0 200px; }

	.arrow {
		flex: 0 0 auto;
		background: #21262d;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 50%;
		width: 1.8rem;
		height: 1.8rem;
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
		transition: background 0.15s, opacity 0.15s;
	}
	.arrow:hover:not(:disabled) { background: #2a2a4a; }
	.arrow:disabled {
		opacity: 0.3;
		cursor: not-allowed;
	}
</style>
