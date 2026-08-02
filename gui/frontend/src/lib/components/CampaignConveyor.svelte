<!--
  BUG-028: 곧 시작 캠페인 marquee (BUG-027 후속 fix).

  수정:
   - wrap 시 시각 점프 (12px) — seqWidth 동적 측정 (trackEl.scrollWidth / 2)
     으로 정확. 마진/갭 계산 오차 제거.
   - 임계값 부정확 — 카드만의 폭 (margin / spacer 제외) 으로 fit 판단.
   - 정지/재생 버튼 추가 (CampaignCarousel 과 동일 패턴).
-->
<script lang="ts">
	import PlayPauseIcon from './PlayPauseIcon.svelte';
	import { onMount, onDestroy, tick } from 'svelte';
	import type { CampaignSummary } from '$lib/types';
	import CampaignCard from './CampaignCard.svelte';
	// BUG-117: 카드 슬롯이 px 고정이라 uiScale 시 안의 rem 글자만 커져 넘쳤음
	// — scale 배율을 곱해 슬롯 폭과 marquee 계산에 함께 반영.
	import { uiScale } from '$lib/stores/uiScale';
	// DEV-205 모듈2: 컨베이어 라벨 i18n.
	import { locale, t } from '$lib/stores/locale';

	let {
		summaries,
		now,
		emptyText,
		secondsPerCard = 6,
		// DEV-080: CampaignCard 의 모드 — 기본 'upcoming'. 'overdue' 도 동일 동작.
		mode = 'upcoming'
	}: {
		summaries: CampaignSummary[];
		now: number;
		emptyText?: string;
		secondsPerCard?: number;
		mode?: 'upcoming' | 'overdue';
	} = $props();

	const displayEmpty = $derived(emptyText ?? t('conveyor.upcomingEmpty', $locale));

	const CARD_W = 200;
	const GAP_PX = 12;
	// spacer 폭 = 카드 1장 (effCardW inline style — BUG-117).

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

	// BUG-117: uiScale 반영 실효 크기.
	let effCardW = $derived(Math.round(CARD_W * $uiScale));
	let effGap = $derived(GAP_PX * $uiScale);

	// BUG-028: 카드만의 폭 (margin/spacer 제외) 으로 fit 판단.
	let cardsOnlyW = $derived(
		summaries.length === 0 ? 0 : summaries.length * effCardW + (summaries.length - 1) * effGap
	);
	let needsMarquee = $derived(viewportW > 0 && cardsOnlyW > viewportW);
	let pixelsPerSec = $derived((effCardW + effGap) / secondsPerCard);

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

	// BUG-031: 드래그 — 일정 거리 (DRAG_THRESHOLD_PX) 이상 이동 전까지는
	// setPointerCapture 를 미루어 카드의 click 이벤트가 정상 디스패치되게.
	// (capture 가 활성화되면 click 이 캡처 element 로 가버려 카드 navigate X.)
	const DRAG_THRESHOLD_PX = 5;
	let pointerDownX = $state(0);
	let pointerActive = $state(false);
	let captured = $state(false);
	function onPointerDown(e: PointerEvent) {
		if (!needsMarquee) return;
		pointerActive = true;
		pointerDownX = e.clientX;
		dragStartX = e.clientX;
		dragStartScroll = scrollX;
	}
	function onPointerMove(e: PointerEvent) {
		if (!pointerActive) return;
		const totalDx = e.clientX - pointerDownX;
		if (!isDragging) {
			if (Math.abs(totalDx) < DRAG_THRESHOLD_PX) return;
			// 임계값 초과 — 드래그로 확정. 이 시점에 capture.
			isDragging = true;
			(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
			captured = true;
		}
		let next = dragStartScroll - (e.clientX - dragStartX);
		next = ((next % seqWidth) + seqWidth) % seqWidth;
		scrollX = next;
	}
	function onPointerUp(e: PointerEvent) {
		const wasDragging = isDragging;
		pointerActive = false;
		isDragging = false;
		if (captured) {
			(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
			captured = false;
		}
		if (wasDragging) {
			dragPauseUntil = performance.now() + 2000;
			// BUG-033: 드래그 직후 발화되는 click 은 카드 anchor 의 navigate 를
			// 막아야 함 (드래그 의도였는데 navigate 되면 UX 망함). 다음 click
			// 한 번만 swallow.
			suppressNextClick = true;
		}
	}

	// BUG-033: 드래그 종료 직후 한 번의 click 을 막기 위한 flag.
	// click 이벤트는 pointerup 직후 발화 — capture 단계에서 잡아 preventDefault.
	let suppressNextClick = false;
	function onClickCapture(e: MouseEvent) {
		if (suppressNextClick) {
			e.preventDefault();
			e.stopPropagation();
			suppressNextClick = false;
		}
	}
</script>

{#if summaries.length === 0}
	<div class="empty">{displayEmpty}</div>
{:else}
	<div
		class="conveyor"
		class:dragging={isDragging}
		class:marquee={needsMarquee}
		bind:this={viewportEl}
		onmouseenter={() => (hoverPause = true)}
		onmouseleave={() => (hoverPause = false)}
		onpointerdown={onPointerDown}
		onpointermove={onPointerMove}
		onpointerup={onPointerUp}
		onpointercancel={onPointerUp}
		onclickcapture={onClickCapture}
		role="region"
		aria-label={mode === 'overdue'
			? t('conveyor.overdueCampaigns', $locale)
			: t('conveyor.upcomingCampaigns', $locale)}
	>
		<div
			class="track"
			bind:this={trackEl}
			style:transform={needsMarquee ? `translateX(${-scrollX}px)` : ''}
		>
			{#each summaries as s (s.id)}
				<div class="slot" style:flex-basis={`${effCardW}px`}>
					<CampaignCard summary={s} {mode} {now} />
				</div>
			{/each}
			{#if needsMarquee}
				<!-- 첫↔끝 시각 분리 spacer. -->
				<div class="spacer" style:flex-basis={`${effCardW}px`} aria-hidden="true"></div>
				{#each summaries as s, i (`dup-${i}`)}
					<div class="slot" style:flex-basis={`${effCardW}px`} aria-hidden="true">
						<CampaignCard summary={s} {mode} {now} />
					</div>
				{/each}
				<!-- 두번째 시퀀스 뒤에도 동일 spacer — 사이클 균형. -->
				<div class="spacer" style:flex-basis={`${effCardW}px`} aria-hidden="true"></div>
			{/if}
		</div>
	</div>

	{#if needsMarquee}
		<div class="controls">
			<button
				class="play-pause"
				type="button"
				onclick={() => (userPaused = !userPaused)}
				aria-label={userPaused ? t('carousel.play', $locale) : t('carousel.pause', $locale)}
				title={userPaused ? t('carousel.autoPlay', $locale) : t('carousel.autoPause', $locale)}
			>
				<PlayPauseIcon paused={userPaused} />
			</button>
		</div>
	{/if}
{/if}

<style>
	.empty {
		color: var(--text-faint);
		font-size: 0.875rem;
		padding: 1rem 0;
	}

	.conveyor {
		overflow: hidden;
		/* BUG-191: 터치에서 손가락을 대면 브라우저가 기본 제스처(세로 스크롤)로
		   포인터를 가져가 pointermove 가 끊기고, 자동 흐름만 멈춘 채 아무 일도
		   안 일어났다(admin 보고). 가로는 우리가 처리하니 세로만 브라우저에
		   넘긴다. */
		touch-action: pan-y;
		padding: 0.25rem 0 0.5rem 0;
		user-select: none;
	}
	/* BUG-031: fade mask 와 grab 커서는 marquee 가 실제로 돌고 있을 때만 적용.
	   카드가 한 화면에 다 들어가는 경우엔 fade 가 좌측을 가려 방해됨. */
	.conveyor.marquee {
		cursor: grab;
		-webkit-mask-image: linear-gradient(
			90deg,
			transparent 0,
			black 32px,
			black calc(100% - 32px),
			transparent 100%
		);
		mask-image: linear-gradient(
			90deg,
			transparent 0,
			black 32px,
			black calc(100% - 32px),
			transparent 100%
		);
	}
	.conveyor.marquee.dragging {
		cursor: grabbing;
	}

	.track {
		display: flex;
		gap: 0.75rem;
		width: max-content;
		will-change: transform;
	}
	.slot {
		flex: 0 0 200px;
	}
	.spacer {
		flex: 0 0 200px;
	}

	/* BUG-031: 실제 드래그 중에만 슬롯 클릭 막음 (capture 이후). 임계값 미만은
	   click 으로 분기되어 자연스럽게 카드 navigate. */
	.conveyor.marquee.dragging .slot {
		pointer-events: none;
	}

	.controls {
		display: flex;
		justify-content: center;
		margin-top: 0.25rem;
	}
	.play-pause {
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 50%;
		width: 1.8rem;
		height: 1.8rem;
		font-size: 0.85rem;
		line-height: 1;
		cursor: pointer;
		transition: background 0.15s;
	}
	.play-pause:hover {
		background: var(--bg-subtle);
	}
</style>
