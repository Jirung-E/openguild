<!--
  DEV-478: 알림함 — Nav 의 종을 누르면 뜬다. 사라진 알림을 최근 것부터 다시 본다.

  길드 알림(플러그인 `notify` · 실패)은 `.guild/notices/` 에서, 앱 알림은 앱이 켜진 동안 들고 있는 것에서.
  열면 다시 읽고, 닫을 때 모두 읽음으로 적는다 — 연 동안은 무엇이 새것인지 보이게 둔다.
-->
<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { locale, t } from '$lib/stores/locale';
	import { inboxItems, markAllRead, refreshInbox, type InboxItem } from '$lib/stores/inbox';
	import { formatRelative, formatTs } from '$lib/utils/datetime';

	let { onclose }: { onclose: () => void } = $props();

	function close() {
		void markAllRead();
		onclose();
	}

	// BUG-322 와 같은 이유 — 타이틀바 클릭은 오버레이가 못 받으니 capture 로 받는다.
	onMount(() => {
		void refreshInbox();
		const onDown = (e: MouseEvent) => {
			const el = e.target;
			if (!(el instanceof Element)) return close();
			if (el.closest('.inbox-wrap') && !el.closest('.ib-ov')) return;
			close();
		};
		window.addEventListener('mousedown', onDown, { capture: true });
		return () => window.removeEventListener('mousedown', onDown, { capture: true });
	});

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			close();
		}
	}

	function from(item: InboxItem): string {
		if (item.source === 'app') return t('inbox.fromApp', $locale);
		if (item.source === 'problem') return t('inbox.fromProblem', $locale);
		return item.plugin ?? t('inbox.fromPlugin', $locale);
	}

	function open(item: InboxItem) {
		if (!item.href) return;
		close();
		void goto(item.href);
	}
</script>

<svelte:window {onkeydown} />

<div class="ib-ov" role="presentation" onclick={close}></div>

<div class="popup ib" role="dialog" aria-label={t('inbox.title', $locale)}>
	<div class="ib-head">
		<span class="ib-title">{t('inbox.title', $locale)}</span>
		<button
			class="btn btn-sm"
			onclick={() => void markAllRead()}
			disabled={!$inboxItems.some((i) => i.unread)}>{t('inbox.markRead', $locale)}</button
		>
	</div>
	{#if $inboxItems.length === 0}
		<p class="ib-empty">{t('inbox.empty', $locale)}</p>
	{:else}
		<ul class="ib-list">
			{#each $inboxItems as item (item.key)}
				<li>
					<button
						class="ib-item"
						class:unread={item.unread}
						class:error={item.error}
						class:link={!!item.href}
						onclick={() => open(item)}
						tabindex={item.href ? 0 : -1}
					>
						<span class="ib-meta">
							{#if item.unread}<span class="ib-dot"><Icon name="dot" size={10} /></span>{/if}
							{#if item.error}<Icon name="warn" size={12} />{/if}
							<span class="ib-from">{from(item)}</span>
							<!-- glyph-ok: 횟수 표시(×3) — 아이콘이 아니라 곱셈 기호다(ToastHost 와 같다) -->
							{#if item.count > 1}<span class="ib-count">×{item.count}</span>{/if}
							<span class="ib-at" title={formatTs(item.at)}
								>{formatRelative(item.at, new Date(), $locale)}</span
							>
						</span>
						<span class="ib-text">{item.text}</span>
					</button>
				</li>
			{/each}
		</ul>
	{/if}
	<p class="ib-foot">{t('inbox.hint', $locale)}</p>
</div>

<style>
	.ib-ov {
		position: fixed;
		inset: 0;
		z-index: 150;
		background: transparent;
	}
	.ib {
		position: absolute;
		top: calc(100% + 6px);
		right: 0;
		z-index: 151;
		width: 22rem;
		max-width: calc(100vw - 2rem);
		max-height: min(32rem, calc(100vh - 6rem));
		max-height: min(32rem, calc(100dvh - 6rem));
		display: flex;
		flex-direction: column;
		padding: var(--sp-8) 0;
	}
	.ib-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 var(--sp-12) var(--sp-8);
		border-bottom: var(--bw) solid var(--bg-subtle);
	}
	.ib-title {
		font-size: var(--fs-sm);
		font-weight: var(--fw-semibold);
		color: var(--text);
	}
	.ib-empty {
		margin: 0;
		padding: var(--sp-16) var(--sp-12);
		font-size: var(--fs-sm);
		color: var(--text-muted);
		text-align: center;
	}
	.ib-list {
		list-style: none;
		margin: 0;
		padding: 0;
		overflow-y: auto;
		flex: 1;
	}
	.ib-item {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
		width: 100%;
		padding: var(--sp-8) var(--sp-12);
		background: transparent;
		border: none;
		border-bottom: var(--bw) solid var(--bg-subtle);
		text-align: left;
		color: var(--text-muted);
		cursor: default;
	}
	.ib-item.link {
		cursor: pointer;
	}
	.ib-item.link:hover {
		background: var(--nav-hover-bg);
	}
	.ib-item.unread {
		color: var(--text);
	}
	.ib-meta {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		font-size: var(--fs-xs);
		color: var(--text-muted);
	}
	.ib-dot {
		display: inline-flex;
		color: var(--accent);
	}
	.ib-item.error .ib-dot,
	.ib-item.error .ib-meta :global(svg) {
		color: var(--danger);
	}
	.ib-from {
		font-weight: var(--fw-semibold);
	}
	.ib-count {
		font-variant-numeric: tabular-nums;
	}
	.ib-at {
		margin-left: auto;
		white-space: nowrap;
	}
	.ib-text {
		font-size: var(--fs-sm);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		/* 플러그인이 긴 출력을 보낼 수 있다 — 몇 줄만 보이고 나머지는 접는다. */
		display: -webkit-box;
		-webkit-line-clamp: 6;
		line-clamp: 6;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.ib-foot {
		margin: 0;
		padding: var(--sp-8) var(--sp-12) 0;
		font-size: var(--fs-xs);
		color: var(--text-muted);
	}
</style>
