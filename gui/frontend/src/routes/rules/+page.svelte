<!--
  DEV-016 (multi-file): 길드 규칙 페이지 — 다중 파일 (`.guild/rules/{slug}.md`).

  레이아웃:
  - 좌측 sidebar: rule slug 목록 + 신규 / 삭제 / 이름변경 버튼.
  - 우측 panel: 선택된 rule 의 markdown view + CodeMirror 편집.

  legacy 단일 `.guild/rules.md` 은 첫 list 호출 시 자동으로
  `.guild/rules/general.md` 로 마이그레이션됨.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { rulesApi, type RuleEntry } from '$lib/api/rules';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	import { oneDark } from '@codemirror/theme-one-dark';

	let loading = $state(true);
	let error = $state<string | null>(null);
	let entries = $state<RuleEntry[]>([]);
	let selectedSlug = $state<string | null>(null);
	let selectedContent = $state<string | null>(null);

	let editMode = $state(false);
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
			} else if (
				selectedSlug == null ||
				!entries.some((e) => e.slug === selectedSlug)
			) {
				selectedSlug = entries[0]?.slug ?? null;
			}
			refreshSelectedContent();
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

	onMount(() => {
		loadList();
	});

	function select(slug: string) {
		if (editMode) {
			if (!confirm('편집 중인 변경 사항이 있습니다. 버리고 이동할까요?')) return;
			cancelEdit();
		}
		selectedSlug = slug;
		refreshSelectedContent();
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
				oneDark,
				EditorView.theme({
					'&': { fontSize: '0.875rem', borderRadius: '6px', height: '100%' },
					'.cm-editor': { borderRadius: '6px', height: '100%' },
					'.cm-scroller': { overflow: 'auto' }
				})
			],
			parent: editorContainer
		});
	}

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
			entries = entries.map((e) =>
				e.slug === selectedSlug ? { ...e, content: text } : e
			);
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

	// ─── 삭제 ───
	async function deleteSelected() {
		if (!selectedSlug) return;
		if (!confirm(`'${selectedSlug}' 규칙을 삭제할까요?`)) return;
		try {
			await rulesApi.delete(selectedSlug);
			selectedSlug = null;
			selectedContent = null;
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
				{#if entries.length === 0}
					<p class="empty-list">규칙 없음. "+ 신규" 로 만들기.</p>
				{:else}
					<ul class="rule-list">
						{#each entries as e (e.slug)}
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
								<button class="btn-edit danger" onclick={deleteSelected}>삭제</button>
							</div>
						{/if}
					</div>

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
							<!-- svelte-ignore a11y_label_has_associated_control -->
							<label class="field-label">
								<span>본문 (Markdown)</span>
								<div class="editor-wrap" bind:this={editorContainer}></div>
							</label>
							<div class="actions">
								<button class="btn-save" onclick={save} disabled={saving}>
									{saving ? '저장…' : '저장'}
								</button>
								<button class="btn-cancel" onclick={cancelEdit} disabled={saving}>
									취소
								</button>
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
				{/if}
			</section>
		</div>
	{/if}
</div>

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
		color: #79c0ff;
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
		color: #f85149;
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
		color: #f85149;
	}
	.err {
		color: #f85149;
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
		background: var(--success-strong);
		border: 1px solid #2ea043;
		color: #fff;
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.btn-save:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.btn-save:hover:not(:disabled) {
		background: var(--success);
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
