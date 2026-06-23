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
	import { redo } from '@codemirror/commands';
	// 편집기 테마 — Compartment 로 다크/라이트 라이브 전환.
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	// DEV-130: Tab 들여쓰기는 editorSettings(tab/space·크기)를 따르는 indentExtensions 가 담당.
	import { indentExtensions } from '$lib/utils/editor-indent';
	import { editorSettings } from '$lib/stores/editorSettings';
	// DEV-140 후속: 메모 편집기에도 XXX-NNN → [[...]] 자동완성.
	import { crossLinkAutocomplete } from '$lib/utils/editor-links';
	// DEV-069 후속(admin #11): 메모에도 첨부 — paste/drag&drop.
	// DEV-188: '첨부' 버튼은 제거(메모는 개인용). paste/drag&drop 첨부는 유지.
	import { attachmentExtension } from '$lib/utils/editor-attach';
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
	// 제거. 매 진입 시 펼침 기본. (DEV-189: '접기'는 영속 X 그대로 유지.)
	let collapsed = $state(false);
	function toggleCollapsed() {
		collapsed = !collapsed;
	}

	// DEV-189: 메모 표시 높이 모드 — 'expand'(전체 높이, 기본) / 'fixed'(고정
	// 높이 + 내부 스크롤). 접기(collapsed)와 별개. fixed/expand 만 localStorage
	// 영속 — 접기는 영속하지 않음(매 진입 시 펼침 유지).
	const HEIGHT_MODE_KEY = 'openguild.memoHeightMode';
	function loadHeightMode(): 'expand' | 'fixed' {
		try {
			return localStorage.getItem(HEIGHT_MODE_KEY) === 'fixed' ? 'fixed' : 'expand';
		} catch {
			return 'expand';
		}
	}
	let heightMode = $state<'expand' | 'fixed'>(loadHeightMode());
	function toggleHeightMode() {
		heightMode = heightMode === 'fixed' ? 'expand' : 'fixed';
		try {
			localStorage.setItem(HEIGHT_MODE_KEY, heightMode);
		} catch {
			/* ignore */
		}
	}

	// DEV-189(admin 후속): 고정 모드 높이는 드래그로 조절 가능 + 영속.
	const MEMO_FIXED_HEIGHT_KEY = 'openguild.memoFixedHeight';
	function loadMemoFixedHeight(): number {
		try {
			const n = parseInt(localStorage.getItem(MEMO_FIXED_HEIGHT_KEY) ?? '', 10);
			if (Number.isFinite(n) && n >= 120 && n <= 2000) return n;
		} catch {
			/* ignore */
		}
		return 360;
	}
	let memoFixedHeight = $state(loadMemoFixedHeight());
	let memoBodyEl: HTMLDivElement | undefined = $state(undefined);
	let memoFixedSaveTimer: ReturnType<typeof setTimeout> | null = null;
	// 고정 모드에서 사용자가 resize 핸들로 높이를 바꾸면 디바운스 저장.
	// (overflow:auto + 고정 height 라 콘텐츠 변화로는 box 높이가 안 바뀜 → resize 만 포착.)
	$effect(() => {
		if (heightMode !== 'fixed' || !memoBodyEl) return;
		const el = memoBodyEl;
		const obs = new ResizeObserver((entries) => {
			for (const e of entries) {
				// border-box 높이로 읽어야 style:height(=border-box, 전역 border-box)와
				// 일치 — contentRect 는 padding/border 제외라 재로드마다 값이 줄어든다.
				const h = Math.round(e.borderBoxSize?.[0]?.blockSize ?? el.offsetHeight);
				if (h < 120 || h > 2000) continue;
				memoFixedHeight = h;
				if (memoFixedSaveTimer) clearTimeout(memoFixedSaveTimer);
				memoFixedSaveTimer = setTimeout(() => {
					try {
						localStorage.setItem(MEMO_FIXED_HEIGHT_KEY, String(h));
					} catch {
						/* ignore */
					}
				}, 250);
			}
		});
		obs.observe(el);
		return () => obs.disconnect();
	});

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
				// DEV-140 후속: XXX-NNN 타이핑 → [[...]] cross-link 자동완성.
				crossLinkAutocomplete(),
				// DEV-069 후속: 첨부 — 클립보드 paste / 파일 drag&drop.
				attachmentExtension((msg) => (saveError = `첨부 실패: ${msg}`), undefined, {
					mediaOnly: true
				}),
				// DEV-117: Windows 표준 redo (Ctrl+Shift+Z) 추가.
				keymap.of([{ key: 'Mod-Shift-z', run: redo, preventDefault: true }]),
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
			<!-- DEV-189: 표시 높이 모드 토글 (고정 ↔ 확장). 내용 있을 때만. -->
			{#if content && content.trim()}
				<button
					class="sec-mode-btn"
					onclick={toggleHeightMode}
					title={heightMode === 'fixed'
						? '메모를 전체 높이로 펼치기 (확장)'
						: '메모를 고정 높이 + 스크롤로 (고정)'}
				>
					{heightMode === 'fixed' ? '⤢ 확장' : '⊟ 고정'}
				</button>
			{/if}
		{/if}
	</div>

	{#if !collapsed}
		{#if loading}
			<p class="state">Loading…</p>
		{:else if loadError}
			<p class="state err">{loadError}</p>
		{:else if editMode}
			<!-- BUG: editor 섹션은 <label> 금지 — 안의 '📎 첨부' 버튼(labelable)이
		     라벨 클릭마다 활성화돼 파일창이 뜬다(admin #13). div 로. -->
			<div class="field-label">
				<!-- DEV-188: '첨부' 버튼 제거(메모는 개인용). 이미지·동영상은
				     드래그&드랍 / Ctrl+V 로 첨부 가능(attachmentExtension). -->
				<span>{label.help} (이미지·동영상은 드래그&드랍 또는 Ctrl+V 로 첨부)</span>
				<div class="editor-wrap" bind:this={editorContainer}></div>
				<!-- DEV-074 fix15: CodeMirror native scrollbar 대신 overlay. -->
				{#if cmScroller}
					<OverlayScrollbar target={cmScroller} />
				{/if}
			</div>
			<div class="actions">
				<button class="btn-save" onclick={save} disabled={saving}>
					{saving ? '저장…' : '저장'}
				</button>
				<button class="btn-cancel" onclick={cancelEdit} disabled={saving}>취소</button>
			</div>
			{#if saveError}<p class="state err">{saveError}</p>{/if}
		{:else if content && content.trim()}
			<!-- DEV-189: 'fixed' 모드면 고정 높이 + 내부 스크롤(드래그로 크기 조절),
			     'expand' 면 전체 높이. -->
			<div
				class="memo-body"
				class:fixed={heightMode === 'fixed'}
				bind:this={memoBodyEl}
				style:height={heightMode === 'fixed' ? `${memoFixedHeight}px` : null}
			>
				<MarkdownView source={content} />
			</div>
		{:else}
			<p class="no-desc">
				{label.emptyHint}
				<button class="link-btn" onclick={enterEdit}>{label.emptyAction}</button>
			</p>
		{/if}
	{/if}
</section>

<style>
	.note-sec {
		margin-bottom: 1.5rem;
	}
	.section-head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 0.5rem;
	}
	/* DEV-107: 섹션 토글. */
	.section-toggle {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		color: inherit;
		font: inherit;
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
		font-size: 0.8rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin: 0;
		transition: color 0.12s;
	}
	.section-title.note-memo {
		color: var(--warning);
	}

	.sec-add-btn {
		padding: 0.15rem 0.6rem;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: transparent;
		color: var(--text-muted);
		font-size: 0.72rem;
		cursor: pointer;
		margin-left: auto;
	}
	.sec-add-btn:hover {
		background: var(--bg-subtle);
		color: var(--text);
	}

	/* DEV-189: 표시 높이 모드 토글 버튼. */
	.sec-mode-btn {
		padding: 0.15rem 0.6rem;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: transparent;
		color: var(--text-muted);
		font-size: 0.72rem;
		cursor: pointer;
	}
	.sec-mode-btn:hover {
		background: var(--bg-subtle);
		color: var(--text);
	}

	/* DEV-189: 'fixed' 모드 — 메모 본문을 고정 높이 + 내부 스크롤. 높이는 인라인
	   style 로 지정되며 resize 핸들로 드래그 조절 가능(영속). */
	.memo-body.fixed {
		overflow-y: auto;
		resize: vertical;
		min-height: 120px;
		max-height: 2000px;
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 0.25rem 0.75rem;
	}

	.state {
		color: var(--text-muted);
		font-size: 0.825rem;
		margin: 0.25rem 0;
	}
	.state.err {
		color: var(--danger);
	}

	.no-desc {
		color: var(--text-faint);
		font-size: 0.825rem;
		margin: 0.25rem 0;
	}
	.link-btn {
		background: none;
		border: none;
		color: var(--accent);
		cursor: pointer;
		padding: 0;
		font: inherit;
		text-decoration: underline;
		margin-left: 0.35rem;
	}

	.field-label {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.field-label > span {
		font-size: 0.75rem;
		color: var(--text-muted);
	}
	.editor-wrap {
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
		min-height: 180px;
		max-height: 90vh;
		resize: vertical;
	}
	/* DEV-074 fix15: native scrollbar 숨김 — OverlayScrollbar 가 대신 그림. */
	.editor-wrap :global(.cm-scroller) {
		scrollbar-width: none;
	}
	.editor-wrap :global(.cm-scroller::-webkit-scrollbar) {
		display: none;
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
