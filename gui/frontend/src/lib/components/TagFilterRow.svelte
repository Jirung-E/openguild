<!--
  REQ-019: 태그 필터 줄 — 도서관(2곳) / 규칙 / 퀘스트 목록 공용.

  네 곳에 **같은 마크업이 복붙**돼 있었다(도서관 안에서만도 두 번). 한 번
  고치려면 네 번 고쳐야 했다 — [[DEV-364]](pill) / [[DEV-272]](글꼴)와 같은
  모양의 문제라 같은 방식으로 하나로 모은다.

  기본은 접힘이다(admin 결정). 태그가 많은 길드에서 이 줄이 몇 줄씩 차지해
  목록을 아래로 밀어냈다.

  **접혀 있어도 고른 태그는 보인다.** 필터가 걸린 채 통째로 숨으면 목록이 왜
  줄었는지 알 수 없고 되돌릴 수단도 없다 — 원래 불편보다 나쁜 상태가 된다.
-->
<script lang="ts">
	import { locale, t } from '$lib/stores/locale';
	import {
		loadTagFilterOpen,
		saveTagFilterOpen,
		visibleTags,
		toggleCount
	} from '$lib/utils/tag-filter-collapse';

	let {
		tags,
		counts,
		selected,
		ontoggle,
		onclear,
		/**
		 * 접힘을 기억할 키. 화면당 하나다.
		 *
		 * 도서관은 사이드바 뷰와 탐색기 뷰가 같은 필터를 그리므로 **같은 키**를
		 * 준다 — 뷰를 바꿔도 접힘이 이어진다.
		 */
		storageKey
	}: {
		tags: string[];
		counts: Map<string, number>;
		selected: Set<string>;
		ontoggle: (tag: string) => void;
		onclear: () => void;
		storageKey: string;
	} = $props();

	// svelte-ignore state_referenced_locally
	let open = $state(loadTagFilterOpen(storageKey));

	function toggleOpen() {
		open = !open;
		saveTagFilterOpen(storageKey, open);
	}

	let shown = $derived(visibleTags(tags, selected, open));
	let total = $derived(toggleCount(tags));
	// 접힌 채 필터가 걸려 있으면 그 사실이 보여야 한다.
	let hiddenCount = $derived(total - shown.length);
</script>

<div class="tag-filter-row" aria-label={t('tagFilter.label', $locale)}>
	<button
		class="tag-toggle"
		class:open
		onclick={toggleOpen}
		aria-expanded={open}
		title={open ? t('tagFilter.collapse', $locale) : t('tagFilter.expand', $locale)}
	>
		<span class="tag-toggle-icon" class:collapsed={!open}>▾</span>
		{t('tagFilter.label', $locale)}
		<span class="tag-toggle-count">{total}</span>
	</button>

	{#each shown as tag (tag)}
		<button
			class="tag-filter-chip"
			class:active={selected.has(tag)}
			onclick={() => ontoggle(tag)}
			title={selected.has(tag)
				? `${tag}${t('questList.filterRemoveSuffix', $locale)}`
				: `${tag}${t('questList.filterAddSuffix', $locale)}`}
		>
			{tag}
			<span class="tag-chip-count">{counts.get(tag) ?? 0}</span>
		</button>
	{/each}

	{#if !open && hiddenCount > 0 && selected.size > 0}
		<!-- 고른 것만 보이는 상태 — 나머지가 몇 개 더 있는지 알려 준다. -->
		<button class="tag-more" onclick={toggleOpen} title={t('tagFilter.expand', $locale)}
			>+{hiddenCount}</button
		>
	{/if}

	{#if selected.size > 0}
		<button class="tag-clear" onclick={onclear} title={t('tagFilter.clearTitle', $locale)}>
			{t('questList.clearAllBtn', $locale)}
		</button>
	{/if}
</div>

<style>
	.tag-filter-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		align-items: center;
		margin-bottom: 0.4rem;
	}
	.tag-toggle {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.15rem 0.5rem;
		background: transparent;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-pill);
		color: var(--text-muted);
		font: inherit;
		font-size: 0.72rem;
		cursor: pointer;
	}
	.tag-toggle:hover {
		color: var(--text);
		border-color: var(--text-faint);
	}
	.tag-toggle-icon {
		display: inline-block;
		font-size: 0.6rem;
		transition: transform 0.12s;
	}
	.tag-toggle-icon.collapsed {
		transform: rotate(-90deg);
	}
	.tag-toggle-count {
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}
	.tag-filter-chip {
		padding: 0.15rem 0.65rem;
		background: color-mix(in srgb, var(--warning) 8%, transparent);
		border: var(--bw) solid color-mix(in srgb, var(--warning) 30%, transparent);
		border-radius: var(--r-pill);
		color: var(--warning);
		font-family: var(--font-mono);
		font-size: 0.72rem;
		cursor: pointer;
	}
	.tag-filter-chip:hover {
		background: color-mix(in srgb, var(--warning) 16%, transparent);
	}
	.tag-filter-chip.active {
		background: color-mix(in srgb, var(--warning) 28%, transparent);
		border-color: var(--warning);
	}
	.tag-chip-count {
		margin-left: 0.25rem;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}
	.tag-more,
	.tag-clear {
		padding: 0.15rem 0.5rem;
		background: transparent;
		border: var(--bw) solid transparent;
		border-radius: var(--r-pill);
		color: var(--text-muted);
		font: inherit;
		font-size: 0.72rem;
		cursor: pointer;
	}
	.tag-more:hover,
	.tag-clear:hover {
		color: var(--text);
		background: var(--bg-subtle);
	}

	/* BUG-194: 좁은 화면에선 머리말이 목록을 밀어낸다 — 줄바꿈 대신 그 줄만
	   가로 스크롤. 접기가 생겼어도 **펼친 상태**에서는 여전히 필요하다. */
	@media (max-width: 640px) {
		.tag-filter-row {
			flex-wrap: nowrap;
			overflow-x: auto;
			scrollbar-width: none;
			max-width: 100%;
		}
		.tag-filter-row::-webkit-scrollbar {
			display: none;
		}
		.tag-filter-chip,
		.tag-toggle,
		.tag-more,
		.tag-clear {
			flex: none;
		}
	}
</style>
