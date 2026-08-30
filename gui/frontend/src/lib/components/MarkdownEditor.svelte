<!--
  DEV-203: 공통 CodeMirror 마크다운 편집기 — quest 상세 / campaign 상세 /
  memo(QuestNoteSection) / rules / library 5곳의 복붙 셋업을 단일화.

  이 중복 때문에 DEV-202(첨부 버튼 제거) 때 rules 한 곳을 놓쳤고, DEV-117
  (Mod-Shift-z redo)도 quest/memo 에만 있고 나머지엔 빠져 있었다. 여기가
  단일 지점이 되면서 아래 목록의 변경은 이 파일 수정으로 끝난다:

  - basicSetup + markdown()
  - 테마 Compartment (다크/라이트 라이브 전환, 커서/undo 보존)
  - indentExtensions(editorSettings) — 설정 변경 시 내용 보존 재생성
  - DEV-172: cross-link 자동완성 — CM 네이티브 autocompletion 대신 댓글(DEV-171)과
    동일한 caret 팝업(WikiAutocompletePopup + wiki-popup-place)을 view.coordsAtPos
    로 구동. worklog(+page.svelte)는 별도 CM 인스턴스라 기존 crossLinkAutocomplete
    (editor-links.ts) 를 그대로 씀 — 여긴 범위 밖(퀘스트 설명: 본문/규칙/메모 3곳).
  - attachmentExtension (paste/drag&drop — mediaOnly 또는 첨부 섹션 콜백)
  - Mod-Shift-z redo (Windows 표준)
  - 높이 localStorage 영속 (모든 편집기 공유 key) + resize 핸들
  - OverlayScrollbar (native 스크롤바 숨김)

  호출측 책임(컴포넌트 밖): 저장/취소 버튼, setUnsaved 이탈 가드, 저장 시
  `value` 읽기 (bind:value 로 항상 최신 동기화됨).
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { get } from 'svelte/store';
	import { EditorView } from 'codemirror';
	import { keymap, type ViewUpdate } from '@codemirror/view';
	import { Prec } from '@codemirror/state';
	import { redo } from '@codemirror/commands';
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	import { indentExtensions } from '$lib/utils/editor-indent';
	import { editorSettings } from '$lib/stores/editorSettings';
	import { attachmentExtension } from '$lib/utils/editor-attach';
	// BUG-215: 터치 기기에서는 drawSelection 을 뺀 구성을 쓴다 — 네이티브 선택이
	// 살아 있어야 "길게 눌러 선택" 이 동작한다.
	// DEV-336: markdownEditorExtensions 가 touch + autoFormat 설정을 함께 반영.
	import { isCoarsePointer, markdownEditorExtensions } from '$lib/utils/editor-setup';
	import OverlayScrollbar from './OverlayScrollbar.svelte';
	// DEV-172: cross-link 자동완성 — 댓글(DEV-171)과 공유하는 caret 팝업.
	import { wikiMatch, type WikiItem } from '$lib/utils/textarea-wikilink';
	import { applyWikiLinkCM, applyWikiPrefixCM } from '$lib/utils/editor-wikilink';
	import { computeWikiPlace, clampWikiLeft, isWikiCaretVisible } from '$lib/utils/wiki-popup-place';
	import { questIndex, loadQuestIndex } from '$lib/stores/questIndex';
	import WikiAutocompletePopup from './WikiAutocompletePopup.svelte';

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

	// DEV-172: cross-link 자동완성 상태 — QuestCommentsSection(DEV-171)과 동일한
	// 패턴(caret 좌표 + 후보 + 선택 index)이나, caret 좌표는 mirror-div 대신
	// CM 의 view.coordsAtPos 로 구한다.
	loadQuestIndex();
	let wiki = $state<{
		from: number;
		to: number;
		items: WikiItem[];
		left: number;
		caretTop: number;
		caretBottom: number;
	} | null>(null);
	/** 팝업의 **실제** 콘텐츠 높이(max-height 로 잘리기 전). ResizeObserver 로 갱신. */
	let wikiPopH = $state(0);
	let wikiPlace = $derived.by(() =>
		wiki ? computeWikiPlace(wiki.caretTop, wiki.caretBottom, wiki.items.length, wikiPopH) : null
	);
	let wikiSel = $state(0);
	let wikiPopEl = $state<HTMLUListElement | undefined>(undefined);
	// BUG-114 패턴: 키보드 이동일 때만 팝업 스크롤, 마우스 호버는 무시.
	let wikiSelFromKeyboard = false;
	// Esc/클릭아웃으로 닫은 토큰 — 같은 토큰에선 재오픈 안 함.
	let wikiDismissed = $state<string | null>(null);

	// 펼침/접힘으로 콘텐츠 높이가 바뀌면 다시 배치.
	$effect(() => {
		const pop = wikiPopEl;
		if (!pop || !wiki) {
			wikiPopH = 0;
			return;
		}
		const measure = () => (wikiPopH = pop.scrollHeight);
		measure();
		const ro = new ResizeObserver(measure);
		ro.observe(pop);
		return () => ro.disconnect();
	});
	// ↑/↓ 로 선택 이동 시 선택 항목이 팝업 스크롤 밖이면 보이도록 스크롤.
	$effect(() => {
		void wikiSel;
		void wiki;
		if (!wiki) {
			return;
		}
		if (!wikiSelFromKeyboard) return;
		wikiSelFromKeyboard = false;
		const pop = wikiPopEl;
		const sel = pop?.querySelector<HTMLElement>('.wiki-opt.sel');
		if (!pop || !sel) return;
		const itemTop = sel.offsetTop;
		const itemBottom = itemTop + sel.offsetHeight;
		if (itemTop < pop.scrollTop) {
			pop.scrollTop = itemTop;
		} else if (itemBottom > pop.scrollTop + pop.clientHeight) {
			pop.scrollTop = itemBottom - pop.clientHeight;
		}
	});
	// 스크롤/리사이즈로 caret 이 움직이면 재배치, 팝업/편집기 밖 클릭이면 닫기.
	$effect(() => {
		if (!wiki || !view) return;
		const v = view;
		const onMove = () => {
			wiki = wiki ? placeWikiCM(v, wiki.from, wiki.to, wiki.items) : null;
		};
		const onDown = (ev: MouseEvent) => {
			if (!wiki) return;
			const tgt = ev.target as Node;
			// 팝업/편집기 내부 클릭(옵션 선택·캐럿 이동)은 닫지 않음.
			if (wikiPopEl?.contains(tgt) || v.dom.contains(tgt)) return;
			dismissWiki(v);
		};
		window.addEventListener('scroll', onMove, true);
		window.addEventListener('resize', onMove);
		window.addEventListener('mousedown', onDown, true);
		return () => {
			window.removeEventListener('scroll', onMove, true);
			window.removeEventListener('resize', onMove);
			window.removeEventListener('mousedown', onDown, true);
		};
	});

	/** caret 좌표(view.coordsAtPos) 기준 팝업 위치. 편집기 보이는 영역 밖이면 null. */
	function placeWikiCM(v: EditorView, from: number, to: number, items: WikiItem[]): typeof wiki {
		const coords = v.coordsAtPos(to);
		if (!coords) return null;
		const editorRect = v.dom.getBoundingClientRect();
		if (!isWikiCaretVisible(coords.top, coords.bottom, editorRect.top, editorRect.bottom)) {
			return null;
		}
		const left = clampWikiLeft(coords.left);
		return { from, to, items, left, caretTop: coords.top, caretBottom: coords.bottom };
	}

	function dismissWiki(v: EditorView) {
		if (wiki) wikiDismissed = v.state.sliceDoc(wiki.from, wiki.to);
		wiki = null;
	}

	function applyWiki(v: EditorView, item: WikiItem) {
		if (!wiki) return;
		if (item.nsPrefix) {
			// nsPrefix(네임스페이스 접두)는 `]]` 를 안 닫는다 — dispatch 가 동기로
			// updateListener 를 태우므로, 그 kind 로 필터된 다음 후보가 바로 이어진다.
			applyWikiPrefixCM(v, wiki.from, wiki.to, item.insert ?? item.id);
			return;
		}
		applyWikiLinkCM(v, wiki.from, wiki.to, item.insert ?? item.id);
		wiki = null;
		wikiDismissed = null;
	}

	/** 문서/선택이 바뀔 때마다 caret 앞 `[[` 컨텍스트를 재평가 (댓글 onWikiInput 과 동형). */
	function onWikiUpdate(u: ViewUpdate) {
		if (!u.docChanged && !u.selectionSet) return;
		const pos = u.state.selection.main.head;
		const m = wikiMatch(u.state.doc.toString(), pos, get(questIndex));
		if (!m) {
			wiki = null;
			wikiDismissed = null;
			return;
		}
		const token = u.state.doc.sliceString(m.from, m.to);
		if (wikiDismissed === token) {
			wiki = null;
			return;
		}
		wikiDismissed = null;
		wiki = placeWikiCM(u.view, m.from, m.to, m.items);
		wikiSel = 0;
		wikiSelFromKeyboard = true;
	}

	// VS 식 키보드 네비/적용 — basicSetup 의 기본 화살표/Enter/Tab 바인딩보다
	// 먼저 가로채야 하므로 Prec.highest.
	const wikiKeymap = Prec.highest(
		keymap.of([
			{
				key: 'ArrowDown',
				run: () => {
					if (!wiki) return false;
					wikiSelFromKeyboard = true;
					wikiSel = (wikiSel + 1) % wiki.items.length;
					return true;
				}
			},
			{
				key: 'ArrowUp',
				run: () => {
					if (!wiki) return false;
					wikiSelFromKeyboard = true;
					wikiSel = (wikiSel - 1 + wiki.items.length) % wiki.items.length;
					return true;
				}
			},
			{
				key: 'Enter',
				run: (v) => {
					if (!wiki) return false;
					applyWiki(v, wiki.items[wikiSel]);
					return true;
				}
			},
			{
				key: 'Tab',
				run: (v) => {
					if (!wiki) return false;
					applyWiki(v, wiki.items[wikiSel]);
					return true;
				}
			},
			{
				key: 'Escape',
				run: (v) => {
					if (!wiki) return false;
					dismissWiki(v);
					return true;
				}
			}
		])
	);

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
		wiki = null;
		container.style.height = `${loadHeight()}px`;
		view = new EditorView({
			// untrack — $effect 안에서 호출되므로 value 를 그대로 읽으면
			// 키 입력마다(value 동기화) 편집기가 재생성되는 루프가 된다.
			doc: untrack(() => value),
			extensions: [
				// BUG-215: 데스크톱은 기존 basicSetup 그대로, 터치만 변형.
				// DEV-336: autoFormat 설정 꺼지면 목록 이어쓰기/자동 들여쓰기/재들여쓰기 제외.
				markdownEditorExtensions({
					touch: isCoarsePointer(),
					autoFormat: untrack(() => $editorSettings.autoFormat)
				}),
				// 테마 — Compartment 로 다크/라이트 라이브 전환 (재생성 X).
				editorThemeCompartment.of(editorThemeExtension(untrack(() => $theme))),
				// DEV-117: Windows 표준 redo. (Tab 들여쓰기는 indentExtensions.)
				keymap.of([{ key: 'Mod-Shift-z', run: redo, preventDefault: true }]),
				// DEV-130: tab/space + 2/4칸 들여쓰기.
				indentExtensions(untrack(() => $editorSettings)),
				// DEV-069: 클립보드 paste / 파일 drag&drop → 첨부 업로드.
				attachmentExtension(onError, onAttach, { mediaOnly }),
				// DEV-140/172: XXX-NNN 타이핑 → [[...]] cross-link 자동완성 —
				// 댓글(DEV-171)과 같은 caret 팝업. 네비 키 가로채기 + 매칭 갱신.
				wikiKeymap,
				EditorView.updateListener.of(onWikiUpdate),
				// CM ↔ value 실시간 동기화 — 호출측 저장 로직이 view 를 직접
				// 만질 필요 없이 bind:value 만 읽으면 됨.
				EditorView.updateListener.of((u) => {
					if (u.docChanged) value = u.state.doc.toString();
				}),
				// DEV-369: CodeMirror 테마는 **JS 객체**라 CSS 파일이 아니다 — 곡률
				// 일괄 치환에서 통째로 빠져 있었다(admin 이 화면에서 발견).
				// 값은 CSS 로 나가므로 토큰을 그대로 쓸 수 있다.
				EditorView.theme({
					'&': { fontSize: '0.875rem', borderRadius: 'var(--r-md)', height: '100%' },
					'.cm-editor': { borderRadius: 'var(--r-md)', height: '100%' },
					// DEV-272: CodeMirror 는 자체 baseTheme 에서 `.cm-scroller` 에
					// `monospace` 를 박아 둔다 — 여기서 덮지 않으면 편집기만
					// 코드 글꼴 설정을 안 따라간다(정작 가장 필요한 곳이다).
					'.cm-scroller': { overflow: 'auto', fontFamily: 'var(--font-mono)' }
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
			wiki = null;
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

	// BUG-222: value 는 그동안 CM → value 단방향(updateListener)으로만
	// 동기화됐다 — 호출측이 저장/등록 후 `value = ''` 처럼 **외부에서**
	// 리셋해도 CM 문서엔 반영할 경로가 없어(마운트 시 1회 doc: value 만
	// 반영) 입력창이 안 비워진 채로 남았다. value 변경을 감시해 CM 문서와
	// 다르면 통째로 치환 — updateListener 가 타이핑마다 value 를 CM 과
	// 동일하게 맞춰두므로(같으면 skip) 무한 루프는 없다.
	$effect(() => {
		const v = value;
		if (!view) return;
		if (view.state.doc.toString() === v) return;
		view.dispatch({
			changes: { from: 0, to: view.state.doc.length, insert: v }
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
<!-- DEV-172: cross-link 자동완성 팝업 — 댓글(DEV-171)과 같은 컴포넌트.
     view 는 wiki 가 세팅될 때 이미 존재함(onWikiUpdate 가 view 를 받아야만
     wiki 를 채움) — view 자체를 반응형으로 만들 필요 없이 wiki 만으로 게이팅. -->
{#if wiki}
	<WikiAutocompletePopup
		items={wiki.items}
		left={wiki.left}
		top={wikiPlace?.top ?? wiki.caretBottom}
		bottom={wikiPlace?.bottom ?? null}
		maxH={wikiPlace?.maxH ?? 224}
		selectedIndex={wikiSel}
		onSelect={(item) => applyWiki(view!, item)}
		onHoverSelect={(i) => {
			wikiSel = i;
		}}
		bind:popupEl={wikiPopEl}
	/>
{/if}

<style>
	.editor-wrap {
		/* DEV-057: 사용자 drag 로 height 조절 — ResizeObserver 가 영속화. */
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		overflow: hidden;
		min-height: 12.5rem;
		max-height: 90vh;
		resize: vertical;
		/* BUG-220: 배경 미지정 + 라이트 테마는 CodeMirror 자체 배경도 없어서,
		   고정(pin)된 댓글처럼 부모가 틴트 배경(.entry-card.pinned)을 가지면
		   그게 편집기 영역까지 그대로 비쳐 보였다. 일반 textarea 모드
		   (.body-input)와 같은 불투명 배경을 명시.*/
		background: var(--bg);
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
