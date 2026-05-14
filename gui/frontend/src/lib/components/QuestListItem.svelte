<script lang="ts">
	import { goto } from '$app/navigation';
	import { URGENCY_COLOR, URGENCY_LABEL, type Quest } from '$lib/types';

	let {
		quest,
		depth,
		hasChildren,
		expanded,
		ontoggle
	}: {
		quest: Quest;
		depth: number;
		hasChildren: boolean;
		expanded: boolean;
		ontoggle: () => void;
	} = $props();
</script>

<div
	class="item"
	style:padding-left={`${depth * 1.5 + 1}rem`}
	role="button"
	tabindex="0"
	onclick={() => goto(`/quests/${quest.quest_id}`)}
	onkeydown={(e) => e.key === 'Enter' && goto(`/quests/${quest.quest_id}`)}
>
	<!-- 접기/펼치기 -->
	<button
		class="toggle"
		class:invisible={!hasChildren}
		onclick={(e) => {
			e.stopPropagation();
			ontoggle();
		}}
		aria-label={expanded ? 'collapse' : 'expand'}
	>
		{expanded ? '▾' : '▸'}
	</button>

	<!-- 타입 뱃지 -->
	<span class="badge type" style:--c={quest.type_color}>{quest.quest_id}</span>

	<!-- 제목 -->
	<span class="title">{quest.title}</span>

	<!-- 긴급도 -->
	<span class="badge urgency" style:--c={URGENCY_COLOR[quest.urgency]}>
		{URGENCY_LABEL[quest.urgency]}
	</span>

	<!-- 상태 -->
	<span class="badge status" style:--c={quest.status_color}>
		{quest.status_name_en}
	</span>
</div>

<style>
	.item {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding-top: 0.55rem;
		padding-bottom: 0.55rem;
		padding-right: 1.5rem;
		border-bottom: 1px solid #21262d;
		cursor: pointer;
		transition: background 0.1s;
	}

	.item:hover {
		background: #161b22;
	}

	.toggle {
		width: 20px;
		flex-shrink: 0;
		background: none;
		border: none;
		color: #8b949e;
		font-size: 0.75rem;
		padding: 0;
		cursor: pointer;
	}

	.toggle.invisible {
		visibility: hidden;
	}

	.title {
		flex: 1;
		font-size: 0.9rem;
		color: #c9d1d9;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.badge {
		flex-shrink: 0;
		padding: 0.15rem 0.55rem;
		border-radius: 20px;
		font-size: 0.75rem;
		font-weight: 500;
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
</style>
