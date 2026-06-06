<!--
  DEV-101 fix2: 델타 드래그 슬라이더.

  네이티브 `<input type="range">` 는 두 문제:
   1. value 가 store/prop 와 양방향 바인딩 안 되면 drag 중 thumb 가 외부 reactivity
      때마다 원래 자리로 튕김.
   2. drag 중 페이지 layout 이 바뀌면 (UI scale 슬라이더처럼 자기 자신을 움직이게
      하는 경우) thumb 좌표가 마우스에서 벗어남 → 손 놓침.

  본 컴포넌트:
   - 트랙 click → thumb 이 그 위치로 점프 (직관적).
   - thumb 잡고 drag → 매 pointermove 의 movementX (델타) 만 누적하여 값 변경.
     마우스의 절대 위치가 아닌 델타라서 페이지가 변형돼도 사용자 입력 그대로 반영.
   - pointer capture 로 트랙 밖에서도 drag 유지.
   - 키보드 ← / → / Home / End 도 지원.

  Props:
   - value: number   (현재 값)
   - min / max / step
   - onInput(v):  drag 도중 매 step 마다 호출 (preview)
   - onChange(v): pointerup / blur 에서 한 번 (commit)
   - ariaLabel
   - disabled
-->
<script lang="ts">
	type Props = {
		value: number;
		min: number;
		max: number;
		step?: number;
		onInput?: (v: number) => void;
		onChange?: (v: number) => void;
		ariaLabel?: string;
		disabled?: boolean;
	};

	let {
		value,
		min,
		max,
		step = 1,
		onInput,
		onChange,
		ariaLabel,
		disabled = false
	}: Props = $props();

	let track: HTMLDivElement | undefined = $state(undefined);
	let dragging = $state(false);
	// drag 중인 미리보기 값. null 이면 외부 value 사용.
	let preview = $state<number | null>(null);

	function effective(): number {
		return preview ?? value;
	}

	function clampToStep(v: number): number {
		const clamped = Math.max(min, Math.min(max, v));
		// step 격자에 스냅.
		const stepped = Math.round((clamped - min) / step) * step + min;
		// 부동소수 누적 오차 정리.
		const decimals = (step.toString().split('.')[1] ?? '').length;
		return Number(stepped.toFixed(decimals));
	}

	function valueFromTrackX(clientX: number): number {
		if (!track) return value;
		const r = track.getBoundingClientRect();
		const ratio = Math.max(0, Math.min(1, (clientX - r.left) / r.width));
		return clampToStep(min + ratio * (max - min));
	}

	function startDrag(e: PointerEvent) {
		if (disabled) return;
		const target = e.currentTarget as HTMLElement;
		target.setPointerCapture(e.pointerId);
		// 트랙 click 위치로 바로 점프 (사용자 요구사항).
		const v = valueFromTrackX(e.clientX);
		preview = v;
		dragging = true;
		onInput?.(v);
		e.preventDefault();
	}

	function onMove(e: PointerEvent) {
		if (!dragging || !track) return;
		// 델타 기반 — `movementX` 는 OS / 브라우저 가속 / 페이지 zoom 무관한
		// raw pixel delta. UI scale 슬라이더가 자기 자신을 움직여도 사용자가
		// 손에 느끼는 그대로.
		const r = track.getBoundingClientRect();
		const range = max - min;
		const dxRatio = e.movementX / r.width;
		const next = clampToStep((preview ?? value) + dxRatio * range);
		if (next !== preview) {
			preview = next;
			onInput?.(next);
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
		const commit = preview ?? value;
		dragging = false;
		preview = null;
		onChange?.(commit);
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
		onInput?.(next);
		onChange?.(next);
	}

	let fillPct = $derived(((effective() - min) / (max - min)) * 100);
</script>

<div
	class="slider"
	class:dragging
	class:disabled
	role="slider"
	tabindex={disabled ? -1 : 0}
	aria-valuemin={min}
	aria-valuemax={max}
	aria-valuenow={effective()}
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
