<!--
  BUG-025: 진행 중 캠페인 carousel — 한 번에 1개, 좌우 꽉 채움.

  - 슬라이드 부드러운 transform 전환.
  - 좌/우 화살표 + dot pagination.
  - 자동 회전 (5초 간격). hover 시 일시정지.
  - 카드 1개면 화살표 / dots 숨김.
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { CampaignSummary } from '$lib/types';
	import CampaignCard from './CampaignCard.svelte';

	let {
		summaries,
		now,
		emptyText = '진행 중인 캠페인이 없습니다.',
		autoRotateMs = 5000
	}: {
		summaries: CampaignSummary[];
		now: number;
		emptyText?: string;
		autoRotateMs?: number;
	} = $props();

	let idx = $state(0);
	let paused = $state(false);
	let rotateHandle: ReturnType<typeof setInterval> | null = null;

	// summaries 갯수가 변하면 idx clamp.
	$effect(() => {
		if (idx >= summaries.length) idx = Math.max(0, summaries.length - 1);
	});

	// 자동 회전. summaries 가 2개 미만이면 의미 없음.
	$effect(() => {
		if (rotateHandle) clearInterval(rotateHandle);
		rotateHandle = null;
		if (summaries.length < 2) return;
		rotateHandle = setInterval(() => {
			if (!paused) idx = (idx + 1) % summaries.length;
		}, autoRotateMs);
	});
	onDestroy(() => {
		if (rotateHandle) clearInterval(rotateHandle);
	});

	function prev() {
		idx = (idx - 1 + summaries.length) % summaries.length;
	}
	function next() {
		idx = (idx + 1) % summaries.length;
	}
</script>

<div
	class="carousel"
	role="region"
	aria-label="진행 중 캠페인"
	onmouseenter={() => (paused = true)}
	onmouseleave={() => (paused = false)}
>
	{#if summaries.length === 0}
		<div class="empty">{emptyText}</div>
	{:else}
		<div class="viewport">
			<div
				class="track"
				style:transform={`translateX(-${idx * 100}%)`}
			>
				{#each summaries as s (s.id)}
					<div class="slot">
						<CampaignCard summary={s} mode="active" {now} />
					</div>
				{/each}
			</div>
		</div>

		{#if summaries.length > 1}
			<div class="controls">
				<button class="arrow" type="button" onclick={prev} aria-label="이전">‹</button>
				<div class="dots" role="tablist">
					{#each summaries as _s, i (i)}
						<button
							class="dot"
							class:active={i === idx}
							type="button"
							onclick={() => (idx = i)}
							aria-label={`캠페인 ${i + 1}`}
						></button>
					{/each}
				</div>
				<button class="arrow" type="button" onclick={next} aria-label="다음">›</button>
			</div>
		{/if}
	{/if}
</div>

<style>
	.carousel {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 0.25rem 0 0.5rem 0;
	}
	.empty { color: #6e7681; font-size: 0.875rem; padding: 1rem 0; }

	.viewport {
		overflow: hidden;
		width: 100%;
	}
	.track {
		display: flex;
		width: 100%;
		transition: transform 0.4s cubic-bezier(0.4, 0, 0.2, 1);
	}
	.slot {
		flex: 0 0 100%;
		min-width: 0;
		padding: 0;
	}

	.controls {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
	}
	.arrow {
		background: #21262d;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 50%;
		width: 1.8rem;
		height: 1.8rem;
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
		transition: background 0.15s;
	}
	.arrow:hover { background: #2a2a4a; }
	.dots {
		display: flex;
		gap: 0.35rem;
	}
	.dot {
		background: #30363d;
		border: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		cursor: pointer;
		transition: background 0.15s, transform 0.15s;
		padding: 0;
	}
	.dot:hover { background: #484f58; }
	.dot.active {
		background: #58a6ff;
		transform: scale(1.4);
	}
</style>
