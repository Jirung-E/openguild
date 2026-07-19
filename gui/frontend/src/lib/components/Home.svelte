<!--
  DEV-011: Home 페이지.
   - 진행 중 캠페인 (전부) 가로 카드 슬라이드
   - "곧 시작되는 캠페인" (1주일 이내, 없으면 가장 빠른 다음) 작은 슬라이드
   - 캠페인 목록 / 캠페인 추가 버튼
   - 최근 추가/수정된 퀘스트 10개 (updated_at DESC)
-->
<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { campaignsApi } from '$lib/api/campaigns';
	import { questsApi } from '$lib/api/quests';
	// DEV-205 모듈2: 홈 문자열 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-015: status 표시 이름 — 언어 반응.
	import { statusLabel } from '$lib/utils/status-label';
	// BUG-025: 진행 중 = carousel (좌우 꽉, 1개씩 자동 회전),
	//          곧 시작 = conveyor (멈춤 없이 흐름).
	import CampaignCarousel from './CampaignCarousel.svelte';
	import CampaignConveyor from './CampaignConveyor.svelte';
	// DEV-076: 마감 임박 / Overdue 퀘스트 (Quest Board 노드 모양 carousel).
	import QuestNodeConveyor from './QuestNodeConveyor.svelte';
	// DEV-167: 작업 기록 요약 (히트맵) — /worklog 상세의 유일한 진입점.
	import WorklogSummaryCard from './WorklogSummaryCard.svelte';
	import type { CampaignSummary, Quest, QuestStatus, QuestType } from '$lib/types';
	import { metaApi } from '$lib/api/meta';
	import { isCampaignDone } from '$lib/utils/campaign-progress';
	// BUG-025: 캠페인 목록 페이지의 sort 옵션을 Home 카드에도 적용.
	import { loadCampaignSort, sortCampaigns, type CampaignSortMode } from '$lib/utils/campaign-sort';

	const RECENT_QUEST_LIMIT = 10;
	const UPCOMING_WINDOW_DAYS = 7;

	// BUG-024: backend 는 모든 active summary 반환. 분류 / 카운트다운은 frontend
	// 가 매초 시계로 처리. 페이지 보고 있는 동안 카드 자동 이동.
	let allActive = $state<CampaignSummary[]>([]);
	// DEV-076: 전체 alive quest. 임박/Overdue 는 전체에서 필터.
	let allQuests = $state<Quest[]>([]);
	// DEV-078: '최근 추가된' → '최근 추가/수정된' 으로 변경. updated_at DESC 정렬
	// (신규 = updated_at == created_at, 수정 = updated_at 갱신 → 자연스럽게 위로).
	let recentQuests = $derived(
		[...allQuests]
			.sort((a, b) => (b.updated_at < a.updated_at ? -1 : 1))
			.slice(0, RECENT_QUEST_LIMIT)
	);
	let types = $state<QuestType[]>([]);
	let statuses = $state<QuestStatus[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let now = $state(Date.now());
	let tickHandle: ReturnType<typeof setInterval> | null = null;
	// BUG-025: 캠페인 목록의 sort 와 sync. mount 후 load + 매번 새로 읽음
	// (목록 페이지 다녀와도 즉시 반영).
	let sort = $state<CampaignSortMode>('recent');

	async function loadHomeData() {
		try {
			const [a, q, t, s] = await Promise.all([
				campaignsApi.activeSummaries(),
				questsApi.list(),
				metaApi.getQuestTypes(),
				metaApi.getQuestStatuses()
			]);
			allActive = a;
			allQuests = q;
			types = t;
			statuses = s;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load home';
		} finally {
			loading = false;
		}
	}

	// DEV-095: Nav reindex 후 데이터 reload — store-bump subscribe.
	import { reindexBump } from '$lib/stores/reindex';
	let lastBump = $state(0);
	$effect(() => {
		const bump = $reindexBump;
		if (bump !== lastBump && bump > 0) {
			lastBump = bump;
			loading = true;
			loadHomeData();
		}
	});

	onMount(async () => {
		await loadHomeData();
		// BUG-025: localStorage 의 sort 옵션 적용 (캠페인 목록 페이지에서 변경
		// 했을 수 있음). 매번 mount 마다 새로 읽음.
		sort = loadCampaignSort();
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
		const inRange = allActive.filter((c) => {
			const start = dateStartMs(c.started_at);
			const end = dateEndMs(c.ended_at);
			if (start !== null && t < start) return false;
			if (end !== null && t > end) return false;
			return true;
		});
		// BUG-025: 캠페인 목록의 sort 옵션 적용.
		return sortCampaigns(inRange, sort, t);
	});

	// DEV-076 → BUG-034: 마감 임박 / Overdue 퀘스트 분류.
	//
	// 기준은 required_due (필수 기한) 만. desired_due 는 정보성.
	// status ∈ {done, cancelled} 는 제외.
	//
	// **임박 임계값 (urgency 별)** — 사용자 피드백 반영, 초기 안보다 lenient:
	//   1=Critical: 30일 이내 (장기 critical 일정도 항상 노출)
	//   2=High:     14일 이내
	//   3=Medium:    7일 이내
	//   4=Low:       3일 이내
	// 이전 (Critical=7/High=4/Medium=1/Low=1) 은 너무 엄격해서 19일 후 medium
	// 마감 퀘스트가 표시 안 되는 케이스 발생 — 사용자가 "왜 안 보이지" 라고
	// 의문 갖는 빈도가 높음. lenient 가 사용성 ↑.
	const IMMINENT_DAYS: Record<number, number> = { 1: 30, 2: 14, 3: 7, 4: 3 };
	const MS_PER_DAY = 24 * 60 * 60 * 1000;

	// BUG-034: 유효 기한 ms — required_due 와 earliest_campaign_due 중 더 빠른 것.
	// 같은 helper 를 SVG 도 사용 (lib/utils/quest-node-svg::effectiveQuestDue).
	function requiredDueMs(q: Quest): number | null {
		const q_due = q.required_due?.trim() || null;
		const c_due = q.earliest_campaign_due?.trim() || null;
		const earliest = q_due && c_due ? (q_due <= c_due ? q_due : c_due) : q_due || c_due;
		if (!earliest) return null;
		// 자정 비교가 자연스러움: 만료 == 그 날 끝.
		const t = new Date(`${earliest}T23:59:59`).getTime();
		return Number.isNaN(t) ? null : t;
	}
	function isDoneLike(q: Quest): boolean {
		return q.status_slug === 'done' || q.status_slug === 'cancelled';
	}

	let imminentQuests = $derived.by(() => {
		const t = now;
		const rows = allQuests
			.filter((q) => !isDoneLike(q))
			.filter((q) => {
				const due = requiredDueMs(q);
				if (due === null) return false;
				if (due < t) return false; // overdue 는 별도
				const window = (IMMINENT_DAYS[q.urgency] ?? 1) * MS_PER_DAY;
				return due - t <= window;
			});
		rows.sort((a, b) => (requiredDueMs(a) ?? 0) - (requiredDueMs(b) ?? 0));
		return rows;
	});

	let overdueQuests = $derived.by(() => {
		const t = now;
		const rows = allQuests
			.filter((q) => !isDoneLike(q))
			.filter((q) => {
				const due = requiredDueMs(q);
				return due !== null && due < t;
			});
		rows.sort((a, b) => (requiredDueMs(a) ?? 0) - (requiredDueMs(b) ?? 0));
		return rows;
	});

	// DEV-142 후속: 미해결 토론 댓글이 달린 퀘스트 — 홈에 '마감 지난 퀘스트' 와
	// 같은 방식(QuestNodeConveyor)으로 표시. 미해결 토론은 해결 전까지 완료 전환이
	// 막히는(DEV-142) 액션 아이템이라 눈에 띄어야 한다.
	let discussionQuests = $derived.by(() => {
		const rows = allQuests.filter((q) => (q.discussion_unresolved ?? 0) > 0);
		// 미해결 토론 수 많은 순 → 같은 수면 최근 수정 순.
		rows.sort(
			(a, b) =>
				(b.discussion_unresolved ?? 0) - (a.discussion_unresolved ?? 0) ||
				(b.updated_at ?? '').localeCompare(a.updated_at ?? '')
		);
		return rows;
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

	// DEV-080 → DEV-081 → DEV-093 fix2: 마감 지난 캠페인.
	//
	// 필터:
	//   - 체크리스트 또는 연결 quest 중 적어도 하나가 있음 (둘 다 없으면 "달성" 모호 → 제외)
	//   - "완료" 상태가 아님 (`isCampaignDone` — 체크리스트 + quest 양쪽 다 100% 가 아님)
	//   - ended_at 지남
	let overdueCampaigns = $derived.by(() => {
		const t = now;
		const rows = allActive.filter((c) => {
			const hasChecklist = c.checklist_total > 0;
			const hasQuests = (c.quest_total ?? 0) > 0;
			if (!hasChecklist && !hasQuests) return false;
			if (isCampaignDone(c)) return false;
			const end = dateEndMs(c.ended_at);
			return end !== null && end < t;
		});
		// 가장 오래 지난 것부터 (= ended_at ASC). 가장 시급한 게 위로.
		rows.sort((a, b) => (dateEndMs(a.ended_at) ?? 0) - (dateEndMs(b.ended_at) ?? 0));
		return rows;
	});

	function typeColor(prefix: string): string {
		return types.find((t) => t.prefix === prefix)?.color ?? '#666';
	}
	// DEV-015: status 표시 이름 — 언어 반응(ko 면 name_ko 우선, 빈 값이면 en).
	function statusName(slug: string): string {
		const s = statuses.find((x) => x.slug === slug);
		return s ? statusLabel(s, $locale) : slug;
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
			<h2>{t('home.activeCampaigns', $locale)} <span class="count">({currentActive.length})</span></h2>
			<CampaignCarousel summaries={currentActive} {now} />

			<!-- ── 곧 시작 ─────────────────────────────── -->
			<h3>{t('home.upcomingCampaigns', $locale)} <span class="count">({upcomingSummaries.length})</span></h3>
			<CampaignConveyor summaries={upcomingSummaries} {now} />

			<!-- ── DEV-080: 마감 지난 캠페인 (있을 때만). 모양 / 동작은 곧 시작과 동일. ── -->
			{#if overdueCampaigns.length > 0}
				<h3>
					{t('home.overdueCampaigns', $locale)}
					<span class="count overdue">({overdueCampaigns.length})</span>
				</h3>
				<CampaignConveyor
					summaries={overdueCampaigns}
					{now}
					mode="overdue"
					emptyText={t('home.overdueCampaignsEmpty', $locale)}
				/>
			{/if}

			<div class="actions">
				<button class="btn-link" type="button" onclick={() => goto('/campaigns')}>
					{t('home.campaignList', $locale)}
				</button>
				<button class="btn-primary" type="button" onclick={() => goto('/campaigns/new')}>
					{t('home.addCampaign', $locale)}
				</button>
			</div>
		</section>

		<!-- DEV-076: 마감 지난 퀘스트 (있을 때만) ──────── -->
		{#if overdueQuests.length > 0}
			<section class="block">
				<h2>
					{t('home.overdueQuests', $locale)}
					<span class="count overdue">({overdueQuests.length})</span>
				</h2>
				<QuestNodeConveyor quests={overdueQuests} mode="overdue" />
			</section>
		{/if}

		<!-- DEV-142 후속: 미해결 토론 댓글 퀘스트 (있을 때만) ──── -->
		{#if discussionQuests.length > 0}
			<section class="block">
				<h2>
					{t('home.discussionComments', $locale)}
					<span class="count overdue">({discussionQuests.length})</span>
				</h2>
				<QuestNodeConveyor quests={discussionQuests} mode="overdue" />
			</section>
		{/if}

		<!-- DEV-076: 마감 임박 퀘스트 ─────────────────── -->
		{#if imminentQuests.length > 0}
			<section class="block">
				<h2>
					{t('home.imminentQuests', $locale)}
					<span class="count">({imminentQuests.length})</span>
				</h2>
				<QuestNodeConveyor quests={imminentQuests} mode="imminent" />
			</section>
		{/if}

		<!-- ── DEV-167: 작업 기록 요약 (히트맵 + 오늘 집계) — admin 요청으로
		     '최근 퀘스트' 위에 배치. ─────────────────────────────────── -->
		<WorklogSummaryCard />

		<!-- ── 최근 추가/수정된 퀘스트 (DEV-078) ─────────────────────── -->
		<section class="block">
			<!-- BUG-029: 최근은 최대 RECENT_QUEST_LIMIT (10) 으로 항상 잘림. 숫자 표시 X.
			     DEV-078: '추가된' → '추가/수정된'. updated_at DESC 정렬로 수정된 것도 위로. -->
			<h2>{t('home.recentQuests', $locale)}</h2>
			{#if recentQuests.length === 0}
				<div class="empty">{t('home.noQuests', $locale)}</div>
			{:else}
				<ul class="quest-list">
					{#each recentQuests as q (q.id)}
						<li>
							<a class="quest-row" href={`/quests/${encodeURIComponent(q.quest_id)}?from=home`}>
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
		max-width: var(--content-max-width, 1100px);
		margin: 0 auto;
	}
	.state {
		padding: 2rem 0;
		color: var(--text-muted);
		font-size: 0.875rem;
	}
	.state.error {
		color: var(--danger);
	}

	.block {
		margin-bottom: 2rem;
	}
	.block h2 {
		font-size: 1.05rem;
		font-weight: 600;
		color: var(--text);
		margin: 0 0 0.4rem 0;
	}
	.block h3 {
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--text-muted);
		margin: 1rem 0 0.2rem 0;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	/* BUG-028: 섹션 제목의 (n) 개수 — 흐릿한 회색. */
	.count {
		color: var(--text-faint);
		font-weight: 400;
		font-size: 0.85em;
		margin-left: 0.25rem;
	}
	/* DEV-076: overdue 개수는 빨간색으로 강조 (시급한 시각 경고). */
	.count.overdue {
		color: var(--danger);
		font-weight: 600;
	}

	.actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
	.btn-link {
		padding: 0.35rem 0.85rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.825rem;
		cursor: pointer;
	}
	.btn-link:hover {
		background: var(--bg-subtle);
	}
	.btn-primary {
		padding: 0.35rem 0.85rem;
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.825rem;
		cursor: pointer;
	}
	.btn-primary:hover {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}

	.empty {
		color: var(--text-faint);
		font-size: 0.875rem;
		padding: 0.75rem 0;
	}

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
		background: var(--bg-elevated);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		color: inherit;
		text-decoration: none;
		transition:
			border-color 0.15s,
			background 0.15s;
	}
	.quest-row:hover {
		border-color: var(--text-faint);
		background: var(--bg-subtle);
	}

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
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
