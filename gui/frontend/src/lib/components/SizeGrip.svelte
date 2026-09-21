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
	import { locale, t } from '$lib/stores/locale';

	let {
		/** 높이를 바꿀 대상. 없으면 아무 일도 안 한다(아직 안 그려진 경우). */
		target,
		/** 이보다 작아지지 않는다. */
		min = 64,
		/** 이보다 커지지 않는다. 0 이면 화면 높이의 90%. */
		max = 0,
		/** 화살표 한 번에 움직이는 크기. */
		step = 24
	}: { target?: HTMLElement; min?: number; max?: number; step?: number } = $props();

	let dragging = $state(false);
	let startY = 0;
	let startH = 0;

	function limit(h: number): number {
		const top = max || Math.round(window.innerHeight * 0.9);
		return Math.min(top, Math.max(min, h));
	}

	function apply(h: number) {
		if (!target) return;
		target.style.height = `${limit(h)}px`;
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
		apply(startH + (e.clientY - startY));
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
		apply(target.getBoundingClientRect().height + d);
	}
</script>

<button
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
		/* 손가락으로 집을 만한 높이 — 눈에 보이는 선은 그보다 얇다. */
		height: 1.125rem;
		padding: 0;
		border: none;
		background: transparent;
		cursor: ns-resize;
		/* 이게 없으면 끌 때 브라우저가 스크롤로 가져간다. */
		touch-action: none;
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
