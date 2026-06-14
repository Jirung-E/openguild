<script lang="ts">
	import { goto } from '$app/navigation';
	import { urgencyColor, urgencyLabel, urgencyOutOfRange, type Quest } from '$lib/types';

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
	onclick={() => goto(`/quests/${quest.quest_id}?from=list`)}
	onkeydown={(e) => e.key === 'Enter' && goto(`/quests/${quest.quest_id}?from=list`)}
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

	<!-- DEV-142 후속: 토론 댓글 — 일반 댓글과 별도 아이콘. 미해결=빨강 / 해결=초록. -->
	{#if (quest.discussion_unresolved ?? 0) > 0}
		<span
			class="discussion-count unresolved"
			title={`미해결 토론 ${quest.discussion_unresolved}개`}
		>
			<span class="dc-icon">🗨</span><span>{quest.discussion_unresolved}</span>
		</span>
	{:else if (quest.discussion_resolved ?? 0) > 0}
		<span class="discussion-count resolved" title={`해결된 토론 ${quest.discussion_resolved}개`}>
			<span class="dc-icon">🗨</span><span>{quest.discussion_resolved}</span>
		</span>
	{/if}

	<!-- DEV-116: 댓글 개수 — 0 이면 표시 X. -->
	{#if (quest.comment_count ?? 0) > 0}
		<span class="comment-count" title={`댓글 ${quest.comment_count}개`}>
			<span class="cc-icon">💬</span><span>{quest.comment_count}</span>
		</span>
	{/if}

	<!-- BUG-060 후속: 원본 urgency 가 범위(1-4) 밖이면 경고. clamp 된 값으로 표시. -->
	{#if urgencyOutOfRange(quest.urgency)}
		<span
			class="urgency-warn"
			title={`urgency 원본값 ${quest.urgency} 가 유효 범위(1-4) 밖 — clamp 표시 중. 파일 정정 필요.`}
		>⚠</span>
	{/if}

	<!-- 긴급도 -->
	<span class="badge urgency" style:--c={urgencyColor(quest.urgency)}>
		{urgencyLabel(quest.urgency)}
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
		border-bottom: 1px solid var(--bg-subtle);
		cursor: pointer;
		transition: background 0.1s;
	}

	.item:hover {
		background: var(--bg-elevated);
	}

	.toggle {
		width: 20px;
		flex-shrink: 0;
		background: none;
		border: none;
		color: var(--text-muted);
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
		color: var(--text);
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
	/* DEV-116: 댓글 개수 — 작은 회색 pill. */
	.comment-count {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		padding: 0.1rem 0.45rem;
		border-radius: 20px;
		font-size: 0.72rem;
		color: var(--text-muted);
		background: var(--bg-subtle);
		border: 1px solid var(--border);
	}
	.cc-icon { font-size: 0.7rem; line-height: 1; }
	/* DEV-142 후속: 토론 댓글 pill — 일반 댓글과 색으로 구분. */
	.discussion-count {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		padding: 0.1rem 0.45rem;
		border-radius: 20px;
		font-size: 0.72rem;
		font-weight: 600;
	}
	.discussion-count.unresolved {
		color: var(--danger);
		background: color-mix(in srgb, var(--danger) 14%, transparent);
		border: 1px solid color-mix(in srgb, var(--danger) 40%, transparent);
	}
	.discussion-count.resolved {
		color: var(--success-strong);
		background: color-mix(in srgb, var(--success) 14%, transparent);
		border: 1px solid color-mix(in srgb, var(--success) 40%, transparent);
	}
	.dc-icon { font-size: 0.7rem; line-height: 1; }
	/* BUG-060 후속: 범위 밖 urgency 경고 — 빨간 ⚠. */
	.urgency-warn {
		flex-shrink: 0;
		color: var(--danger);
		font-size: 0.85rem;
		line-height: 1;
		cursor: help;
	}
</style>
