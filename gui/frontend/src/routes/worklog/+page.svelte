<!--
  DEV-167: 작업 기록 상세 페이지 — /worklog[?date=YYYY-MM-DD].

  - 헤더: 일/주/월 단위 토글 + ◀ 날짜 ▶ / 오늘.
  - 노트: .guild/worklog/{date}.md — 인라인 편집 (CodeMirror + cross-link).
    주/월 뷰에선 기간 내 일별 노트 나열 (읽기 전용 — 편집은 일 뷰에서).
  - 타임라인: quest_history(상태/타입) + quest/campaign 댓글 + 생성.
    항목 클릭 → quest/campaign 상세. 주/월은 날짜 구분선 그룹핑.
  - 하단 집계.
  Nav 미노출 — HOME 요약 카드가 진입점 (admin 결정).
-->
<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { setUnsaved } from '$lib/stores/unsaved';
	import {
		worklogApi,
		type WorklogReport,
		type WorklogNote,
		type ActivityRow
	} from '$lib/api/worklog';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	import { indentExtensions } from '$lib/utils/editor-indent';
	import { editorSettings } from '$lib/stores/editorSettings';
	import { crossLinkAutocomplete } from '$lib/utils/editor-links';

	type Unit = 'day' | 'week' | 'month' | 'range';
	const UNITS: Unit[] = ['day', 'week', 'month', 'range'];
	const UNIT_LABEL: Record<Unit, string> = { day: '일', week: '주', month: '월', range: '구간' };

	let unit = $state<Unit>('day');
	/** 기준 날짜 (일 뷰 = 그 날, 주/월 뷰 = 그 날이 속한 기간). */
	let anchor = $state(fmt(new Date()));
	// 임의 구간(range) 뷰의 시작/끝 — admin 요청.
	let rangeFrom = $state(fmt(new Date()));
	let rangeTo = $state(fmt(new Date()));

	let loading = $state(true);
	let error = $state<string | null>(null);
	let report = $state<WorklogReport | null>(null);
	let notes = $state<WorklogNote[]>([]);

	function fmt(d: Date): string {
		const y = d.getFullYear();
		const m = String(d.getMonth() + 1).padStart(2, '0');
		const day = String(d.getDate()).padStart(2, '0');
		return `${y}-${m}-${day}`;
	}
	function parse(s: string): Date {
		const [y, m, d] = s.split('-').map(Number);
		return new Date(y, m - 1, d);
	}

	/** unit + anchor → [from, to] (날짜 포함). */
	function range(): { from: string; to: string } {
		if (unit === 'range') {
			// 역순 입력이면 스왑.
			return rangeFrom <= rangeTo
				? { from: rangeFrom, to: rangeTo }
				: { from: rangeTo, to: rangeFrom };
		}
		const a = parse(anchor);
		if (unit === 'day') return { from: anchor, to: anchor };
		if (unit === 'week') {
			const start = new Date(a);
			start.setDate(start.getDate() - start.getDay()); // 일요일 시작.
			const end = new Date(start);
			end.setDate(end.getDate() + 6);
			return { from: fmt(start), to: fmt(end) };
		}
		const start = new Date(a.getFullYear(), a.getMonth(), 1);
		const end = new Date(a.getFullYear(), a.getMonth() + 1, 0);
		return { from: fmt(start), to: fmt(end) };
	}

	const rangeLabel = $derived.by(() => {
		const { from, to } = range();
		if (unit === 'day') return anchor;
		if (unit === 'month') return anchor.slice(0, 7);
		return `${from} ~ ${to}`;
	});

	// 월 뷰용 — <input type="month"> 는 'YYYY-MM' 값을 쓰므로 anchor 와 변환.
	const anchorMonth = $derived(anchor.slice(0, 7));
	function onMonthInput(e: Event) {
		const v = (e.currentTarget as HTMLInputElement).value;
		if (!/^\d{4}-\d{2}$/.test(v)) return; // 지움 등 — 무시.
		anchor = `${v}-01`;
		syncUrl();
		load();
	}

	async function load() {
		loading = true;
		error = null;
		try {
			const { from, to } = range();
			const [r, n] = await Promise.all([
				worklogApi.activities(from, to),
				worklogApi.notes(from, to)
			]);
			report = r;
			notes = n;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	}

	function step(dir: -1 | 1) {
		const a = parse(anchor);
		if (unit === 'day') a.setDate(a.getDate() + dir);
		else if (unit === 'week') a.setDate(a.getDate() + dir * 7);
		else a.setMonth(a.getMonth() + dir);
		anchor = fmt(a);
		syncUrl();
		load();
	}
	function goToday() {
		anchor = fmt(new Date());
		syncUrl();
		load();
	}
	// 날짜 직접 입력 — bind:value 로 anchor 는 이미 갱신됨. 빈 값(지움)은 무시.
	function onAnchorInput() {
		if (!/^\d{4}-\d{2}-\d{2}$/.test(anchor)) {
			anchor = fmt(new Date());
		}
		syncUrl();
		load();
	}
	function setUnit(u: Unit) {
		if (unit === u) return;
		unit = u;
		if (u === 'range') {
			// 직전 뷰의 기간을 초기값으로 — 빈 구간에서 시작하지 않게.
			rangeFrom = anchor;
			rangeTo = anchor;
		}
		load();
	}
	function onRangeInput() {
		if (!/^\d{4}-\d{2}-\d{2}$/.test(rangeFrom) || !/^\d{4}-\d{2}-\d{2}$/.test(rangeTo)) return;
		load();
	}

	function syncUrl() {
		const cur = new URLSearchParams(window.location.search).get('date');
		if (cur === anchor) return;
		// 아래 URL $effect 가 이 로컬 변경에 반응해 anchor 를 옛 URL 값으로
		// 되돌리지 않도록 먼저 기록 (goto 는 비동기 — 사용자 보고 버그).
		lastUrlDate = anchor;
		goto(`/worklog?date=${anchor}`, { replaceState: true, keepFocus: true, noScroll: true });
	}

	onMount(() => {
		const dateParam = new URLSearchParams(window.location.search).get('date');
		if (dateParam && /^\d{4}-\d{2}-\d{2}$/.test(dateParam)) {
			anchor = dateParam;
			lastUrlDate = dateParam;
		}
		load();
	});

	// URL(?date=) 진리원 — 뒤로가기/딥링크 (BUG-104 패턴).
	//
	// 사용자 보고 버그: 이 effect 가 `anchor` 도 반응형 의존이라, 날짜 입력으로
	// anchor 가 바뀌는 즉시(goto 로 URL 이 갱신되기 전에) 재실행돼 "URL 의 옛
	// 날짜 ≠ anchor" 조건에 걸려 anchor 를 옛 값으로 되돌렸다 — 날짜를 골라도
	// 처음 값으로 계속 복귀. URL 값이 실제로 바뀌었을 때만 반응하도록
	// lastUrlDate 로 가드.
	let lastUrlDate: string | null = null;
	$effect(() => {
		const d = $page.url.searchParams.get('date');
		if (d === lastUrlDate) return;
		lastUrlDate = d;
		if (d && /^\d{4}-\d{2}-\d{2}$/.test(d) && d !== anchor) {
			anchor = d;
			load();
		}
	});

	// ─── 노트 편집 (일 뷰 전용) ───
	let editMode = $state(false);
	$effect(() => setUnsaved('worklog-note-edit', editMode));
	onDestroy(() => setUnsaved('worklog-note-edit', false));
	let saving = $state(false);
	let saveError = $state<string | null>(null);
	let editorContainer: HTMLDivElement | undefined = $state(undefined);
	let editorView: EditorView | null = null;

	const dayNote = $derived(notes.find((n) => n.date === anchor)?.content ?? null);

	async function enterEdit() {
		editMode = true;
		saveError = null;
		await tick();
		if (!editorContainer) return;
		editorView?.destroy();
		editorView = new EditorView({
			doc: dayNote ?? '',
			extensions: [
				basicSetup,
				markdown(),
				editorThemeCompartment.of(editorThemeExtension($theme)),
				crossLinkAutocomplete(),
				indentExtensions($editorSettings),
				EditorView.theme({
					'&': { fontSize: '0.875rem', height: '100%' },
					'.cm-scroller': { overflow: 'auto' }
				})
			],
			parent: editorContainer
		});
	}
	$effect(() => {
		const t = $theme;
		editorView?.dispatch({
			effects: editorThemeCompartment.reconfigure(editorThemeExtension(t))
		});
	});
	function cancelEdit() {
		editorView?.destroy();
		editorView = null;
		editMode = false;
		saveError = null;
	}
	async function saveNote() {
		saving = true;
		saveError = null;
		try {
			const text = editorView ? editorView.state.doc.toString() : '';
			await worklogApi.noteSet(anchor, text);
			cancelEdit();
			await load();
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'save failed';
		} finally {
			saving = false;
		}
	}

	// ─── 타임라인 표시 ───
	const KIND_BADGE: Record<string, { label: string; cls: string }> = {
		status: { label: '상태', cls: 'b-status' },
		type: { label: '타입', cls: 'b-status' },
		comment: { label: '댓글', cls: 'b-comment' },
		created: { label: '생성', cls: 'b-created' }
	};
	function activityHref(a: ActivityRow): string {
		return /^C-\d+$/.test(a.slug)
			? `/campaigns/${encodeURIComponent(a.slug)}`
			: `/quests/${encodeURIComponent(a.slug)}`;
	}
	function firstLine(s: string): string {
		return s.split('\n', 1)[0] ?? '';
	}
	// 주/월 뷰: 날짜별 그룹핑.
	const grouped = $derived.by(() => {
		if (!report) return [] as { date: string; rows: ActivityRow[] }[];
		const out: { date: string; rows: ActivityRow[] }[] = [];
		for (const a of report.activities) {
			const d = a.ts.slice(0, 10);
			const last = out[out.length - 1];
			if (last && last.date === d) last.rows.push(a);
			else out.push({ date: d, rows: [a] });
		}
		return out;
	});
</script>

<div class="page">
	<div class="hdr">
		<h1>🕘 작업 기록</h1>
		<div class="controls">
			<div class="unit">
				{#each UNITS as u (u)}
					<button class:on={unit === u} onclick={() => setUnit(u)}>{UNIT_LABEL[u]}</button>
				{/each}
			</div>
			<div class="nav">
				{#if unit === 'range'}
					<!-- 임의 구간 (admin 요청) — 시작/끝 날짜 직접 지정. -->
					<input
						class="anchor-date"
						type="date"
						bind:value={rangeFrom}
						onchange={onRangeInput}
						aria-label="구간 시작"
					/>
					<span class="range-tilde">~</span>
					<input
						class="anchor-date"
						type="date"
						bind:value={rangeTo}
						onchange={onRangeInput}
						aria-label="구간 끝"
					/>
				{:else}
					<button onclick={() => step(-1)} aria-label="이전">◀</button>
					{#if unit === 'month'}
						<!-- 월 뷰는 native month picker — 월만 고름 (admin 요청). -->
						<input
							class="anchor-date"
							type="month"
							value={anchorMonth}
							onchange={onMonthInput}
							aria-label="월 선택"
						/>
					{:else}
						<!-- 날짜 직접 입력 (admin 요청) — quest 기한 편집과 동일한
						     native date input. 주 뷰에선 고른 날짜가 속한 주로 이동. -->
						<input
							class="anchor-date"
							type="date"
							bind:value={anchor}
							onchange={onAnchorInput}
							aria-label="날짜 선택"
						/>
					{/if}
					{#if unit === 'week'}
						<span class="range-label">{rangeLabel}</span>
					{/if}
					<button onclick={() => step(1)} aria-label="다음">▶</button>
					<button onclick={goToday}>오늘</button>
				{/if}
			</div>
		</div>
	</div>

	{#if loading}
		<div class="state">Loading…</div>
	{:else if error}
		<div class="state err">{error}</div>
	{:else if report}
		<!-- 노트 -->
		{#if unit === 'day'}
			<div class="note">
				<div class="note-head">
					<span>📝 노트 — {anchor}</span>
					{#if !editMode}
						<button class="btn" onclick={enterEdit}>{dayNote ? '편집' : '작성'}</button>
					{/if}
				</div>
				{#if editMode}
					<div class="note-edit">
						<div class="editor-wrap" bind:this={editorContainer}></div>
						<div class="actions">
							<button class="btn primary" onclick={saveNote} disabled={saving}>
								{saving ? '저장…' : '저장'}
							</button>
							<button class="btn" onclick={cancelEdit} disabled={saving}>취소</button>
						</div>
						{#if saveError}<p class="err">{saveError}</p>{/if}
					</div>
				{:else if dayNote}
					<div class="note-body"><MarkdownView source={dayNote} /></div>
				{:else}
					<div class="note-body muted">노트 없음 — "작성" 으로 남기기.</div>
				{/if}
			</div>
		{:else if notes.length > 0}
			<div class="note">
				<div class="note-head"><span>📝 기간 내 노트 {notes.length}건</span></div>
				{#each notes as n (n.date)}
					<div class="note-day">
						<button class="note-date" onclick={() => goto(`/worklog?date=${n.date}`)}>
							{n.date}
						</button>
						<div class="note-body"><MarkdownView source={n.content ?? ''} /></div>
					</div>
				{/each}
			</div>
		{/if}

		<!-- 타임라인 -->
		<div class="count">활동 {report.activities.length}건</div>
		{#if report.activities.length === 0}
			<div class="state">활동 없음.</div>
		{:else}
			<div class="timeline">
				{#each grouped as g (g.date)}
					{#if unit !== 'day'}
						<div class="day-sep">── {g.date} ──</div>
					{/if}
					<!-- BUG-118 (admin 보고): 짧은 시간에 상태를 반복 토글하면 같은 ts/
					     slug/kind/summary 조합이 여러 번 나올 수 있어 (ts+slug+kind+
					     summary) 키가 unique 하지 않았음 — Svelte 5 는 keyed each 의
					     중복 키를 허용 안 하고 첫 렌더에서 throw, "로딩" 화면이 그대로
					     굳어버림(loading 상태 자체는 false 로 바뀌지만 그걸 반영할
					     렌더가 죽어서 화면이 안 바뀜). ActivityRow 엔 안정적인 고유 id
					     가 없어(여러 테이블 UNION) index 로 키를 대체.
					-->
					{#each g.rows as a, ri (ri)}
						<a class="row" href={activityHref(a)}>
							<span class="ts">{a.ts.slice(11, 16)}</span>
							<span class="badge {KIND_BADGE[a.kind]?.cls ?? ''}">
								{KIND_BADGE[a.kind]?.label ?? a.kind}
							</span>
							<span class="slug">{a.slug}</span>
							<span class="summary">{firstLine(a.summary)}</span>
						</a>
					{/each}
				{/each}
			</div>
			<div class="totals">
				<span><b>{report.counts.status_changes}</b> 상태변경</span>
				<span><b>{report.counts.comments}</b> 댓글</span>
				<span><b>{report.counts.created}</b> 생성</span>
				<span class="right">done 전환 <b>{report.counts.done_transitions}</b></span>
			</div>
		{/if}
	{/if}
</div>

<style>
	.page {
		padding: 1.25rem 1.5rem 2rem;
		max-width: var(--content-max-width, 900px);
		margin: 0 auto;
	}
	.hdr {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin-bottom: 1rem;
		flex-wrap: wrap;
	}
	.hdr h1 {
		font-size: 1.15rem;
		font-weight: 600;
		margin: 0;
	}
	.controls {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
	.unit {
		display: inline-flex;
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
	}
	.unit button {
		border: none;
		background: transparent;
		color: var(--text-muted);
		padding: 0.25rem 0.75rem;
		font-size: 0.78rem;
		cursor: pointer;
	}
	.unit button + button {
		border-left: 1px solid var(--border);
	}
	.unit button.on {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		color: var(--accent);
	}
	.nav {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
	}
	.nav button {
		border: 1px solid var(--border);
		background: transparent;
		color: var(--text);
		border-radius: 6px;
		padding: 0.2rem 0.55rem;
		font-size: 0.78rem;
		cursor: pointer;
	}
	.nav button:hover {
		background: var(--bg-subtle);
	}
	.range-label {
		font-size: 0.85rem;
		font-weight: 600;
		min-width: 7.5rem;
		text-align: center;
	}
	/* 날짜 직접 입력 — quest 기한 편집 input 과 같은 native date/month. */
	.anchor-date {
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 6px;
		padding: 0.15rem 0.4rem;
		font-size: 0.82rem;
	}
	.range-tilde {
		color: var(--text-muted);
		font-size: 0.82rem;
	}

	.note {
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg-elevated);
		margin-bottom: 1rem;
	}
	.note-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--border);
		font-size: 0.78rem;
		color: var(--text-muted);
	}
	.note-body {
		padding: 0.6rem 0.75rem;
		font-size: 0.85rem;
	}
	.note-body.muted {
		color: var(--text-muted);
	}
	.note-day {
		border-bottom: 1px solid var(--border);
	}
	.note-day:last-child {
		border-bottom: none;
	}
	.note-date {
		background: transparent;
		border: none;
		color: var(--accent);
		font-size: 0.78rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		padding: 0.5rem 0.75rem 0;
		cursor: pointer;
		text-decoration: underline;
	}
	.note-edit {
		padding: 0.6rem 0.75rem;
	}
	.editor-wrap {
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
		height: 220px;
		resize: vertical;
	}

	.count {
		font-size: 0.75rem;
		color: var(--text-muted);
		margin: 0 0 0.35rem;
	}
	.timeline {
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--bg-elevated);
		padding: 0.25rem 0.75rem;
	}
	.day-sep {
		font-size: 0.72rem;
		color: var(--text-muted);
		padding: 0.5rem 0 0.2rem;
	}
	.row {
		display: flex;
		gap: 0.6rem;
		align-items: baseline;
		padding: 0.4rem 0;
		border-bottom: 1px solid var(--border);
		text-decoration: none;
		color: var(--text);
		font-size: 0.83rem;
		min-width: 0;
	}
	.row:last-child {
		border-bottom: none;
	}
	.row:hover {
		background: color-mix(in srgb, var(--accent) 6%, transparent);
	}
	.ts {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.75rem;
		color: var(--text-muted);
		flex: none;
		width: 2.8rem;
	}
	.badge {
		flex: none;
		font-size: 0.68rem;
		padding: 0.05rem 0.5rem;
		border-radius: 9px;
	}
	.b-status {
		background: color-mix(in srgb, var(--success) 15%, transparent);
		color: var(--success);
	}
	.b-comment {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		color: var(--accent);
	}
	.b-created {
		background: color-mix(in srgb, var(--warning) 15%, transparent);
		color: var(--warning);
	}
	.slug {
		flex: none;
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.75rem;
		color: var(--accent);
	}
	.summary {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.totals {
		display: flex;
		gap: 1rem;
		padding: 0.6rem 0.2rem;
		font-size: 0.75rem;
		color: var(--text-muted);
	}
	.totals b {
		color: var(--text);
		font-weight: 600;
	}
	.totals .right {
		margin-left: auto;
	}

	.state {
		color: var(--text-muted);
		padding: 1rem 0;
		font-size: 0.875rem;
	}
	.state.err,
	.err {
		color: var(--danger);
	}

	.btn {
		padding: 0.25rem 0.65rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.78rem;
		cursor: pointer;
	}
	.btn:hover:not(:disabled) {
		background: var(--bg-subtle);
	}
	.btn.primary {
		background: var(--btn-primary-bg);
		border-color: var(--btn-primary-border);
		color: var(--btn-primary-text);
	}
	.btn.primary:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover);
	}
	.btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.actions {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
</style>
