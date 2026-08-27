<!--
  DEV-011: Campaign 목록 페이지 (/campaigns).
   - 정렬: 최근 추가 순 (기본) / 남은 날짜 순 / 수동 (display_order)
   - 어드민이 수동 모드일 때 ↑↓ 버튼으로 순서 변경 (display_order 갱신)
   - 각 카드 클릭 → /campaigns/<slug> detail
   - 우상단 "+ 새 캠페인" 버튼
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { campaignsApi } from '$lib/api/campaigns';
	// DEV-205 모듈2: 캠페인 목록 문자열 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-259: alert() 잔재 제거 — 앱 공용 toast 로 통일.
	import { showToast } from '$lib/stores/toast';
	import type { Campaign, CampaignSummary } from '$lib/types';
	import { isDateOverdue } from '$lib/utils/datetime';

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

	let filtered = $derived.by(() => {
		const base = statusFilter === 'all' ? all : all.filter((c) => c.status === statusFilter);
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
</script>

<div class="page">
	<div class="header">
		<h1>{t('campaignList.title', $locale)}</h1>
		<button class="btn-primary" onclick={() => goto('/campaigns/new')}
			>{t('campaignList.new', $locale)}</button
		>
	</div>

	<div class="controls">
		<label>
			{t('campaignList.statusLabel', $locale)}
			<select bind:value={statusFilter}>
				<option value="all">{t('campaignList.statusAll', $locale)}</option>
				<option value="active">{t('campaignList.statusActive', $locale)}</option>
				<option value="done">{t('campaignList.statusDone', $locale)}</option>
			</select>
		</label>
		<label>
			{t('campaignList.sortLabel', $locale)}
			<select bind:value={sort}>
				<option value="recent">{t('campaignList.sortRecent', $locale)}</option>
				<option value="remaining">{t('campaignList.sortRemaining', $locale)}</option>
				<option value="manual">{t('campaignList.sortManual', $locale)}</option>
			</select>
		</label>
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

<style>
	.page {
		padding: 1.25rem 1.5rem;
		max-width: var(--content-max-width, 1100px);
		margin: 0 auto;
	}
	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 1rem;
	}
	.header h1 {
		font-size: 1.25rem;
		color: var(--text);
		margin: 0;
	}
	.btn-primary {
		padding: 0.4rem 0.85rem;
		background: var(--btn-primary-bg);
		border: var(--bw) solid var(--btn-primary-border);
		border-radius: var(--r-md);
		color: var(--btn-primary-text);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-primary:hover {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}

	.controls {
		display: flex;
		gap: 1rem;
		margin-bottom: 1rem;
	}
	.controls label {
		font-size: 0.825rem;
		color: var(--text-muted);
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}
	.controls select {
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-sm);
		padding: 0.25rem 0.5rem;
		font-size: 0.825rem;
	}

	.state {
		color: var(--text-muted);
		padding: 1.5rem 0;
		font-size: 0.875rem;
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
		gap: 4px;
	}
	.row {
		display: flex;
		align-items: stretch;
		gap: 4px;
	}
	.main {
		flex: 1;
		/* admin 요청: 2단 배치 — 위 줄에 메타(슬러그·상태·기간), 아래 줄에 제목. */
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		align-items: stretch;
		padding: 0.6rem 0.85rem;
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
		gap: 0.5rem;
		margin-top: 0.1rem;
	}
	.prog-label {
		flex: none;
		min-width: 3.2rem;
		font-size: 0.72rem;
		color: var(--text-faint);
	}
	.prog-bar {
		flex: 1;
		min-width: 3rem;
		height: 4px;
		border-radius: var(--r-xs);
		background: var(--bg-subtle);
		overflow: hidden;
	}
	.prog-fill {
		display: block;
		height: 100%;
		border-radius: var(--r-xs);
		background: var(--accent);
		transition: width 0.2s;
	}
	.prog-fill.done {
		background: var(--success);
	}
	.prog-text {
		flex: none;
		font-size: 0.72rem;
		color: var(--text-muted);
		font-variant-numeric: tabular-nums;
	}

	/* 메타 줄 — 좁으면 기간이 다음 줄로 넘어간다(제목 줄은 건드리지 않는다). */
	.meta-line {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.5rem;
	}
	.slug {
		font-size: 0.75rem;
		color: var(--text-muted);
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		flex: none;
	}
	.title {
		color: var(--text);
		font-size: 0.9rem;
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
		font-size: 0.75rem;
		color: var(--text-muted);
	}
	/* DEV-079: 기한 지남 + status != done — 빨강. */
	.period.overdue {
		color: var(--danger);
		font-weight: 600;
	}

	.reorder {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.reorder button {
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-sm);
		width: 1.8rem;
		font-size: 0.75rem;
		cursor: pointer;
	}
	.reorder button:hover {
		background: var(--bg-subtle);
	}
</style>
