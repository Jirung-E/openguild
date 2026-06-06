<!--
  DEV-012: Quest Detail 의 메모 섹션 (개인용, gitignored).

  파일: .guild/quests/{slug}.memo.md — 본인만 보는 단일 markdown.
  - 부재 / 비어있음: "메모 작성" 안내.
  - 읽기: MarkdownView.
  - 편집: CodeMirror (markdown + oneDark). Quest Detail editor height 키 공유.

  DEV-094 이후 댓글 (공개) 은 entry 단위 — `QuestCommentsSection` 별도.
  본 컴포넌트는 메모 (개인 노트) 전용 — entry 단위 불필요한 freeform 텍스트.

  `mode` prop 은 호환을 위해 받지만 무시 — 항상 memo 동작.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import MarkdownView from './MarkdownView.svelte';
	import { commentsApi } from '$lib/api/comments';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	import { oneDark } from '@codemirror/theme-one-dark';

	// `mode` prop 은 호환성을 위해 받지만 동작 분기 X — 항상 memo.
	// svelte 가 "초기값만 캡쳐" 경고 안 내도록 destructure 에서 제외.
	let { slug }: { slug: string; mode?: 'memo' | 'comments' } = $props();

	const label = {
		heading: '메모 (Memo)',
		emptyAction: '메모 작성',
		emptyHint: '개인 메모. gitignored (팀 공유 X).',
		help: '본인만 보는 비공개 메모 (`.guild/quests/{slug}.memo.md`, gitignored).'
	};

	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let content = $state<string | null>(null);

	let editMode = $state(false);
	let editText = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);

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
		return 360;
	}

	async function load() {
		loading = true;
		loadError = null;
		try {
			const res = await commentsApi.getMemo(slug);
			content = res.content;
		} catch (e) {
			loadError = e instanceof Error ? e.message : 'load failed';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void slug;
		load();
	});

	async function enterEdit() {
		editText = content ?? '';
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
		saving = true;
		saveError = null;
		try {
			const text = editorView ? editorView.state.doc.toString() : editText;
			const res = await commentsApi.setMemo(slug, text);
			content = res.content;
			cancelEdit();
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'save failed';
		} finally {
			saving = false;
		}
	}

	onMount(() => {
		return () => editorView?.destroy();
	});
</script>

<section class="note-sec">
	<div class="section-head">
		<h2 class="section-title note-memo">{label.heading}</h2>
		{#if !editMode && !loading && !loadError}
			<button class="sec-add-btn" onclick={enterEdit}>
				{content && content.trim() ? '✎ 편집' : `+ ${label.emptyAction}`}
			</button>
		{/if}
	</div>

	{#if loading}
		<p class="state">Loading…</p>
	{:else if loadError}
		<p class="state err">{loadError}</p>
	{:else if editMode}
		<!-- svelte-ignore a11y_label_has_associated_control -->
		<label class="field-label">
			<span>{label.help}</span>
			<div class="editor-wrap" bind:this={editorContainer}></div>
		</label>
		<div class="actions">
			<button class="btn-save" onclick={save} disabled={saving}>
				{saving ? '저장…' : '저장'}
			</button>
			<button class="btn-cancel" onclick={cancelEdit} disabled={saving}>취소</button>
		</div>
		{#if saveError}<p class="state err">{saveError}</p>{/if}
	{:else if content && content.trim()}
		<MarkdownView source={content} />
	{:else}
		<p class="no-desc">
			{label.emptyHint}
			<button class="link-btn" onclick={enterEdit}>{label.emptyAction}</button>
		</p>
	{/if}
</section>

<style>
	.note-sec { margin-bottom: 1.5rem; }
	.section-head {
		display: flex; align-items: center; gap: 0.75rem;
		margin-bottom: 0.5rem;
	}
	.section-title {
		font-size: 0.8rem; font-weight: 600;
		text-transform: uppercase; letter-spacing: 0.05em; margin: 0;
	}
	.section-title.note-memo { color: #f0883e; }

	.sec-add-btn {
		padding: 0.15rem 0.6rem;
		border: 1px solid var(--border); border-radius: 4px;
		background: transparent; color: var(--text-muted);
		font-size: 0.72rem; cursor: pointer;
		margin-left: auto;
	}
	.sec-add-btn:hover { background: var(--bg-subtle); color: var(--text); }

	.state { color: var(--text-muted); font-size: 0.825rem; margin: 0.25rem 0; }
	.state.err { color: var(--danger); }

	.no-desc { color: var(--text-faint); font-size: 0.825rem; margin: 0.25rem 0; }
	.link-btn {
		background: none; border: none; color: var(--accent);
		cursor: pointer; padding: 0; font: inherit; text-decoration: underline;
		margin-left: 0.35rem;
	}

	.field-label { display: flex; flex-direction: column; gap: 0.35rem; }
	.field-label > span { font-size: 0.75rem; color: var(--text-muted); }
	.editor-wrap {
		border: 1px solid var(--border); border-radius: 6px;
		overflow: hidden; min-height: 180px; max-height: 90vh;
		resize: vertical;
	}

	.actions { display: flex; gap: 0.4rem; margin-top: 0.5rem; }
	.btn-save {
		padding: 0.35rem 0.85rem;
		background: var(--success-strong); border: 1px solid var(--success-strong);
		color: #fff; border-radius: 6px; cursor: pointer; font-size: 0.825rem;
	}
	.btn-save:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-save:hover:not(:disabled) { background: var(--success-strong); }
	.btn-cancel {
		padding: 0.35rem 0.85rem;
		background: transparent; border: 1px solid var(--border);
		color: var(--text); border-radius: 6px; cursor: pointer; font-size: 0.825rem;
	}
	.btn-cancel:hover:not(:disabled) { background: var(--bg-subtle); }
</style>
