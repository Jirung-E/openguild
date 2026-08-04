<!--
  DEV-203: 공통 CodeMirror 마크다운 편집기 — quest 상세 / campaign 상세 /
  memo(QuestNoteSection) / rules / library 5곳의 복붙 셋업을 단일화.

  이 중복 때문에 DEV-202(첨부 버튼 제거) 때 rules 한 곳을 놓쳤고, DEV-117
  (Mod-Shift-z redo)도 quest/memo 에만 있고 나머지엔 빠져 있었다. 여기가
  단일 지점이 되면서 아래 목록의 변경은 이 파일 수정으로 끝난다:

  - basicSetup + markdown()
  - 테마 Compartment (다크/라이트 라이브 전환, 커서/undo 보존)
  - indentExtensions(editorSettings) — 설정 변경 시 내용 보존 재생성
  - crossLinkAutocomplete (XXX-NNN → [[...]])
  - attachmentExtension (paste/drag&drop — mediaOnly 또는 첨부 섹션 콜백)
  - Mod-Shift-z redo (Windows 표준)
  - 높이 localStorage 영속 (모든 편집기 공유 key) + resize 핸들
  - OverlayScrollbar (native 스크롤바 숨김)

  호출측 책임(컴포넌트 밖): 저장/취소 버튼, setUnsaved 이탈 가드, 저장 시
  `value` 읽기 (bind:value 로 항상 최신 동기화됨).
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { keymap } from '@codemirror/view';
	import { redo } from '@codemirror/commands';
	import { markdown } from '@codemirror/lang-markdown';
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	import { indentExtensions } from '$lib/utils/editor-indent';
	import { editorSettings } from '$lib/stores/editorSettings';
	import { attachmentExtension } from '$lib/utils/editor-attach';
	import { crossLinkAutocomplete } from '$lib/utils/editor-links';
	// BUG-215: 터치 기기에서는 drawSelection 을 뺀 구성을 쓴다 — 네이티브 선택이
	// 살아 있어야 "길게 눌러 선택" 이 동작한다.
	import { isCoarsePointer, touchSetup } from '$lib/utils/editor-setup';
	import OverlayScrollbar from './OverlayScrollbar.svelte';

	let {
		value = $bindable(''),
		onError,
		onAttach = undefined,
		mediaOnly = false,
		defaultHeight = 480
	}: {
		/** 편집 내용 — CM 입력이 실시간 반영되므로 저장 시 이 값을 그대로 사용. */
		value?: string;
		/** 첨부 업로드 실패 메시지 콜백 (호출측이 saveError 등에 표시). */
		onError: (msg: string) => void;
		/**
		 * 첨부 섹션 등록 콜백(quest/campaign/library) — 비미디어 파일을 본문
		 * 인라인 대신 첨부 섹션으로. 미지정 + mediaOnly=false 조합은 없음.
		 */
		onAttach?: (rel: string, name: string) => void | Promise<void>;
		/** true = 이미지/동영상만 허용(memo/rules — 첨부 섹션이 없는 곳). */
		mediaOnly?: boolean;
		/** 저장된 높이가 없을 때 초기 높이(px). */
		defaultHeight?: number;
	} = $props();

	// DEV-057: 편집창 사용자 크기 영속화 — 모든 마크다운 편집기가 공유(일관 UX).
	const HEIGHT_KEY = 'openguild.questEditorHeight';
	function loadHeight(): number {
		try {
			const n = parseInt(localStorage.getItem(HEIGHT_KEY) ?? '', 10);
			if (Number.isFinite(n) && n >= 200 && n <= 2000) return n;
		} catch {
			/* ignore */
		}
		return defaultHeight;
	}
	let heightSaveTimer: ReturnType<typeof setTimeout> | null = null;
	function scheduleHeightSave(px: number) {
		if (heightSaveTimer) clearTimeout(heightSaveTimer);
		heightSaveTimer = setTimeout(() => {
			try {
				localStorage.setItem(HEIGHT_KEY, String(Math.round(px)));
			} catch {
				/* ignore */
			}
		}, 250);
	}

	let container: HTMLDivElement | undefined = $state(undefined);
	let view: EditorView | null = null;
	// DEV-074 fix15: `.cm-scroller` ref — OverlayScrollbar target.
	let cmScroller: HTMLElement | null = $state(null);
	let resizeObserver: ResizeObserver | null = null;

	function init() {
		if (!container) return;
		view?.destroy();
		view = null;
		container.style.height = `${loadHeight()}px`;
		view = new EditorView({
			// untrack — $effect 안에서 호출되므로 value 를 그대로 읽으면
			// 키 입력마다(value 동기화) 편집기가 재생성되는 루프가 된다.
			doc: untrack(() => value),
			extensions: [
				// BUG-215: 데스크톱은 기존 basicSetup 그대로, 터치만 변형.
				isCoarsePointer() ? touchSetup() : basicSetup,
				markdown(),
				// 테마 — Compartment 로 다크/라이트 라이브 전환 (재생성 X).
				editorThemeCompartment.of(editorThemeExtension(untrack(() => $theme))),
				// DEV-117: Windows 표준 redo. (Tab 들여쓰기는 indentExtensions.)
				keymap.of([{ key: 'Mod-Shift-z', run: redo, preventDefault: true }]),
				// DEV-130: tab/space + 2/4칸 들여쓰기.
				indentExtensions(untrack(() => $editorSettings)),
				// DEV-069: 클립보드 paste / 파일 drag&drop → 첨부 업로드.
				attachmentExtension(onError, onAttach, { mediaOnly }),
				// DEV-140: XXX-NNN 타이핑 → [[...]] cross-link 자동완성.
				crossLinkAutocomplete(),
				// CM ↔ value 실시간 동기화 — 호출측 저장 로직이 view 를 직접
				// 만질 필요 없이 bind:value 만 읽으면 됨.
				EditorView.updateListener.of((u) => {
					if (u.docChanged) value = u.state.doc.toString();
				}),
				EditorView.theme({
					'&': { fontSize: '0.875rem', borderRadius: '6px', height: '100%' },
					'.cm-editor': { borderRadius: '6px', height: '100%' },
					'.cm-scroller': { overflow: 'auto' }
				})
			],
			parent: container
		});
		cmScroller = container.querySelector('.cm-scroller') as HTMLElement | null;
		// resize 핸들로 크기 바꿀 때마다 디바운스 영속화.
		resizeObserver?.disconnect();
		resizeObserver = new ResizeObserver((entries) => {
			for (const entry of entries) {
				scheduleHeightSave(entry.contentRect.height);
			}
		});
		resizeObserver.observe(container);
	}

	// 컨테이너 바인딩 시 생성, 컴포넌트 파괴 시 정리 — 호출측은 {#if editMode}
	// 안에 두기만 하면 수명주기가 자동.
	$effect(() => {
		if (!container) return;
		init();
		return () => {
			cmScroller = null;
			view?.destroy();
			view = null;
			resizeObserver?.disconnect();
			resizeObserver = null;
			if (heightSaveTimer) clearTimeout(heightSaveTimer);
		};
	});

	// 테마 변경 — 재생성 없이 확장만 교체 (커서/스크롤/undo 보존).
	$effect(() => {
		const t = $theme;
		view?.dispatch({
			effects: editorThemeCompartment.reconfigure(editorThemeExtension(t))
		});
	});

	// DEV-130: 들여쓰기 설정 변경 시 재생성 — value 가 실시간 동기화되므로
	// 내용은 보존됨. 최초 구독(마운트 직후)은 skip.
	let prevSettings: unknown = undefined;
	$effect(() => {
		const s = $editorSettings;
		if (prevSettings === undefined) {
			prevSettings = s;
			return;
		}
		if (s === prevSettings) return;
		prevSettings = s;
		if (view) init();
	});
</script>

<!-- CodeMirror 가 div 안에 편집 영역을 동적으로 생성. -->
<div class="editor-wrap" bind:this={container}></div>
<!-- DEV-074 fix15: CodeMirror native scrollbar 대신 overlay. -->
{#if cmScroller}
	<OverlayScrollbar target={cmScroller} />
{/if}

<style>
	.editor-wrap {
		/* DEV-057: 사용자 drag 로 height 조절 — ResizeObserver 가 영속화. */
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
		min-height: 200px;
		max-height: 90vh;
		resize: vertical;
	}
	.editor-wrap :global(.cm-editor) {
		outline: none;
	}
	.editor-wrap :global(.cm-editor.cm-focused) {
		outline: none;
		border: none;
	}
	/* DEV-074 fix15: native scrollbar 숨김 — OverlayScrollbar 가 대신 그림. */
	.editor-wrap :global(.cm-scroller) {
		scrollbar-width: none;
	}
	.editor-wrap :global(.cm-scroller::-webkit-scrollbar) {
		display: none;
	}
</style>
