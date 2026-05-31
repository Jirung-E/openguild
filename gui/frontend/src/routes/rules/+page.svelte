<!--
  DEV-016: 길드 규칙 (Guild Rules) 페이지.

  팀 컨벤션 / 그라운드 룰 / 브랜치 네이밍 / 커밋 메시지 형식 / 코드 리뷰 기준 등
  길드별 자유 markdown 문서. 파일: `.guild/rules.md` (frontmatter 없음).

  - 파일 없으면 "아직 규칙 없음 + [작성하기]" 안내.
  - 읽기: MarkdownView (Quest / Campaign 본문과 동일 톤).
  - 편집: CodeMirror (markdown + oneDark) — 다른 페이지와 동일 패턴.
-->
<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { rulesApi } from '$lib/api/rules';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	import { oneDark } from '@codemirror/theme-one-dark';

	let loading = $state(true);
	let error = $state<string | null>(null);
	let content = $state<string | null>(null);

	let editMode = $state(false);
	let editText = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	let editorContainer: HTMLDivElement | undefined = $state(undefined);
	let editorView: EditorView | null = null;

	// Quest Detail 과 동일 키 — 사용자가 한 번 조정한 높이가 모든 markdown editor 에 적용.
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

	onMount(async () => {
		try {
			const res = await rulesApi.get();
			content = res.content;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
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
			const res = await rulesApi.set(text);
			content = res.content;
			cancelEdit();
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'save failed';
		} finally {
			saving = false;
		}
	}
</script>

<div class="page">
	<div class="top-bar">
		<h1>길드 규칙</h1>
		{#if !editMode && !loading && !error}
			<button class="btn-edit" onclick={enterEdit}>
				{content && content.trim() ? '✎ 편집' : '+ 작성'}
			</button>
		{/if}
	</div>

	{#if loading}
		<div class="state">Loading…</div>
	{:else if error}
		<div class="state err">{error}</div>
	{:else if editMode}
		<div class="edit-form">
			<!-- CodeMirror 가 div 내부에 동적으로 textarea 생성 — svelte 정적
			     분석으로는 label 의 associated control 미확인. ignore. -->
			<!-- svelte-ignore a11y_label_has_associated_control -->
			<label class="field-label">
				<span>본문 (Markdown)</span>
				<div class="editor-wrap" bind:this={editorContainer}></div>
			</label>
			<div class="actions">
				<button class="btn-save" onclick={save} disabled={saving}>
					{saving ? '저장…' : '저장'}
				</button>
				<button class="btn-cancel" onclick={cancelEdit} disabled={saving}>취소</button>
			</div>
			{#if saveError}<p class="err">{saveError}</p>{/if}
		</div>
	{:else if content && content.trim()}
		<MarkdownView source={content} />
	{:else}
		<div class="empty">
			아직 작성된 규칙이 없습니다. <button class="link" onclick={enterEdit}>지금 작성</button>
		</div>
	{/if}
</div>

<style>
	.page {
		padding: 1.25rem 1.5rem 2rem;
		max-width: 900px;
		margin: 0 auto;
	}
	.top-bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}
	.top-bar h1 {
		font-size: 1.1rem;
		font-weight: 600;
		color: #c9d1d9;
		margin: 0;
	}
	.btn-edit {
		margin-left: auto;
		padding: 0.3rem 0.7rem;
		background: transparent;
		border: 1px solid #30363d;
		border-radius: 6px;
		color: #c9d1d9;
		font-size: 0.825rem;
		cursor: pointer;
	}
	.btn-edit:hover { background: #21262d; }

	.state { color: #8b949e; padding: 1rem 0; font-size: 0.875rem; }
	.state.err { color: #f85149; }
	.err { color: #f85149; font-size: 0.825rem; margin: 0.5rem 0 0; }

	.empty {
		color: #6e7681;
		font-size: 0.9rem;
		padding: 2rem 0;
		text-align: center;
	}
	.empty .link {
		background: none;
		border: none;
		color: #58a6ff;
		cursor: pointer;
		font: inherit;
		text-decoration: underline;
	}

	.edit-form { display: flex; flex-direction: column; gap: 0.5rem; }
	.field-label { display: flex; flex-direction: column; gap: 0.35rem; }
	.field-label > span { font-size: 0.8rem; color: #8b949e; }
	.editor-wrap {
		border: 1px solid #30363d;
		border-radius: 6px;
		overflow: hidden;
		min-height: 200px;
		max-height: 90vh;
		resize: vertical;
	}

	.actions { display: flex; gap: 0.4rem; margin-top: 0.5rem; }
	.btn-save {
		padding: 0.35rem 0.85rem;
		background: #238636;
		border: 1px solid #2ea043;
		color: #fff;
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.btn-save:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-save:hover:not(:disabled) { background: #2ea043; }
	.btn-cancel {
		padding: 0.35rem 0.85rem;
		background: transparent;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.btn-cancel:hover:not(:disabled) { background: #21262d; }
</style>
