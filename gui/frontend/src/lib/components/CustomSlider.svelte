<!--
  DEV-101 fix3: 단일 onChange 즉시 적용 슬라이더.

  네이티브 `<input type="range">` 의 문제:
   - UI scale 슬라이더처럼 자기 자신을 움직이면 절대 위치 매핑이 깨짐.
   - value 가 외부 store 와 단방향 바인딩되어 reactive cascade 가 thumb 을 튕김.

  본 컴포넌트:
   - 트랙 click → thumb 이 그 위치로 점프 (즉시 commit).
   - thumb 잡고 drag → 매 pointermove 의 movementX (델타) 만 누적해 값 변경
     (즉시 commit). 마우스 절대 위치가 아닌 델타라서 페이지가 변형돼도
     사용자 입력 그대로 반영.
   - pointer capture 로 트랙 밖에서도 drag 유지.
   - 키보드 ← / → / Home / End / PageUp / PageDown.

  Props:
   - value: number       (현재 값 — 외부 source of truth)
   - min / max / step
   - onChange(v): 값이 한 step 이상 변할 때마다 호출 (drag 중에도 매번)
   - ariaLabel
   - disabled
-->
<script lang="ts">
	// DEV-101 / DEV-093 후속: pure math 는 lib/utils/slider 로 추출 + vitest 회귀.
	import { onDestroy } from 'svelte';
	import {
		clampToStep as clampToStepUtil,
		valueFromTrackPx,
		pixelsPerUnit as pixelsPerUnitUtil
	} from '$lib/utils/slider';

	type Props = {
		value: number;
		min: number;
		max: number;
		step?: number;
		onChange?: (v: number) => void;
		// BUG-141 후속 4차: 드래그 시작/종료 알림 — 값이 비싼 부수효과(예: UI
		// scale 의 root font-size 변경 → 전체 reflow)를 유발하는 소비자가
		// 드래그 중엔 값싼 프리뷰만 하고 실제 반영은 드래그 종료 시 1회로
		// 미룰 수 있도록. 일반 슬라이더는 안 써도 무방(옵셔널).
		onDragStart?: () => void;
		onDragEnd?: () => void;
		ariaLabel?: string;
		disabled?: boolean;
	};

	let {
		value,
		min,
		max,
		step = 1,
		onChange,
		onDragStart,
		onDragEnd,
		ariaLabel,
		disabled = false
	}: Props = $props();

	let track: HTMLDivElement | undefined = $state(undefined);
	let dragging = $state(false);
	// BUG-141 후속: 이전엔 매 pointermove 의 movementX 를 누적했는데,
	// WebKitGTK 가 연속 드래그 중 다수의 raw motion 샘플을 적은 수의
	// pointermove 이벤트로 coalescing 하는 경우 movementX 누적이 어긋나
	// (드롭된 샘플만큼 델타 유실) "값이 뚝뚝 끊겨서 갱신"되는 것으로 보임
	// (체감상 프레임 제한 걸린 것처럼). 대신 드래그 시작 시점의 절대
	// screenX 를 기준으로 매번 새로 계산 — 이벤트가 얼마나 많이
	// coalescing 되든 각 이벤트 시점의 절대 위치만 정확하면 되므로 누적
	// 오차가 생기지 않는다. screenX 는 CSS transform(페이지 zoom) 의
	// 영향을 안 받는다는 movementX 의 장점은 그대로 유지.
	let dragStartScreenX = 0;
	let dragStartValue = 0;
	let pxPerUnit = 1;

	// BUG-141 후속 2차: localStorage/font-size 등 다운스트림 개별 지점을
	// rAF 로 묶어도 여전히 버벅임 — 원인은 그쪽이 아니라 pointermove 자체가
	// (setPointerCapture 상태에서) 프레임당 여러 번 들어올 때마다 매번
	// onChange → store.set → Svelte 반응성 사이클 전체를 돌리고 있었던 것.
	// 다운스트림 각 지점을 개별로 막는 대신 여기, 즉 소스(raw pointermove)
	// 에서부터 프레임당 최대 1회로 상한 — 이러면 onChange 이후에 뭐가
	// 붙든(스토어 반응성, DOM 쓰기, persist 등) 전부 자동으로 보호된다.
	let moveRafId: number | null = null;
	let pendingStepped: number | null = null;

	function clampToStep(v: number): number {
		return clampToStepUtil(v, min, max, step);
	}

	function valueFromTrackX(clientX: number): number {
		if (!track) return value;
		const r = track.getBoundingClientRect();
		return valueFromTrackPx(clientX - r.left, r.width, min, max, step);
	}

	function commit(v: number) {
		const next = clampToStep(v);
		if (next !== value) {
			onChange?.(next);
		}
	}

	// BUG-141 후속 3차: `setPointerCapture` 는 GDK/X11 포인터 grab 과 엮여
	// 있어, WebKitGTK 에서 캡처가 걸린 상태로 DOM 을 갱신하면 컴포지터의
	// 프레임 스케줄링 자체가 막히는 것으로 의심됨(rAF 로 호출 빈도를 프레임당
	// 1회로 낮춰도 여전히 심하게 버벅임 — 빈도 문제가 아니라 캡처 자체가
	// 원인일 가능성). 캡처 대신 `window` 레벨 리스너로 드래그를 추적 —
	// 트랙 밖으로 나가도 이동은 그대로 잡히면서, GDK 포인터 grab 은 걸지
	// 않는다.
	function startDrag(e: PointerEvent) {
		if (disabled || !track) return;
		// 트랙 click 위치로 바로 점프.
		const r = track.getBoundingClientRect();
		pxPerUnit = pixelsPerUnitUtil(r.width, min, max);
		const v = valueFromTrackX(e.clientX);
		dragStartScreenX = e.screenX;
		dragStartValue = v;
		dragging = true;
		onDragStart?.();
		commit(v);
		e.preventDefault();
		window.addEventListener('pointermove', onMove);
		window.addEventListener('pointerup', endDrag);
		window.addEventListener('pointercancel', endDrag);
	}

	function flushMove() {
		moveRafId = null;
		if (pendingStepped === null) return;
		const stepped = pendingStepped;
		pendingStepped = null;
		if (stepped !== value) {
			commit(stepped);
		}
	}

	function onMove(e: PointerEvent) {
		if (!dragging || !track) return;
		// BUG-141 후속: 누적이 아니라 드래그 시작점 대비 절대 델타 — 이벤트
		// coalescing 에 영향받지 않음(위 dragStartScreenX 주석 참조).
		const delta = (e.screenX - dragStartScreenX) / pxPerUnit;
		const next = Math.max(min, Math.min(max, dragStartValue + delta));
		pendingStepped = clampToStep(next);
		if (moveRafId === null) moveRafId = requestAnimationFrame(flushMove);
	}

	function endDrag() {
		if (!dragging) return;
		dragging = false;
		window.removeEventListener('pointermove', onMove);
		window.removeEventListener('pointerup', endDrag);
		window.removeEventListener('pointercancel', endDrag);
		// 드래그 중 마지막 pending 이동이 아직 rAF 를 못 탄 채 남아있으면
		// 즉시 반영 — 안 그러면 놓친 마지막 값이 있는 채로 onDragEnd 의
		// "최종 커밋"이 그 이전 값으로 실행될 수 있음.
		if (moveRafId !== null) {
			cancelAnimationFrame(moveRafId);
			flushMove();
		}
		onDragEnd?.();
	}

	onDestroy(() => {
		if (moveRafId !== null) cancelAnimationFrame(moveRafId);
		window.removeEventListener('pointermove', onMove);
		window.removeEventListener('pointerup', endDrag);
		window.removeEventListener('pointercancel', endDrag);
	});

	function onKey(e: KeyboardEvent) {
		if (disabled) return;
		let next: number | null = null;
		switch (e.key) {
			case 'ArrowLeft':
			case 'ArrowDown':
				next = clampToStep(value - step);
				break;
			case 'ArrowRight':
			case 'ArrowUp':
				next = clampToStep(value + step);
				break;
			case 'PageDown':
				next = clampToStep(value - step * 10);
				break;
			case 'PageUp':
				next = clampToStep(value + step * 10);
				break;
			case 'Home':
				next = min;
				break;
			case 'End':
				next = max;
				break;
			default:
				return;
		}
		e.preventDefault();
		commit(next);
	}

	let fillPct = $derived(((value - min) / (max - min)) * 100);
