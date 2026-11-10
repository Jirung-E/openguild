<!--
  DEV-478: 알림 카드 하나 — 오른쪽 아래 토스트와 알림함이 같은 카드를 쓴다(같은 알림이라고 바로 알아보게).

  [아이콘 원] 종류 · 보낸 곳 ……… 시각
              제목
              본문(한 줄, 넘치면 펼치기)
              (→ 갈 곳)
  마우스를 올리면 오른쪽 위 모서리에 × (그 알림을 지운다). 안 읽었으면 왼쪽 위 모서리에 점.
  카드를 누르면 갈 곳으로(없으면 `onopen` 이 정한 대로).
-->
<script lang="ts">
	import Icon from './Icon.svelte';
	import { locale, t } from '$lib/stores/locale';
	import { loadQuestIndex, questIndex } from '$lib/stores/questIndex';
	import { formatRelative, formatTs } from '$lib/utils/datetime';
	import type { NoticeCardData } from '$lib/utils/noticeCard';

	let {
		card,
		onopen,
		ondismiss
	}: {
		card: NoticeCardData;
		onopen: (card: NoticeCardData) => void;
		ondismiss: (card: NoticeCardData) => void;
	} = $props();

	let expanded = $state(false);
	let overflowing = $state(false);
	let bodyEl: HTMLSpanElement | undefined = $state(undefined);

	// 한 줄에 다 들어가면 펼치기 단추가 없다. 접힌 채로만 잰다 — 펼친 뒤에는 그대로 둔다.
	$effect(() => {
		const el = bodyEl;
		if (!el || expanded) return;
		const measure = () => (overflowing = el.scrollWidth > el.clientWidth + 1);
		measure();
		const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(measure) : undefined;
		ro?.observe(el);
		return () => ro?.disconnect();
	});

	const kindText = $derived.by(() => {
		const label = t(`notice.kind.${card.kind}`, $locale);
		return card.who ? `${label} · ${card.who}` : label;
	});
	const title = $derived(
		card.failedLine
			? t('notice.lineFailed', $locale).replace('{line}', card.failedLine)
			: card.title
	);
	const targetTitle = $derived(
		card.target ? $questIndex.get(card.target.toUpperCase())?.title : undefined
	);

	// 갈 곳의 제목을 보이려면 번호 → 제목 색인이 있어야 한다(한 번 읽고 기억한다).
	$effect(() => {
		if (card.target) void loadQuestIndex();
	});

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			onopen(card);
		}
	}
</script>

<div
	class="nc"
	class:problem={card.tone === 'problem'}
	class:link={!!card.href}
	role="button"
	tabindex="0"
	title={card.href ? t('notif.open', $locale) : undefined}
	onclick={() => onopen(card)}
	{onkeydown}
