<!--
  REQ-015: 2단 화면의 구분선 드래그 핸들 (도서관 / 규칙 공용).

  레이아웃의 **가운데 열**로 들어가 gap 자리를 그대로 차지한다. 그래서 기존
  간격·여백이 하나도 안 바뀌고, 사이드바의 `border-right`(눈에 보이는 구분선)
  바로 옆이 잡는 영역이 된다.

  `SearchPalette` 의 세로 핸들(`.dp-resize`)과 같은 방식이되, 그쪽은 팔레트
  안에서만 쓰는 1회용이라 재사용하지 않고 여기서 가로용으로 새로 만들었다.

  마우스만이 아니라 **키보드로도** 조절된다 — 드래그는 포인터가 있어야만
  가능한 조작이라 그것만 두면 키보드 사용자는 이 기능을 못 쓴다.
-->
<script lang="ts">
	import { get } from 'svelte/store';
	import {
		paneWidth,
		resetPaneWidth,
		applyDragDelta,
		clampPaneRem,
		MIN_PANE_REM,
		MAX_PANE_REM,
		type PaneId
	} from '$lib/stores/paneWidth';
	import { locale, t } from '$lib/stores/locale';

	let { pane }: { pane: PaneId } = $props();

	// `pane` 은 화면마다 고정이다(도서관은 항상 'library', 규칙은 'rules') —
	// 이 컴포넌트를 다른 pane 으로 바꿔 끼우는 사용처가 없으므로 최초 값으로
	// store 를 한 번만 잡는다. store 자체가 pane 별 싱글턴이라 재마운트해도
	// 같은 것을 돌려받는다.
	// svelte-ignore state_referenced_locally
	const width = paneWidth(pane);

	/** 그 시점의 root font-size — 배율이 바뀌면 값도 바뀐다. */
	function rootPx(): number {
		if (typeof document === 'undefined') return 16;
		return Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
	}

	function startDrag(e: PointerEvent) {
		// 마우스 오른쪽/가운데 버튼은 무시.
		if (e.button !== 0) return;
		e.preventDefault();
		const startX = e.clientX;
		const startRem = get(width);
		const px = rootPx();
		const target = e.currentTarget as HTMLElement;
		// pointer capture — 드래그 중 커서가 창 밖으로 나가도 이벤트가 이어진다.
		// 합성 이벤트(테스트/자동화)처럼 실제 포인터가 없으면 던지는데, 캡처는
		// 편의일 뿐이라 실패해도 드래그 자체는 계속돼야 한다.
		try {
			target.setPointerCapture(e.pointerId);
		} catch {
			/* 캡처 불가 — 이벤트는 target 에 직접 걸므로 그대로 진행. */
		}

		const onMove = (ev: PointerEvent) => {
			width.set(applyDragDelta(startRem, ev.clientX - startX, px));
		};
		const onUp = () => {
			try {
				target.releasePointerCapture?.(e.pointerId);
			} catch {
				/* 위에서 캡처가 안 됐으면 해제도 실패한다 — 무시. */
			}
			target.removeEventListener('pointermove', onMove);
			target.removeEventListener('pointerup', onUp);
			target.removeEventListener('pointercancel', onUp);
			document.body.style.userSelect = '';
			document.body.style.cursor = '';
		};
		// 드래그 중 텍스트가 선택되면 회색 반전이 따라다녀 산만하다.
		document.body.style.userSelect = 'none';
		document.body.style.cursor = 'col-resize';
		target.addEventListener('pointermove', onMove);
		target.addEventListener('pointerup', onUp);
		target.addEventListener('pointercancel', onUp);
	}

	/** 좌우 방향키 = 1rem, Home/End = 최소/최대. */
	function onKeyDown(e: KeyboardEvent) {
		const step = e.shiftKey ? 0.25 : 1;
		let next: number | null = null;
		if (e.key === 'ArrowLeft') next = get(width) - step;
		else if (e.key === 'ArrowRight') next = get(width) + step;
		else if (e.key === 'Home') next = MIN_PANE_REM;
		else if (e.key === 'End') next = MAX_PANE_REM;
		if (next === null) return;
		e.preventDefault();
		width.set(clampPaneRem(next));
	}
</script>

<!-- separator role 은 **focusable 이면** 조절 가능한 위젯으로 읽힌다(ARIA:
     "window splitter"). 그래서 tabindex 와 방향키 처리가 붙어 있는 것이 맞다 —
     svelte 의 a11y 린트는 separator 를 항상 non-interactive 로 보아 경고하지만
     이 조합은 규격대로다. aria-valuenow 는 정수 rem 으로 — 소수점까지 읽어
     주면 시끄럽다. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
	class="pane-resizer"
	role="separator"
	aria-orientation="vertical"
	aria-label={t('pane.resizeHandle', $locale)}
	aria-valuenow={Math.round($width)}
	aria-valuemin={MIN_PANE_REM}
	aria-valuemax={MAX_PANE_REM}
	tabindex="0"
	title={t('pane.resizeHint', $locale)}
	onpointerdown={startDrag}
	onkeydown={onKeyDown}
	ondblclick={() => resetPaneWidth(pane)}
></div>

<style>
	.pane-resizer {
		/* 레이아웃의 gap 열을 그대로 채운다 — 폭은 부모가 정한다. */
		align-self: stretch;
		cursor: col-resize;
		display: flex;
		align-items: center;
		justify-content: center;
		/* 평소엔 보이지 않는다. 사이드바의 border-right 가 이미 구분선이라
		   선을 하나 더 그리면 두 줄로 보인다. */
		background: transparent;
		border: none;
		padding: 0;
		/* 세로 스크롤 중 실수로 잡히지 않도록. */
		touch-action: none;
	}
	/* 잡을 수 있다는 것을 hover / focus 로만 알린다. */
	.pane-resizer::before {
		content: '';
		width: 0.1875rem;
		height: 2.5rem;
		border-radius: var(--r-pill);
		background: transparent;
		transition: background 0.12s;
	}
	.pane-resizer:hover::before {
		background: var(--text-faint);
	}
	.pane-resizer:active::before {
		background: var(--accent);
	}
	.pane-resizer:focus-visible {
		outline: var(--bw) solid var(--accent);
		outline-offset: -0.125rem;
	}
	.pane-resizer:focus-visible::before {
		background: var(--accent);
	}
</style>
