<!--
  DEV-440 후속: 검색해서 하나 고르는 목록 — 퀘스트 · 캠페인 고르기가 같이 쓴다.

  예전에는 QuestCombobox 와 CampaignCombobox 가 거의 같은 코드(검색 · ↑↓ · Enter · Esc · 빈 결과 · 목록 모양)를
  두 벌 들고 있었다. 여기 한 벌로 두고, 두 컴포넌트는 무엇을 찾고(`matches`) 한 줄을 어떻게 그리는지(`row`)만 정한다.

  - 후보는 부모가 거른 뒤 넘긴다(사이클 · 자기 자신 · 이미 연결된 것).
  - 입력에 치면 `matches` 로 거른다. ↑/↓ 이동, Enter 고르기, Esc 취소.
-->
<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import { onMount, tick } from 'svelte';
	// DEV-074 fix16: 검색 결과 list 도 overlay scrollbar.
	import OverlayScrollbar from './OverlayScrollbar.svelte';
	import { locale, t } from '$lib/stores/locale';

	let {
		items,
		key,
		matches,
		title,
		before,
		after,
		placeholder,
		testid,
		onselect,
		oncancel
	}: {
		items: T[];
		key: (item: T) => string | number;
		/** 소문자로 다듬은 검색어에 맞는가. */
		matches: (item: T, q: string) => boolean;
		/** 줄 가운데의 제목. */
		title: (item: T) => string;
		/** 제목 앞 · 뒤에 둘 것(번호 칩 · 상태 칩). */
		before?: Snippet<[T]>;
		after?: Snippet<[T]>;
		placeholder: string;
		/** 시험이 찾는 이름 — 입력은 `{testid}-input`, 줄은 `{testid}-option`. */
		testid: string;
		onselect: (item: T) => void;
		oncancel: () => void;
	} = $props();

	let query = $state('');
	let highlightIdx = $state(0);
	let inputEl: HTMLInputElement | undefined = $state(undefined);
	let listEl: HTMLUListElement | undefined = $state(undefined);

	const filtered = $derived.by(() => {
		const q = query.trim().toLowerCase();
		return q ? items.filter((x) => matches(x, q)) : items;
	});

	$effect(() => {
		// 검색어가 바뀌면 맨 위부터.
		void query;
		highlightIdx = 0;
	});

	onMount(async () => {
		await tick();
		inputEl?.focus();
	});

	function pick(idx: number) {
		if (idx < 0 || idx >= filtered.length) return;
		onselect(filtered[idx]);
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			oncancel();
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			highlightIdx = Math.min(highlightIdx + 1, filtered.length - 1);
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
		class="input"
		type="text"
		{placeholder}
		onkeydown={onKeydown}
		data-testid="{testid}-input"
	/>

	{#if filtered.length === 0}
		<div class="cb-empty">{t('combobox.noResults', $locale)}</div>
	{:else}
		<ul class="cb-list" role="listbox" bind:this={listEl}>
			{#each filtered as item, i (key(item))}
				<li role="option" aria-selected={i === highlightIdx} class:on={i === highlightIdx}>
					<button
						type="button"
						class="cb-row"
						onmouseenter={() => (highlightIdx = i)}
						onclick={() => pick(i)}
						data-testid="{testid}-option"
					>
						{@render before?.(item)}
						<span class="title">{title(item)}</span>
						{@render after?.(item)}
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
		gap: var(--sp-6);
	}

	.cb-empty {
		padding: var(--sp-8) var(--sp-12);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		border: var(--bw) dashed var(--bg-subtle);
		border-radius: var(--r-md);
		text-align: center;
	}

	.cb-list {
		list-style: none;
		margin: 0;
		padding: 0;
		/* BUG-160: 220px = 5줄뿐이라 후보가 조금만 많아도 대부분이 스크롤 뒤로
		   숨었다(사용자 지적: 팝업이 너무 작아 보기 불편). 창 높이의 70% 안에서
		   최대 600px(≈17줄) — vh 를 함께 써야 작은 창에서 화면을 넘지 않는다. */
		/* BUG-253: 600px → 37.5rem (기본 배율에서 같은 값). 70vh 상한은 그대로 —
		   작은 창에서 화면을 넘지 않게 하는 장치다. */
		max-height: min(70vh, 37.5rem);
		max-height: min(70dvh, 37.5rem);
		overflow-y: auto;
		/* DEV-074 fix16: native scrollbar 숨김 — OverlayScrollbar 가 대신 그림. */
		scrollbar-width: none;
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-md);
		background: var(--bg);
	}
	.cb-list::-webkit-scrollbar {
		display: none;
	}
	.cb-list li {
		border-bottom: var(--bw) solid var(--bg-elevated);
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
		gap: var(--sp-8);
		padding: var(--sp-6) var(--sp-12);
		cursor: pointer;
		font-size: var(--fs-md);
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

	/* DEV-364 후속: 이 화면의 pill 은 slug·상태 **둘 다** 예전에 굵기를 정하지
	   않아 400 을 상속했다. `.pill`(500)과 `.pill.mono`(600)는 여기선 과하다 —
	   목록 행이 조밀해서 칩만 도드라진다.

	   `.pill.sm` 으로 적는 이유: `.pill.mono`(0,2,0)를 이겨야 하는데 컴포넌트
	   스코프 클래스가 붙어 (0,3,0)이 된다. 그냥 `.pill` 로 쓰면 (0,2,0) 이라
	   순서로 밀려 slug 쪽이 600 으로 남는다.

	   글꼴 굵기는 모양 공식이 아니라 화면 사정이라 `check:pill` 이 막지 않는다. */
	.cb-row :global(.pill.sm) {
		font-weight: var(--fw-regular);
	}
	/* DEV-364: 모양은 global.css 의 `.pill` 이 정본. 칩은 부르는 쪽 snippet 이라 :global. */
	.title {
		flex: 1;
		color: var(--text);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
