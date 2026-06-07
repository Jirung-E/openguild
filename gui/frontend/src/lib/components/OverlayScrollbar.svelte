<!--
  DEV-074 fix13: window 스크롤 overlay 스크롤바.

  목적:
   - 짧은 화면에서 항상 예약된 우측 띠 (scrollbar-gutter: stable) 의 거슬림 제거.
   - 스크롤 필요할 때만 우측에 얇은 thumb 가 컨텐츠 *위* 에 떠 있고 컨텐츠 폭은
     0px 만큼만 차지 — 진짜 overlay.

  방식:
   - global.css 에서 `html` 의 native scrollbar 숨김 (스크롤 자체는 유지).
   - 본 컴포넌트가 `<svelte:window>` 로 scrollY / 뷰포트 / 문서 높이 추적.
   - position: fixed thumb 만 우측에 그리고 pointer drag 로 window.scrollTo.
   - 스크롤 / drag / hover 시 잠깐 보이고 1.2초 후 사라짐.
   - 라이브러리 X — 순수 Svelte + CSS, 약 80 LOC.

  한계:
   - window 스크롤만 처리. 내부 overflow 컨테이너 (CodeMirror / 모달 list 등) 는
     기존 thin 스타일 그대로 (별도 quest 시 inner 도 동일 패턴 적용 가능).
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';

	let scrollY = $state(0);
	let viewportH = $state(0);
	let docHeight = $state(0);
	let visible = $state(false);
	let dragging = $state(false);
	let hideTimer: ReturnType<typeof setTimeout> | null = null;
	let ro: ResizeObserver | null = null;

	function measure() {
		if (typeof window === 'undefined') return;
		scrollY = window.scrollY;
		viewportH = window.innerHeight;
		docHeight = document.documentElement.scrollHeight;
	}

	let needed = $derived(docHeight > viewportH + 1);
	let thumbH = $derived.by(() => {
		if (!needed) return 0;
		// 최소 32px — 너무 짧으면 잡기 어려움.
		return Math.max(32, (viewportH / docHeight) * viewportH);
	});
	let maxScroll = $derived(Math.max(0, docHeight - viewportH));
	let trackH = $derived(Math.max(0, viewportH - thumbH));
	let thumbTop = $derived(maxScroll > 0 ? (scrollY / maxScroll) * trackH : 0);

	function showTemp() {
		visible = true;
		if (hideTimer) clearTimeout(hideTimer);
		hideTimer = setTimeout(() => {
			if (!dragging) visible = false;
		}, 1200);
	}

	function onScroll() {
		scrollY = window.scrollY;
		showTemp();
	}

	function onResize() {
		measure();
	}

	let dragStartY = 0;
	let dragStartScroll = 0;

	function onDown(e: PointerEvent) {
		e.preventDefault();
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		dragging = true;
		dragStartY = e.clientY;
		dragStartScroll = window.scrollY;
		visible = true;
	}

	function onMove(e: PointerEvent) {
		if (!dragging || trackH <= 0) return;
		const dy = e.clientY - dragStartY;
		const ratio = dy / trackH;
		window.scrollTo(0, dragStartScroll + ratio * maxScroll);
	}

	function onUp(e: PointerEvent) {
		if (!dragging) return;
		try {
			(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
		} catch {
			/* 이미 해제됐을 수 있음 */
		}
		dragging = false;
		showTemp();
	}

	onMount(() => {
		measure();
		window.addEventListener('scroll', onScroll, { passive: true });
		window.addEventListener('resize', onResize);
		ro = new ResizeObserver(measure);
		ro.observe(document.documentElement);
		ro.observe(document.body);
	});

	onDestroy(() => {
		window.removeEventListener('scroll', onScroll);
		window.removeEventListener('resize', onResize);
		ro?.disconnect();
		if (hideTimer) clearTimeout(hideTimer);
	});
</script>

{#if needed}
	<div
		class="overlay-thumb"
		class:visible
		class:dragging
		style:height="{thumbH}px"
		style:top="{thumbTop}px"
		onpointerdown={onDown}
		onpointermove={onMove}
		onpointerup={onUp}
		onpointercancel={onUp}
		role="scrollbar"
		tabindex="-1"
		aria-controls="body"
		aria-valuenow={maxScroll > 0 ? Math.round((scrollY / maxScroll) * 100) : 0}
		aria-valuemin="0"
		aria-valuemax="100"
		aria-orientation="vertical"
	></div>
{/if}

<style>
	.overlay-thumb {
		position: fixed;
		right: 3px;
		width: 7px;
		background: var(--scrollbar-thumb);
		border-radius: 4px;
		z-index: 9999;
		pointer-events: auto;
		cursor: pointer;
		opacity: 0;
		transition: opacity 0.2s, width 0.12s, background 0.12s;
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
