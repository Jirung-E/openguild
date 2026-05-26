<!--
  BUG-027: 곧 시작 캠페인 컨베이어 (재구현).

  사용자 결정 — BUG-026 의 finite carousel 폐기, 다시 marquee.
  단 BUG-026 의 문제점 해결:
   - 적은 양에도 도는 문제 → overflow 측정 후 다 들어가면 단순 flex.
   - 끝 인지 어려움 → 첫 ↔ 끝 사이 시각적 큰 gap (한 카드 폭).
   - 첫 카드 갑자기 사라졌다 나옴 → 복제 + 50% 시점 리셋은 그대로,
     첫↔끝 gap 으로 boundary 명확.
   - 마우스 드래그로 즉시 이동 (좌/우). 드래그 후 2초간 자동 흐름 정지.
   - hover 시 자동 흐름 일시정지 (드래그 / 카드 클릭 편의).
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
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

	const CARD_W = 200; // CSS .slot width
	const GAP_PX = 12; // 0.75rem
	const LOOP_GAP_PX = CARD_W + GAP_PX; // 첫↔끝 시각적 큰 gap (= 카드 1장 폭)

	let viewportEl: HTMLDivElement | undefined = $state(undefined);
	let trackEl: HTMLDivElement | undefined = $state(undefined);
	let viewportW = $state(0);
	// 진짜 (드래그 안 함 + hover 안 함) 자연 흐름 위치 (시간 기반).
	// trackEl scroll 로 이동. CSS animation 사용 안 함 (드래그 / 정지 통합 위해).
	let scrollX = $state(0);
	let dragStartX = 0;
	let dragStartScroll = 0;
	let isDragging = $state(false);
	let hoverPause = $state(false);
	let dragPauseUntil = $state(0);
	let resizeObs: ResizeObserver | null = null;
	let rafHandle: number | null = null;
	let lastFrameTime = 0;

	// 단일 카드 시퀀스 폭 = 카드 N × (CARD_W + GAP_PX). 두 번 반복하므로
	// scrollX 가 이 값에 도달하면 0 으로 리셋 (loop).
	let seqWidth = $derived(summaries.length * (CARD_W + GAP_PX) + LOOP_GAP_PX);
	let trackW = $derived(seqWidth * 2);
	let needsMarquee = $derived(seqWidth > viewportW + 1);

	// 픽셀 / 초 — 카드 1장이 secondsPerCard 초에 지나도록.
	let pixelsPerSec = $derived((CARD_W + GAP_PX) / secondsPerCard);

	function paused(t: number): boolean {
		if (isDragging) return true;
		if (hoverPause) return true;
		if (dragPauseUntil > 0 && t < dragPauseUntil) return true;
		return false;
	}

	function loop(t: number) {
		if (lastFrameTime === 0) lastFrameTime = t;
		const dt = (t - lastFrameTime) / 1000;
		lastFrameTime = t;
		if (needsMarquee && !paused(performance.now())) {
			scrollX += pixelsPerSec * dt;
			if (scrollX >= seqWidth) scrollX -= seqWidth;
		}
		rafHandle = requestAnimationFrame(loop);
	}

	function measure() {
		if (viewportEl) viewportW = viewportEl.clientWidth;
	}

	onMount(() => {
		measure();
		if (typeof ResizeObserver !== 'undefined') {
			resizeObs = new ResizeObserver(() => measure());
			if (viewportEl) resizeObs.observe(viewportEl);
		}
		rafHandle = requestAnimationFrame(loop);
	});
	onDestroy(() => {
		resizeObs?.disconnect();
		if (rafHandle !== null) cancelAnimationFrame(rafHandle);
	});

	// 드래그 핸들러 — pointer events.
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
		// 드래그 = 보이는 방향과 반대 (오른쪽으로 끌면 왼쪽 카드 보임 = scrollX 감소).
		let next = dragStartScroll - dx;
		// loop 범위 안에서 wrap.
		next = ((next % seqWidth) + seqWidth) % seqWidth;
		scrollX = next;
	}
	function onPointerUp(e: PointerEvent) {
		if (!isDragging) return;
		isDragging = false;
		(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
		// 사용자 드래그 후 2초간 자동 흐름 정지.
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
			class:no-transition={isDragging}
			bind:this={trackEl}
			style:transform={`translateX(${-scrollX}px)`}
			style:width={`${trackW}px`}
		>
			{#each summaries as s, i (s.id)}
				<div class="slot" class:loop-gap-after={i === summaries.length - 1 && needsMarquee}>
					<CampaignCard summary={s} mode="upcoming" {now} />
				</div>
			{/each}
			{#if needsMarquee}
				{#each summaries as s, i (`dup-${i}`)}
					<div class="slot" class:loop-gap-after={i === summaries.length - 1} aria-hidden="true">
						<CampaignCard summary={s} mode="upcoming" {now} />
					</div>
				{/each}
			{/if}
		</div>
	</div>
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
		/* 부드러운 자연 흐름 위해 transform transition 짧게. 드래그 중엔 instant. */
		transition: transform 0.05s linear;
		will-change: transform;
	}
	.track.no-transition { transition: none; }
	.slot { flex: 0 0 200px; pointer-events: auto; }
	/* BUG-027: 마지막 카드와 첫 카드(복제 또는 wrap) 사이 시각적 gap.
	   카드 1개 폭 만큼 margin-right 추가 → 끝 명시. */
	.slot.loop-gap-after { margin-right: 200px; }

	/* 드래그 중에는 카드 클릭 막기 (실수로 detail 진입 방지).
	   pointerup 직후엔 click 이벤트가 트랙으로 가지만 카드 link 는 별도 click. */
	.conveyor.dragging .slot { pointer-events: none; }
</style>
