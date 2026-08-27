<script lang="ts">
	import Icon from './Icon.svelte';
	import { goto } from '$app/navigation';
	import { urgencyColor, urgencyLabel, urgencyOutOfRange, type Quest } from '$lib/types';
	// DEV-015: status 표시 이름 — 언어 반응.
	import { locale } from '$lib/stores/locale';
	import { questStatusLabel } from '$lib/utils/status-label';

	let {
		quest,
		depth,
		hasChildren,
		expanded,
		ontoggle,
		// admin 요청: 리스트 뷰는 접기/펼치기가 없으므로 토글 자리를 아예 비운다
		// — 트리 뷰에서만 필요한 들여쓰기다.
		flat = false
	}: {
		quest: Quest;
		depth: number;
		hasChildren: boolean;
		expanded: boolean;
		ontoggle: () => void;
		flat?: boolean;
	} = $props();

	/** 2단(모바일) 배치에서 제목을 SLUG 와 같은 x 에 맞추기 위한 들여쓰기.
	 *  트리 뷰는 토글 폭(20px) + gap(0.6rem) 만큼, 리스트 뷰는 0. */
	const titleIndent = $derived(flat ? '0px' : 'calc(20px + 0.6rem)');
</script>

<div
	class="item"
	style:padding-left={`${depth * 1.5 + 1}rem`}
	style:--title-indent={titleIndent}
	role="button"
	tabindex="0"
	onclick={() => goto(`/quests/${quest.quest_id}?from=list`)}
	onkeydown={(e) => e.key === 'Enter' && goto(`/quests/${quest.quest_id}?from=list`)}
>
	<!-- 접기/펼치기 — 리스트 뷰에선 렌더하지 않는다(자리도 차지하지 않게). -->
	{#if !flat}
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
	{/if}

	<!-- 타입 뱃지 -->
	<span class="pill mono" style:--c={quest.type_color}>{quest.quest_id}</span>

	<!-- 제목 -->
	<span class="title">{quest.title}</span>

	<!-- DEV-142 후속: 토론 댓글 — 일반 댓글과 별도 아이콘. 미해결=빨강 / 해결=초록. -->
	{#if (quest.discussion_unresolved ?? 0) > 0}
		<span
			class="discussion-count unresolved"
			title={`미해결 토론 ${quest.discussion_unresolved}개`}
		>
			<span class="dc-icon">✗</span><span>{quest.discussion_unresolved}</span>
		</span>
	{:else if (quest.discussion_resolved ?? 0) > 0}
		<span class="discussion-count resolved" title={`해결된 토론 ${quest.discussion_resolved}개`}>
			<span class="dc-icon">✓</span><span>{quest.discussion_resolved}</span>
		</span>
	{/if}

	<!-- DEV-116: 댓글 개수 — 0 이면 표시 X. -->
	{#if (quest.comment_count ?? 0) > 0}
		<span class="comment-count" title={`댓글 ${quest.comment_count}개`}>
			<span class="cc-icon"><Icon name="comment" size={12} /></span><span
				>{quest.comment_count}</span
			>
		</span>
	{/if}

	<!-- BUG-060 후속: 원본 urgency 가 범위(1-4) 밖이면 경고. clamp 된 값으로 표시. -->
	{#if urgencyOutOfRange(quest.urgency)}
		<span
			class="urgency-warn"
			title={`urgency 원본값 ${quest.urgency} 가 유효 범위(1-4) 밖 — clamp 표시 중. 파일 정정 필요.`}
			>⚠</span
		>
	{/if}

	<!-- 긴급도 -->
	<span class="pill" style:--c={urgencyColor(quest.urgency)}>
		{urgencyLabel(quest.urgency, $locale)}
	</span>

	<!-- 상태 -->
	<span class="pill" style:--c={quest.status_color}>
		{questStatusLabel(quest, $locale)}
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
		border-bottom: var(--bw) solid var(--bg-subtle);
		cursor: pointer;
		transition: background 0.1s;
	}

	.item:hover {
		background: var(--bg-elevated);
	}

	.toggle {
		width: 1.25rem;
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

	/* DEV-116: 댓글 개수 — 작은 회색 pill. */
	.comment-count {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		padding: 0.1rem 0.45rem;
		border-radius: var(--r-pill);
		font-size: 0.72rem;
		color: var(--text-muted);
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
	}
	.cc-icon {
		font-size: 0.7rem;
		line-height: 1;
	}
	/* DEV-142 후속: 토론 댓글 pill — 일반 댓글과 색으로 구분. */
	.discussion-count {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		padding: 0.1rem 0.45rem;
		border-radius: var(--r-pill);
		font-size: 0.72rem;
		font-weight: 600;
	}
	.discussion-count.unresolved {
		color: var(--danger);
		background: color-mix(in srgb, var(--danger) 14%, transparent);
		border: var(--bw) solid color-mix(in srgb, var(--danger) 40%, transparent);
	}
	.discussion-count.resolved {
		color: var(--success-strong);
		background: color-mix(in srgb, var(--success) 14%, transparent);
		border: var(--bw) solid color-mix(in srgb, var(--success) 40%, transparent);
	}
	.dc-icon {
		font-size: 0.9rem;
		line-height: 1;
		font-weight: 700;
	}
	/* BUG-060 후속: 범위 밖 urgency 경고 — 빨간 ⚠. */
	.urgency-warn {
		flex-shrink: 0;
		color: var(--danger);
		font-size: 0.85rem;
		line-height: 1;
		cursor: help;
	}

	/* admin 요청: 좁은 화면에서 `SLUG 배지들` / `제목` 2단으로.
	   한 줄에 다 넣으면 배지들이 자리를 먼저 가져가 제목이 몇 글자만 남는다.
	   마크업은 그대로 두고 wrap + order 로만 바꾼다 — 제목만 마지막 순서로
	   보내고 폭을 100% 로 주면 자기 줄을 통째로 쓴다.
	   (미디어 쿼리는 기본 규칙보다 **뒤**에 둔다 — 특이성이 같으면 순서가
	    이긴다. BUG-200 에서 이걸 놓쳐 수정이 통째로 무효였다.) */
	@media (max-width: 640px) {
		.item {
			flex-wrap: wrap;
			row-gap: 0.15rem;
		}
		.title {
			order: 10;
			flex: 1 1 100%;
			min-width: 0;
			/* admin 요청: 제목이 SLUG 보다 왼쪽에서 시작해 어긋나 보였다.
			   트리 뷰는 토글 폭만큼 밀어 SLUG 와 같은 x 에서 시작하게 하고,
			   리스트 뷰는 토글이 없으니 0(둘 다 왼쪽 끝). */
			padding-left: var(--title-indent, 0px);
		}
	}
</style>
