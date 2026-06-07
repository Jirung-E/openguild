<!--
  QuestCombobox — 퀘스트를 검색해서 하나 선택할 수 있는 콤보박스.

  - 부모로부터 후보 목록(quests prop)을 받음. 사이클·자기 자신·이미 부모 있는
    퀘스트 등의 필터링은 호출자(또는 backend candidates API)에서 미리 처리.
  - 입력 필드에 타이핑하면 quest_id / title 양쪽으로 부분 일치 검색.
  - 선택 시 onselect(questId) 호출. 취소 시 oncancel().
  - Esc 로 닫기, ↑/↓ 로 항목 이동, Enter 로 선택.
-->
<script lang="ts">
	import type { Quest } from '$lib/types';
	import { onMount, tick } from 'svelte';
	// DEV-074 fix16: 검색 결과 list 도 overlay scrollbar.
	import OverlayScrollbar from './OverlayScrollbar.svelte';

	let {
		quests,
		placeholder = '퀘스트 검색 (ID 또는 제목)',
		onselect,
		oncancel
	}: {
		quests: Quest[];
		placeholder?: string;
		onselect: (questId: number) => void;
		oncancel: () => void;
	} = $props();

	let query = $state('');
	let highlightIdx = $state(0);
	let inputEl: HTMLInputElement | undefined = $state(undefined);
	let listEl: HTMLUListElement | undefined = $state(undefined);

	const filtered = $derived(() => {
		const q = query.trim().toLowerCase();
		if (!q) return quests;
		return quests.filter(
			(x) =>
				x.quest_id.toLowerCase().includes(q) ||
				x.title.toLowerCase().includes(q)
		);
	});

	$effect(() => {
		// query 가 바뀔 때마다 highlight 인덱스 재설정
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
		onselect(list[idx].id);
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
</script>

<div class="cb-wrap">
	<input
		bind:this={inputEl}
		bind:value={query}
		class="cb-input"
		type="text"
		{placeholder}
		onkeydown={onKeydown}
		data-testid="quest-combobox-input"
	/>

	{#if filtered().length === 0}
		<div class="cb-empty">결과 없음</div>
	{:else}
		<ul class="cb-list" role="listbox" bind:this={listEl}>
			{#each filtered() as q, i (q.id)}
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
						data-testid="quest-combobox-option"
					>
						<span class="badge" style:--c={q.type_color}>{q.quest_id}</span>
						<span class="title">{q.title}</span>
						<span class="status" style:--c={q.status_color}>{q.status_name_en}</span>
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
	.cb-input:focus { border-color: var(--accent); }

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
		max-height: 220px;
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
	.cb-list li { border-bottom: 1px solid var(--bg-elevated); }
	.cb-list li:last-child { border-bottom: none; }
	.cb-list li.on { background: var(--bg-elevated); }
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
	.cb-row:focus { outline: 1px solid var(--accent); outline-offset: -1px; }

	.badge {
		flex-shrink: 0;
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
		color: var(--text);
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
