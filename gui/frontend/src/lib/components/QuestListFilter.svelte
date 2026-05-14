<script lang="ts">
	import type { QuestStatus, QuestType } from '$lib/types';

	let {
		types,
		statuses,
		typeIds = $bindable(new Set<number>()),
		statusIds = $bindable(new Set<number>())
	}: {
		types: QuestType[];
		statuses: QuestStatus[];
		typeIds: Set<number>;
		statusIds: Set<number>;
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
</style>
