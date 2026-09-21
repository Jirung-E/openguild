<!--
  BUG-318: **손가락으로 입력칸 크기를 바꾼다.**

  지금까지는 CSS `resize: vertical` 하나에 기댔다. 그건 브라우저가 오른쪽 아래 구석에
  그려 주는 손잡이인데, **모바일에서는 사실상 못 쓴다** — iOS 사파리는 `resize` 를 아예
  무시하고, 안드로이드 크롬은 그리기는 해도 손가락으로 끌리지 않는다. 그래서 휴대폰에서는
  입력칸 높이를 바꿀 방법이 없었다(admin).

  마우스와 손가락을 함께 받는 손잡이를 직접 둔다(포인터 이벤트 하나로 둘 다 받는다).
  데스크톱의 기존 손잡이는 그대로 둔다 — 익숙한 자리를 뺏을 이유가 없다.

  키보드로도 바꿀 수 있어야 한다(위/아래 화살표). 그래서 버튼이다.
-->
<script lang="ts">
	import { getContext } from 'svelte';
	import { locale, t } from '$lib/stores/locale';
	import { COMPOSE_DOCK, type ComposeDockContext } from '$lib/utils/compose-dock-context';

	/**
	 * 한계값의 단위는 **rem 이다**(BUG-321, admin: "사이즈 조절 핸들은 ui 크기에 따라
	 * 달라져야한다"). 이 앱은 UI 크기를 루트 글꼴로 조절한다([[uiScale]]) — px 로 못 박으면
	 * 크게 쓰는 사람에게는 최소 높이가 상대적으로 쪼그라들고, 화살표 한 칸도 너무 잘다.
	 * 모양(CSS)은 이미 rem 이라 따라간다.
	 *
	 * 화면 높이 한계만 예외다 — 화면은 UI 크기와 무관하게 그만큼이다.
	 */
	let {
		/** 높이를 바꿀 대상. 없으면 아무 일도 안 한다(아직 안 그려진 경우). */
		target,
		/** 이보다 작아지지 않는다(rem). */
		min = 4,
		/** 이보다 커지지 않는다(rem). 0 이면 화면 높이의 90%. */
		max = 0,
		/** 화살표 한 번에 움직이는 크기(rem). */
		step = 1.5
	}: { target?: HTMLElement; min?: number; max?: number; step?: number } = $props();

	/** 지금 1rem 이 몇 px 인가 — UI 크기를 바꾸면 이 값이 바뀐다. */
	function rem(): number {
		const px = parseFloat(getComputedStyle(document.documentElement).fontSize);
		return Number.isFinite(px) && px > 0 ? px : 16;
	}

	let root: HTMLButtonElement | undefined = $state(undefined);

	/**
	 * BUG-323: **팝업일 때는 위쪽에 붙는다**(admin: "아래쪽에 붙이니까 확장시키는게 불편함").
	 *
	 * 팝업은 화면 바닥에 붙어 있다. 그러니 아래 손잡이를 내려 끌어도 상자는 **위로** 자란다 —
	 * 손이 가는 방향과 자라는 방향이 반대다. 위쪽에 두고 위로 끌면 손 가는 대로 커진다.
	 *
	 * 팝업 밖(제자리·작업 기록·메모 상자)에서는 그대로 아래다.
	 */
	const dock = getContext<ComposeDockContext | undefined>(COMPOSE_DOCK);
	const atTop = $derived(dock?.docked() ?? false);

	/** 위아래로 남길 틈(rem). 2px 어치다. */
	const SNUG = 0.125;

	/**
	 * **위아래 간격을 같게 맞춘다**(admin).
	 *
	 * 손잡이가 붙는 자리는 곳마다 다르다 — 줄 간격이 0.4rem 인 곳, 0.35rem 인 곳, 아예
	 * 없는 곳(작업 기록)이 있고, 바로 아래 오는 [보내기] 줄은 제 위쪽 여백을 한 번 더
	 * 얹는다. 그래서 여백을 CSS 에 못 박으면 어디선가는 겹치고 어디선가는 벌어진다.
	 * 실제로 그랬다 — 한 곳을 2px 로 맞추니 작업 기록에서는 4.4px 을 파고들었다.
	 *
	 * 그래서 **붙고 나서 제자리를 보고** 정한다. 늘어선 방식(flex/grid)에서는 줄 간격과
	 * 아래 것의 위쪽 여백을 빼면 정확히 맞는다. 평범한 블록에서는 여백이 서로 합쳐지므로
	 * (margin collapsing) 빼지 않고 원하는 만큼만 둔다 — 겹치는 일이 없다.
	 */
	function snug() {
		const el = root;
		const parent = el?.parentElement;
		if (!el || !parent) return;
		const want = SNUG * rem();
		const ps = getComputedStyle(parent);
		const lanes = ps.display.includes('flex') || ps.display.includes('grid');
		if (!lanes) {
			el.style.marginTop = `${want}px`;
			el.style.marginBottom = `${want}px`;
			return;
		}
		const gap = parseFloat(ps.rowGap) || 0;
		if (atTop) {
			// 위로 올라가면(`order`) 아래에 오는 것은 입력칸이다 — 제 여백을 따로 안 얹는다.
			el.style.marginTop = `${want - gap}px`;
			el.style.marginBottom = `${want - gap}px`;
			return;
		}
		const next = el.nextElementSibling;
		const nextMt =
			next instanceof HTMLElement ? parseFloat(getComputedStyle(next).marginTop) || 0 : 0;
		el.style.marginTop = `${want - gap}px`;
		el.style.marginBottom = `${want - gap - nextMt}px`;
	}

	$effect(() => {
		if (!root) return;
		void atTop; // 위아래가 바뀌면 여백도 다시 맞춘다.
		snug();
		// UI 크기를 바꾸면 이 버튼의 높이(rem)가 바뀐다 — 그걸 신호로 다시 맞춘다.
		const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => snug());
		ro?.observe(root);
		return () => ro?.disconnect();
	});

	let dragging = $state(false);
	let startY = 0;
	let startH = 0;

	function limit(h: number): number {
		const r = rem();
		// 화면보다 커질 수는 없다 — 이쪽은 UI 크기와 무관하다.
		const screen = Math.round(window.innerHeight * 0.9);
		const top = Math.min(max ? max * r : screen, screen);
		return Math.min(top, Math.max(min * r, h));
	}

	function apply(h: number) {
		if (!target) return;
		const px = limit(h);
		target.style.height = `${px}px`;
		// BUG-319: 붙은 입력창(ComposeDock)은 입력칸에 높이 상한을 걸어 둔다. 그걸 안 풀면
		// 손잡이로 정한 높이가 그냥 덮어써져 끌어도 안 움직인다. 상한을 **여기서 정해 준다** —
		// 변수는 물려받으므로 편집기 안쪽까지 같이 따라간다. 상한을 안 거는 곳에서는
		// 아무 일도 안 일어난다.
		target.style.setProperty('--compose-cap', `${px}px`);
	}

	function down(e: PointerEvent) {
		if (!target) return;
		startY = e.clientY;
		startH = target.getBoundingClientRect().height;
		dragging = true;
		// 손가락이 떠나도 이 손잡이가 계속 받는다 — 빠르게 끌면 손가락이 손잡이 밖으로 나간다.
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		// 손가락이면 이걸 안 막을 때 화면이 같이 스크롤된다.
		e.preventDefault();
	}

	function move(e: PointerEvent) {
		if (!dragging) return;
		const dy = e.clientY - startY;
		// 위쪽 손잡이는 올릴수록 커진다.
		apply(startH + (atTop ? -dy : dy));
	}

	function up(e: PointerEvent) {
		if (!dragging) return;
		dragging = false;
		(e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
	}

	function key(e: KeyboardEvent) {
		if (!target) return;
		const d = e.key === 'ArrowDown' ? step : e.key === 'ArrowUp' ? -step : 0;
		if (!d) return;
		e.preventDefault();
		// 손잡이가 위에 있으면 ↑ 가 키우는 쪽이다 — 손잡이가 가는 방향과 같다.
		apply(target.getBoundingClientRect().height + (atTop ? -d : d) * rem());
	}
</script>

<button
	bind:this={root}
	class="size-grip"
	class:dragging
	type="button"
	aria-label={t('common.resizeBox', $locale)}
	title={t('common.resizeBox', $locale)}
	onpointerdown={down}
	onpointermove={move}
	onpointerup={up}
	onpointercancel={up}
	onkeydown={key}
>
	<span class="grip-bar"></span>
</button>

<style>
	.size-grip {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		/* BUG-321: **자리는 적게, 집히기는 넉넉하게**(admin: 너무 많은 높이를 차지한다).
		   자리는 0.75rem 만 먹고, 실제로 집히는 영역은 아래 `::after` 가 위아래로 더 넓힌다.
		   위 상자와의 간격도 조금 당겨 붙인다 — 손잡이는 그 상자에 딸린 것처럼 보여야 한다. */
		position: relative;
		/* admin: 위아래 간격을 같게, 높이 8 — 100% 배율에서 2 + 8 + 2 = 12px 이다.
		   위아래 여백은 **붙는 자리를 보고 script 가 정한다**(아래 `snug`) — 부르는 곳마다
		   줄 간격이 달라서 여기에 못 박으면 어디선가는 겹치고 어디선가는 벌어진다. */
		height: 0.5rem;
		padding: 0;
		border: none;
		background: transparent;
		cursor: ns-resize;
		/* 이게 없으면 끌 때 브라우저가 스크롤로 가져간다. */
		touch-action: none;
	}
	/* 눈에는 안 보이지만 손가락은 여기까지 집을 수 있다. 버튼 안이라 눌림도 버튼으로 간다. */
	.size-grip::after {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		top: -0.4375rem;
		bottom: -0.4375rem;
	}
	.grip-bar {
		width: 2.5rem;
		height: 0.1875rem;
		border-radius: var(--r-pill);
		background: var(--border);
	}
	.size-grip:hover .grip-bar,
	.size-grip.dragging .grip-bar,
	.size-grip:focus-visible .grip-bar {
		background: var(--text-faint);
	}
</style>
