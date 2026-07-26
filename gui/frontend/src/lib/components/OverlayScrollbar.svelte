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
	import { onDestroy } from 'svelte';

	type Props = { target?: HTMLElement | null | undefined };
	let { target = null }: Props = $props();

	// BUG-157: thumb 는 position:fixed + viewport 좌표(getBoundingClientRect)로
	// 그리는데, 조상에 transform/backdrop-filter 가 있으면(검색 팔레트의
	// translateX(-50%) 등) fixed 의 기준이 그 조상으로 바뀌어 thumb 가 화면 밖에
	// 그려졌다(= "스크롤바 안 보임"). DOM 위치와 무관하게 viewport 기준이 되도록
	// thumb 를 document.body 로 포털한다 — 이벤트 리스너는 노드에 붙어 있어 이동
	// 후에도 유지되고, svelte scoped class 도 노드에 hash 로 박혀 있어 스타일 유지.
	function portalToBody(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() {
				node.remove();
			}
		};
	}

	let scrollTop = $state(0);
	let viewportH = $state(0);
	let contentH = $state(0);
	// target 의 viewport 내 좌표. null = window 전용 (오른쪽 끝, top 0).
	let rectTop = $state(0);
	let rectRight = $state(0);
	// BUG-138: window 모드에서 sticky 헤더(타이틀바+메뉴바) 높이. 트랙을 그만큼
	// 아래에서 시작시켜 헤더에 가리거나 겹치지 않게. container 모드는 0.
	let topInset = $state(0);

	let visible = $state(false);
	let dragging = $state(false);
	let hideTimer: ReturnType<typeof setTimeout> | null = null;
	let ro: ResizeObserver | null = null;
	// 컨텐츠(scrollHeight) 변화 감지용 — ResizeObserver 는 컨테이너 자신의 box
	// 크기만 보고, 필터/접기 등으로 내부 행이 늘거나 줄어 scrollHeight 가 바뀌는
	// 건 못 잡는다 (= thumb 크기가 안 갱신돼 끝까지 안 내려가는 버그). 자식
	// 추가/삭제(childList) 를 MutationObserver 로 잡아 재측정.
	let mo: MutationObserver | null = null;
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
			topInset = 0;
		} else {
			scrollTop = window.scrollY;
			viewportH = window.innerHeight;
			contentH = document.documentElement.scrollHeight;
			rectTop = 0;
			rectRight = 0;
			// BUG-138: 스크롤바 트랙이 sticky 헤더(타이틀바+메뉴바) 아래에서
			// 시작하도록 그 높이만큼 inset. main 의 문서상 top(offsetTop) =
			// 앞선 in-flow sticky 헤더들의 높이 합. getBoundingClientRect+scrollY
			// 로 offsetParent 와 무관하게 구한다.
			const mainEl = document.querySelector('main');
			topInset = mainEl
				? Math.max(0, mainEl.getBoundingClientRect().top + window.scrollY)
				: 0;
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
	// BUG-138: 트랙 시작점(baseTop)과 트랙 영역 높이(regionH) — window 모드는
	// sticky 헤더 아래(topInset)에서 시작하고 그만큼 짧아진다. container 모드는
	// 기존대로 컨테이너 top(rectTop) 기준 전체 높이.
	let baseTop = $derived(target ? rectTop : topInset);
	let regionH = $derived(target ? viewportH : Math.max(0, viewportH - topInset));
	let thumbH = $derived.by(() => {
		if (!needed) return 0;
		return Math.max(32, Math.min(regionH, (viewportH / contentH) * regionH));
	});
	let maxScroll = $derived(Math.max(0, contentH - viewportH));
	let trackH = $derived(Math.max(0, regionH - thumbH));
	let thumbTop = $derived(baseTop + (maxScroll > 0 ? (scrollTop / maxScroll) * trackH : 0));
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

	// BUG-157 (재수정): 배선을 onMount 로 **한 번만** 하던 것이 컨테이너 모드가
	// 통째로 죽어 있던 진짜 원인이다.
	//
	// 호출측은 `<div bind:this={rowsEl}>` … `<OverlayScrollbar target={rowsEl} />`
	// 형태인데, 자식 컴포넌트의 onMount 시점엔 아직 `rowsEl` 이 null 이다.
	// 그래서 measure()/리스너/옵저버가 전부 **window 모드**로 붙어버리고,
	// 이후 prop 이 실제 엘리먼트로 갱신돼도 다시 배선하는 코드가 없었다.
	// 결과: 컨테이너 크기가 아니라 문서 전체 기준으로 계산 → 대개
	// `needed === false` → thumb 자체가 렌더되지 않음(= "스크롤바 안 보임").
	//
	// `$effect` 로 옮겨 target 이 바뀔 때마다 재측정 + 재배선한다(이전 배선은
	// cleanup 에서 해제). null → 엘리먼트 전환이 자동으로 처리된다.
	$effect(() => {
		const t = target; // 의존성 등록 — t 가 바뀌면 cleanup 후 재실행.
		if (typeof window === 'undefined') return;
		measure();

		const scrollSrc: Window | HTMLElement = t ?? window;
		const handler = onScroll;
		scrollSrc.addEventListener('scroll', handler, { passive: true });
		window.addEventListener('resize', onWinResize);
		// target 모드: window 스크롤로 target 의 위치가 변하면 fixed thumb 좌표도 갱신.
		if (t) window.addEventListener('scroll', onWinScroll, { passive: true });

		const localRo = new ResizeObserver(scheduleRemeasure);
		localRo.observe(t ?? document.documentElement);
		if (!t) localRo.observe(document.body);
		// 컨텐츠 mutation (행 추가/삭제, 댓글 접기/펼치기 등) → 재측정.
		const localMo = new MutationObserver(scheduleRemeasure);
		localMo.observe(t ?? document.body, { childList: true, subtree: true });
		ro = localRo;
		mo = localMo;

		return () => {
			scrollSrc.removeEventListener('scroll', handler);
			window.removeEventListener('resize', onWinResize);
			if (t) window.removeEventListener('scroll', onWinScroll);
			localRo.disconnect();
			localMo.disconnect();
		};
	});

	onDestroy(() => {
		if (hideTimer) clearTimeout(hideTimer);
		if (rafHandle !== null) cancelAnimationFrame(rafHandle);
	});
</script>

{#if needed}
	<div
		use:portalToBody
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
