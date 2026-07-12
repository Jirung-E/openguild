<!--
  DEV-074 fix13 / fix14: window 또는 임의 overflow 컨테이너 overlay 스크롤바.

  방식:
   - target 지정 안 함 → window 스크롤 추적, position: fixed thumb 우측에.
   - target = HTMLElement → 그 컨테이너의 scrollTop / scrollHeight / clientHeight
     추적. position: fixed thumb 을 target 의 getBoundingClientRect 우측 가장자리
     에 맞춰 그림 (viewport 좌표계 fixed 라 페이지 / 컨테이너 스크롤 모두 자연).
   - 양쪽 모두 컨텐츠 폭 0px 차지.

  사용:
   - 기본: `<OverlayScrollbar />` (layout 에 한 번)
   - 컨테이너: `<OverlayScrollbar target={listEl} />` (그 div 의 native scrollbar
     는 별도 CSS 로 숨겨야 함 — `scrollbar-width: none` + webkit pseudo).

  라이브러리 X — 순수 Svelte + CSS, 약 130 LOC.
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';

	type Props = { target?: HTMLElement | null | undefined };
	let { target = null }: Props = $props();

	let scrollTop = $state(0);
	let viewportH = $state(0);
	let contentH = $state(0);
	// target 의 viewport 내 좌표. null = window 전용 (오른쪽 끝, top 0).
	let rectTop = $state(0);
	let rectRight = $state(0);

	let visible = $state(false);
	let dragging = $state(false);
	let hideTimer: ReturnType<typeof setTimeout> | null = null;
	let ro: ResizeObserver | null = null;
	// 컨텐츠(scrollHeight) 변화 감지용 — ResizeObserver 는 컨테이너 자신의 box
	// 크기만 보고, 필터/접기 등으로 내부 행이 늘거나 줄어 scrollHeight 가 바뀌는
	// 건 못 잡는다 (= thumb 크기가 안 갱신돼 끝까지 안 내려가는 버그). 자식
	// 추가/삭제(childList) 를 MutationObserver 로 잡아 재측정.
	let mo: MutationObserver | null = null;
	let scrollUnsub: (() => void) | null = null;
	let rafHandle: number | null = null;

	function measure() {
		if (typeof window === 'undefined') return;
		if (target) {
			scrollTop = target.scrollTop;
			viewportH = target.clientHeight;
			contentH = target.scrollHeight;
			const r = target.getBoundingClientRect();
			rectTop = r.top;
			rectRight = window.innerWidth - r.right;
		} else {
			scrollTop = window.scrollY;
			viewportH = window.innerHeight;
			contentH = document.documentElement.scrollHeight;
			rectTop = 0;
			rectRight = 0;
		}
	}

	function scheduleRemeasure() {
		if (rafHandle !== null) return;
		rafHandle = requestAnimationFrame(() => {
			rafHandle = null;
			measure();
		});
	}

	let needed = $derived(contentH > viewportH + 1);
	let thumbH = $derived.by(() => {
		if (!needed) return 0;
		return Math.max(32, (viewportH / contentH) * viewportH);
	});
	let maxScroll = $derived(Math.max(0, contentH - viewportH));
	let trackH = $derived(Math.max(0, viewportH - thumbH));
	let thumbTop = $derived(rectTop + (maxScroll > 0 ? (scrollTop / maxScroll) * trackH : 0));
	let thumbRight = $derived(rectRight + 3);

	function showTemp() {
		visible = true;
		if (hideTimer) clearTimeout(hideTimer);
		hideTimer = setTimeout(() => {
			if (!dragging) visible = false;
		}, 1200);
	}

	function onScroll() {
		if (target) {
			scrollTop = target.scrollTop;
		} else {
			scrollTop = window.scrollY;
		}
		showTemp();
	}

	function onWinResize() {
		measure();
	}

	function onWinScroll() {
		// window 스크롤로 target 의 viewport 위치 변동 → rectTop 재측정.
		if (target) scheduleRemeasure();
	}

	let dragStartY = 0;
	let dragStartScroll = 0;

	function onDown(e: PointerEvent) {
		e.preventDefault();
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		dragging = true;
		dragStartY = e.clientY;
		dragStartScroll = scrollTop;
		visible = true;
	}

	function onMove(e: PointerEvent) {
		if (!dragging || trackH <= 0) return;
		const dy = e.clientY - dragStartY;
		const ratio = dy / trackH;
		const next = dragStartScroll + ratio * maxScroll;
		if (target) {
			target.scrollTop = next;
		} else {
			window.scrollTo(0, next);
		}
	}

	function onUp(e: PointerEvent) {
		if (!dragging) return;
		try {
			(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
		} catch {
			/* 이미 해제 */
		}
		dragging = false;
		showTemp();
	}

	onMount(() => {
		measure();
		const scrollSrc: Window | HTMLElement = target ?? window;
		const handler = onScroll;
		scrollSrc.addEventListener('scroll', handler, { passive: true });
		scrollUnsub = () => scrollSrc.removeEventListener('scroll', handler);
		window.addEventListener('resize', onWinResize);
		// target 모드: window 스크롤로 target 의 위치 변하면 fixed thumb 좌표도 갱신.
		if (target) {
			window.addEventListener('scroll', onWinScroll, { passive: true });
		}
		ro = new ResizeObserver(scheduleRemeasure);
		ro.observe(target ?? document.documentElement);
		if (!target) ro.observe(document.body);
		// 컨텐츠 mutation (행 추가/삭제, 댓글 접기/펼치기 등) → 재측정.
		const moTarget = target ?? document.body;
		mo = new MutationObserver(scheduleRemeasure);
		mo.observe(moTarget, { childList: true, subtree: true });
	});

	onDestroy(() => {
		scrollUnsub?.();
		window.removeEventListener('resize', onWinResize);
		if (target) window.removeEventListener('scroll', onWinScroll);
		ro?.disconnect();
		mo?.disconnect();
		if (hideTimer) clearTimeout(hideTimer);
		if (rafHandle !== null) cancelAnimationFrame(rafHandle);
	});
</script>

{#if needed}
	<div
		class="overlay-thumb"
		class:visible
		class:dragging
		style:height="{thumbH}px"
		style:top="{thumbTop}px"
		style:right="{thumbRight}px"
		style:z-index={target ? 9999 : 90}
		onpointerdown={onDown}
		onpointermove={onMove}
		onpointerup={onUp}
		onpointercancel={onUp}
		role="scrollbar"
		tabindex="-1"
		aria-controls="scroll-target"
		aria-valuenow={maxScroll > 0 ? Math.round((scrollTop / maxScroll) * 100) : 0}
		aria-valuemin="0"
		aria-valuemax="100"
		aria-orientation="vertical"
	></div>
{/if}

<style>
	.overlay-thumb {
		position: fixed;
		width: 7px;
		background: var(--scrollbar-thumb);
		border-radius: 4px;
		z-index: 9999;
		pointer-events: auto;
		cursor: pointer;
		opacity: 0;
		transition:
			opacity 0.2s,
			width 0.12s,
			background 0.12s;
		touch-action: none;
	}
	.overlay-thumb.visible,
	.overlay-thumb.dragging,
	.overlay-thumb:hover {
		opacity: 1;
	}
	.overlay-thumb:hover,
	.overlay-thumb.dragging {
		width: 10px;
		background: var(--scrollbar-thumb-hover);
	}
</style>
