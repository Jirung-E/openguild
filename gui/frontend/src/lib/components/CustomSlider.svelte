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
		ariaLabel?: string;
		disabled?: boolean;
	};

	let { value, min, max, step = 1, onChange, ariaLabel, disabled = false }: Props = $props();

	let track: HTMLDivElement | undefined = $state(undefined);
	let dragging = $state(false);
	// drag 시작 시점의 값 — movementX 누적 베이스. drag 중 외부 value 가
	// 즉시 commit 으로 갱신되더라도 본 값을 기준으로 정확히 누적.
	let dragAcc = 0;
	let pxPerUnit = 1;

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

	function startDrag(e: PointerEvent) {
		if (disabled || !track) return;
		const target = e.currentTarget as HTMLElement;
		target.setPointerCapture(e.pointerId);
		// 트랙 click 위치로 바로 점프.
		const r = track.getBoundingClientRect();
		pxPerUnit = pixelsPerUnitUtil(r.width, min, max);
		const v = valueFromTrackX(e.clientX);
		dragAcc = v;
		dragging = true;
		commit(v);
		e.preventDefault();
	}

	function onMove(e: PointerEvent) {
		if (!dragging || !track) return;
		// 델타 — movementX 가 페이지 zoom / UI scale 변화와 무관.
		dragAcc = Math.max(min, Math.min(max, dragAcc + e.movementX / pxPerUnit));
		const stepped = clampToStep(dragAcc);
		if (stepped !== value) {
			commit(stepped);
		}
	}

	function endDrag(e: PointerEvent) {
		if (!dragging) return;
		const target = e.currentTarget as HTMLElement;
		try {
			target.releasePointerCapture(e.pointerId);
		} catch {
			/* 이미 해제됐을 수 있음 */
		}
		dragging = false;
	}

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
	onpointermove={onMove}
	onpointerup={endDrag}
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
		height: 4px;
		border-radius: 2px;
		background: var(--bg-subtle);
	}
	.fill {
		height: 100%;
		background: var(--accent);
		border-radius: 2px;
	}
	.thumb {
		position: absolute;
		top: 50%;
		width: 14px;
		height: 14px;
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