>
	<span class="nc-dot" class:on={card.unread} aria-hidden="true"></span>
	<div class="nc-kind">
		<span class="nc-icon" class:bell={card.tone === 'notify'} aria-hidden="true"
			><Icon
				name={card.tone === 'problem' ? 'warn' : card.tone === 'created' ? 'plus-circle' : 'bell'}
				size="1rem"
			/></span
		>
		<span class="nc-who">{kindText}</span>
		<!-- glyph-ok: 횟수 표시(×3) — 아이콘이 아니라 곱셈 기호다 -->
		{#if card.count > 1}<span class="nc-count">×{card.count}</span>{/if}
		{#if card.at}
			<span class="nc-at" title={formatTs(card.at)}
				>{formatRelative(card.at, new Date(), $locale)}</span
			>
		{/if}
	</div>
	<div class="nc-main">
		<span class="nc-title">{title}</span>
		{#if card.body}
			<span
				class="nc-body"
				class:open={expanded}
				class:room={overflowing || expanded}
				bind:this={bodyEl}>{card.body}</span
			>
		{/if}
		{#if card.target}
			<span class="nc-to">
				<span class="pill"
					><Icon
						name="arrow-right"
						size="0.875rem"
					/>{card.target}{#if targetTitle}&nbsp;{targetTitle}{/if}</span
				>
			</span>
		{/if}
	</div>
	{#if card.body && (overflowing || expanded)}
		<button
			class="nc-more"
			class:open={expanded}
			title={t(expanded ? 'notice.collapse' : 'notice.expand', $locale)}
			aria-label={t(expanded ? 'notice.collapse' : 'notice.expand', $locale)}
			aria-expanded={expanded}
			onclick={(e) => {
				e.stopPropagation();
				expanded = !expanded;
			}}><Icon name="caret-down-sm" size="1rem" /></button
		>
	{/if}
	<button
		class="nc-x"
		title={t('inbox.dismiss', $locale)}
		aria-label={t('inbox.dismiss', $locale)}
		onclick={(e) => {
			e.stopPropagation();
			ondismiss(card);
		}}><Icon name="close" size="0.75rem" /></button
	>
</div>

<style>
	.nc {
		--c: var(--accent);
		position: relative;
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		padding: var(--sp-8) var(--sp-12);
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: var(--shadow-md);
		color: var(--text);
		line-height: var(--lh-snug);
		text-align: left;
		cursor: default;
		pointer-events: auto;
		transition: border-color var(--dur-fast);
	}
	.nc.problem {
		--c: var(--danger);
	}
	.nc.link {
		cursor: pointer;
	}
	.nc:hover {
		border-color: var(--text-faint);
	}
	.nc:focus-visible {
		outline: none;
		box-shadow: var(--shadow-md), var(--ring);
	}

	/* 안 읽음 — 왼쪽 위 모서리에 걸친 점. */
	.nc-dot {
		position: absolute;
		top: -0.3125rem; /* token-ok: 점 반지름만큼 모서리 밖으로 */
		left: -0.3125rem; /* token-ok: 점 반지름만큼 모서리 밖으로 */
		width: 0.625rem; /* token-ok: 점 크기 */
		height: 0.625rem; /* token-ok: 점 크기 */
		border-radius: 50%;
		background: var(--c);
		border: 0.125rem solid var(--bg); /* token-ok: 배경과 가르는 둘레 */
		box-sizing: content-box;
		opacity: 0;
		transition: opacity var(--dur-mid);
		pointer-events: none;
	}
	.nc-dot.on {
		opacity: 1;
	}

	.nc-kind {
		display: flex;
		align-items: center;
		gap: var(--sp-6);
		font-size: var(--fs-sm);
		color: var(--text-muted);
		min-width: 0;
	}
	.nc.problem .nc-who {
		color: var(--danger);
	}
	.nc-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		width: 1.6875rem; /* token-ok: 아이콘 원 크기 */
		height: 1.6875rem; /* token-ok: 아이콘 원 크기 */
		border-radius: 50%;
		color: var(--c);
		background: color-mix(in srgb, var(--c) 15%, transparent);
	}
	/* 종 도형은 16 격자에서 아래로 0.85 쏠려 있다(y 3.6~14.1) — 원 안에서 가운데로 보이게 올린다. */
	.nc-icon.bell :global(svg) {
		transform: translateY(-0.053rem); /* token-ok: 도형의 광학 중심 보정 */
	}
	.nc-who {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.nc-count {
		font-variant-numeric: tabular-nums;
	}
	.nc-at {
		margin-left: auto;
		flex-shrink: 0;
		font-size: var(--fs-xs);
		color: var(--text-faint);
		white-space: nowrap;
	}

	.nc-main {
		display: flex;
		flex-direction: column;
		padding-left: var(--sp-4);
		min-width: 0;
	}
	.nc-title {
		font-size: var(--fs-md);
		font-weight: var(--fw-medium);
		color: var(--text);
		overflow-wrap: anywhere;
	}
	.nc-body {
		font-size: var(--fs-sm);
		color: var(--text-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.nc-body.room {
		/* 펼치기 단추 자리 */
		padding-right: var(--sp-24);
	}
	.nc-body.open {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.nc-to {
		display: flex;
		margin-top: var(--sp-6);
		min-width: 0;
	}
	.nc-to .pill {
		--c: var(--accent);
		gap: var(--sp-4);
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* 아이콘만 있는 단추 — 가운데 정렬. */
	.nc-more,
	.nc-x {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		cursor: pointer;
	}
	.nc-more {
		position: absolute;
		right: var(--sp-8);
		bottom: var(--sp-8);
		width: 1.25rem; /* token-ok: 아이콘 단추 크기 */
		height: 1.25rem; /* token-ok: 아이콘 단추 크기 */
		border: none;
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--text-muted);
	}
	.nc-more:hover {
		background: var(--bg-subtle);
	}
	.nc-more :global(svg) {
		transition: transform var(--dur-mid);
	}
	.nc-more.open :global(svg) {
		transform: rotate(180deg);
	}

	/* 지우기 — 마우스를 올렸을 때만, 오른쪽 위 모서리에 걸친 원. */
	.nc-x {
		position: absolute;
		top: -0.6875rem; /* token-ok: 원 중심을 모서리 바깥에 */
		right: -0.6875rem; /* token-ok: 원 중심을 모서리 바깥에 */
		width: 1.375rem; /* token-ok: 닫기 원 크기 */
		height: 1.375rem; /* token-ok: 닫기 원 크기 */
		border-radius: 50%;
		border: var(--bw) solid var(--border);
		background: var(--bg-elevated);
		color: var(--text-muted);
		box-shadow: var(--shadow-sm);
		opacity: 0;
		transform: scale(0.7);
		/* 숨어 있어도 마우스는 받는다 — 카드 밖에서 × 를 누르러 올 때 × 자리에 닿으면 떠야 한다(admin). × 는
		   카드의 자식이라 × 에 닿으면 .nc:hover 도 켜진다. */
		transition:
			opacity var(--dur-fast),
			transform var(--dur-fast);
	}
	/* 숨을 때 0.7 배로 줄어도 마우스를 받는 자리는 보일 때 크기 그대로(1 / 0.7 ≈ 1.43 배 → 사방 22%). */
	.nc-x::after {
		content: '';
		position: absolute;
		inset: -22%;
		border-radius: 50%;
	}
	/* 누른 뒤 포커스가 남아도 × 가 남지 않게 — 키보드로 옮겨 왔을 때만(focus-visible). */
	.nc:hover .nc-x,
	.nc:focus-visible .nc-x,
	.nc-x:hover,
	.nc-x:focus-visible {
		opacity: 1;
		transform: scale(1);
	}
	.nc-x:hover {
		color: var(--text);
	}
	/* 터치 화면에는 마우스 올림이 없다 — × 를 늘 보인다(안 그러면 휴대폰에서 지울 길이 없다). */
	@media (hover: none) {
		.nc-x {
			opacity: 1;
			transform: none;
		}
	}
</style>
