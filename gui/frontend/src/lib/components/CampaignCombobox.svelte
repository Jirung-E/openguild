<!--
  BUG-030: 캠페인 콤보박스 — QuestCombobox 의 캠페인 버전.

  Quest Detail 페이지의 "Campaigns" 섹션에서 캠페인 연결 시 사용.
  기존엔 native <datalist> 였는데, 다른 곳 (sub-quest / prereq 선택) 과 통일감
  없어서 같은 모달 + 콤보박스 패턴으로 교체.

  - 부모로부터 후보 캠페인 목록 (campaigns prop) 을 받음. 이미 연결된 항목 등의
    필터링은 호출자가 미리 처리.
  - 입력 필드에 타이핑하면 campaign_slug / title 양쪽으로 부분 일치 검색.
  - 선택 시 onselect(slug). 취소 시 oncancel().
  - Esc 닫기, ↑/↓ 이동, Enter 선택.
-->
<script lang="ts">
	import type { Campaign } from '$lib/types';
	import { onMount, tick } from 'svelte';
	// DEV-074 fix16: 검색 결과 list 도 overlay scrollbar.
	import OverlayScrollbar from './OverlayScrollbar.svelte';
	// DEV-205(2차): i18n.
	import { locale, t } from '$lib/stores/locale';

	let {
		campaigns,
		placeholder,
		onselect,
		oncancel
	}: {
		campaigns: Campaign[];
		placeholder?: string;
		onselect: (slug: string) => void;
		oncancel: () => void;
	} = $props();

	const effectivePlaceholder = $derived(placeholder ?? t('combobox.campaignPlaceholder', $locale));

	let query = $state('');
	let highlightIdx = $state(0);
	let inputEl: HTMLInputElement | undefined = $state(undefined);
	let listEl: HTMLUListElement | undefined = $state(undefined);

	const filtered = $derived(() => {
		const q = query.trim().toLowerCase();
		if (!q) return campaigns;
		return campaigns.filter(
			(x) => x.campaign_slug.toLowerCase().includes(q) || x.title.toLowerCase().includes(q)
		);
	});

	$effect(() => {
		void query;
		highlightIdx = 0;
	});

	onMount(async () => {
		await tick();
		inputEl?.focus();
	});

	function pick(idx: number) {
		const list = filtered();
		if (idx < 0 || idx >= list.length) return;
		onselect(list[idx].campaign_slug);
	}

	function onKeydown(e: KeyboardEvent) {
		const list = filtered();
		if (e.key === 'Escape') {
			e.preventDefault();
			oncancel();
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			highlightIdx = Math.min(highlightIdx + 1, list.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			highlightIdx = Math.max(highlightIdx - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			pick(highlightIdx);
		}
	}

	// Campaign status 색 — Quest Detail 페이지의 스타일과 동일.
	function statusColor(status: string): string {
		switch (status) {
			case 'active':
				return 'var(--success)';
			case 'done':
				return 'var(--text-muted)';
			default:
				return 'var(--text-muted)';
		}
	}
</script>

<div class="cb-wrap">
	<input
		bind:this={inputEl}
		bind:value={query}
		class="cb-input"
		type="text"
		placeholder={effectivePlaceholder}
		onkeydown={onKeydown}
		data-testid="campaign-combobox-input"
	/>

	{#if filtered().length === 0}
		<div class="cb-empty">{t('combobox.noResults', $locale)}</div>
	{:else}
		<ul class="cb-list" role="listbox" bind:this={listEl}>
			{#each filtered() as c, i (c.id)}
				<li role="option" aria-selected={i === highlightIdx} class:on={i === highlightIdx}>
					<button
						type="button"
						class="cb-row"
						onmouseenter={() => (highlightIdx = i)}
						onclick={() => pick(i)}
						data-testid="campaign-combobox-option"
					>
						<span class="pill mono sm slug">{c.campaign_slug}</span>
						<span class="title">{c.title}</span>
						<span class="pill sm" style:--c={statusColor(c.status)}>{c.status}</span>
					</button>
				</li>
			{/each}
		</ul>
		{#if listEl}
			<OverlayScrollbar target={listEl} />
		{/if}
	{/if}
</div>

<style>
	/* QuestCombobox 와 동일한 시각 톤 — 통일감 유지. */
	.cb-wrap {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.cb-input {
		padding: 0.4rem 0.7rem;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-strong);
		font-size: 0.875rem;
		outline: none;
	}
	.cb-input:focus {
		border-color: var(--accent);
	}

	.cb-empty {
		padding: 0.6rem 0.8rem;
		color: var(--text-faint);
		font-size: 0.8rem;
		border: 1px dashed var(--bg-subtle);
		border-radius: 6px;
		text-align: center;
	}

	.cb-list {
		list-style: none;
		margin: 0;
		padding: 0;
		/* BUG-160: QuestCombobox 와 동일 — 220px(5줄)은 너무 좁았다. */
		/* BUG-253: 600px → 37.5rem (기본 배율에서 같은 값). 70vh 상한은 그대로 —
		   작은 창에서 화면을 넘지 않게 하는 장치다. */
		max-height: min(70vh, 37.5rem);
		overflow-y: auto;
		/* DEV-074 fix16: native scrollbar 숨김 — OverlayScrollbar 가 대신 그림. */
		scrollbar-width: none;
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		background: var(--bg);
	}
	.cb-list::-webkit-scrollbar {
		display: none;
	}
	.cb-list li {
		border-bottom: 1px solid var(--bg-elevated);
	}
	.cb-list li:last-child {
		border-bottom: none;
	}
	.cb-list li.on {
		background: var(--bg-elevated);
	}
	.cb-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4rem 0.7rem;
		cursor: pointer;
		font-size: 0.85rem;
		width: 100%;
		background: none;
		border: none;
		color: inherit;
		text-align: left;
		font: inherit;
	}
	.cb-row:focus {
		outline: 1px solid var(--accent);
		outline-offset: -1px;
	}

	/* slug pill — campaign 색 (Quest Detail 페이지의 .campaign-badge 와 동일 톤). */
	/* DEV-364: 모양은 global.css 의 `.pill` 이 정본 — 색만 정한다.
	   캠페인은 타입 개념이 없어 accent 고정. */
	.pill.slug {
		--c: var(--accent);
	}
	.title {
		flex: 1;
		color: var(--text);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
