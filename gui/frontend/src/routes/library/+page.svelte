<!--
  DEV-217: 도서관 페이지 — `.guild/library/{BOOK-NNN}.md` 목록/편집.

  rules 페이지 패턴 재사용:
  - 좌측 sidebar: 문서 목록(BOOK-NNN + 제목) + 신규 버튼.
  - 우측 panel: 선택된 문서의 markdown view + CodeMirror 편집 + 제목 변경/삭제.

  rules 와의 차이: 식별자가 slug 가 아니라 자동 부여 BOOK 번호 — 신규 모달은
  제목만 입력, "이름 변경" 대신 "제목 변경"(번호는 불변). 딥링크는 ?id=BOOK-NNN
  (cross-link 대상 — DEV-218).
-->
<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { setUnsaved } from '$lib/stores/unsaved';
	import { libraryApi, type Book } from '$lib/api/library';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	import { indentExtensions } from '$lib/utils/editor-indent';
	import { editorSettings } from '$lib/stores/editorSettings';
	// DEV-097 대체 결정: 도서관 문서가 공유 자료의 보금자리 — 첨부 지원 필수.
	import { attachmentExtension } from '$lib/utils/editor-attach';
	import { crossLinkAutocomplete } from '$lib/utils/editor-links';
	import { loadQuestIndex } from '$lib/stores/questIndex';

	let loading = $state(true);
	let error = $state<string | null>(null);
	let books = $state<Book[]>([]);
	let selectedId = $state<string | null>(null);

	const selected = $derived(books.find((b) => b.book_id === selectedId) ?? null);

	let editMode = $state(false);
	$effect(() => setUnsaved('library-edit', editMode));
	onDestroy(() => setUnsaved('library-edit', false));
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	// 신규(제목 입력) / 제목 변경 모달.
	let creating = $state(false);
	let createTitle = $state('');
	let createError = $state<string | null>(null);

	let retitling = $state(false);
	let retitleText = $state('');
	let retitleError = $state<string | null>(null);

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

	async function loadList(preferId?: string | null) {
		loading = true;
		error = null;
		try {
			books = await libraryApi.list();
			if (preferId && books.some((b) => b.book_id === preferId)) {
				selectedId = preferId;
			} else if (selectedId == null || !books.some((b) => b.book_id === selectedId)) {
				selectedId = books[0]?.book_id ?? null;
			}
			// 목록 변동(생성/삭제/제목변경) → cross-link 인덱스 재적재 (DEV-218 대비,
			// rules 페이지의 DEV-173 후속과 동일 이유).
			loadQuestIndex(true);
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		// 딥링크 — `/library?id=BOOK-NNN` 진입 시 해당 문서 선택.
		const idParam = new URLSearchParams(window.location.search).get('id');
		loadList(idParam);
	});

	// BUG-104 와 동일: URL(?id=) 을 선택 상태의 진리원으로 — 문서간 [[링크]] 이동
	// + 뒤로가기 복원.
	$effect(() => {
		const id = $page.url.searchParams.get('id');
		if (id && id !== selectedId && books.some((b) => b.book_id === id)) {
			select(id);
		}
	});

	function syncUrl(id: string) {
		const cur = new URLSearchParams(window.location.search).get('id');
		if (cur === id) return;
		goto(`/library?id=${encodeURIComponent(id)}`, {
			replaceState: true,
			keepFocus: true,
			noScroll: true
		});
	}

	let confirmDiscardId = $state<string | null>(null);

	function select(id: string) {
		if (editMode) {
			confirmDiscardId = id;
			return;
		}
		selectedId = id;
		syncUrl(id);
	}

	function applyPendingSelect() {
		const id = confirmDiscardId;
		confirmDiscardId = null;
		if (!id) return;
		cancelEdit();
		selectedId = id;
		syncUrl(id);
	}

	// ─── 편집 ───
	async function enterEdit() {
		if (!selected) return;
		editMode = true;
		saveError = null;
		await tick();
		initEditor(selected.body);
	}

	function initEditor(doc: string) {
		if (!editorContainer) return;
		if (editorView) {
			editorView.destroy();
			editorView = null;
		}
		editorContainer.style.height = `${loadEditorHeight()}px`;
		editorView = new EditorView({
			doc,
			extensions: [
				basicSetup,
				markdown(),
				editorThemeCompartment.of(editorThemeExtension($theme)),
				attachmentExtension((msg) => (saveError = `첨부 업로드 실패: ${msg}`), undefined, {
					mediaOnly: true
				}),
				crossLinkAutocomplete(),
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
		if (!selectedId) return;
		saving = true;
		saveError = null;
		try {
			const text = editorView ? editorView.state.doc.toString() : '';
			const updated = await libraryApi.update(selectedId, { body: text });
			books = books.map((b) => (b.book_id === selectedId ? updated : b));
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
		createTitle = '';
		createError = null;
	}
	function cancelCreate() {
		creating = false;
		createError = null;
	}
	async function submitCreate() {
		const title = createTitle.trim();
		if (!title) {
			createError = '제목을 입력하세요.';
			return;
		}
		try {
			const created = await libraryApi.create(title, '');
			creating = false;
			await loadList(created.book_id);
		} catch (e) {
			createError = e instanceof Error ? e.message : '생성 실패';
		}
	}

	// ─── 삭제 ───
	let confirmDeleteId = $state<string | null>(null);
	function askDeleteSelected() {
		if (!selectedId) return;
		confirmDeleteId = selectedId;
	}
	async function deleteSelected() {
		const target = confirmDeleteId;
		confirmDeleteId = null;
		if (!target) return;
		try {
			await libraryApi.delete(target);
			if (selectedId === target) selectedId = null;
			await loadList();
		} catch (e) {
			alert(e instanceof Error ? e.message : '삭제 실패');
		}
	}

	// ─── 제목 변경 ───
	function openRetitle() {
		if (!selected) return;
		retitling = true;
		retitleText = selected.title;
		retitleError = null;
	}
	function cancelRetitle() {
		retitling = false;
		retitleError = null;
	}
	async function submitRetitle() {
		if (!selectedId) return;
		const title = retitleText.trim();
		if (!title || title === selected?.title) {
			cancelRetitle();
			return;
		}
		try {
			const updated = await libraryApi.update(selectedId, { title });
			books = books.map((b) => (b.book_id === selectedId ? updated : b));
			retitling = false;
			loadQuestIndex(true);
		} catch (e) {
			retitleError = e instanceof Error ? e.message : '제목 변경 실패';
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
					<h2>도서관</h2>
					<button class="btn-new" onclick={openCreate} title="신규 문서">+ 신규</button>
				</div>
				{#if books.length === 0}
					<p class="empty-list">문서 없음. "+ 신규" 로 만들기.</p>
				{:else}
					<ul class="book-list">
						{#each books as b (b.book_id)}
							<li>
								<button
									class="book-item"
									class:active={b.book_id === selectedId}
									onclick={() => select(b.book_id)}
								>
									<span class="book-id">{b.book_id}</span>
									<span class="book-title">{b.title}</span>
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
							placeholder="문서 제목"
							bind:value={createTitle}
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
				{#if !selected}
					<div class="empty">
						{#if books.length === 0}
							"+ 신규" 로 첫 문서를 만드세요.
						{:else}
							좌측에서 문서를 선택하세요.
						{/if}
					</div>
				{:else}
					<div class="top-bar">
						<h1 class="doc-title">
							<span class="doc-id">{selected.book_id}</span>
							{selected.title}
						</h1>
						{#if !editMode}
							<div class="top-actions">
								<button class="btn-edit" onclick={enterEdit}>
									{selected.body.trim() ? '✎ 편집' : '+ 작성'}
								</button>
								<button class="btn-edit" onclick={openRetitle}>제목 변경</button>
								<button class="btn-edit danger" onclick={askDeleteSelected}>삭제</button>
							</div>
						{/if}
					</div>

					{#if retitling}
						<div class="modal-inline">
							<input
								class="text-input"
								type="text"
								placeholder="새 제목"
								bind:value={retitleText}
								onkeydown={(e) => e.key === 'Enter' && submitRetitle()}
							/>
							{#if retitleError}<p class="err">{retitleError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitRetitle}>변경</button>
								<button class="btn-cancel" onclick={cancelRetitle}>취소</button>
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
					{:else if selected.body.trim()}
						<MarkdownView source={selected.body} />
					{:else}
						<div class="empty">
							아직 작성된 본문이 없습니다.
							<button class="link" onclick={enterEdit}>지금 작성</button>
						</div>
					{/if}
				{/if}
			</section>
		</div>
	{/if}
</div>

<ConfirmDialog
	open={confirmDeleteId !== null}
	title="문서 삭제"
	message={`'${confirmDeleteId ?? ''}' 문서를 삭제할까요? (번호는 재사용되지 않습니다)`}
	confirmLabel="삭제"
	danger
	onconfirm={deleteSelected}
	oncancel={() => (confirmDeleteId = null)}
/>

<ConfirmDialog
	open={confirmDiscardId !== null}
	title="편집중 이동"
	message="편집 중인 변경 사항이 있습니다. 버리고 이동할까요?"
	confirmLabel="버리고 이동"
	danger
	onconfirm={applyPendingSelect}
	oncancel={() => (confirmDiscardId = null)}
/>

<style>
	.page {
		padding: 1.25rem 1.5rem 2rem;
		max-width: var(--content-max-width, 1200px);
		margin: 0 auto;
	}
	.layout {
		display: grid;
		grid-template-columns: 260px 1fr;
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
	.book-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}
	.book-item {
		width: 100%;
		text-align: left;
		padding: 0.35rem 0.5rem;
		background: transparent;
		border: none;
		border-radius: 4px;
		color: var(--text);
		font-size: 0.85rem;
		cursor: pointer;
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
	}
	.book-item:hover {
		background: var(--bg-elevated);
	}
	.book-item.active {
		background: color-mix(in srgb, var(--accent) 12%, transparent);
	}
	.book-id {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.72rem;
		color: var(--accent);
	}
	.book-title {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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
	.doc-title {
		font-size: 1.1rem;
		font-weight: 600;
		color: var(--text);
		margin: 0;
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		min-width: 0;
	}
	.doc-id {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.85rem;
		color: var(--accent);
		flex: none;
	}
	.top-actions {
		margin-left: auto;
		display: flex;
		gap: 0.4rem;
		flex: none;
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
