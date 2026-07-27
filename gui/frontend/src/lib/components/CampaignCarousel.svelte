<!--
  BUG-025: 진행 중 캠페인 carousel — 한 번에 1개, 좌우 꽉 채움.

  - 슬라이드 부드러운 transform 전환.
  - 좌/우 화살표 + dot pagination.
  - 자동 회전 (5초 간격). hover 시 일시정지.
  - 카드 1개면 화살표 / dots 숨김.
-->
<script lang="ts">
	import PlayPauseIcon from './PlayPauseIcon.svelte';
	import { onMount, onDestroy } from 'svelte';
	import type { CampaignSummary } from '$lib/types';
	import CampaignCard from './CampaignCard.svelte';
	// DEV-205 모듈2: 캐러셀 라벨 i18n.
	import { locale, t } from '$lib/stores/locale';

	let {
		summaries,
		now,
		emptyText,
		autoRotateMs = 3000
	}: {
		summaries: CampaignSummary[];
		now: number;
		emptyText?: string;
		autoRotateMs?: number;
	} = $props();

	const displayEmpty = $derived(emptyText ?? t('carousel.activeEmpty', $locale));

	let idx = $state(0);
	let hoverPause = $state(false);
	// BUG-027: 사용자가 명시적으로 정지/재생 토글. hover pause 와 독립.
	let userPaused = $state(false);

	// summaries 갯수가 변하면 idx clamp.
	$effect(() => {
		if (idx >= summaries.length) idx = Math.max(0, summaries.length - 1);
	});

	// BUG-029: 자동 회전 — onMount 로 한 번만 등록.
	//
	// 이전 (BUG-028) 은 `$effect` 로 setInterval 을 만들고 cleanup 으로 정리
	// 했는데 — 부모 Home 의 `currentActive` 가 `now` 매초 갱신으로 재derive
	// 되며 summaries prop 참조가 매초 새로 바뀜. `$effect` 가 `summaries.length`
	// 를 읽어 의존성으로 잡고 있어 매초 cleanup + 재등록 → 3초 타이머가 절대
	// 만료 안 함.
	//
	// 해결: onMount 로 1회만 setInterval 등록. 콜백 안에서 summaries.length /
	// hoverPause / userPaused 를 fire 시점에 읽음 — 참조 안정성 무관.
	let rotateHandle: ReturnType<typeof setInterval> | null = null;
	onMount(() => {
		rotateHandle = setInterval(() => {
			if (hoverPause || userPaused) return;
			const count = summaries.length;
			if (count < 2) return;
			idx = (idx + 1) % count;
		}, autoRotateMs);
	});
	onDestroy(() => {
		if (rotateHandle) {
			clearInterval(rotateHandle);
			rotateHandle = null;
		}
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
	aria-label={t('carousel.active', $locale)}
	onmouseenter={() => (hoverPause = true)}
	onmouseleave={() => (hoverPause = false)}
>
	{#if summaries.length === 0}
		<div class="empty">{displayEmpty}</div>
	{:else}
		<div class="viewport">
			<div class="track" style:transform={`translateX(-${idx * 100}%)`}>
				{#each summaries as s (s.id)}
					<div class="slot">
						<CampaignCard summary={s} mode="active" {now} />
					</div>
				{/each}
			</div>
		</div>

		{#if summaries.length > 1}
			<div class="controls">
				<button class="arrow" type="button" onclick={prev} aria-label={t('carousel.prev', $locale)}>‹</button>
				<div class="dots" role="tablist">
					{#each summaries as _s, i (i)}
						<button
							class="dot"
							class:active={i === idx}
							type="button"
							onclick={() => (idx = i)}
							aria-label={`${t('carousel.active', $locale)} ${i + 1}`}
						></button>
					{/each}
				</div>
				<button class="arrow" type="button" onclick={next} aria-label={t('carousel.next', $locale)}>›</button>
				<!-- BUG-027: 자동 회전 정지/재생 토글. -->
				<button
					class="play-pause"
					type="button"
					onclick={() => (userPaused = !userPaused)}
					aria-label={userPaused ? t('carousel.play', $locale) : t('carousel.pause', $locale)}
					title={userPaused ? t('carousel.autoPlay', $locale) : t('carousel.autoPause', $locale)}
				>
					<PlayPauseIcon paused={userPaused} />
				</button>
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
	.empty {
		color: var(--text-faint);
		font-size: 0.875rem;
		padding: 1rem 0;
	}

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
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 50%;
		width: 1.8rem;
		height: 1.8rem;
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
		transition: background 0.15s;
	}
	.arrow:hover {
		background: var(--bg-subtle);
	}
	.dots {
		display: flex;
		gap: 0.35rem;
	}
	.dot {
		background: var(--border);
		border: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		cursor: pointer;
		transition:
			background 0.15s,
			transform 0.15s;
		padding: 0;
	}
	.dot:hover {
		background: var(--text-faint);
	}
	.dot.active {
		background: var(--accent);
		transform: scale(1.4);
	}

	/* BUG-027: 정지/재생 토글 — 화살표와 같은 스타일 + 위치는 dots 우측. */
	.play-pause {
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 50%;
		width: 1.8rem;
		height: 1.8rem;
		font-size: 0.85rem;
		line-height: 1;
		cursor: pointer;
		transition: background 0.15s;
		margin-left: 0.4rem;
	}
	.play-pause:hover {
		background: var(--bg-subtle);
	}
</style>
