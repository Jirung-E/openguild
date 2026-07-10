<!--
  BUG-025: 캠페인 단일 카드 — CampaignCarousel / CampaignConveyor 가 공유.

  mode:
   - 'active' (진행 중): 슬러그 + 제목 + 기간 + 종료 임박 카운트다운 + 진행률.
                         완료 시 progress bar 초록 + ✓ 표시.
   - 'upcoming' (곧 시작): 슬러그/진행률 제거. 제목 + "n일 남음 (YYYY-MM-DD)".

  width 는 부모가 결정 (carousel = 100%, conveyor = 200px 등).
-->
<script lang="ts">
	// BUG-033: goto 제거 — anchor href 로 native navigate.
	import type { CampaignSummary } from '$lib/types';
	import { formatRemaining } from '$lib/utils/datetime';
	import { isCampaignDone } from '$lib/utils/campaign-progress';

	let {
		summary,
		mode,
		now
	}: {
		summary: CampaignSummary;
		// DEV-080: 'overdue' 모드 추가 — 마감 지난 / 미완료 캠페인.
		// upcoming 과 시각/크기 동일, 라벨만 "n일 지남" + 빨강.
		mode: 'active' | 'upcoming' | 'overdue';
		now: number;
	} = $props();

	// DEV-093 fix2: 완료 판정 로직은 `lib/utils/campaign-progress` 로 추출 + 회귀
	// 테스트 (vitest). 본 컴포넌트 / Home overdue 필터 공유.
	let completed = $derived(isCampaignDone(summary));

	function fmtPeriod(): string {
		const a = summary.started_at?.trim() || '';
		const b = summary.ended_at?.trim() || '';
		if (!a && !b) return '기간 미정';
		if (a && !b) return `${a} ~`;
		if (!a && b) return `~ ${b}`;
		return `${a} ~ ${b}`;
	}

	function progressText(): string {
		if (summary.checklist_total === 0) return '체크리스트 없음';
		const pct = Math.round(summary.progress * 100);
		return `${summary.checklist_checked}/${summary.checklist_total} (${pct}%)`;
	}

	// DEV-093: 링크된 quest 의 진행률 — 두 번째 progress bar.
	let questTotal = $derived(summary.quest_total ?? 0);
	let questDone = $derived(summary.quest_done ?? 0);
	let questPct = $derived(summary.quest_progress ?? 0);
	let questFull = $derived(questTotal > 0 && questDone === questTotal);
	function questProgressText(): string {
		if (questTotal === 0) return '링크된 퀘스트 없음';
		const pct = Math.round(questPct * 100);
		return `${questDone}/${questTotal} (${pct}%)`;
	}

	// DEV-233(기획 변경, admin 2026-07-10): 기본은 기존 단일 채움 그대로 두고,
	// hover 시에만 상태별 stacked 세그먼트 + 카운트 팝업으로 전환. 마우스가
	// 바를 벗어나면 원래대로. 완료된 캠페인(questFull)은 hover 해도 기존 초록
	// 단색 그대로(admin 확인 "완료 시 단색 유지 고려" — 의미 없는 정보라 skip).
	let questBarEl = $state<HTMLDivElement | null>(null);
	let questBarHover = $state(false);
	let tooltipTop = $state(0);
	let tooltipLeft = $state(0);
	function onQuestBarEnter() {
		if (questFull || !questBarEl) return;
		const r = questBarEl.getBoundingClientRect();
		tooltipTop = r.top;
		tooltipLeft = r.left;
		questBarHover = true;
	}
	function onQuestBarLeave() {
		questBarHover = false;
	}
	const showStack = $derived(
		questBarHover && !questFull && (summary.quest_status_counts?.length ?? 0) > 0
	);

	function activeRemainingLabel(): string {
		if (!summary.ended_at?.trim()) return '';
		return formatRemaining(summary.ended_at, now, 'until-end');
	}

	// BUG-031 → DEV-079: 진행중 카드의 남은 기간 색.
	// - ≤ 7일 남음: urgent (빨강)
	// - 기한 지남: urgent (빨강, 동일)
	// - 그 외: 회색
	// 이전엔 overdue (remaining < 0) 도 회색으로 처리되어 한참 지난 카드가
	// 일반처럼 보이던 문제 — overdue 도 시각 경고로.
	const ACTIVE_URGENT_DAYS = 7;
	let activeRemainingIsUrgent = $derived.by(() => {
		const e = summary.ended_at?.trim();
		if (!e) return false;
		const endMs = new Date(`${e}T23:59:59`).getTime();
		if (Number.isNaN(endMs)) return false;
		const remaining = endMs - now;
		// 지남 (remaining < 0) 또는 7일 이내 남음.
		return remaining <= ACTIVE_URGENT_DAYS * 24 * 60 * 60 * 1000;
	});

	function upcomingRemainingLabel(): string {
		if (!summary.started_at?.trim()) return '';
		return formatRemaining(summary.started_at, now, 'until-start');
	}

	// DEV-080: 'overdue' 모드용 — "n일 지남". ended_at 기준.
	function overdueElapsedLabel(): string {
		const e = summary.ended_at?.trim();
		if (!e) return '';
		const endMs = new Date(`${e}T23:59:59`).getTime();
		if (Number.isNaN(endMs)) return '';
		const elapsedMs = now - endMs;
		if (elapsedMs <= 0) return '';
		const days = Math.floor(elapsedMs / (24 * 60 * 60 * 1000));
		if (days < 1) {
			const hr = Math.floor(elapsedMs / (60 * 60 * 1000));
			return hr < 1 ? '방금 지남' : `${hr}시간 지남`;
		}
		return `${days}일 지남`;
	}

	// BUG-033: `<button onclick={goto}>` 대신 native `<a href>` 사용. button +
	// JS handler 는 conveyor 의 pointer 이벤트와 미묘하게 충돌해 click 이 발화
	// 안 되는 경우가 있음. anchor 의 native href 는 conveyor 가 e.preventDefault()
	// 를 호출하지 않는 한 무조건 navigate.
	let href = $derived(`/campaigns/${encodeURIComponent(summary.campaign_slug)}`);

	// DEV-087: 배너 이미지 — 좌측 (글자 영역) 불투명 → 우측 투명 fade.
	import { campaignBannerUrl } from '$lib/utils/banner';
	let bannerUrl = $state<string | null>(null);
	$effect(() => {
		const slug = summary.campaign_slug;
		const img = summary.image_path;
		campaignBannerUrl(slug, img)
			.then((u) => (bannerUrl = u))
			.catch(() => (bannerUrl = null));
	});
	// DEV-087 fix: 배너를 CSS background-image 대신 실제 <img> 로 렌더.
	// WebView2 는 custom 스킴(asset://...) URL 을 CSS url() 에선 막고 <img src>
	// 에선 허용하는 사례가 있어, 첨부(DEV-069)와 동일하게 <img> 경로로 통일해야
	// 홈 carousel 에서도 표시된다. fade 는 별도 오버레이 div 로 분리.
	let showBanner = $derived(mode === 'active' && !!bannerUrl);
