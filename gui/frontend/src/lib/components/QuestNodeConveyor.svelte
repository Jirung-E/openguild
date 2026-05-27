<!--
  DEV-076: Home 페이지에서 "마감 임박 / Overdue" 퀘스트를 Quest Board 노드
  모양으로 가로 회전 표시. CampaignConveyor 와 거의 동일하지만 카드 폭이
  다르고 (NODE_W=284) 클릭 시 Quest 상세로 이동.

  - 한 화면에 다 안 들어가면 marquee 자동 회전.
  - 마우스 hover 정지 / 드래그 시 정지 + 2초 hold / 정지 토글 버튼.
  - overdue 인 항목은 빨간 stroke 강조 (overlayColor).
-->
<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	import { goto } from '$app/navigation';
	import type { Quest } from '$lib/types';
	import {
		makeQuestNodeSvgUrl,
		QUEST_NODE_W,
		QUEST_NODE_H
	} from '$lib/utils/quest-node-svg';

	let {
		quests,
		mode = 'imminent',
		secondsPerCard = 5
	}: {
		quests: Quest[];
		mode?: 'imminent' | 'overdue';
		secondsPerCard?: number;
	} = $props();

	const GAP_PX = 12;
	const CARD_W = QUEST_NODE_W;

	let viewportEl: HTMLDivElement | undefined = $state(undefined);
	let trackEl: HTMLDivElement | undefined = $state(undefined);
	let viewportW = $state(0);
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

	let cardsOnlyW = $derived(
		quests.length === 0 ? 0 : quests.length * CARD_W + (quests.length - 1) * GAP_PX
	);
	let needsMarquee = $derived(viewportW > 0 && cardsOnlyW > viewportW);
	let pixelsPerSec = $derived((CARD_W + GAP_PX) / secondsPerCard);

	function isPaused(t: number): boolean {
		if (isDragging || hoverPause || userPaused) return true;
		if (dragPauseUntil > 0 && t < dragPauseUntil) return true;
		return false;
	}

	function loop(t: number) {
		if (lastFrameTime === 0) lastFrameTime = t;
		const dt = (t - lastFrameTime) / 1000;
		lastFrameTime = t;
		if (needsMarquee && seqWidth > 0 && !isPaused(performance.now())) {
			scrollX += pixelsPerSec * dt;
			if (scrollX >= seqWidth) {
				scrollX -= seqWidth * Math.floor(scrollX / seqWidth);
			}
		}
		rafHandle = requestAnimationFrame(loop);
	}

	async function measure() {
		await tick();
		if (viewportEl) viewportW = viewportEl.clientWidth;
		if (trackEl) seqWidth = needsMarquee ? trackEl.scrollWidth / 2 : 0;
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
		void quests.length;
		measure();
	});
	onDestroy(() => {
		resizeObs?.disconnect();
		if (rafHandle !== null) cancelAnimationFrame(rafHandle);
	});

	// BUG-035: 드래그 임계값 — CampaignConveyor 와 동일 패턴. 임계값 미만은
	// capture 미루어 카드 click 자연 발화 보장.
	const DRAG_THRESHOLD_PX = 5;
	let pointerDownX = $state(0);
	let pointerActive = $state(false);
	let captured = $state(false);
	let suppressNextClick = false;
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
			suppressNextClick = true;
		}
	}
	function onClickCapture(e: MouseEvent) {
		if (suppressNextClick) {
			e.preventDefault();
			e.stopPropagation();
			suppressNextClick = false;
		}
	}

	// overdue 모드에서는 빨간 stroke overlay.
	const OVERLAY_COLOR = '#f85149';
	function overlayFor(_q: Quest): string | undefined {
		return mode === 'overdue' ? OVERLAY_COLOR : undefined;
	}

	function openQuest(q: Quest) {
		goto(`/quests/${encodeURIComponent(q.quest_id)}?from=home`);
	}
</script>

{#if quests.length > 0}
	<!-- BUG-036: class:marquee 가 일부 환경에서 반영 안 됨 (Vite HMR cache?).
	     인라인 style 로 mask + cursor 직접 — class 의존성 제거. -->
	<div
		class="conveyor"
		class:dragging={isDragging}
		class:marquee={needsMarquee}
		style:cursor={needsMarquee ? 'grab' : 'default'}
		style:-webkit-mask-image={needsMarquee
			? 'linear-gradient(90deg, transparent 0, #000 32px, #000 calc(100% - 32px), transparent 100%)'
			: 'none'}
		style:mask-image={needsMarquee
			? 'linear-gradient(90deg, transparent 0, #000 32px, #000 calc(100% - 32px), transparent 100%)'
			: 'none'}
		bind:this={viewportEl}
		onmouseenter={() => (hoverPause = true)}
		onmouseleave={() => (hoverPause = false)}
		onpointerdown={onPointerDown}
		onpointermove={onPointerMove}
		onpointerup={onPointerUp}
		onpointercancel={onPointerUp}
		onclickcapture={onClickCapture}
		role="region"
		aria-label={mode === 'overdue' ? '마감 지남 퀘스트' : '마감 임박 퀘스트'}
	>
		<div
			class="track"
			bind:this={trackEl}
			style:transform={needsMarquee ? `translateX(${-scrollX}px)` : ''}
		>
			<!-- BUG-035: SVG 노드 우하단에 이미 기한 표시 — 별도 .due-label 제거
			     (중복으로 두 번 보이던 문제). tooltip 의 기한도 SVG 와 동일하게
			     "유효 기한" (campaign / quest 중 더 가까운) 으로 표기. -->
			{#each quests as q (q.id)}
				<button
					type="button"
					class="slot"
					title={`${q.quest_id}  ${q.title}`}
					onclick={() => openQuest(q)}
				>
					<img
						src={makeQuestNodeSvgUrl(q, overlayFor(q))}
						alt={`${q.quest_id} ${q.title}`}
						width={CARD_W}
						height={QUEST_NODE_H}
						draggable="false"
					/>
				</button>
			{/each}
			{#if needsMarquee}
				<div class="spacer" aria-hidden="true"></div>
				{#each quests as q, i (`dup-${i}`)}
					<button
						type="button"
						class="slot"
						aria-hidden="true"
						tabindex="-1"
						onclick={() => openQuest(q)}
					>
						<img
							src={makeQuestNodeSvgUrl(q, overlayFor(q))}
							alt=""
							width={CARD_W}
							height={QUEST_NODE_H}
							draggable="false"
						/>
					</button>
				{/each}
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
	.conveyor {
		overflow: hidden;
		padding: 0.25rem 0 0.5rem 0;
		user-select: none;
	}
	/* BUG-036: fade mask / cursor 는 인라인 style 로 직접 적용 (위 div 참조).
	   .marquee class 는 .dragging combinator (drag 중 슬롯 click 차단) 용으로만 유지. */
	.conveyor.marquee.dragging { cursor: grabbing; }

	.track {
		display: flex;
		align-items: flex-start;
		gap: 0.75rem;
		width: max-content;
		will-change: transform;
	}
	.slot {
		flex: 0 0 284px;
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		color: inherit;
		display: flex;
		flex-direction: column;
		align-items: stretch;
	}
	.spacer { flex: 0 0 284px; }
	/* BUG-035: 실제 드래그 중 슬롯 클릭 차단. marquee 가 아닐 땐 자연스러운 click. */
	.conveyor.marquee.dragging .slot { pointer-events: none; }

	.slot img {
		display: block;
		border-radius: 6px;
	}
	/* BUG-035: due-label 제거 — SVG 노드 우하단에 이미 동일 정보 표시. */

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
