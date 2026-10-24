<!--
  DEV-011: Campaign 목록 페이지 (/campaigns).
   - 정렬: 최근 추가 순 (기본) / 남은 날짜 순 / 수동 (display_order)
   - 어드민이 수동 모드일 때 ↑↓ 버튼으로 순서 변경 (display_order 갱신)
   - 각 카드 클릭 → /campaigns/<slug> detail
   - 우상단 "+ 새 캠페인" 버튼
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { campaignsApi } from '$lib/api/campaigns';
	// DEV-205 모듈2: 캠페인 목록 문자열 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-259: alert() 잔재 제거 — 앱 공용 toast 로 통일.
	import { showToast } from '$lib/stores/toast';
	import type { Campaign, CampaignSummary } from '$lib/types';
	import { isDateOverdue } from '$lib/utils/datetime';
	import NewCampaignModal from '$lib/components/NewCampaignModal.svelte';
	// DEV-452 (REQ-039): 검색칸 — 퀘스트 목록과 같은 부품([검색 | 쿼리]), 키만 캠페인 것.
	import SearchField, { type SearchMode } from '$lib/components/SearchField.svelte';
	import QueryNotes from '$lib/components/QueryNotes.svelte';

	// BUG-025: sort 옵션을 localStorage 에 저장 (lib/utils/campaign-sort) →
	// Home 의 카드 정렬도 같은 값 적용.
	import {
		loadCampaignSort,
		saveCampaignSort,
		sortCampaigns,
		type CampaignSortMode
	} from '$lib/utils/campaign-sort';

	// admin 요청: 목록에서도 진행도를 보여준다. `list()` 는 진행도가 없는 원본
	// 행이라 summary 엔드포인트로 바꿨다(카드/상세와 같은 계산을 재사용).
	let all = $state<CampaignSummary[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let sort = $state<CampaignSortMode>(loadCampaignSort());
	let statusFilter = $state<'all' | 'active' | 'done'>('all');

	// sort 변경마다 localStorage 저장.
	$effect(() => {
		saveCampaignSort(sort);
	});

	onMount(async () => {
		try {
			all = await campaignsApi.listSummaries();
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	});

	// DEV-452: 검색 · 쿼리 — 둘 다 코어가 거르고(`GET /api/campaigns?q=`), 여기는 맞는 번호만 받아
	// 상태 선택과 **그리고**로 겹친다. 검색은 글 전체를 따옴표로 묶은 쿼리다(제목 · 설명 · 번호에서 찾는다).
	// 틀린 동안은 거르지 않는다(퀘스트 목록과 같다). 주소 `?search=` · `?q=` 에 실린다.
	const CAMPAIGN_QUERY_KEYS = [
		['status', 'status:active', 'campaignList.queryHelpStatus'],
		['start', 'start:>=2026-10-01', 'campaignList.queryHelpStart'],
		['end', 'end:overdue', 'campaignList.queryHelpEnd'],
		['quest', 'quest:DEV-001', 'campaignList.queryHelpQuest']
	] as const;
	const initialSearch = $page.url.searchParams.get('search') ?? '';
	const initialQuery = $page.url.searchParams.get('q') ?? '';
	let search = $state(initialSearch);
	let query = $state(initialQuery);
	let mode = $state<SearchMode>(initialQuery.trim() && !initialSearch.trim() ? 'query' : 'search');
	let queryHelpOpen = $state(false);
	let queryMatch = $state<Set<string> | null>(null);
	let queryError = $state<string | null>(null);
	let querySeq = 0;
	let queryTimer: ReturnType<typeof setTimeout> | null = null;
	$effect(() => {
		const words = search.trim().replaceAll('"', '');
		const q = mode === 'query' ? query.trim() : words ? `"${words}"` : '';
		void all; // 다시 불러오면(순서 바꾸기 등) 맞는 것도 다시 묻는다.
		if (queryTimer) clearTimeout(queryTimer);
		const seq = ++querySeq;
		if (!q) {
			queryMatch = null;
			queryError = null;
			return;
		}
		queryTimer = setTimeout(() => {
			campaignsApi
				.matchQuery(q)
				.then((rows) => {
					if (seq !== querySeq) return;
					queryMatch = new Set(rows.map((r) => r.campaign_slug));
					queryError = null;
				})
				.catch((e) => {
					if (seq !== querySeq) return;
					queryMatch = null;
					queryError = e instanceof Error ? e.message : String(e);
				});
		}, 250);
	});
	// 주소에 싣는다 — 새로고침 · 공유한 링크가 같은 목록을 연다.
	$effect(() => {
		const want = { search: search.trim(), q: query.trim() };
		const url = new URL(window.location.href);
		let changed = false;
		for (const [k, v] of Object.entries(want)) {
			if ((url.searchParams.get(k) ?? '') === v) continue;
			if (v) url.searchParams.set(k, v);
			else url.searchParams.delete(k);
			changed = true;
		}
		if (changed)
			goto(`${url.pathname}${url.search}`, { replaceState: true, keepFocus: true, noScroll: true });
	});

	let filtered = $derived.by(() => {
		const byStatus = statusFilter === 'all' ? all : all.filter((c) => c.status === statusFilter);
		const base = queryMatch ? byStatus.filter((c) => queryMatch!.has(c.campaign_slug)) : byStatus;
		return sortCampaigns(base, sort);
	});

	async function moveOrder(c: CampaignSummary, delta: number) {
		const next = (c.display_order ?? 0) + delta;
		try {
			await campaignsApi.update(c.campaign_slug, { display_order: next });
			all = await campaignsApi.listSummaries();
		} catch (e) {
			showToast(
				e instanceof Error ? e.message : t('campaignList.orderChangeFailed', $locale),
				'error'
			);
		}
	}

	/**
	 * 목록 행에 보여줄 진행도 — 체크리스트와 연결 퀘스트를 **따로**(admin 결정).
	 * 카드(CampaignCard)와 같은 구성이라 두 화면의 읽는 법이 같아진다.
	 * 항목이 없는 쪽은 줄 자체를 만들지 않는다(둘 다 없으면 빈 배열).
	 */
	function progressRows(
		c: CampaignSummary
	): { key: 'checklist' | 'quest'; done: number; total: number; ratio: number; full: boolean }[] {
		const rows: {
			key: 'checklist' | 'quest';
			done: number;
			total: number;
			ratio: number;
			full: boolean;
		}[] = [];
		const cTotal = c.checklist_total ?? 0;
		if (cTotal > 0) {
			const done = c.checklist_checked ?? 0;
			rows.push({
				key: 'checklist',
				done,
				total: cTotal,
				ratio: done / cTotal,
				full: done === cTotal
			});
		}
		const qTotal = c.quest_total ?? 0;
		if (qTotal > 0) {
			const done = c.quest_done ?? 0;
			rows.push({ key: 'quest', done, total: qTotal, ratio: done / qTotal, full: done === qTotal });
		}
		return rows;
	}

	/** 기간 표시 — 목록은 summary 를 쓰므로 두 타입에 공통인 필드만 받는다. */
	function fmtPeriod(c: { started_at: string | null; ended_at: string | null }): string {
		const a = c.started_at?.trim() || '';
		const b = c.ended_at?.trim() || '';
		if (!a && !b) return t('campaignList.periodUndefined', $locale);
		if (a && !b) return `${a} ~`;
		if (!a && b) return `~ ${b}`;
		return `${a} ~ ${b}`;
	}
	// DEV-395: 새 캠페인은 모달 — 새 퀘스트와 같은 방식.
	// `?new=1` 로 들어오면 열어 준다(옛 `/campaigns/new` 링크가 여기로 온다).
	let showNewCampaign = $state(false);
	onMount(() => {
		if ($page.url.searchParams.get('new') === '1') showNewCampaign = true;
	});
</script>

<div class="page">
	<div class="header">
		<h1>{t('campaignList.title', $locale)}</h1>
		<!-- DEV-395: 예전엔 `/campaigns/new` 페이지로 갔다 — 새 퀘스트와 같은 모달로 통일. -->
		<button class="btn btn-primary" onclick={() => (showNewCampaign = true)}
			>{t('campaignList.new', $locale)}</button
		>
	</div>

	<div class="controls">
		<label>
			{t('campaignList.statusLabel', $locale)}
			<select class="input" bind:value={statusFilter}>
				<option value="all">{t('campaignList.statusAll', $locale)}</option>
				<option value="active">{t('campaignList.statusActive', $locale)}</option>
				<option value="done">{t('campaignList.statusDone', $locale)}</option>
			</select>
		</label>
		<label>
			{t('campaignList.sortLabel', $locale)}
			<select class="input" bind:value={sort}>
				<option value="recent">{t('campaignList.sortRecent', $locale)}</option>
				<option value="remaining">{t('campaignList.sortRemaining', $locale)}</option>
				<option value="manual">{t('campaignList.sortManual', $locale)}</option>
			</select>
		</label>
		<SearchField
			bind:search
			bind:query
			bind:mode
			bind:helpOpen={queryHelpOpen}
			queryError={mode === 'query' ? queryError : null}
			placeholder={t('campaignList.searchPlaceholder', $locale)}
			queryPlaceholderKey="campaignList.queryPlaceholder"
			testids={{ search: 'campaign-search', query: 'campaign-query' }}
			grow
		/>
		<QueryNotes
			error={mode === 'query' ? queryError : null}
			open={mode === 'query' && queryHelpOpen}
			keys={CAMPAIGN_QUERY_KEYS}
			rulesKey="campaignList.queryHelpRules"
			testid="campaign-query"
		/>
	</div>

	{#if loading}
		<div class="state">Loading…</div>
	{:else if error}
		<div class="state error">{error}</div>
	{:else if filtered.length === 0}
		<div class="state">{t('campaignList.empty', $locale)}</div>
	{:else}
		<ul class="list">
			{#each filtered as c (c.id)}
				<li class="row">
					<!-- admin 요청: 한 줄에 다 넣지 않고 2단으로 —
					     1행 [슬러그 · 상태 · 기간] / 2행 [제목].
					     제목이 가장 길고 중요한데 예전 배치에선 가운데에서 눌려
					     잘리기 쉬웠다(BUG-198 의 세로 출력도 같은 자리). -->
					<a class="main" href={`/campaigns/${encodeURIComponent(c.campaign_slug)}`}>
						<span class="meta-line">
							<span class="slug">{c.campaign_slug}</span>
							<span class="pill status-{c.status}">{c.status}</span>
							<!-- DEV-079: 종료 기한 지났는데 status != done 이면 period 빨강. -->
							<span class="period" class:overdue={isDateOverdue(c.ended_at, c.status)}
								>{fmtPeriod(c)}</span
							>
						</span>
						<span class="title">{c.title}</span>
						<!-- admin 요청: 목록에서도 진행도. 체크리스트 + 연결 퀘스트를
						     합산한 하나의 바 + `완료/전체` 숫자. 둘 다 없으면 생략. -->
						{#each progressRows(c) as p (p.key)}
							<span class="prog">
								<span class="prog-label"
									>{p.key === 'checklist'
										? t('campaignList.progressChecklist', $locale)
										: t('campaignList.progressQuests', $locale)}</span
								>
								<span
									class="prog-bar"
									role="img"
									aria-label={`${Math.round(p.ratio * 100)}% (${p.done}/${p.total})`}
								>
									<span
										class="prog-fill"
										class:done={p.full}
										style:width={`${Math.round(p.ratio * 100)}%`}
									></span>
								</span>
								<span class="prog-text">{p.done}/{p.total}</span>
							</span>
						{/each}
					</a>
					{#if sort === 'manual'}
						<div class="reorder">
							<button title="up" onclick={() => moveOrder(c, -1)}>↑</button>
							<button title="down" onclick={() => moveOrder(c, 1)}>↓</button>
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>

{#if showNewCampaign}
	<NewCampaignModal
		onclose={() => (showNewCampaign = false)}
		oncreated={(c) => goto(`/campaigns/${encodeURIComponent(c.campaign_slug)}`)}
	/>
{/if}

<style>
	.page {
		padding: var(--sp-16) var(--sp-24);
		max-width: var(--content-max-width, 1100px);
		margin: 0 auto;
	}
	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: var(--sp-16);
	}
	.header h1 {
		font-size: var(--fs-xl);
		color: var(--text);
		margin: 0;
	}

	.controls {
		display: flex;
		/* DEV-452: 쿼리 칸은 다음 줄에 한 줄을 다 쓴다. */
		flex-wrap: wrap;
		gap: var(--sp-8) var(--sp-16);
		margin-bottom: var(--sp-16);
	}
	.controls label {
		font-size: var(--fs-sm);
		color: var(--text-muted);
		display: flex;
		align-items: center;
		gap: var(--sp-6);
	}

	.state {
		color: var(--text-muted);
		padding: var(--sp-24) 0;
		font-size: var(--fs-md);
	}
	.state.error {
		color: var(--danger);
	}

	.list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}
	.row {
		display: flex;
		align-items: stretch;
		gap: var(--sp-4);
	}
	.main {
		flex: 1;
		/* admin 요청: 2단 배치 — 위 줄에 메타(슬러그·상태·기간), 아래 줄에 제목. */
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
		align-items: stretch;
		padding: var(--sp-8) var(--sp-12);
		background: var(--bg-elevated);
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-md);
		text-decoration: none;
		color: inherit;
	}
	.main:hover {
		border-color: var(--text-faint);
		background: var(--bg-subtle);
	}

	/* 진행도 — 제목 아래 얇은 바 + 완료/전체. 카드의 progress-bar 와 같은 결. */
	.prog {
		display: flex;
		align-items: center;
		gap: var(--sp-8);
		margin-top: var(--sp-2);
	}
	.prog-label {
		flex: none;
		min-width: 3.2rem;
		font-size: var(--fs-xs);
		color: var(--text-faint);
	}
	.prog-bar {
		/* BUG-254 계열(admin 보고): 바 두께가 px 이라 UI 배율을 안 따라갔다.
		   곡률(`--r-xs`)은 rem 이라 배율에서 함께 커지는데 두께만 그대로여서,
		   배율을 올리면 바가 과하게 둥근 실선처럼 보였다. 16px 기준 환산이라
		   기본 배율에서 두께는 그대로다. */
		flex: 1;
		min-width: 3rem;
		height: 0.25rem;
		border-radius: var(--r-xs);
		background: var(--bg-subtle);
		overflow: hidden;
	}
	.prog-fill {
		display: block;
		height: 100%;
		border-radius: var(--r-xs);
		background: var(--accent);
		transition: width var(--dur-mid);
	}
	.prog-fill.done {
		background: var(--success);
	}
	.prog-text {
		flex: none;
		font-size: var(--fs-xs);
		color: var(--text-muted);
		font-variant-numeric: tabular-nums;
	}

	/* 메타 줄 — 좁으면 기간이 다음 줄로 넘어간다(제목 줄은 건드리지 않는다). */
	.meta-line {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--sp-8);
	}
	.slug {
		font-size: var(--fs-xs);
		color: var(--text-muted);
		font-family: var(--font-mono);
		flex: none;
	}
	.title {
		color: var(--text);
		font-size: var(--fs-md);
		/* BUG-198: 예전엔 한 줄 안에서 pill·기간에 밀려 한 글자 폭까지 눌렸다.
		   이제 제목이 자기 줄을 통째로 쓰므로 그럴 일이 없다. 아주 긴 제목만
		   자연스럽게 감싸도록. */
		overflow-wrap: anywhere;
	}
	/* DEV-364: 모양은 global.css 의 `.pill` 이 정본 — 색과 대소문자만 정한다. */
	.pill {
		text-transform: uppercase;
	}
	.status-active {
		--c: var(--success);
	}
	.status-done {
		--c: var(--text-muted);
	}
	.period {
		font-size: var(--fs-xs);
		color: var(--text-muted);
	}
	/* DEV-079: 기한 지남 + status != done — 빨강. */
	.period.overdue {
		color: var(--danger);
		font-weight: var(--fw-semibold);
	}

	.reorder {
		display: flex;
		flex-direction: column;
		gap: var(--sp-2);
	}
	.reorder button {
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-sm);
		width: 1.8rem;
		font-size: var(--fs-xs);
		cursor: pointer;
	}
	.reorder button:hover {
		background: var(--bg-subtle);
	}
</style>