</script>

<a class="card {mode}" class:completed {href} draggable="false" data-sveltekit-preload-data="hover">
	{#if showBanner}
		<!-- DEV-087 fix: 배너 이미지 레이어 + fade 오버레이 (좌측 불투명 → 우측 투명). -->
		<img class="banner-img" src={bannerUrl} alt="" aria-hidden="true" draggable="false" />
		<div class="banner-fade" aria-hidden="true"></div>
	{/if}
	{#if mode === 'active'}
		<div class="head">
			<span class="slug">{summary.campaign_slug}</span>
			{#if completed}<span class="done-mark" title="완료">✓ 완료</span>{/if}
		</div>
		<div class="title">{summary.title}</div>
		<div class="meta">
			<div class="period">
				{fmtPeriod()}
				{#if activeRemainingLabel()}
					<!-- BUG-031: 7일 이내만 빨간 강조. 그 외는 평이한 회색. -->
					<span class="remaining" class:urgent={activeRemainingIsUrgent}
						>({activeRemainingLabel()})</span
					>
				{/if}
			</div>
			<!-- 체크리스트 progress -->
			<div class="progress-row" title="체크리스트 진행률">
				<span class="progress-label">체크</span>
				<div class="progress-bar">
					<div
						class="progress-fill"
						class:done={completed}
						style:width={`${Math.round(summary.progress * 100)}%`}
					></div>
				</div>
				<div class="progress-text" class:done-text={completed}>
					{progressText()}
				</div>
			</div>
			<!-- DEV-093: 링크 퀘스트 progress (있을 때만). DEV-233: hover 시 상태별 stacked. -->
			{#if questTotal > 0}
				<div class="progress-row" title="링크된 퀘스트의 완료 비율 (status.counts_as_done)">
					<span class="progress-label">퀘스트</span>
					<div
						class="progress-bar"
						bind:this={questBarEl}
						role="img"
						aria-label={questProgressText()}
						onmouseenter={onQuestBarEnter}
						onmouseleave={onQuestBarLeave}
					>
						{#if showStack}
							{#each summary.quest_status_counts ?? [] as sc (sc.status_slug)}
								<div
									class="progress-seg"
									style:width={`${(sc.count / questTotal) * 100}%`}
									style:background={sc.status_color}
								></div>
							{/each}
						{:else}
							<div
								class="progress-fill"
								class:done={questFull}
								style:width={`${Math.round(questPct * 100)}%`}
							></div>
						{/if}
					</div>
					<div class="progress-text" class:done-text={questFull}>
						{questProgressText()}
					</div>
				</div>
			{/if}
		</div>
	{:else if mode === 'upcoming'}
		<div class="title small">{summary.title}</div>
		<div class="meta">
			<div class="period">
				<span class="remaining accent">{upcomingRemainingLabel()}</span>
				{#if summary.started_at?.trim()}
					<span class="start-date">({summary.started_at})</span>
				{/if}
			</div>
		</div>
	{:else}
		<!-- DEV-080: overdue 모드 — upcoming 과 동일 size, 라벨만 "n일 지남" 빨강. -->
		<div class="title small">{summary.title}</div>
		<div class="meta">
			<div class="period">
				<span class="remaining urgent">{overdueElapsedLabel()}</span>
				{#if summary.ended_at?.trim()}
					<span class="start-date">({summary.ended_at})</span>
				{/if}
			</div>
		</div>
	{/if}
</a>

<!-- DEV-233: 카드가 overflow:hidden(배너 클립용) 이라 absolute 팝업은 잘림 —
     position:fixed + 좌표 계산(BUG-125 emoji 팝업과 동일 접근)으로 escape. -->
{#if showStack}
	<div class="quest-status-tooltip" style:top={`${tooltipTop}px`} style:left={`${tooltipLeft}px`}>
		{#each summary.quest_status_counts ?? [] as sc (sc.status_slug)}
			<div class="tooltip-row">
				<span class="tooltip-dot" style:background={sc.status_color}></span>
				<span class="tooltip-name">{sc.status_name_en}</span>
				<span class="tooltip-count"
					>{sc.count}개 ({Math.round((sc.count / questTotal) * 100)}%)</span
				>
			</div>
		{/each}
	</div>
{/if}

<style>
	/* BUG-033: `<a>` 로 변경. button 의 default 스타일 / focus outline 제거 +
	   anchor 의 underline 제거. 시각은 종전과 동일. */
	.card {
		width: 100%;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 0.85rem 1rem;
		text-align: left;
		cursor: pointer;
		transition:
			border-color 0.15s,
			background 0.15s;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		color: inherit;
		font: inherit;
		text-decoration: none;
		box-sizing: border-box;
		/* DEV-087 fix: 배너 img/fade 레이어를 둥근 모서리로 클립. */
		position: relative;
		overflow: hidden;
	}
	/* DEV-087 fix: 배너 이미지 — 카드 우측을 채우고, fade 오버레이가 좌측 글자
	   영역을 카드 배경색으로 덮어 가독성 확보 (기존 CSS background 와 동일 효과). */
	.banner-img {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: cover;
		object-position: right center;
		z-index: 0;
		pointer-events: none;
		user-select: none;
	}
	.banner-fade {
		position: absolute;
		inset: 0;
		z-index: 1;
		pointer-events: none;
		background: linear-gradient(
			90deg,
			var(--bg-elevated) 0%,
			var(--bg-elevated) 35%,
			color-mix(in srgb, var(--bg-elevated) 35%, transparent) 70%,
			transparent 100%
		);
	}
	/* 배너 레이어 위에 본문 컨텐츠. */
	.card > *:not(.banner-img):not(.banner-fade) {
		position: relative;
		z-index: 2;
	}
	.card:hover {
		background: var(--bg-subtle);
		border-color: var(--text-faint);
	}
	/* BUG-027: active 카드 세로 길이 늘림 (사용자 피드백 — 너무 짧음). */
	.card.active {
		min-height: 180px;
		padding: 1.1rem 1.4rem;
		gap: 0.85rem;
	}
	.card.upcoming {
		padding: 0.65rem 0.8rem;
		gap: 0.35rem;
	}
	/* DEV-080 → DEV-081: overdue 카드 — upcoming 과 동일 패딩 + completed 와
	   동일 패턴의 빨강 그라데이션 + border (완료와 시각 대칭, 색만 빨강).
	   DEV-074 fix8: tint 강도 + base 색을 토큰 (--card-hl-strength / --card-hl-base)
	   으로 추상화 → global.css 에서 theme 별로 분리 정의. */
	.card.overdue {
		padding: 0.65rem 0.8rem;
		gap: 0.35rem;
		border-color: var(--danger);
		background: linear-gradient(
			180deg,
			color-mix(in srgb, var(--danger) var(--card-hl-strength), var(--card-hl-base)) 0%,
			var(--card-hl-base) var(--card-hl-fade)
		);
	}
	/* BUG-025: 100% 달성 카드 — 초록 border 강조. */
	.card.completed {
		border-color: var(--success-strong);
		background: linear-gradient(
			180deg,
			color-mix(in srgb, var(--success) var(--card-hl-strength), var(--card-hl-base)) 0%,
			var(--card-hl-base) var(--card-hl-fade)
		);
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.slug {
		font-size: 0.7rem;
		color: var(--text-muted);
		letter-spacing: 0.04em;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
	}
	.done-mark {
		font-size: 0.7rem;
		font-weight: 700;
		color: var(--success);
		letter-spacing: 0.02em;
	}
	.title {
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--text);
		line-height: 1.3;
	}
	.title.small {
		font-size: 0.85rem;
	}
	/* BUG-027: active 카드의 title 도 더 크게. */
	.card.active .title {
		font-size: 1.1rem;
		line-height: 1.4;
	}

	.meta {
		margin-top: auto;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.period {
		font-size: 0.75rem;
		color: var(--text-muted);
		display: flex;
		gap: 0.35rem;
		align-items: center;
		flex-wrap: wrap;
	}
	/* BUG-031: 기본은 회색 (한참 남은 캠페인에도 빨강 X). 7일 이내만 urgent
	   modifier 로 빨강. upcoming 의 accent 는 종전대로 파랑. */
	.remaining {
		color: var(--text-muted);
		font-weight: 500;
	}
	.remaining.urgent {
		color: var(--danger);
		font-weight: 600;
	}
	.remaining.accent {
		color: var(--accent);
		font-weight: 600;
	}
	.start-date {
		color: var(--text-faint);
	}

	.progress-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	/* DEV-093: progress 종류 라벨 (체크 / 퀘스트) — 짧은 모노스페이스. */
	.progress-label {
		font-size: 0.65rem;
		color: var(--text-faint);
		min-width: 2.4rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		letter-spacing: 0.02em;
	}
	.progress-bar {
		flex: 1;
		height: 4px;
		background: var(--bg-subtle);
		border-radius: 2px;
		overflow: hidden;
		display: flex;
	}
	.progress-fill {
		height: 100%;
		background: var(--accent);
		transition:
			width 0.2s,
			background 0.2s;
	}
	/* BUG-025: 100% 시 초록 */
	.progress-fill.done {
		background: var(--success-strong);
	}
	/* DEV-233: hover 시 상태별 stacked 세그먼트 — progress-fill 대신 렌더. */
	.progress-seg {
		height: 100%;
	}
	/* DEV-233: 상태별 카운트 팝업 — position:fixed 로 카드의 overflow:hidden 회피. */
	.quest-status-tooltip {
		position: fixed;
		transform: translateY(-100%) translateY(-6px);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 0.4rem 0.6rem;
		font-size: 0.72rem;
		z-index: 50;
		box-shadow: 0 4px 12px color-mix(in srgb, black 25%, transparent);
		pointer-events: none;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		white-space: nowrap;
	}
	.tooltip-row {
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}
	.tooltip-dot {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 50%;
		flex: none;
	}
	.tooltip-name {
		color: var(--text);
		font-weight: 600;
	}
	.tooltip-count {
		color: var(--text-muted);
	}
	.progress-text {
		font-size: 0.7rem;
		color: var(--text-muted);
		white-space: nowrap;
	}
	.progress-text.done-text {
		color: var(--success);
		font-weight: 600;
	}
</style>
