<!--
  DEV-138: 설정 퀵메뉴 — Nav 의 ⚙ 클릭 시 dropdown.

  자주 쓰는 표시 설정 (테마 / UI 크기 / 컨텐츠 폭) 을 페이지 이동 없이 즉시
  조절. '전체 설정' 으로 /settings 진입. DEV-125 의 standalone 테마 토글
  버튼은 본 메뉴로 흡수.
-->
<script lang="ts">
	import { theme, setTheme, type ThemeChoice } from '$lib/stores/theme';
	import {
		uiScale,
		setUiScale,
		MIN_SCALE,
		MAX_SCALE
	} from '$lib/stores/uiScale';
	import {
		contentWidth,
		setContentWidth,
		MIN_CONTENT_WIDTH,
		MAX_CONTENT_WIDTH
	} from '$lib/stores/contentWidth';
	import CustomSlider from './CustomSlider.svelte';

	let { onclose }: { onclose: () => void } = $props();

	const THEME_OPTIONS: { value: ThemeChoice; label: string }[] = [
		{ value: 'system', label: '시스템' },
		{ value: 'light', label: '라이트' },
		{ value: 'dark', label: '다크' }
	];

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			onclose();
		}
	}
</script>

<svelte:window onkeydown={onkeydown} />

<!-- 바깥 클릭 닫기 — 투명 오버레이. -->
<div class="qm-ov" role="presentation" onclick={onclose}></div>

<div class="qm" role="menu" aria-label="설정 퀵메뉴">
	<div class="qm-row">
		<span class="qm-label">테마</span>
		<div class="qm-seg" role="group" aria-label="테마">
			{#each THEME_OPTIONS as o (o.value)}
				<button
					class="qm-seg-btn"
					class:active={$theme === o.value}
					onclick={() => setTheme(o.value)}
				>{o.label}</button>
			{/each}
		</div>
	</div>

	<div class="qm-row">
		<span class="qm-label">UI 크기</span>
		<div class="qm-slider">
			<CustomSlider
				value={$uiScale}
				min={MIN_SCALE}
				max={MAX_SCALE}
				step={0.01}
				ariaLabel="UI 크기"
				onChange={setUiScale}
			/>
			<span class="qm-val">{Math.round($uiScale * 100)}%</span>
		</div>
	</div>

	<div class="qm-row">
		<span class="qm-label">컨텐츠 폭</span>
		<div class="qm-slider">
			<CustomSlider
				value={$contentWidth}
				min={MIN_CONTENT_WIDTH}
				max={MAX_CONTENT_WIDTH}
				step={10}
				ariaLabel="컨텐츠 폭"
				onChange={setContentWidth}
			/>
			<span class="qm-val">{$contentWidth}px</span>
		</div>
	</div>

	<a class="qm-all" href="/settings" onclick={onclose}>전체 설정 →</a>
</div>

<style>
	.qm-ov {
		position: fixed;
		inset: 0;
		z-index: 150;
		background: transparent;
	}
	.qm {
		position: absolute;
		top: calc(100% + 6px);
		right: 0;
		z-index: 151;
		width: 280px;
		padding: 0.75rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: 0 10px 30px var(--shadow);
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
	}
	.qm-row {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.qm-label {
		font-size: 0.72rem;
		color: var(--text-muted);
	}
	.qm-seg {
		display: flex;
		gap: 0.25rem;
	}
	.qm-seg-btn {
		flex: 1;
		padding: 0.3rem 0;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.78rem;
		cursor: pointer;
	}
	.qm-seg-btn:hover { color: var(--text); border-color: var(--text-faint); }
	.qm-seg-btn.active {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		border-color: var(--accent);
		color: var(--accent);
	}
	.qm-slider {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.qm-slider :global(.slider) { flex: 1; }
	.qm-val {
		min-width: 3.2rem;
		text-align: right;
		font-size: 0.75rem;
		color: var(--text);
		font-variant-numeric: tabular-nums;
	}
	.qm-all {
		margin-top: 0.1rem;
		padding: 0.4rem 0;
		text-align: center;
		border-top: 1px solid var(--bg-subtle);
		color: var(--accent);
		font-size: 0.8rem;
		text-decoration: none;
	}
	.qm-all:hover { text-decoration: underline; }
</style>
