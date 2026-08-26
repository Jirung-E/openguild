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
	import { saveShortcut } from '$lib/utils/save-shortcut';
	import {
		worklogApi,
		type WorklogReport,
		type WorklogNote
	} from '$lib/api/worklog';
	// REQ-006: compact 묶음 로직은 단위 테스트를 붙이려고 utils 로 분리했다.
	import {
		activityHref,
		groupByDay,
		groupByDoc,
		groupTimeLabel,
		firstLine
	} from '$lib/utils/worklog-group';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	// DEV-302: 제목/노트 라벨에 섞여 있던 이모지(🕘/📝)를 아이콘으로 분리.
	import Icon from '$lib/components/Icon.svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	import { indentExtensions } from '$lib/utils/editor-indent';
	import { editorSettings } from '$lib/stores/editorSettings';
	import { crossLinkAutocomplete } from '$lib/utils/editor-links';
	// DEV-205(2차): i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-205(3차): 네이티브 date/month input(OS 로케일 고정) → 언어 반응 DateField.
	import DateField from '$lib/components/DateField.svelte';

	type Unit = 'day' | 'week' | 'month' | 'range';
	const UNITS: Unit[] = ['day', 'week', 'month', 'range'];

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

	// 월 뷰용 — DateField(mode="month") 는 'YYYY-MM' 값을 쓰므로 anchor 와 변환.
	// DEV-205(3차): 네이티브 <input type="month"> 는 표기/팝업이 OS 로케일 고정
	// (앱 언어 무시) — 커스텀 달력 팝업을 가진 DateField 로 교체.
	const anchorMonth = $derived(anchor.slice(0, 7));
	function onMonthInput(v: string) {
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
		// BUG-120: pushState(기본) — 날짜/단위 변경마다 새 history entry 를 쌓아
		// 브라우저 뒤로가기로 이전에 보던 날짜로 한 단계씩 되짚어갈 수 있게 한다.
		goto(`/worklog?date=${anchor}`, { keepFocus: true, noScroll: true });
	}

	onMount(() => {
		const dateParam = new URLSearchParams(window.location.search).get('date');
		if (dateParam && /^\d{4}-\d{2}-\d{2}$/.test(dateParam)) {
			anchor = dateParam;
			lastUrlDate = dateParam;
		} else {
			// BUG-126(admin 보고): date 없이 진입(기본=오늘)하면 그 상태가 URL 에
			// 반영 안 돼 있었음 — 이 진입 지점 자체가 history 상 "날짜 없음" 으로
			// 남아, 여러 날짜를 거쳐 뒤로가기하면 이 지점에서 원래 보던(오늘)
			// 날짜로 안 돌아오고 직전 날짜에 멈춰있는 것처럼 보였다(규칙 페이지와
			// 동일 원인 — 아래 $effect 가 date 없는 URL 을 무시하도록 짜여 있었음).
			// 진입 즉시 실제 anchor 를 URL 에 replaceState 로 명시.
			lastUrlDate = anchor;
			goto(`/worklog?date=${anchor}`, {
				replaceState: true,
				keepFocus: true,
				noScroll: true
			});
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
	// BUG-126 후속: date 가 사라진 경우(뒤로가기로 date 없는 최초 진입 지점에
	// 복귀 / nav 링크를 같은 라우트에서 다시 클릭)도 무시하지 않고 오늘로 리셋.
	let lastUrlDate: string | null = null;
	$effect(() => {
		const d = $page.url.searchParams.get('date');
		if (d === lastUrlDate) return;
		lastUrlDate = d;
		if (d && /^\d{4}-\d{2}-\d{2}$/.test(d) && d !== anchor) {
			anchor = d;
			load();
		} else if (!d && anchor !== fmt(new Date())) {
			anchor = fmt(new Date());
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
	async function saveNote(keepEditing = false) {
		if (saving) return;
		saving = true;
		saveError = null;
		try {
			const text = editorView ? editorView.state.doc.toString() : '';
			await worklogApi.noteSet(anchor, text);
			if (keepEditing) {
				// load()는 loading 분기로 편집기를 파괴하므로 단축키 저장에서는
				// 원본 notes 배열만 갱신해 파생 dayNote와 미저장 판정을 맞추고,
				// 현재 EditorView와 커서는 그대로 유지한다.
				const existing = notes.some((note) => note.date === anchor);
				notes = existing
					? notes.map((note) => (note.date === anchor ? { ...note, content: text } : note))
					: [...notes, { date: anchor, content: text }];
			} else {
				cancelEdit();
				await load();
			}
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'save failed';
		} finally {
			saving = false;
		}
	}
	// ─── 타임라인 표시 ───
	// DEV-205(2차): 라벨은 locale 반응이어야 해서 const 맵 대신 함수로 — cls 는
	// 언어 무관이라 그대로 정적 맵 유지.
	const KIND_BADGE_CLS: Record<string, string> = {
		status: 'b-status',
		type: 'b-status',
		comment: 'b-comment',
		created: 'b-created',
		// DEV-236: 토론 resolve/reopen 전환.
		discussion: 'b-discussion',
		// DEV-288: 규칙·도서관 문서 변경.
		rule: 'b-doc',
		book: 'b-doc'
	};
	function kindBadgeLabel(kind: string): string {
		switch (kind) {
			case 'status':
				return t('worklogPage.badge.status', $locale);
			case 'type':
				return t('worklogPage.badge.type', $locale);
			case 'comment':
				return t('worklogPage.badge.comment', $locale);
			case 'created':
				return t('worklogPage.badge.created', $locale);
			case 'discussion':
				return t('worklogPage.badge.discussion', $locale);
			// DEV-288: 규칙/도서관 변경.
			case 'rule':
				return t('worklogPage.badge.rule', $locale);
			case 'book':
				return t('worklogPage.badge.book', $locale);
			default:
				return kind;
		}
	}
	function unitLabel(u: Unit): string {
		switch (u) {
			case 'day':
				return t('worklogPage.unit.day', $locale);
			case 'week':
				return t('worklogPage.unit.week', $locale);
			case 'month':
				return t('worklogPage.unit.month', $locale);
			case 'range':
				return t('worklogPage.unit.range', $locale);
		}
	}
	// 주/월 뷰: 날짜별 그룹핑.
	const grouped = $derived(report ? groupByDay(report.activities) : []);

	// REQ-006: compact 뷰 — 같은 날 같은 문서에 가한 조작을 문서 단위로 묶는다.
	// 평면 목록은 한 퀘스트를 작업하면 상태변경·댓글·수정이 줄줄이 풀려 나와,
	// 활동이 많은 날 "실제로 몇 개의 문서를 건드렸는지" 가 안 보인다.
	//
	// 뷰 모드는 localStorage 영속 — 매번 다시 고르게 하면 성가시다
	// (QuestNoteSection 의 heightMode 와 같은 정책).
	const VIEW_MODE_KEY = 'openguild.worklogViewMode';
	function loadViewMode(): 'compact' | 'full' {
		try {
			return localStorage.getItem(VIEW_MODE_KEY) === 'full' ? 'full' : 'compact';
		} catch {
			return 'compact';
		}
	}
	let viewMode = $state<'compact' | 'full'>(loadViewMode());
	function setViewMode(m: 'compact' | 'full') {
		viewMode = m;
		try {
			localStorage.setItem(VIEW_MODE_KEY, m);
		} catch {
			/* ignore */
		}
	}

	/** compact 뷰에서 펼쳐 놓은 그룹 키(`날짜|slug`). 기본은 전부 접힘. */
	let expandedDocs = $state(new Set<string>());
	function docKey(date: string, slug: string) {
		return `${date}|${slug}`;
	}
	function toggleDoc(date: string, slug: string) {
		const k = docKey(date, slug);
		// Set 을 직접 mutate 하면 Svelte 5 가 변화를 못 본다 — 새 Set 으로 교체.
		const next = new Set(expandedDocs);
		if (next.has(k)) next.delete(k);
		else next.add(k);
		expandedDocs = next;
	}
</script>

<div class="page">
	<div class="hdr">
		<h1><Icon name="clock" size={16} />{t('worklogPage.title', $locale)}</h1>
		<div class="controls">
			<div class="unit">
				{#each UNITS as u (u)}
					<button class:on={unit === u} onclick={() => setUnit(u)}>{unitLabel(u)}</button>
				{/each}
			</div>
			<!-- REQ-006: 표시 방식 — 문서별 묶기(기본) / 전체 시간순. -->
			<div class="unit viewmode">
				<button
					class:on={viewMode === 'compact'}
					onclick={() => setViewMode('compact')}
					title={t('worklogPage.view.compactHint', $locale)}
				>
					{t('worklogPage.view.compact', $locale)}
				</button>
				<button
					class:on={viewMode === 'full'}
					onclick={() => setViewMode('full')}
					title={t('worklogPage.view.fullHint', $locale)}
				>
					{t('worklogPage.view.full', $locale)}
				</button>
			</div>
			<div class="nav">
				{#if unit === 'range'}
					<!-- 임의 구간 (admin 요청) — 시작/끝 날짜 직접 지정. -->
					<DateField
						bind:value={rangeFrom}
						onpick={onRangeInput}
						ariaLabel={t('worklogPage.rangeStartAria', $locale)}
					/>
					<span class="range-tilde">~</span>
					<DateField
						bind:value={rangeTo}
						onpick={onRangeInput}
						ariaLabel={t('worklogPage.rangeEndAria', $locale)}
					/>
				{:else}
					<button onclick={() => step(-1)} aria-label={t('worklogPage.prevAria', $locale)}>◀</button
					>
					{#if unit === 'month'}
						<!-- 월 뷰는 월만 고름 (admin 요청) — DateField month 모드. -->
						<DateField
							mode="month"
							value={anchorMonth}
							onpick={onMonthInput}
							ariaLabel={t('worklogPage.monthSelectAria', $locale)}
						/>
					{:else}
						<!-- 날짜 직접 입력 (admin 요청) — quest 기한 편집과 동일한 DateField.
						     주 뷰에선 고른 날짜가 속한 주로 이동. -->
						<DateField
							bind:value={anchor}
							onpick={onAnchorInput}
							ariaLabel={t('worklogPage.dateSelectAria', $locale)}
						/>
					{/if}
					{#if unit === 'week'}
						<span class="range-label">{rangeLabel}</span>
					{/if}
					<button onclick={() => step(1)} aria-label={t('worklogPage.nextAria', $locale)}>▶</button>
					<button onclick={goToday}>{t('worklogPage.today', $locale)}</button>
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
					<span><Icon name="memo" size={12} />{t('worklogPage.notePre', $locale)}{anchor}</span>
					{#if !editMode}
						<button class="btn" onclick={enterEdit}
							>{dayNote
								? t('worklogPage.noteEdit', $locale)
								: t('worklogPage.noteWrite', $locale)}</button
						>
					{/if}
				</div>
				{#if editMode}
					<div
						class="note-edit"
						use:saveShortcut={{ disabled: saving, onSave: () => void saveNote(true) }}
					>
						<div class="editor-wrap" bind:this={editorContainer}></div>
						<div class="actions">
							<button class="btn primary" onclick={() => saveNote()} disabled={saving}>
								{saving ? t('worklogPage.saving', $locale) : t('worklogPage.save', $locale)}
							</button>
							<button class="btn" onclick={cancelEdit} disabled={saving}
								>{t('worklogPage.cancel', $locale)}</button
							>
						</div>
						{#if saveError}<p class="err">{saveError}</p>{/if}
					</div>
				{:else if dayNote}
					<div class="note-body"><MarkdownView source={dayNote} /></div>
				{:else}
					<div class="note-body muted">
						{t('worklogPage.noNotePre', $locale)}{t('worklogPage.noteWrite', $locale)}{t(
							'worklogPage.noNotePost',
							$locale
						)}
					</div>
				{/if}
			</div>
		{:else if notes.length > 0}
			<div class="note">
				<div class="note-head">
					<span
						><Icon name="memo" size={12} />{t(
							'worklogPage.notesInRangePre',
							$locale
						)}{notes.length}{t('worklogPage.notesInRangePost', $locale)}</span
					>
				</div>
				{#each notes as n (n.date)}
					<div class="note-day">
						<!-- BUG-126(admin 요청): 링크일 필요 없음 — 그냥 날짜 표시. -->
						<span class="note-date">{n.date}</span>
						<div class="note-body"><MarkdownView source={n.content ?? ''} /></div>
					</div>
				{/each}
			</div>
		{/if}

		<!-- 타임라인 -->
		<div class="count">
			{t('worklogPage.activitiesPre', $locale)}{report.activities.length}{t(
				'worklogPage.activitiesPost',
				$locale
			)}
		</div>
		{#if report.activities.length === 0}
			<div class="state">{t('worklogPage.noActivity', $locale)}</div>
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
					{#if viewMode === 'compact'}
						{#each groupByDoc(g.rows) as dg (dg.slug)}
							{@const open = expandedDocs.has(docKey(g.date, dg.slug))}
							<div class="docgroup" class:open>
								<div class="docrow">
									<button
										class="docexp"
										onclick={() => toggleDoc(g.date, dg.slug)}
										aria-expanded={open}
										title={dg.slug}
									>
										<span class="toggle-icon" class:collapsed={!open}>▼</span>
									</button>
									<span class="ts">{groupTimeLabel(dg)}</span>
									<a class="slug doclink" href={dg.href}>{dg.slug}</a>
									<span class="kinds">
										{#each dg.kinds as k (k)}
											<span class="badge {KIND_BADGE_CLS[k] ?? ''}">{kindBadgeLabel(k)}</span>
										{/each}
									</span>
									<span class="cnt">{dg.rows.length}{t('worklogPage.group.count', $locale)}</span>
									<span class="summary">{firstLine(dg.rows[0].summary)}</span>
								</div>
								{#if open}
									{#each dg.rows as a, ri (ri)}
										<a class="row sub" href={activityHref(a)}>
											<span class="ts">{a.ts.slice(11, 16)}</span>
											<span class="badge {KIND_BADGE_CLS[a.kind] ?? ''}">
												{kindBadgeLabel(a.kind)}
											</span>
											<span class="summary">{firstLine(a.summary)}</span>
										</a>
									{/each}
								{/if}
							</div>
						{/each}
					{:else}
						{#each g.rows as a, ri (ri)}
							<a class="row" href={activityHref(a)}>
								<span class="ts">{a.ts.slice(11, 16)}</span>
								<span class="badge {KIND_BADGE_CLS[a.kind] ?? ''}">
									{kindBadgeLabel(a.kind)}
								</span>
								<span class="slug">{a.slug}</span>
								<span class="summary">{firstLine(a.summary)}</span>
							</a>
						{/each}
					{/if}
				{/each}
			</div>
			<div class="totals">
				<span
					><b>{report.counts.status_changes}</b>
					{t('worklogPage.summary.statusChanges', $locale)}</span
				>
				<span><b>{report.counts.comments}</b> {t('worklogPage.summary.comments', $locale)}</span>
				<span><b>{report.counts.created}</b> {t('worklogPage.summary.created', $locale)}</span>
				{#if report.counts.doc_changes > 0}
					<span
						><b>{report.counts.doc_changes}</b> {t('worklogPage.summary.docChanges', $locale)}</span
					>
				{/if}
				{#if report.counts.discussion_events > 0}
					<span
						><b>{report.counts.discussion_events}</b>
						{t('worklogPage.summary.discussion', $locale)}</span
					>
				{/if}
				<span class="right"
					>{t('worklogPage.summary.doneTransitions', $locale)}
					<b>{report.counts.done_transitions}</b></span
				>
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
		/* DEV-302: 아이콘 + 제목 정렬. */
		display: inline-flex;
		align-items: center;
		gap: 0.35em;
		font-size: 1.15rem;
		font-weight: 600;
		margin: 0;
	}
	.controls {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		/* DEV-257: 모바일 폭에서 단위 토글/기간 이동 묶음이 화면 밖으로
		   밀리지 않게 줄바꿈 허용. */
		flex-wrap: wrap;
		row-gap: 0.4rem;
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
		/* DEV-302: 라벨 앞 아이콘 정렬 — span 안에서 함께 흐르게. */
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--border);
		font-size: 0.78rem;
		color: var(--text-muted);
	}
	.note-head > span {
		display: inline-flex;
		align-items: center;
		gap: 0.35em;
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
		display: block;
		color: var(--text-muted);
		font-size: 0.78rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		padding: 0.5rem 0.75rem 0;
	}
	.note-edit {
		padding: 0.6rem 0.75rem;
	}
	.editor-wrap {
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
		height: 13.75rem;
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
	/* REQ-006: compact 뷰 — 문서 단위 묶음. */
	.docgroup {
		border-bottom: 1px solid var(--border);
	}
	.docrow {
		display: flex;
		gap: 0.6rem;
		align-items: baseline;
		padding: 0.4rem 0;
		font-size: 0.83rem;
		min-width: 0;
	}
	/* 묶음 헤더의 시각은 `01:22–01:26` 처럼 범위라 단일 시각(.ts 기본 2.8rem)보다
	   넓어야 한다 — 좁으면 두 줄로 접힌다. */
	.docrow .ts {
		width: 5.4rem;
		white-space: nowrap;
	}
	/* 펼침 버튼은 화살표만 — 행 전체를 링크로 두면 펼치기와 이동이 충돌한다.
	   그래서 링크는 slug 에만 건다(문서로 이동), 나머지는 펼침/접힘. */
	.docexp {
		flex: none;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		color: var(--text-muted);
		line-height: 1;
	}
	.toggle-icon {
		font-size: 0.6rem;
		display: inline-block;
		transition: transform 0.12s;
	}
	.toggle-icon.collapsed {
		transform: rotate(-90deg);
	}
	.doclink {
		text-decoration: none;
	}
	.doclink:hover {
		text-decoration: underline;
	}
	.kinds {
		display: flex;
		gap: 0.25rem;
		flex: none;
	}
	.cnt {
		flex: none;
		font-size: 0.72rem;
		color: var(--text-muted);
	}
	/* 펼친 개별 조작은 한 단계 들여쓴다 — 묶음에 속한 것이 보이도록. */
	.row.sub {
		border-bottom: none;
		padding-left: 1.5rem;
		opacity: 0.85;
	}
	.row.sub:last-child {
		padding-bottom: 0.5rem;
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
	.b-discussion {
		background: color-mix(in srgb, var(--danger) 15%, transparent);
		color: var(--danger);
	}
	/* DEV-288: 규칙·도서관 문서 변경. */
	.b-doc {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		color: var(--accent);
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
		/* BUG-202: 좁은 화면에서 한 줄에 안 들어가면 항목 **안에서** 줄바꿈돼
		   숫자와 라벨이 따로 흩어졌다. 항목 단위로 줄을 넘기고, 항목 내부는
		   붙여 둔다. */
		flex-wrap: wrap;
		gap: 0.35rem 1rem;
		padding: 0.6rem 0.2rem;
		font-size: 0.75rem;
		color: var(--text-muted);
	}
	.totals > span {
		white-space: nowrap;
	}
	.totals b {
		color: var(--text);
		font-weight: 600;
	}
	.totals .right {
		margin-left: auto;
	}
	@media (max-width: 640px) {
		/* 줄이 바뀌면 auto 여백이 그 줄만 밀어 어색해진다 — 좁을 땐 그냥 흐름대로. */
		.totals .right {
			margin-left: 0;
		}
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
