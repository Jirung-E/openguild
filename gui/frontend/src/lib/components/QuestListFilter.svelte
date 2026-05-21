<script lang="ts">
	import type { QuestStatus, QuestType } from '$lib/types';

	let {
		types,
		statuses,
		typeIds = $bindable(new Set<number>()),
		statusIds = $bindable(new Set<number>()),
		search = $bindable(''),
		titleOnly = $bindable(false)
	}: {
		types: QuestType[];
		statuses: QuestStatus[];
		typeIds: Set<number>;
		statusIds: Set<number>;
		search?: string;
		titleOnly?: boolean;
	} = $props();

	function toggle(set: Set<number>, id: number): Set<number> {
		const next = new Set(set);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		return next;
	}
</script>

<div class="filter-bar">
	<div class="filter-group">
		<button class:active={typeIds.size === 0} onclick={() => (typeIds = new Set())}>All</button>
		{#each types as t}
			<button
				class:active={typeIds.has(t.id)}
				style:--c={t.color}
				onclick={() => (typeIds = toggle(typeIds, t.id))}
			>
				{t.prefix}
			</button>
		{/each}
	</div>

	<div class="divider"></div>

	<div class="filter-group">
		<button class:active={statusIds.size === 0} onclick={() => (statusIds = new Set())}>All</button>
		{#each statuses as s}
			<button
				class:active={statusIds.has(s.id)}
				style:--c={s.color}
				onclick={() => (statusIds = toggle(statusIds, s.id))}
			>
				{s.name_en}
			</button>
		{/each}
	</div>

	<div class="divider"></div>

	<!-- DEV-037: 검색 -->
	<div class="search-group">
		<label class="search-input-wrap">
			<span class="sr-only">검색</span>
			<input
				type="search"
				class="search-input"
				placeholder="검색 (제목 / 본문)"
				bind:value={search}
				data-testid="quest-search-input"
			/>
			{#if search}
				<button
					type="button"
					class="search-clear"
					title="검색어 지우기"
					onclick={() => (search = '')}
					data-testid="quest-search-clear"
				>×</button>
			{/if}
		</label>
		<label class="search-opt">
			<input
				type="checkbox"
				bind:checked={titleOnly}
				data-testid="quest-search-title-only"
			/>
			<span>제목만</span>
		</label>
	</div>
</div>

<style>
	.filter-bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem 1.5rem;
		background: #161b22;
		border-bottom: 1px solid #21262d;
		flex-wrap: wrap;
	}

	.filter-group {
		display: flex;
		gap: 0.25rem;
		flex-wrap: wrap;
	}

	.divider {
		width: 1px;
		height: 20px;
		background: #21262d;
	}

	button {
		padding: 0.25rem 0.65rem;
		border: 1px solid #30363d;
		border-radius: 20px;
		background: transparent;
		color: #8b949e;
		font-size: 0.8rem;
		cursor: pointer;
		transition: all 0.15s;
	}

	button:hover {
		border-color: #8b949e;
		color: #c9d1d9;
	}

	button.active {
		background: color-mix(in srgb, var(--c, #4a90d9) 20%, transparent);
		border-color: var(--c, #4a90d9);
		color: var(--c, #4a90d9);
	}

	/* --- 검색 영역 --- */
	.search-group {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
	}
	.search-input-wrap {
		position: relative;
		display: inline-flex;
		align-items: center;
	}
	.sr-only {
		position: absolute;
		width: 1px; height: 1px; padding: 0; margin: -1px;
		overflow: hidden; clip: rect(0,0,0,0); white-space: nowrap; border: 0;
	}
	.search-input {
		padding: 0.3rem 1.8rem 0.3rem 0.7rem;
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #c9d1d9;
		font-size: 0.8rem;
		min-width: 200px;
		outline: none;
		transition: border-color 0.15s;
	}
	.search-input:focus { border-color: #58a6ff; }
	.search-input::-webkit-search-cancel-button { display: none; }
	.search-clear {
		position: absolute;
		right: 0.3rem;
		padding: 0 0.4rem;
		border: none;
		border-radius: 12px;
		background: transparent;
		color: #6e7681;
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
	}
	.search-clear:hover { color: #e94f4f; background: transparent; }
	.search-opt {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.8rem;
		color: #8b949e;
		cursor: pointer;
		user-select: none;
	}
	.search-opt input { cursor: pointer; }
	.search-opt:hover { color: #c9d1d9; }
</style>
