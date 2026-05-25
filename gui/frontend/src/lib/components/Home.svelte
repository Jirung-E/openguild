<!--
  DEV-011: Home 페이지.
   - 진행 중 캠페인 (전부) 가로 카드 슬라이드
   - "곧 시작되는 캠페인" (1주일 이내, 없으면 가장 빠른 다음) 작은 슬라이드
   - 캠페인 목록 / 캠페인 추가 버튼
   - 최근 추가된 퀘스트 10개
-->
<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
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
	const UPCOMING_WINDOW_DAYS = 7;

	// BUG-024: backend 는 모든 active summary 반환. 분류 / 카운트다운은 frontend
	// 가 매초 시계로 처리. 페이지 보고 있는 동안 카드 자동 이동.
	let allActive = $state<CampaignSummary[]>([]);
	let recentQuests = $state<Quest[]>([]);
	let types = $state<QuestType[]>([]);
	let statuses = $state<QuestStatus[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let now = $state(Date.now());
	let tickHandle: ReturnType<typeof setInterval> | null = null;

	onMount(async () => {
		try {
			const [a, q, t, s] = await Promise.all([
				campaignsApi.activeSummaries(),
				questsApi.list(),
				metaApi.getQuestTypes(),
				metaApi.getQuestStatuses()
			]);
			allActive = a;
			recentQuests = q.slice(0, RECENT_QUEST_LIMIT);
			types = t;
			statuses = s;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load home';
		} finally {
			loading = false;
		}
		// BUG-024: 매초 now 갱신 — 카드 분류 / 카운트다운 자동 reactive.
		tickHandle = setInterval(() => {
			now = Date.now();
		}, 1000);
	});
	onDestroy(() => {
		if (tickHandle) clearInterval(tickHandle);
		tickHandle = null;
	});

	// ── BUG-024: 분류 (now 변할 때마다 자동 재계산) ───────────────────
	// 시작일 / 종료일은 'YYYY-MM-DD' (또는 빈 문자열). 빈 문자열 = "무기한".
	function dateStartMs(d: string | null | undefined): number | null {
		if (!d?.trim()) return null;
		const t = new Date(`${d}T00:00:00`).getTime();
		return Number.isNaN(t) ? null : t;
	}
	function dateEndMs(d: string | null | undefined): number | null {
		if (!d?.trim()) return null;
		const t = new Date(`${d}T23:59:59`).getTime();
		return Number.isNaN(t) ? null : t;
	}

	let currentActive = $derived.by(() => {
		const t = now;
		return allActive.filter((c) => {
			const start = dateStartMs(c.started_at);
			const end = dateEndMs(c.ended_at);
			// 시작 전이면 진행 중 아님.
			if (start !== null && t < start) return false;
			// 종료 후면 진행 중 아님.
			if (end !== null && t > end) return false;
			return true;
		});
	});

	let upcomingSummaries = $derived.by(() => {
		const t = now;
		const winEnd = t + UPCOMING_WINDOW_DAYS * 24 * 60 * 60 * 1000;
		// 1주 이내 시작 예정.
		const within = allActive
			.filter((c) => {
				const start = dateStartMs(c.started_at);
				return start !== null && start > t && start <= winEnd;
			})
			.sort((a, b) => {
				const sa = dateStartMs(a.started_at) ?? Infinity;
				const sb = dateStartMs(b.started_at) ?? Infinity;
				return sa - sb;
			});
		if (within.length > 0) return within;
		// fallback: 1주 윈도우에 없으면 가장 빠른 미래 시작 1개.
		const futureSorted = allActive
			.filter((c) => {
				const start = dateStartMs(c.started_at);
				return start !== null && start > t;
			})
			.sort((a, b) => {
				const sa = dateStartMs(a.started_at) ?? Infinity;
				const sb = dateStartMs(b.started_at) ?? Infinity;
				return sa - sb;
			});
		return futureSorted.slice(0, 1);
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
				summaries={currentActive}
				mode="active"
				{now}
				emptyText="진행 중인 캠페인이 없습니다."
			/>

			<!-- ── 곧 시작 ─────────────────────────────── -->
			<h3>곧 시작되는 캠페인</h3>
			<CampaignCardSlider
				summaries={upcomingSummaries}
				mode="upcoming"
				{now}
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

	/* BUG-021: Quest List 의 pill 스타일 통일 (color-mix bg + border). */
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
	.badge.status {
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