</script>

<div
	class="slider"
	class:dragging
	class:disabled
	role="slider"
	tabindex={disabled ? -1 : 0}
	aria-valuemin={min}
	aria-valuemax={max}
	aria-valuenow={value}
	aria-label={ariaLabel}
	aria-disabled={disabled}
	onkeydown={onKey}
	onpointerdown={startDrag}
	onpointercancel={endDrag}
	bind:this={track}
>
	<div class="track">
		<div class="fill" style:width="{fillPct}%"></div>
	</div>
	<div class="thumb" style:left="{fillPct}%"></div>
</div>

<style>
	.slider {
		position: relative;
		flex: 1;
		min-width: 8rem;
		height: 1.5rem;
		display: flex;
		align-items: center;
		cursor: pointer;
		touch-action: none;
		user-select: none;
		outline: none;
	}
	.slider.disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}
	.slider:focus-visible .thumb {
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 35%, transparent);
	}
	.track {
		position: absolute;
		left: 0;
		right: 0;
		top: 50%;
		transform: translateY(-50%);
		height: 0.25rem;
		border-radius: var(--r-xs);
		background: var(--bg-subtle);
	}
	.fill {
		height: 100%;
		background: var(--accent);
		border-radius: var(--r-xs);
	}
	.thumb {
		position: absolute;
		top: 50%;
		width: 0.875rem;
		height: 0.875rem;
		border-radius: 50%;
		background: var(--accent);
		border: 2px solid var(--bg);
		transform: translate(-50%, -50%);
		transition: box-shadow 0.1s;
	}
	.slider.dragging .thumb {
		box-shadow: 0 0 0 6px color-mix(in srgb, var(--accent) 25%, transparent);
	}
</style>
