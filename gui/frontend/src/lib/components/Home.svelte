<!--
  DEV-011: Home 페이지.
   - 진행 중 캠페인 (전부) 가로 카드 슬라이드
   - "곧 시작되는 캠페인" (1주일 이내, 없으면 가장 빠른 다음) 작은 슬라이드
   - 캠페인 목록 / 캠페인 추가 버튼
   - 최근 추가된 퀘스트 10개
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { campaignsApi } from '$lib/api/campaigns';
	import { questsApi } from '$lib/api/quests';
	import CampaignCardSlider from './CampaignCardSlider.svelte';
	import type {
		CampaignSummary,
		Quest,
		QuestStatus,
		QuestType
	} from '$lib/types';
	import { metaApi } from '$lib/api/meta';

	const RECENT_QUEST_LIMIT = 10;

	let activeSummaries = $state<CampaignSummary[]>([]);
	let upcomingSummaries = $state<CampaignSummary[]>([]);
	let recentQuests = $state<Quest[]>([]);
	let types = $state<QuestType[]>([]);
	let statuses = $state<QuestStatus[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		try {
			const [a, u, q, t, s] = await Promise.all([
				campaignsApi.activeSummaries(),
				campaignsApi.upcomingSummaries(7),
				questsApi.list(),
				metaApi.getQuestTypes(),
				metaApi.getQuestStatuses()
			]);
			activeSummaries = a;
			upcomingSummaries = u;
			// 최근 추가 순 정렬 — id DESC 가 quest 의 자연 순서.
			recentQuests = q.slice(0, RECENT_QUEST_LIMIT);
			types = t;
			statuses = s;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load home';
		} finally {
			loading = false;
		}
	});

	function typeColor(prefix: string): string {
		return types.find((t) => t.prefix === prefix)?.color ?? '#666';
	}
	function statusName(slug: string): string {
		return statuses.find((s) => s.slug === slug)?.name_en ?? slug;
	}
	function statusColor(slug: string): string {
		return statuses.find((s) => s.slug === slug)?.color ?? '#666';
	}
</script>

<div class="home">
	{#if loading}
		<div class="state">Loading…</div>
	{:else if error}
		<div class="state error">{error}</div>
	{:else}
		<!-- ── 진행 중 캠페인 ─────────────────────────── -->
		<section class="block">
			<h2>진행 중 캠페인</h2>
			<CampaignCardSlider
				summaries={activeSummaries}
				size="lg"
				emptyText="진행 중인 캠페인이 없습니다."
			/>

			<!-- ── 곧 시작 ─────────────────────────────── -->
			<h3>곧 시작되는 캠페인</h3>
			<CampaignCardSlider
				summaries={upcomingSummaries}
				size="sm"
				emptyText="곧 시작 예정인 캠페인이 없습니다."
			/>

			<div class="actions">
				<button class="btn-link" type="button" onclick={() => goto('/campaigns')}>
					캠페인 목록
				</button>
				<button class="btn-primary" type="button" onclick={() => goto('/campaigns/new')}>
					+ 캠페인 추가
				</button>
			</div>
		</section>

		<!-- ── 최근 추가된 퀘스트 ─────────────────────── -->
		<section class="block">
			<h2>최근 추가된 퀘스트</h2>
			{#if recentQuests.length === 0}
				<div class="empty">아직 퀘스트가 없습니다.</div>
			{:else}
				<ul class="quest-list">
					{#each recentQuests as q (q.id)}
						<li>
							<a
								class="quest-row"
								href={`/quests/${encodeURIComponent(q.quest_id)}?from=home`}
							>
								<span class="badge type" style:--c={typeColor(q.type_prefix)}>
									{q.quest_id}
								</span>
								<span class="title">{q.title}</span>
								<span class="badge status" style:--c={statusColor(q.status_slug)}>
									{statusName(q.status_slug)}
								</span>
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
</div>

<style>
	.home {
		padding: 1.25rem 1.5rem 2rem;
		max-width: 1100px;
		margin: 0 auto;
	}
	.state { padding: 2rem 0; color: #8b949e; font-size: 0.875rem; }
	.state.error { color: #f85149; }

	.block { margin-bottom: 2rem; }
	.block h2 {
		font-size: 1.05rem;
		font-weight: 600;
		color: #c9d1d9;
		margin: 0 0 0.4rem 0;
	}
	.block h3 {
		font-size: 0.8rem;
		font-weight: 500;
		color: #8b949e;
		margin: 1rem 0 0.2rem 0;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
	.btn-link {
		padding: 0.35rem 0.85rem;
		background: transparent;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #c9d1d9;
		font-size: 0.825rem;
		cursor: pointer;
	}
	.btn-link:hover { background: #21262d; }
	.btn-primary {
		padding: 0.35rem 0.85rem;
		background: #238636;
		border: 1px solid #2ea043;
		border-radius: 6px;
		color: #fff;
		font-size: 0.825rem;
		cursor: pointer;
	}
	.btn-primary:hover { background: #2ea043; }

	.empty { color: #6e7681; font-size: 0.875rem; padding: 0.75rem 0; }

	.quest-list {
		list-style: none;
		padding: 0;
		margin: 0.4rem 0 0 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.quest-row {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.5rem 0.75rem;
		background: #161b22;
		border: 1px solid #21262d;
		border-radius: 6px;
		color: inherit;
		text-decoration: none;
		transition: border-color 0.15s, background 0.15s;
	}
	.quest-row:hover { border-color: #484f58; background: #1c2128; }

	.badge {
		font-size: 0.7rem;
		padding: 0.1rem 0.45rem;
		border-radius: 4px;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		color: var(--c);
		border: 1px solid var(--c);
		flex-shrink: 0;
	}
	.badge.status {
		font-family: inherit;
		font-size: 0.7rem;
		margin-left: auto;
	}
	.title {
		flex: 1;
		font-size: 0.875rem;
		color: #c9d1d9;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
