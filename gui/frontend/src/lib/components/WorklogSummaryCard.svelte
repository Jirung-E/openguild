<!--
  DEV-167: HOME 작업 기록 요약 카드 — GitHub 잔디 스타일 히트맵(최근 12주) +
  오늘 집계 한 줄. worklog 는 자동 생성이라 자주 찾는 페이지가 아님(admin
  결정) — Nav 미노출, 이 카드가 /worklog 상세의 유일한 진입점.

  - 카드 헤더/집계 클릭 → /worklog (오늘).
  - 히트맵 날짜 칸 클릭 → /worklog?date=YYYY-MM-DD (그 날짜 선택된 채).
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { worklogApi, type DailyCount, type WorklogReport } from '$lib/api/worklog';

	const WEEKS = 12;

	let loading = $state(true);
	let counts = $state<Map<string, number>>(new Map());
	let total = $state(0);
	let today = $state<WorklogReport | null>(null);

	function fmt(d: Date): string {
		const y = d.getFullYear();
		const m = String(d.getMonth() + 1).padStart(2, '0');
		const day = String(d.getDate()).padStart(2, '0');
		return `${y}-${m}-${day}`;
	}

	// 히트맵 격자: 열=주(과거→현재), 행=요일(일=0). GitHub 관례.
	// 마지막 열이 이번 주 — 오늘 이후 칸은 null (렌더 안 함).
	interface Cell {
		date: string;
		count: number;
	}
	let grid = $state<(Cell | null)[][]>([]);

	onMount(async () => {
		const now = new Date();
		const end = fmt(now);
		// 시작 = (WEEKS-1)주 전 일요일.
		const start = new Date(now);
		start.setDate(start.getDate() - start.getDay() - (WEEKS - 1) * 7);
		const from = fmt(start);
		try {
			const [summary, report] = await Promise.all([
				worklogApi.summary(from, end),
				worklogApi.activities(end, end)
			]);
			const map = new Map<string, number>();
			let sum = 0;
			for (const s of summary as DailyCount[]) {
				map.set(s.date, s.count);
				sum += s.count;
			}
			counts = map;
			total = sum;
			today = report;

			const g: (Cell | null)[][] = [];
			for (let w = 0; w < WEEKS; w++) {
				const col: (Cell | null)[] = [];
				for (let d = 0; d < 7; d++) {
					const cur = new Date(start);
					cur.setDate(cur.getDate() + w * 7 + d);
					if (cur > now) {
						col.push(null);
					} else {
						const date = fmt(cur);
						col.push({ date, count: map.get(date) ?? 0 });
					}
				}
				g.push(col);
			}
			grid = g;
		} catch {
			/* 보조 위젯 — 실패 시 카드 자체를 숨김 */
		} finally {
			loading = false;
		}
	});

	// 활동량 → 농도 5단계 (0 = 없음).
	function level(count: number): number {
		if (count <= 0) return 0;
		if (count <= 3) return 1;
		if (count <= 8) return 2;
		if (count <= 15) return 3;
		return 4;
	}

	function openDetail(date?: string) {
		goto(date ? `/worklog?date=${date}` : '/worklog');
	}

	const lastActivity = $derived(
		today && today.activities.length > 0
			? today.activities[today.activities.length - 1]
			: null
	);
</script>

{#if !loading && grid.length > 0}
	<section class="block">
		<div class="card">
			<button class="head" onclick={() => openDetail()} title="작업 기록 상세">
				<h2>🕘 작업 기록</h2>
				<span class="range">최근 {WEEKS}주 · 총 {total} 활동 ›</span>
			</button>
			<div class="heat" role="img" aria-label="최근 {WEEKS}주 활동 히트맵">
				{#each grid as col, w (w)}
					<div class="week">
						{#each col as cell, d (d)}
							{#if cell}
								<button
									class="cell l{level(cell.count)}"
									title="{cell.date} — 활동 {cell.count}건"
									aria-label="{cell.date} 활동 {cell.count}건"
									onclick={() => openDetail(cell.date)}
								></button>
							{:else}
								<span class="cell future"></span>
							{/if}
						{/each}
					</div>
				{/each}
			</div>
			{#if today}
				<button class="today" onclick={() => openDetail()}>
					<span class="lbl">오늘</span>
					<span><b>{today.counts.status_changes}</b> 상태변경</span>
					<span><b>{today.counts.comments}</b> 댓글</span>
					<span><b>{today.counts.created}</b> 생성</span>
					{#if lastActivity}
						<span class="last">
							마지막 활동 {lastActivity.ts.slice(11, 16)} · {lastActivity.slug}
						</span>
					{/if}
				</button>
			{/if}
		</div>
	</section>
{/if}

<style>
	.block {
		margin-bottom: 1.5rem;
	}
	.card {
		border: 1px solid var(--border);
		border-radius: 10px;
		background: var(--bg-elevated);
		padding: 0.9rem 1rem;
	}
	.head {
		display: flex;
		align-items: center;
		width: 100%;
		gap: 0.6rem;
		background: transparent;
		border: none;
		padding: 0 0 0.6rem;
		cursor: pointer;
		color: var(--text);
	}
	.head h2 {
		font-size: 0.95rem;
		font-weight: 600;
		margin: 0;
	}
	.range {
		margin-left: auto;
		font-size: 0.75rem;
		color: var(--text-muted);
	}
	.head:hover .range {
		color: var(--text);
	}

	.heat {
		display: flex;
		gap: 3px;
		margin-bottom: 0.7rem;
	}
	.week {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.cell {
		width: 11px;
		height: 11px;
		border-radius: 2px;
		border: 1px solid var(--border);
		padding: 0;
		background: var(--bg);
		cursor: pointer;
	}
	.cell.future {
		visibility: hidden;
		cursor: default;
	}
	/* 농도 — success(초록) 계열 4단계, 테마 토큰 기반 (no-hex). */
	.cell.l1 {
		background: color-mix(in srgb, var(--success) 25%, var(--bg));
	}
	.cell.l2 {
		background: color-mix(in srgb, var(--success) 45%, var(--bg));
	}
	.cell.l3 {
		background: color-mix(in srgb, var(--success) 70%, var(--bg));
	}
	.cell.l4 {
		background: var(--success);
	}
	.cell:hover:not(.future) {
		outline: 1px solid var(--accent);
	}

	.today {
		display: flex;
		align-items: center;
		gap: 0.9rem;
		width: 100%;
		padding: 0.55rem 0 0;
		border: none;
		border-top: 1px solid var(--border);
		background: transparent;
		font-size: 0.75rem;
		color: var(--text-muted);
		cursor: pointer;
		text-align: left;
	}
	.today b {
		color: var(--text);
		font-weight: 600;
	}
	.today .lbl {
		color: var(--text-muted);
	}
	.today .last {
		margin-left: auto;
	}
</style>
