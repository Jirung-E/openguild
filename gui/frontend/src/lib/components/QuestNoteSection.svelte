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
	import { onMount, onDestroy, tick } from 'svelte';
	// DEV-153: 메모 편집 중이면 이탈 가드에 보고.
	import { setUnsaved } from '$lib/stores/unsaved';
	import MarkdownView from './MarkdownView.svelte';
	import { commentsApi as questCommentsApi, campaignCommentsApi } from '$lib/api/comments';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	// DEV-117: Windows 표준 redo (Ctrl+Shift+Z) keymap.
	import { keymap } from '@codemirror/view';
	import { redo, indentWithTab } from '@codemirror/commands';
	// 편집기 테마 — Compartment 로 다크/라이트 라이브 전환.
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	// DEV-074 fix15: CodeMirror native scrollbar 대신 overlay.
	import OverlayScrollbar from './OverlayScrollbar.svelte';

	// `mode` prop 은 호환성을 위해 받지만 동작 분기 X — 항상 memo.
	// svelte 가 "초기값만 캡쳐" 경고 안 내도록 destructure 에서 제외.
	// DEV-100: scope — quest (기본) / campaign.
	let {
		slug,
		scope = 'quest'
	}: { slug: string; mode?: 'memo' | 'comments'; scope?: 'quest' | 'campaign' } = $props();
	const commentsApi = $derived(scope === 'campaign' ? campaignCommentsApi : questCommentsApi);

	const label = {
		heading: '메모 (Memo)',
		emptyAction: '메모 작성',
		emptyHint: '개인 메모. gitignored (팀 공유 X).',
		help: '본인만 보는 비공개 메모 (`.guild/quests/{slug}.memo.md`, gitignored).'
	};

	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let content = $state<string | null>(null);

	// DEV-107 fix1: 섹션 접기 (메모) — 사용자 피드백 반영해 localStorage 영속
	// 제거. 매 진입 시 펼침 기본.
	let collapsed = $state(false);
	function toggleCollapsed() {
		collapsed = !collapsed;
	}

	let editMode = $state(false);
	// DEV-153: 메모 편집 중이면 이탈 가드에 보고. (이 컴포넌트는 항상 memo —
	// 댓글은 QuestCommentsSection 이 'comments:*' key 로 별도 보고.)
	$effect(() => setUnsaved(`note:${scope}`, editMode));
	onDestroy(() => setUnsaved(`note:${scope}`, false));
	let editText = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	let editorContainer: HTMLDivElement | undefined = $state(undefined);
	let editorView: EditorView | null = null;
	// DEV-074 fix15: `.cm-scroller` ref.
	let cmScroller: HTMLElement | null = $state(null);

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
				// 테마 — Compartment 로 다크/라이트 라이브 전환 (재생성 X).
				editorThemeCompartment.of(editorThemeExtension($theme)),
				// DEV-117: Windows 표준 redo (Ctrl+Shift+Z) 추가.
				// DEV-130: Tab = 들여쓰기 (focus 이동 X). Esc 후 Tab 으로 탈출 가능.
				keymap.of([{ key: 'Mod-Shift-z', run: redo, preventDefault: true }, indentWithTab]),
				EditorView.theme({
					'&': { fontSize: '0.875rem', borderRadius: '6px', height: '100%' },
					'.cm-editor': { borderRadius: '6px', height: '100%' },
					'.cm-scroller': { overflow: 'auto' }
				})
			],
			parent: editorContainer
		});
		// DEV-074 fix15: .cm-scroller ref → OverlayScrollbar target.
		cmScroller = editorContainer.querySelector('.cm-scroller') as HTMLElement | null;
	}

	// 테마 변경 시 재생성 없이 테마 확장만 교체 (커서/스크롤/undo 보존).
	$effect(() => {
		const t = $theme;
		editorView?.dispatch({
			effects: editorThemeCompartment.reconfigure(editorThemeExtension(t))
		});
	});

	function cancelEdit() {
		cmScroller = null;
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
		<!-- DEV-107: 섹션 토글. -->
		<button
			type="button"
			class="section-toggle"
			onclick={toggleCollapsed}
			aria-expanded={!collapsed}
			title={collapsed ? '메모 펼치기' : '메모 접기'}
		>
			<span class="toggle-icon" class:collapsed>▼</span>
			<h2 class="section-title note-memo">{label.heading}</h2>
		</button>
		{#if !collapsed && !editMode && !loading && !loadError}
			<button class="sec-add-btn" onclick={enterEdit}>
				{content && content.trim() ? '✎ 편집' : `+ ${label.emptyAction}`}
			</button>
		{/if}
	</div>

	{#if !collapsed}
	{#if loading}
		<p class="state">Loading…</p>
	{:else if loadError}
		<p class="state err">{loadError}</p>
	{:else if editMode}
		<!-- svelte-ignore a11y_label_has_associated_control -->
		<label class="field-label">
			<span>{label.help}</span>
			<div class="editor-wrap" bind:this={editorContainer}></div>
			<!-- DEV-074 fix15: CodeMirror native scrollbar 대신 overlay. -->
			{#if cmScroller}
				<OverlayScrollbar target={cmScroller} />
			{/if}
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
	{/if}
</section>

<style>
	.note-sec { margin-bottom: 1.5rem; }
	.section-head {
		display: flex; align-items: center; gap: 0.75rem;
		margin-bottom: 0.5rem;
	}
	/* DEV-107: 섹션 토글. */
	.section-toggle {
		display: flex; align-items: center; gap: 0.4rem;
		background: none; border: none; padding: 0; cursor: pointer;
		color: inherit; font: inherit;
	}
	.toggle-icon {
		font-size: 0.65rem;
		color: var(--text-muted);
		transition: transform 0.12s;
		display: inline-block;
	}
	.toggle-icon.collapsed {
		transform: rotate(-90deg);
	}
	.section-title {
		font-size: 0.8rem; font-weight: 600;
		text-transform: uppercase; letter-spacing: 0.05em; margin: 0;
		transition: color 0.12s;
	}
	.section-title.note-memo { color: var(--warning); }

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
	/* DEV-074 fix15: native scrollbar 숨김 — OverlayScrollbar 가 대신 그림. */
	.editor-wrap :global(.cm-scroller) { scrollbar-width: none; }
	.editor-wrap :global(.cm-scroller::-webkit-scrollbar) { display: none; }

	.actions { display: flex; gap: 0.4rem; margin-top: 0.5rem; }
	.btn-save {
		padding: 0.35rem 0.85rem;
		background: var(--btn-primary-bg); border: 1px solid var(--btn-primary-border);
		color: var(--btn-primary-text); border-radius: 6px; cursor: pointer; font-size: 0.825rem;
	}
	.btn-save:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-save:hover:not(:disabled) { background: var(--btn-primary-bg-hover); border-color: var(--btn-primary-border-hover); }
	.btn-cancel {
		padding: 0.35rem 0.85rem;
		background: transparent; border: 1px solid var(--border);
		color: var(--text); border-radius: 6px; cursor: pointer; font-size: 0.825rem;
	}
	.btn-cancel:hover:not(:disabled) { background: var(--bg-subtle); }
</style>
