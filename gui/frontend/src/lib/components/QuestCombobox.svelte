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
	// DEV-205(2차): i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-015: status 표시 이름 — 언어 반응.
	import { questStatusLabel } from '$lib/utils/status-label';

	let {
		quests,
		placeholder,
		onselect,
		oncancel
	}: {
		quests: Quest[];
		placeholder?: string;
		onselect: (questId: number) => void;
		oncancel: () => void;
	} = $props();

	const effectivePlaceholder = $derived(placeholder ?? t('combobox.questPlaceholder', $locale));

	let query = $state('');
	let highlightIdx = $state(0);
	let inputEl: HTMLInputElement | undefined = $state(undefined);
	let listEl: HTMLUListElement | undefined = $state(undefined);

	const filtered = $derived(() => {
		const q = query.trim().toLowerCase();
		if (!q) return quests;
		return quests.filter(
			(x) => x.quest_id.toLowerCase().includes(q) || x.title.toLowerCase().includes(q)
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
		placeholder={effectivePlaceholder}
		onkeydown={onKeydown}
		data-testid="quest-combobox-input"
	/>

	{#if filtered().length === 0}
		<div class="cb-empty">{t('combobox.noResults', $locale)}</div>
	{:else}
		<ul class="cb-list" role="listbox" bind:this={listEl}>
			{#each filtered() as q, i (q.id)}
				<li role="option" aria-selected={i === highlightIdx} class:on={i === highlightIdx}>
					<button
						type="button"
						class="cb-row"
						onmouseenter={() => (highlightIdx = i)}
						onclick={() => pick(i)}
						data-testid="quest-combobox-option"
					>
						<span class="pill mono sm" style:--c={q.type_color}>{q.quest_id}</span>
						<span class="title">{q.title}</span>
						<span class="pill sm" style:--c={q.status_color}>{questStatusLabel(q, $locale)}</span>
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
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
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

	/* DEV-364 후속: 이 화면의 slug pill 은 예전에 굵기를 정하지 않아 400 을
	   상속했다. `.pill.mono` 의 600 은 여기선 과하다 — 목록 행이 조밀해서
	   식별자만 도드라진다. 글꼴 굵기는 모양 공식이 아니라 화면 사정이라
	   `.pill` 이 막지 않는다(check:pill 의 허용 속성). */
	.pill.mono {
		font-weight: 400;
	}
	/* DEV-364: 모양은 global.css 의 `.pill` 이 정본. */
	.title {
		flex: 1;
		color: var(--text);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
