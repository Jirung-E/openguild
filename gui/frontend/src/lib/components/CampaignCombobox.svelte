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

	let {
		campaigns,
		placeholder = '캠페인 검색 (C-NNN 또는 제목)',
		onselect,
		oncancel
	}: {
		campaigns: Campaign[];
		placeholder?: string;
		onselect: (slug: string) => void;
		oncancel: () => void;
	} = $props();

	let query = $state('');
	let highlightIdx = $state(0);
	let inputEl: HTMLInputElement | undefined = $state(undefined);

	const filtered = $derived(() => {
		const q = query.trim().toLowerCase();
		if (!q) return campaigns;
		return campaigns.filter(
			(x) =>
				x.campaign_slug.toLowerCase().includes(q) ||
				x.title.toLowerCase().includes(q)
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
				return '#56d364';
			case 'done':
				return '#8b949e';
			default:
				return '#8b949e';
		}
	}
</script>

<div class="cb-wrap">
	<input
		bind:this={inputEl}
		bind:value={query}
		class="cb-input"
		type="text"
		{placeholder}
		onkeydown={onKeydown}
		data-testid="campaign-combobox-input"
	/>

	{#if filtered().length === 0}
		<div class="cb-empty">결과 없음</div>
	{:else}
		<ul class="cb-list" role="listbox">
			{#each filtered() as c, i (c.id)}
				<li
					role="option"
					aria-selected={i === highlightIdx}
					class:on={i === highlightIdx}
				>
					<button
						type="button"
						class="cb-row"
						onmouseenter={() => (highlightIdx = i)}
						onclick={() => pick(i)}
						data-testid="campaign-combobox-option"
					>
						<span class="badge slug">{c.campaign_slug}</span>
						<span class="title">{c.title}</span>
						<span class="status" style:--c={statusColor(c.status)}>{c.status}</span>
					</button>
				</li>
			{/each}
		</ul>
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
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #e6edf3;
		font-size: 0.875rem;
		outline: none;
	}
	.cb-input:focus { border-color: #58a6ff; }

	.cb-empty {
		padding: 0.6rem 0.8rem;
		color: #484f58;
		font-size: 0.8rem;
		border: 1px dashed #21262d;
		border-radius: 6px;
		text-align: center;
	}

	.cb-list {
		list-style: none;
		margin: 0;
		padding: 0;
		max-height: 220px;
		overflow-y: auto;
		border: 1px solid #21262d;
		border-radius: 6px;
		background: #0d1117;
	}
	.cb-list li { border-bottom: 1px solid #161b22; }
	.cb-list li:last-child { border-bottom: none; }
	.cb-list li.on { background: #161b22; }
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
	.cb-row:focus { outline: 1px solid #58a6ff; outline-offset: -1px; }

	/* slug pill — campaign 색 (Quest Detail 페이지의 .campaign-badge 와 동일 톤). */
	.badge.slug {
		flex-shrink: 0;
		--c: #4a9eff;
		padding: 0.1rem 0.45rem;
		border-radius: 12px;
		font-size: 0.7rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		background: color-mix(in srgb, var(--c) 16%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	.title {
		flex: 1;
		color: #c9d1d9;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.status {
		flex-shrink: 0;
		padding: 0.1rem 0.5rem;
		border-radius: 12px;
		font-size: 0.7rem;
		background: color-mix(in srgb, var(--c) 14%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
</style>
