<!--
  BUG-028: 곧 시작 캠페인 marquee (BUG-027 후속 fix).

  수정:
   - wrap 시 시각 점프 (12px) — seqWidth 동적 측정 (trackEl.scrollWidth / 2)
     으로 정확. 마진/갭 계산 오차 제거.
   - 임계값 부정확 — 카드만의 폭 (margin / spacer 제외) 으로 fit 판단.
   - 정지/재생 버튼 추가 (CampaignCarousel 과 동일 패턴).
-->
<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	import type { CampaignSummary } from '$lib/types';
	import CampaignCard from './CampaignCard.svelte';

	let {
		summaries,
		now,
		emptyText = '곧 시작 예정인 캠페인이 없습니다.',
		secondsPerCard = 6
	}: {
		summaries: CampaignSummary[];
		now: number;
		emptyText?: string;
		secondsPerCard?: number;
	} = $props();

	const CARD_W = 200;
	const GAP_PX = 12;
	// 첫↔끝 시각 분리용 spacer 폭 (= 카드 1장).
	const LOOP_SPACER_PX = 200;

	let viewportEl: HTMLDivElement | undefined = $state(undefined);
	let trackEl: HTMLDivElement | undefined = $state(undefined);
	let viewportW = $state(0);
	// 한 사이클의 시퀀스 폭. 마운트 / summaries 변경 후 측정.
	let seqWidth = $state(0);
	let scrollX = $state(0);
	let dragStartX = 0;
	let dragStartScroll = 0;
	let isDragging = $state(false);
	let hoverPause = $state(false);
	let userPaused = $state(false);
	let dragPauseUntil = $state(0);
	let resizeObs: ResizeObserver | null = null;
	let rafHandle: number | null = null;
	let lastFrameTime = 0;

	// BUG-028: 카드만의 폭 (margin/spacer 제외) 으로 fit 판단.
	let cardsOnlyW = $derived(
		summaries.length === 0
			? 0
			: summaries.length * CARD_W + (summaries.length - 1) * GAP_PX
	);
	let needsMarquee = $derived(viewportW > 0 && cardsOnlyW > viewportW);
	let pixelsPerSec = $derived((CARD_W + GAP_PX) / secondsPerCard);

	function isPaused(t: number): boolean {
		if (isDragging) return true;
		if (hoverPause) return true;
		if (userPaused) return true;
		if (dragPauseUntil > 0 && t < dragPauseUntil) return true;
		return false;
	}

	function loop(t: number) {
		if (lastFrameTime === 0) lastFrameTime = t;
		const dt = (t - lastFrameTime) / 1000;
		lastFrameTime = t;
		if (needsMarquee && seqWidth > 0 && !isPaused(performance.now())) {
			scrollX += pixelsPerSec * dt;
			// 다중 wrap 대비 modulo.
			if (scrollX >= seqWidth) {
				scrollX -= seqWidth * Math.floor(scrollX / seqWidth);
			}
		}
		rafHandle = requestAnimationFrame(loop);
	}

	async function measure() {
		await tick();
		if (viewportEl) viewportW = viewportEl.clientWidth;
		// BUG-028: seqWidth = 실측 trackEl.scrollWidth / 2 — margin/gap 오차 zero.
		if (trackEl) {
			seqWidth = needsMarquee ? trackEl.scrollWidth / 2 : 0;
		}
	}

	onMount(() => {
		measure();
		if (typeof ResizeObserver !== 'undefined') {
			resizeObs = new ResizeObserver(() => measure());
			if (viewportEl) resizeObs.observe(viewportEl);
		}
		rafHandle = requestAnimationFrame(loop);
	});
	$effect(() => {
		void summaries.length;
		measure();
	});
	onDestroy(() => {
		resizeObs?.disconnect();
		if (rafHandle !== null) cancelAnimationFrame(rafHandle);
	});

	// 드래그.
	function onPointerDown(e: PointerEvent) {
		if (!needsMarquee) return;
		isDragging = true;
		dragStartX = e.clientX;
		dragStartScroll = scrollX;
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
	}
	function onPointerMove(e: PointerEvent) {
		if (!isDragging) return;
		const dx = e.clientX - dragStartX;
		let next = dragStartScroll - dx;
		next = ((next % seqWidth) + seqWidth) % seqWidth;
		scrollX = next;
	}
	function onPointerUp(e: PointerEvent) {
		if (!isDragging) return;
		isDragging = false;
		(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
		dragPauseUntil = performance.now() + 2000;
	}
</script>

{#if summaries.length === 0}
	<div class="empty">{emptyText}</div>
{:else}
	<div
		class="conveyor"
		class:dragging={isDragging}
		bind:this={viewportEl}
		onmouseenter={() => (hoverPause = true)}
		onmouseleave={() => (hoverPause = false)}
		onpointerdown={onPointerDown}
		onpointermove={onPointerMove}
		onpointerup={onPointerUp}
		onpointercancel={onPointerUp}
		role="region"
		aria-label="곧 시작 캠페인"
	>
		<div
			class="track"
			bind:this={trackEl}
			style:transform={needsMarquee ? `translateX(${-scrollX}px)` : ''}
		>
			{#each summaries as s (s.id)}
				<div class="slot">
					<CampaignCard summary={s} mode="upcoming" {now} />
				</div>
			{/each}
			{#if needsMarquee}
				<!-- 첫↔끝 시각 분리 spacer. -->
				<div class="spacer" aria-hidden="true"></div>
				{#each summaries as s, i (`dup-${i}`)}
					<div class="slot" aria-hidden="true">
						<CampaignCard summary={s} mode="upcoming" {now} />
					</div>
				{/each}
				<!-- 두번째 시퀀스 뒤에도 동일 spacer — 사이클 균형. -->
				<div class="spacer" aria-hidden="true"></div>
			{/if}
		</div>
	</div>

	{#if needsMarquee}
		<div class="controls">
			<button
				class="play-pause"
				type="button"
				onclick={() => (userPaused = !userPaused)}
				aria-label={userPaused ? '재생' : '정지'}
				title={userPaused ? '자동 회전 재생' : '자동 회전 정지'}
			>
				{userPaused ? '▶' : '⏸'}
			</button>
		</div>
	{/if}
{/if}

<style>
	.empty { color: #6e7681; font-size: 0.875rem; padding: 1rem 0; }

	.conveyor {
		overflow: hidden;
		padding: 0.25rem 0 0.5rem 0;
		cursor: grab;
		user-select: none;
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
	.conveyor.dragging { cursor: grabbing; }

	.track {
		display: flex;
		gap: 0.75rem;
		width: max-content;
		will-change: transform;
	}
	.slot { flex: 0 0 200px; }
	.spacer { flex: 0 0 200px; }

	.conveyor.dragging .slot { pointer-events: none; }

	.controls {
		display: flex;
		justify-content: center;
		margin-top: 0.25rem;
	}
	.play-pause {
		background: #21262d;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 50%;
		width: 1.8rem;
		height: 1.8rem;
		font-size: 0.85rem;
		line-height: 1;
		cursor: pointer;
		transition: background 0.15s;
	}
	.play-pause:hover { background: #2a2a4a; }
</style>
