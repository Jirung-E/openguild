<!--
  DEV-016 (multi-file): 길드 규칙 페이지 — 다중 파일 (`.guild/rules/{slug}.md`).

  레이아웃:
  - 좌측 sidebar: rule slug 목록 + 신규 / 삭제 / 이름변경 버튼.
  - 우측 panel: 선택된 rule 의 markdown view + CodeMirror 편집.

  legacy 단일 `.guild/rules.md` 은 첫 list 호출 시 자동으로
  `.guild/rules/general.md` 로 마이그레이션됨.
-->
<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	// BUG-104: 선택 규칙을 URL(?slug=) 에 반영 — 규칙간 링크 이동 + 뒤로가기 복원.
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	// DEV-153: 편집 중이면 이탈 가드에 보고 (라우트 이탈용. 같은 페이지 내 규칙
	// 전환 경고는 아래 confirmDiscardSlug 모달이 별도로 담당).
	import { setUnsaved } from '$lib/stores/unsaved';
	import { rulesApi, type RuleEntry } from '$lib/api/rules';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	// 편집기 테마 — Compartment 로 다크/라이트 라이브 전환.
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	// DEV-130: Tab 들여쓰기 — editorSettings(tab/space·크기)를 따르는 indentExtensions.
	import { indentExtensions } from '$lib/utils/editor-indent';
	import { editorSettings } from '$lib/stores/editorSettings';
	// DEV-069: 규칙 편집기에도 첨부 (drag&drop / Ctrl+V / 버튼).
	import { attachmentExtension } from '$lib/utils/editor-attach';
	// DEV-140 후속: 규칙 편집기에도 XXX-NNN → [[...]] 자동완성.
	import { crossLinkAutocomplete } from '$lib/utils/editor-links';
	// DEV-173 후속: 규칙 생성/삭제/이름변경/저장(제목 변경) 시 cross-link 인덱스
	// 재적재 — 안 하면 방금 만든 규칙이 [[링크]] 에서 미존재(빨강)로 남음.
	import { loadQuestIndex } from '$lib/stores/questIndex';
	// DEV-243: 태그 — quest 와 동일한 정의(색/설명) registry 공유.
	import TagPills from '$lib/components/TagPills.svelte';
	import { adminApi } from '$lib/api/admin';
	import type { QuestTagDef } from '$lib/types';
	import { showToast } from '$lib/stores/toast';
	// DEV-182: 생성/변경 시각 표시 — quest 상세와 동일 포맷 유틸.
	import { formatTs, formatRelative } from '$lib/utils/datetime';

	let loading = $state(true);
	let error = $state<string | null>(null);
	let entries = $state<RuleEntry[]>([]);
	let selectedSlug = $state<string | null>(null);
	let selectedContent = $state<string | null>(null);

	let editMode = $state(false);
	// DEV-153: 편집 중이면 이탈 가드에 보고.
	$effect(() => setUnsaved('rules-edit', editMode));
	onDestroy(() => setUnsaved('rules-edit', false));
	let editText = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	// 신규 / 이름변경 모달.
	let creating = $state(false);
	let createSlug = $state('');
	let createError = $state<string | null>(null);

	let renaming = $state(false);
	let renameSlug = $state('');
	let renameError = $state<string | null>(null);

	let editorContainer: HTMLDivElement | undefined = $state(undefined);
	let editorView: EditorView | null = null;

	const EDITOR_HEIGHT_KEY = 'openguild.questEditorHeight';
	function loadEditorHeight(): number {
		try {
			const raw = localStorage.getItem(EDITOR_HEIGHT_KEY);
			const n = raw ? parseInt(raw, 10) : NaN;
			if (Number.isFinite(n) && n >= 200 && n <= 2000) return n;
		} catch {
			/* ignore */
		}
		return 480;
	}

	async function loadList(preferSlug?: string | null) {
		loading = true;
		error = null;
		try {
			const res = await rulesApi.list();
			entries = res.entries ?? [];
			// 선택 유지 / 자동 선택.
			if (preferSlug && entries.some((e) => e.slug === preferSlug)) {
				selectedSlug = preferSlug;
			} else if (selectedSlug == null || !entries.some((e) => e.slug === selectedSlug)) {
				selectedSlug = entries[0]?.slug ?? null;
			}
			refreshSelectedContent();
			// DEV-173 후속: 목록이 바뀌었을 수 있으니(생성/삭제/이름변경 후 재호출됨)
			// cross-link 인덱스도 재적재.
			loadQuestIndex(true);
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	}

	function refreshSelectedContent() {
		if (!selectedSlug) {
			selectedContent = null;
			return;
		}
		const e = entries.find((x) => x.slug === selectedSlug);
		selectedContent = e ? e.content : null;
	}

	// DEV-243: 태그 — entries 안의 RuleEntry.tags 를 그대로 파생.
	let tagDefs = $state<QuestTagDef[]>([]);
	onMount(async () => {
		tagDefs = await adminApi.listTagDefs().catch(() => [] as QuestTagDef[]);
	});
	const selectedTags = $derived(entries.find((e) => e.slug === selectedSlug)?.tags ?? []);
	// DEV-182: 생성/변경 시각.
	const selectedEntry = $derived(entries.find((e) => e.slug === selectedSlug) ?? null);
	async function setRuleTags(tags: string[]) {
		if (!selectedSlug) return;
		try {
			const updated = await rulesApi.setTags(selectedSlug, tags);
			entries = entries.map((e) =>
				e.slug === selectedSlug ? { ...e, tags: updated.tags } : e
			);
		} catch (e) {
			showToast(e instanceof Error ? e.message : '태그 저장 실패', 'error');
		}
	}

	// DEV-243 후속(admin 지적): 태그는 달 수 있는데 태그로 찾을 방법이 없었음.
	// quest 의 DEV-068 tag 필터(AND, chip 클릭 토글)와 동일 패턴.
	let filterTags = $state(new Set<string>());
	const allTagOptions = $derived.by(() => {
		const set = new Set<string>();
		for (const e of entries) for (const t of e.tags ?? []) set.add(t);
		return Array.from(set).sort();
	});
	const tagCounts = $derived.by(() => {
		const m = new Map<string, number>();
		for (const e of entries) for (const t of e.tags ?? []) m.set(t, (m.get(t) ?? 0) + 1);
		return m;
	});
	function toggleTagFilter(t: string) {
		const next = new Set(filterTags);
		if (next.has(t)) next.delete(t);
		else next.add(t);
		filterTags = next;
	}
	const filteredEntries = $derived(
		filterTags.size === 0
			? entries
			: entries.filter((e) => {
					const eTags = new Set(e.tags ?? []);
					for (const t of filterTags) if (!eTags.has(t)) return false;
					return true;
				})
	);

	onMount(() => {
		// DEV-173: cross-link 딥링크 — `/rules?slug=xxx` 로 진입 시 해당 규칙 선택.
		const slugParam = new URLSearchParams(window.location.search).get('slug');
		loadList(slugParam).then(() => {
			// BUG-126(admin 보고): slug 없이 진입(예: nav 의 "Rules" 링크)하면
			// loadList 가 entries[0] 을 자동 선택하지만 URL 엔 반영 안 됐음 —
			// 그 결과 "이 진입 지점" 자체가 history 상 "선택 없음" 으로 남아,
			// 여러 규칙을 거쳐 뒤로가기하면 이 지점에서 (원래 selectedSlug 였던)
			// 첫 규칙으로 안 돌아오고 직전 선택에 멈춰있는 것처럼 보였다 —
			// $effect 가 slug 없는 URL 은 무시하도록 짜여 있었기 때문(아래).
			// 진입 즉시 실제 선택을 URL 에 replaceState 로 명시해 이 지점부터
			// 항상 URL == 상태가 성립하게 만든다.
			if (selectedSlug && selectedSlug !== slugParam) {
				const cur = new URLSearchParams(window.location.search);
				cur.set('slug', selectedSlug);
				goto(`/rules?${cur.toString()}`, {
					replaceState: true,
					keepFocus: true,
					noScroll: true
				});
			}
		});
	});

	// BUG-104: URL(?slug=) 을 선택 상태의 진리원으로.
	//
	// (1) 규칙 본문의 다른 규칙 [[링크]] 클릭 — 같은 라우트 내 이동이라 onMount 가
	//     재실행되지 않아 선택이 안 바뀌던 문제: $page.url 반응형 구독으로 해결.
	// (2) 퀘스트/캠페인 링크를 따라갔다 뒤로가기 — 선택이 컴포넌트 state 뿐이라
	//     첫 규칙으로 초기화되던 문제: 선택 시 URL 에 기록해 해결
	//     (복귀 시 history 의 ?slug= 를 onMount 가 읽음).
	// BUG-126 후속: slug 가 사라진 경우(뒤로가기로 slug 없는 최초 진입 지점에
	// 복귀 / nav 의 "Rules" 링크를 같은 라우트에서 다시 클릭)도 첫 규칙으로
	// 리셋 — 이전엔 무시해서 selectedSlug 가 직전 값에 멈춰있었다.
	$effect(() => {
		const slug = $page.url.searchParams.get('slug');
		if (slug && slug !== selectedSlug && entries.some((e) => e.slug === slug)) {
			select(slug);
		} else if (!slug && entries.length > 0 && selectedSlug !== entries[0].slug) {
			selectedSlug = entries[0].slug;
			refreshSelectedContent();
		}
	});

	function syncUrl(slug: string) {
		const cur = new URLSearchParams(window.location.search).get('slug');
		if (cur === slug) return;
		// BUG-120: pushState(기본) — 규칙 선택마다 새 history entry 를 쌓아
		// 브라우저 뒤로가기로 이전에 본 규칙을 한 단계씩 되짚어갈 수 있게 한다.
		goto(`/rules?slug=${encodeURIComponent(slug)}`, {
			keepFocus: true,
			noScroll: true
		});
	}

	// DEV-119: 편집중 다른 slug 선택 시 native confirm 대신 인앱 모달.
	let confirmDiscardSlug = $state<string | null>(null);

	function select(slug: string) {
		if (editMode) {
			confirmDiscardSlug = slug;
			return;
		}
		selectedSlug = slug;
		refreshSelectedContent();
		syncUrl(slug);
	}

	function applyPendingSelect() {
		const slug = confirmDiscardSlug;
		confirmDiscardSlug = null;
		if (!slug) return;
		cancelEdit();
		selectedSlug = slug;
		refreshSelectedContent();
		syncUrl(slug); // BUG-104
	}

	// ─── 편집 ───
	async function enterEdit() {
		if (!selectedSlug) return;
		editText = selectedContent ?? '';
		editMode = true;
		saveError = null;
		await tick();
		initEditor();
	}

	function initEditor() {
		if (!editorContainer) return;
		if (editorView) {
			editorView.destroy();
			editorView = null;
		}
		editorContainer.style.height = `${loadEditorHeight()}px`;
		editorView = new EditorView({
			doc: editText,
			extensions: [
				basicSetup,
				markdown(),
				// 테마 — Compartment 로 다크/라이트 라이브 전환.
				editorThemeCompartment.of(editorThemeExtension($theme)),
				// DEV-069: 첨부 — 클립보드 paste / 파일 drag&drop.
				attachmentExtension((msg) => (saveError = `첨부 업로드 실패: ${msg}`), undefined, {
					mediaOnly: true
				}),
				// DEV-140 후속: XXX-NNN 타이핑 → [[...]] cross-link 자동완성.
				crossLinkAutocomplete(),
				// DEV-130: Tab = 들여쓰기 (focus 이동 X) — 설정대로 커서 위치에 삽입.
				indentExtensions($editorSettings),
				EditorView.theme({
					'&': { fontSize: '0.875rem', borderRadius: '6px', height: '100%' },
					'.cm-editor': { borderRadius: '6px', height: '100%' },
					'.cm-scroller': { overflow: 'auto' }
				})
			],
			parent: editorContainer
		});
	}

	// 테마 변경 시 재생성 없이 테마 확장만 교체 (커서/스크롤/undo 보존).
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

	async function save() {
		if (!selectedSlug) return;
		saving = true;
		saveError = null;
		try {
			const text = editorView ? editorView.state.doc.toString() : editText;
			const res = await rulesApi.set(selectedSlug, text);
			selectedContent = res.content;
			// 메모리 목록도 갱신 — 페이지 reload 안 해도 sidebar 정합.
			entries = entries.map((e) => (e.slug === selectedSlug ? { ...e, content: text } : e));
			// DEV-173 후속: 제목(첫 # 헤딩)이 바뀌었을 수 있음 — 인덱스 재적재.
			loadQuestIndex(true);
			cancelEdit();
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'save failed';
		} finally {
			saving = false;
		}
	}

	// ─── 신규 ───
	function openCreate() {
		creating = true;
		createSlug = '';
		createError = null;
	}
	function cancelCreate() {
		creating = false;
		createError = null;
	}
	async function submitCreate() {
		const slug = createSlug.trim();
		if (!slug) {
			createError = 'slug 를 입력하세요.';
			return;
		}
		try {
			await rulesApi.create(slug, '');
			creating = false;
			await loadList(slug); // 새로 만든 것 자동 선택.
		} catch (e) {
			createError = e instanceof Error ? e.message : '생성 실패';
		}
	}

	// ─── 삭제 (DEV-118: 인앱 확인 모달) ───
	let confirmDeleteSlug = $state<string | null>(null);
	function askDeleteSelected() {
		if (!selectedSlug) return;
		confirmDeleteSlug = selectedSlug;
	}
	async function deleteSelected() {
		const target = confirmDeleteSlug;
		confirmDeleteSlug = null;
		if (!target) return;
		try {
			await rulesApi.delete(target);
			if (selectedSlug === target) {
				selectedSlug = null;
				selectedContent = null;
			}
			await loadList();
		} catch (e) {
			alert(e instanceof Error ? e.message : '삭제 실패');
		}
	}

	// ─── 이름변경 ───
	function openRename() {
		if (!selectedSlug) return;
		renaming = true;
		renameSlug = selectedSlug;
		renameError = null;
	}
	function cancelRename() {
		renaming = false;
		renameError = null;
	}
	async function submitRename() {
		if (!selectedSlug) return;
		const newSlug = renameSlug.trim();
		if (!newSlug || newSlug === selectedSlug) {
			cancelRename();
			return;
		}
		try {
			await rulesApi.rename(selectedSlug, newSlug);
			renaming = false;
			await loadList(newSlug);
		} catch (e) {
			renameError = e instanceof Error ? e.message : '이름 변경 실패';
		}
	}
</script>

<div class="page">
	{#if loading}
		<div class="state">Loading…</div>
	{:else if error}
		<div class="state err">{error}</div>
	{:else}
		<div class="layout">
			<!-- 좌측 sidebar -->
			<aside class="sidebar">
				<div class="sidebar-head">
					<h2>규칙 목록</h2>
					<button class="btn-new" onclick={openCreate} title="신규 규칙">+ 신규</button>
				</div>
				{#if allTagOptions.length > 0}
					<div class="tag-filter-row" aria-label="태그 필터">
						{#each allTagOptions as t (t)}
							<button
								class="tag-filter-chip"
								class:active={filterTags.has(t)}
								onclick={() => toggleTagFilter(t)}
								title={filterTags.has(t) ? `${t} 필터 해제` : `${t} 필터 추가`}
							>
								{t}
								<span class="tag-chip-count">{tagCounts.get(t) ?? 0}</span>
							</button>
						{/each}
						{#if filterTags.size > 0}
							<button
								class="tag-clear"
								onclick={() => (filterTags = new Set())}
								title="태그 필터 모두 해제"
							>
								× 전체 해제
							</button>
						{/if}
					</div>
				{/if}
				{#if entries.length === 0}
					<p class="empty-list">규칙 없음. "+ 신규" 로 만들기.</p>
				{:else if filteredEntries.length === 0}
					<p class="empty-list">태그 필터에 맞는 규칙 없음.</p>
				{:else}
					<ul class="rule-list">
						{#each filteredEntries as e (e.slug)}
							<li>
								<button
									class="rule-item"
									class:active={e.slug === selectedSlug}
									onclick={() => select(e.slug)}
								>
									{e.slug}
								</button>
							</li>
						{/each}
					</ul>
				{/if}

				{#if creating}
					<div class="modal-inline">
						<input
							class="text-input"
							type="text"
							placeholder="slug (예: release-process)"
							bind:value={createSlug}
							onkeydown={(e) => e.key === 'Enter' && submitCreate()}
						/>
						{#if createError}<p class="err">{createError}</p>{/if}
						<div class="actions">
							<button class="btn-save" onclick={submitCreate}>생성</button>
							<button class="btn-cancel" onclick={cancelCreate}>취소</button>
						</div>
					</div>
				{/if}
			</aside>

			<!-- 우측 panel -->
			<section class="panel">
				{#if !selectedSlug}
					<div class="empty">
						{#if entries.length === 0}
							"+ 신규" 로 첫 규칙을 만드세요.
						{:else}
							좌측에서 규칙을 선택하세요.
						{/if}
					</div>
				{:else}
					<div class="top-bar">
						<h1 class="slug-title"># {selectedSlug}</h1>
						{#if !editMode}
							<div class="top-actions">
								<button class="btn-edit" onclick={enterEdit}>
									{selectedContent && selectedContent.trim() ? '✎ 편집' : '+ 작성'}
								</button>
								<button class="btn-edit" onclick={openRename}>이름 변경</button>
								<button class="btn-edit danger" onclick={askDeleteSelected}>삭제</button>
							</div>
						{/if}
					</div>

					<!-- DEV-182: 생성 / 변경 시각. -->
					{#if selectedEntry}
						<div class="meta-times">
							<span class="meta-item">
								<span class="meta-label">생성</span>
								<time
									class="meta-val"
									datetime={selectedEntry.created_at}
									title={formatTs(selectedEntry.created_at)}
								>
									{formatTs(selectedEntry.created_at)}
								</time>
							</span>
							<span class="meta-sep">·</span>
							<span class="meta-item">
								<span class="meta-label">변경</span>
								<time
									class="meta-val"
									datetime={selectedEntry.updated_at}
									title={formatTs(selectedEntry.updated_at)}
								>
									{formatRelative(selectedEntry.updated_at)}
								</time>
							</span>
						</div>
					{/if}

					{#if renaming}
						<div class="modal-inline">
							<input
								class="text-input"
								type="text"
								placeholder="새 slug"
								bind:value={renameSlug}
								onkeydown={(e) => e.key === 'Enter' && submitRename()}
							/>
							{#if renameError}<p class="err">{renameError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitRename}>변경</button>
								<button class="btn-cancel" onclick={cancelRename}>취소</button>
							</div>
						</div>
					{/if}

					{#if editMode}
						<div class="edit-form">
							<div class="field-label">
								<span>본문 (Markdown) — 첨부는 드래그&드랍 / Ctrl+V</span>
								<div class="editor-wrap" bind:this={editorContainer}></div>
							</div>
							<div class="actions">
								<button class="btn-save" onclick={save} disabled={saving}>
									{saving ? '저장…' : '저장'}
								</button>
								<button class="btn-cancel" onclick={cancelEdit} disabled={saving}> 취소 </button>
							</div>
							{#if saveError}<p class="err">{saveError}</p>{/if}
						</div>
					{:else if selectedContent && selectedContent.trim()}
						<MarkdownView source={selectedContent} />
					{:else}
						<div class="empty">
							아직 작성된 본문이 없습니다.
							<button class="link" onclick={enterEdit}>지금 작성</button>
						</div>
					{/if}
					{#if !editMode}
						<!-- DEV-243: 태그. -->
						<TagPills tags={selectedTags} {tagDefs} onSetTags={setRuleTags} />
					{/if}
				{/if}
			</section>
		</div>
	{/if}
</div>

<!-- DEV-118: 규칙 삭제 확인 모달. -->
<ConfirmDialog
	open={confirmDeleteSlug !== null}
	title="규칙 삭제"
	message={`'${confirmDeleteSlug ?? ''}' 규칙을 삭제할까요?`}
	confirmLabel="삭제"
	danger
	onconfirm={deleteSelected}
	oncancel={() => (confirmDeleteSlug = null)}
/>

<!-- DEV-119: 편집중 다른 slug 선택 시 미저장 확인 모달. -->
<ConfirmDialog
	open={confirmDiscardSlug !== null}
	title="편집중 이동"
	message="편집 중인 변경 사항이 있습니다. 버리고 이동할까요?"
	confirmLabel="버리고 이동"
	danger
	onconfirm={applyPendingSelect}
	oncancel={() => (confirmDiscardSlug = null)}
/>

<style>
	.page {
		padding: 1.25rem 1.5rem 2rem;
		max-width: var(--content-max-width, 1200px);
		margin: 0 auto;
	}
	.layout {
		display: grid;
		grid-template-columns: 240px 1fr;
		gap: 1.25rem;
		min-height: 70vh;
	}
	.sidebar {
		border-right: 1px solid var(--bg-subtle);
		padding-right: 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.sidebar-head {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
	.sidebar-head h2 {
		font-size: 0.8rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-muted);
		margin: 0;
	}
	.btn-new {
		margin-left: auto;
		padding: 0.15rem 0.55rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-muted);
		font-size: 0.72rem;
		cursor: pointer;
	}
	.btn-new:hover {
		background: var(--bg-subtle);
		color: var(--text);
	}
	.empty-list {
		color: var(--text-muted);
		font-size: 0.78rem;
		padding: 0.5rem 0;
	}
	.rule-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	/* DEV-243 후속: 태그 필터 chip — quest QuestList.svelte 와 동일 패턴. */
	.tag-filter-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		align-items: center;
		margin-bottom: 0.4rem;
	}
	.tag-filter-chip {
		padding: 0.15rem 0.65rem;
		background: color-mix(in srgb, var(--warning) 8%, transparent);
		border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
		border-radius: 20px;
		color: var(--warning);
		font-size: 0.72rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		cursor: pointer;
		transition:
			background 0.1s,
			border-color 0.1s;
	}
	.tag-filter-chip:hover {
		background: color-mix(in srgb, var(--warning) 18%, transparent);
	}
	.tag-filter-chip.active {
		background: color-mix(in srgb, var(--warning) 28%, transparent);
		border-color: color-mix(in srgb, var(--warning) 70%, transparent);
		color: color-mix(in srgb, var(--warning) 60%, white);
	}
	.tag-chip-count {
		display: inline-block;
		margin-left: 0.4rem;
		padding: 0 0.4rem;
		min-width: 1.1rem;
		text-align: center;
		font-size: 0.65rem;
		color: var(--text-muted);
		background: var(--bg-subtle);
		border-radius: 10px;
	}
	.tag-clear {
		padding: 0.15rem 0.55rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 20px;
		color: var(--text-muted);
		font-size: 0.7rem;
		cursor: pointer;
	}
	.tag-clear:hover {
		background: var(--bg-subtle);
		color: var(--text);
	}
	.rule-item {
		width: 100%;
		text-align: left;
		padding: 0.35rem 0.5rem;
		background: transparent;
		border: none;
		border-radius: 4px;
		color: var(--text);
		font-size: 0.85rem;
		cursor: pointer;
	}
	.rule-item:hover {
		background: var(--bg-elevated);
	}
	.rule-item.active {
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent-secondary);
	}

	.panel {
		min-width: 0;
	}

	.top-bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}
	.slug-title {
		font-size: 1.1rem;
		font-weight: 600;
		color: var(--text);
		margin: 0;
	}
	/* DEV-182: 생성/변경 시각 — quest 상세 페이지와 동일 스타일. */
	.meta-times {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.4rem;
		font-size: 0.72rem;
		color: var(--text-faint);
		margin-bottom: 0.85rem;
	}
	.meta-item {
		display: inline-flex;
		gap: 0.3rem;
		align-items: baseline;
	}
	.meta-label {
		color: var(--text-faint);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.meta-val {
		color: var(--text-muted);
		font-variant-numeric: tabular-nums;
	}
	.meta-sep {
		color: var(--border);
	}
	.top-actions {
		margin-left: auto;
		display: flex;
		gap: 0.4rem;
	}
	.btn-edit {
		padding: 0.3rem 0.7rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.825rem;
		cursor: pointer;
	}
	.btn-edit:hover {
		background: var(--bg-subtle);
	}
	.btn-edit.danger {
		color: var(--danger);
		border-color: color-mix(in srgb, var(--danger) 45%, transparent);
	}
	.btn-edit.danger:hover {
		background: color-mix(in srgb, var(--danger) 18%, transparent);
	}

	.modal-inline {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		padding: 0.6rem;
		background: var(--bg);
		border: 1px dashed var(--border);
		border-radius: 6px;
		margin: 0.5rem 0;
	}
	.text-input {
		padding: 0.35rem 0.55rem;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 4px;
		font-size: 0.85rem;
	}

	.state {
		color: var(--text-muted);
		padding: 1rem 0;
		font-size: 0.875rem;
	}
	.state.err {
		color: var(--danger);
	}
	.err {
		color: var(--danger);
		font-size: 0.825rem;
		margin: 0.25rem 0 0;
	}
	.empty {
		color: var(--text-muted);
		font-size: 0.9rem;
		padding: 2rem 0;
		text-align: center;
	}
	.empty .link {
		background: none;
		border: none;
		color: var(--accent);
		cursor: pointer;
		font: inherit;
		text-decoration: underline;
	}

	.edit-form {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.field-label {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.field-label > span {
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.editor-wrap {
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
		min-height: 200px;
		max-height: 90vh;
		resize: vertical;
	}

	.actions {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
	.btn-save {
		padding: 0.35rem 0.85rem;
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		color: var(--btn-primary-text);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.btn-save:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.btn-save:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
	.btn-cancel {
		padding: 0.35rem 0.85rem;
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.btn-cancel:hover:not(:disabled) {
		background: var(--bg-subtle);
	}
</style>
