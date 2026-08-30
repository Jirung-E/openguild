<!--
  DEV-094: Quest Detail 의 댓글 섹션 (entry 단위 + 답글).

  - 목록: top-level entry → 본문 + 답글 목록 (들여쓰기).
  - 편집: 본인 entry "✎ 편집" → inline textarea (작성 시각/작성자 보존).
  - 삭제: "× 삭제" 확인 후 entry 제거. parent 삭제 시 자식은 orphan reference
    로 남되, render 단계에서 "(삭제된 댓글에 대한 답글)" 로 안내.
  - 답글: 각 entry 의 "답글 쓰기" → 스레드 하단 inline form. parent_id 자동 채움.
  - 새 top-level 댓글: 하단 폼.

  DEV-200: 2단 들여쓰기 threading — root > level-1 답글 > level-2(답글의 답글).
  level-2 보다 깊은 체인은 가장 가까운 level-1 조상 밑에 flatten 하고
  ↩ #id 링크가 실제 답글 대상을 가리킨다. parent_id 체인은 데이터에 그대로 기록.
-->
<script lang="ts">
	import Icon from './Icon.svelte';
	import { tick, onDestroy, onMount } from 'svelte';
	// BUG-258: 딥링크 스크롤은 레이아웃이 잦아든 뒤에 시작해야 한다.
	import { scrollIntoViewWhenSettled } from '$lib/utils/page-scroll';
	// DEV-296: `?comment=N` 딥링크 — 작업기록에서 그 댓글로 바로 스크롤.
	import { page } from '$app/stores';
	import MarkdownView from './MarkdownView.svelte';
	// DEV-132 후속(admin 보고): 커스텀 반응 입력을 이모지 1개로 제한.
	import { isSingleEmoji } from '$lib/utils/emoji';
	// DEV-153: 작성/편집/답글 중이면 이탈 가드에 보고.
	import { setUnsaved } from '$lib/stores/unsaved';
	import { saveShortcut } from '$lib/utils/save-shortcut';
	// DEV-259: alert() 잔재 제거 — 앱 공용 toast 로 통일.
	import { showToast } from '$lib/stores/toast';
	import {
		commentsApi as questCommentsApi,
		campaignCommentsApi,
		type CommentEntry
	} from '$lib/api/comments';
	// DEV-118: native confirm() 대신 인앱 모달.
	import ConfirmDialog from './ConfirmDialog.svelte';
	// DEV-130: Tab = tab 문자 삽입 (focus 이동 X).
	import { tabInsert } from '$lib/actions/tab-insert';
	// DEV-151: 댓글 textarea 첨부 — paste/drag&drop/버튼.
	import { textareaAttach } from '$lib/utils/editor-attach';
	// DEV-289: 댓글 입력창을 마크다운 편집기로 토글 (본문/메모와 동일 컴포넌트).
	import MarkdownEditor from './MarkdownEditor.svelte';
	// BUG-157: cross-link 자동완성 팝업 스크롤도 커스텀(overlay)으로 통일.
	import OverlayScrollbar from './OverlayScrollbar.svelte';
	// DEV-140/171: 댓글 textarea cross-link 자동완성 — caret 위치 팝업 + 실재 ID 제안.
	import {
		wikiMatch,
		applyWikiLink,
		applyWikiPrefix,
		caretXY,
		type WikiItem
	} from '$lib/utils/textarea-wikilink';
	// DEV-172: 팝업 배치 계산 + UI 는 본문 편집기(MarkdownEditor)와 공유.
	import { computeWikiPlace, clampWikiLeft, isWikiCaretVisible } from '$lib/utils/wiki-popup-place';
	import WikiAutocompletePopup from './WikiAutocompletePopup.svelte';
	import { questIndex, loadQuestIndex } from '$lib/stores/questIndex';
	import { get } from 'svelte/store';
	// DEV-205(모듈4): 댓글 섹션 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-235: 접기 상태(답글/본문) 영속 — 보드의 collapsedLanes(DEV-105) 와
	// 같은 길드별 namespace 패턴.
	import { resolveGuildKeyPrefix, guildKey } from '$lib/utils/guild-storage';

	loadQuestIndex();
	// DEV-171: caret 위치 팝업 — 활성 textarea + 후보 + 화면 좌표 + 선택 index.
	let wiki = $state<{
		el: HTMLTextAreaElement;
		from: number;
		to: number;
		items: WikiItem[];
		left: number;
		// BUG-209: 예전엔 여기서 top/bottom 을 **높이 추정치로** 정해버렸다. 항목
		// 높이가 30px 이라는 가정이 DEV-297(선택 항목 펼침)로 깨지면서 팝업이 화면
		// 밖으로 삐져나갔다. 이제 caret 좌표만 들고 있고, 실제 렌더 높이를 재서
		// 배치한다(아래 wikiPlace).
		caretTop: number;
		caretBottom: number;
	} | null>(null);
	/** 팝업의 **실제** 콘텐츠 높이(max-height 로 잘리기 전). ResizeObserver 로 갱신. */
	let wikiPopH = $state(0);
	// DEV-172: 배치 계산은 MarkdownEditor(CodeMirror) 와 공유하는 순수 함수로 이전.
	let wikiPlace = $derived.by(() =>
		wiki ? computeWikiPlace(wiki.caretTop, wiki.caretBottom, wiki.items.length, wikiPopH) : null
	);
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
	let wikiSel = $state(0);
	let wikiPopEl = $state<HTMLUListElement | undefined>(undefined);
	// BUG-114: mouseenter(호버)로도 wikiSel 이 바뀌는데, 이 effect 가 그때마다
	// scrollIntoView 를 불러 스크롤바를 마우스로 드래그하는 도중 커서가 옆의
	// 항목 위를 지나칠 때마다 스크롤이 강제로 되돌아가 — 스크롤바를 움직일 수
	// 없는 것처럼 보였다. 키보드(↑/↓) 이동일 때만 스크롤, 마우스 호버는 무시.
	let wikiSelFromKeyboard = false;
	// DEV-297 수정: 선택된 항목은 **제자리에서 펼쳐** 전체 제목을 보여준다.
	// 예전엔 위/아래에 팝업을 띄웠는데 그게 이웃 항목을 통째로 가렸다(admin 보고).
	// DEV-359: 호버로 고른 항목도 같다 — 키보드만 펼치고 호버는 툴팁이라
	// 두 방식이 섞여 오히려 어색했다. 스크롤 보정은 keepRowAnchored 가 한다.
	// DEV-171 후속: ↑/↓ 로 선택 이동 시 선택 항목이 팝업 스크롤 밖이면 보이도록 스크롤.
	$effect(() => {
		void wikiSel;
		void wiki;
		if (!wiki) return;
		if (!wikiSelFromKeyboard) return;
		wikiSelFromKeyboard = false;
		// BUG-163: scrollIntoView({block:'nearest'}) 가 WebView 에서 항목 높이가
		// 아니라 팝업 높이만큼 스크롤해 여러 항목을 건너뛰었다. 팝업 스크롤을
		// 직접 계산 — 선택 항목이 보이는 영역 위/아래로 벗어난 만큼만 이동.
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

	// DEV-171 후속: Esc/클릭아웃으로 닫은 토큰 — 같은 토큰에선 재오픈 안 함
	// ('esc 눌러도 다시 뜨던' 문제). 토큰이 바뀌면 해제.
	let wikiDismissed = $state<string | null>(null);

	function onWikiInput(e: Event) {
		// 네비/적용 키(↑↓ Enter Tab Esc)는 재계산 skip — wikiSel 리셋/재오픈 방지.
		if (
			e instanceof KeyboardEvent &&
			wiki &&
			(e.key === 'ArrowDown' ||
				e.key === 'ArrowUp' ||
				e.key === 'Enter' ||
				e.key === 'Tab' ||
				e.key === 'Escape')
		)
			return;
		const el = e.currentTarget as HTMLTextAreaElement;
		const caret = el.selectionStart ?? 0;
		const m = wikiMatch(el.value, caret, get(questIndex));
		if (!m) {
			wiki = null;
			wikiDismissed = null;
			return;
		}
		const token = el.value.slice(m.from, m.to);
		if (wikiDismissed === token) {
			// 닫은 토큰 그대로면 재오픈 안 함.
			wiki = null;
			return;
		}
		wikiDismissed = null;
		wiki = placeWiki(el, m.from, m.to, m.items);
		wikiSel = 0;
		// BUG-209: 팝업 <ul> 은 후보가 바뀌어도 같은 엘리먼트라 **이전 스크롤 위치가
		// 남는다**. 그러면 선택은 0 번인데 화면엔 중간이 보여 "팝업 밖 항목이
		// 선택된" 상태가 되고, 다음 ↑/↓ 이 엉뚱하게 튀는 것처럼 보였다. 키보드
		// 이동과 같은 경로로 선택 항목을 보이는 위치까지 스크롤시킨다.
		wikiSelFromKeyboard = true;
	}

	// caret 기준 팝업 위치 — 화면 밖이면 숨김 + 좌우 clamp.
	// 아래로 뜨면 top=caret 아래(기존 그대로), 위로 뜨면 bottom anchor 로 caret 바로 위
	// (팝업 높이와 무관 — 추정 오차로 입력부를 가리던 문제 해결).
	function placeWiki(
		el: HTMLTextAreaElement,
		from: number,
		to: number,
		items: WikiItem[]
	): typeof wiki {
		const c = caretXY(el, to);
		const rect = el.getBoundingClientRect();
		const caretTop = rect.top + c.top - el.scrollTop;
		const caretBottom = caretTop + c.height;
		// caret(자동완성 대상)이 입력창 보이는 영역 ∩ 뷰포트 밖이면 팝업 숨김.
		if (!isWikiCaretVisible(caretTop, caretBottom, rect.top, rect.bottom)) return null;
		const rawLeft = rect.left + c.left - el.scrollLeft;
		const left = clampWikiLeft(rawLeft);
		// 세로 배치는 wikiPlace 가 실제 높이를 재서 결정한다 — 여기선 caret 좌표만.
		return { el, from, to, items, left, caretTop, caretBottom };
	}

	function repositionWiki() {
		if (!wiki) return;
		wiki = placeWiki(wiki.el, wiki.from, wiki.to, wiki.items);
	}
	// 닫기(Esc/클릭아웃) — 현재 토큰을 기억해 즉시 재오픈 방지.
	function dismissWiki() {
		if (wiki) wikiDismissed = wiki.el.value.slice(wiki.from, wiki.to);
		wiki = null;
	}
	$effect(() => {
		if (!wiki) return;
		const onMove = () => repositionWiki();
		const onDown = (ev: MouseEvent) => {
			if (!wiki) return;
			const tgt = ev.target as Node;
			// 팝업/현재 textarea 내부 클릭(옵션 선택·캐럿 이동)은 닫지 않음.
			if (wikiPopEl?.contains(tgt) || wiki.el === tgt) return;
			dismissWiki();
		};
		// capture=true 로 textarea 내부/조상 스크롤·외부 클릭까지 포착.
		window.addEventListener('scroll', onMove, true);
		window.addEventListener('resize', onMove);
		window.addEventListener('mousedown', onDown, true);
		return () => {
			window.removeEventListener('scroll', onMove, true);
			window.removeEventListener('resize', onMove);
			window.removeEventListener('mousedown', onDown, true);
		};
	});
	function applyWiki(item: WikiItem) {
		if (!wiki) return;
		if (item.nsPrefix) {
			// DEV-219 후속: 네임스페이스 접두만 삽입(`]]` 안 닫음) — execCommand 가
			// 동기로 발화하는 input 이벤트를 타고 onWikiInput 이 이미 그 kind 로
			// 필터된 다음 후보로 wiki state 를 갱신했으므로 여기서 건드리지 않음.
			applyWikiPrefix(wiki.el, wiki.from, wiki.to, item.insert ?? item.id);
			return;
		}
		// DEV-173: 규칙은 원본 대소문자 slug 로 삽입 (insert 우선).
		applyWikiLink(wiki.el, wiki.from, wiki.to, item.insert ?? item.id);
		wiki = null;
		wikiDismissed = null;
	}
	// VS 식 키보드 네비/적용 (팝업 떠 있을 때만 가로챔).
	function onWikiKeydown(e: KeyboardEvent) {
		if (!wiki) return;
		const n = wiki.items.length;
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			wikiSelFromKeyboard = true;
			wikiSel = (wikiSel + 1) % n;
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			wikiSelFromKeyboard = true;
			wikiSel = (wikiSel - 1 + n) % n;
		} else if (e.key === 'Enter' || e.key === 'Tab') {
			// Tab 도 적용. tabInsert(use:action) 의 탭 삽입을 막으려 즉시 전파 중단.
			e.preventDefault();
			e.stopImmediatePropagation();
			applyWiki(wiki.items[wikiSel]);
		} else if (e.key === 'Escape') {
			e.preventDefault();
			e.stopImmediatePropagation();
			dismissWiki();
		}
	}

	// DEV-100: scope — quest (기본) / campaign. API base 만 다름.
	// BUG-083(잠정): 댓글 첨부 버튼은 제거, paste/drop 은 미디어(이미지/동영상)만
	// 인라인 허용. per-comment 첨부 기능은 On Hold.
	let { slug, scope = 'quest' }: { slug: string; scope?: 'quest' | 'campaign' } = $props();
	const commentsApi = $derived(scope === 'campaign' ? campaignCommentsApi : questCommentsApi);

	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let entries = $state<CommentEntry[]>([]);

	// DEV-107 fix1: 섹션 접기 — 사용자 피드백 반영해 localStorage 영속 제거.
	// 매 진입 시 펼침 기본. 일회성 토글.
	let collapsed = $state(false);
	function toggleCollapsed() {
		collapsed = !collapsed;
	}

	// DEV-107 fix1: root entry (top-level 댓글) 별 답글 접기.
	// 클릭 시 그 root 의 답글 전체 숨김 (들여쓰기 손실 없음 — 그냥 표시 안 함).
	//
	// DEV-235: 이건(섹션 전체 접기와 달리, 위 DEV-107 주석 참고) 페이지 이동
	// 후에도 유지되길 원한다는 admin 보고 — 길드+quest/campaign 별
	// localStorage 로 영속. 아래 persistReady/guildKeyPrefix 참조.
	let collapsedRoots = $state(new Set<number>());
	function toggleRootCollapsed(rootId: number) {
		const next = new Set(collapsedRoots);
		if (next.has(rootId)) next.delete(rootId);
		else next.add(rootId);
		collapsedRoots = next;
	}

	// DEV-235: 접기 상태 localStorage 키 — 길드(guildKeyPrefix) + scope + slug
	// 별로 독립. slug 가 바뀌어도(같은 컴포넌트 인스턴스가 재사용되는 경우)
	// 다시 계산되도록 함수로.
	let guildKeyPrefix = $state('');
	let prefixReady = $state(false);
	// restorePersisted() 가 state 를 채우는 동안(및 그 직후 첫 반영) 아래
	// 저장용 $effect 들이 "빈 값으로 되읽어와 저장"하며 방금 복원한 값을
	// 다시 저장하는 것 자체는 무해하지만, slug 전환 중간에 이전 slug 값으로
	// 잘못 저장되는 걸 막기 위한 가드.
	let persistReady = $state(false);
	function collapseStorageKey(suffix: string): string {
		return guildKey(guildKeyPrefix, `comments.${scope}.${slug}.${suffix}`);
	}
	function readIdSet(key: string): Set<number> {
		try {
			const raw = localStorage.getItem(key);
			if (!raw) return new Set();
			const arr = JSON.parse(raw);
			return Array.isArray(arr) ? new Set(arr.filter((n) => typeof n === 'number')) : new Set();
		} catch {
			return new Set();
		}
	}
	function writeIdSet(key: string, s: Set<number>) {
		try {
			localStorage.setItem(key, JSON.stringify([...s]));
		} catch {
			/* 무시 */
		}
	}
	function restorePersisted() {
		persistReady = false;
		collapsedRoots = readIdSet(collapseStorageKey('collapsedRoots'));
		collapsedBodies = readIdSet(collapseStorageKey('collapsedBodies'));
		persistReady = true;
	}
	onMount(async () => {
		guildKeyPrefix = await resolveGuildKeyPrefix();
		prefixReady = true;
	});
	// slug/scope 가 바뀔 때(라우트 전환으로 컴포넌트가 재사용되는 경우)도
	// 그 quest/campaign 전용 값으로 다시 복원.
	$effect(() => {
		void slug;
		void scope;
		if (!prefixReady) return;
		restorePersisted();
	});
	$effect(() => {
		const s = collapsedRoots;
		if (!persistReady) return;
		writeIdSet(collapseStorageKey('collapsedRoots'), s);
	});
	$effect(() => {
		const s = collapsedBodies;
		if (!persistReady) return;
		writeIdSet(collapseStorageKey('collapsedBodies'), s);
	});

	// DEV-108: 이모지 반응 — 고정 4종.
	// DEV-132: + 사용자 커스텀(길드 전체 — quest/campaign 무관하게 공유,
	// "자주 쓰는 이모지" 개념이라 scope/slug 로 나누지 않음). localStorage
	// 영속 — 저장 포맷(reactions attr)은 이미 임의 문자열이라 backend 변경 없음.
	// DEV-139: 전체 노출 대신 slack 스타일 — 활성 pill + '+' popup picker.
	const REACTION_SET = ['👍', '✅', '❓', '❌']; // emoji-ok: 반응 자체가 이모지 (DEV-108)
	let customReactions = $state<string[]>([]);
	function customReactionsKey(): string {
		return guildKey(guildKeyPrefix, 'commentCustomReactions');
	}
	$effect(() => {
		if (!prefixReady) return;
		try {
			const raw = localStorage.getItem(customReactionsKey());
			const arr = raw ? JSON.parse(raw) : [];
			customReactions = Array.isArray(arr) ? arr.filter((x) => typeof x === 'string') : [];
		} catch {
			customReactions = [];
		}
	});
	function persistCustomReactions() {
		try {
			localStorage.setItem(customReactionsKey(), JSON.stringify(customReactions));
		} catch {
			/* 무시 */
		}
	}
	const allReactions = $derived([...REACTION_SET, ...customReactions]);
	let customEmojiInput = $state('');
	let customEmojiError = $state<string | null>(null);
	function addCustomReaction(id: number) {
		const emoji = customEmojiInput.trim();
		if (!emoji) return;
		if (!isSingleEmoji(emoji)) {
			customEmojiError = t('comment.emojiOne', $locale);
			return;
		}
		customEmojiError = null;
		customEmojiInput = '';
		if (!REACTION_SET.includes(emoji) && !customReactions.includes(emoji)) {
			customReactions = [...customReactions, emoji];
			persistCustomReactions();
		}
		toggleReaction(id, emoji);
	}
	function removeCustomReaction(emoji: string) {
		customReactions = customReactions.filter((e) => e !== emoji);
		persistCustomReactions();
	}
	let pickerOpenFor = $state<number | null>(null);
	// BUG-125(admin 보고): 이모지 버튼이 foot-left(왼쪽)로 옮겨진 뒤에도 팝업이
	// CSS `right:0` 로 여전히 버튼 왼쪽으로(=화면 밖으로) 펼쳐졌음. 자동완성
	// 팝업(placeWiki)과 동일하게 버튼 위치에서 JS 로 계산 + 화면 경계 clamp —
	// 오른쪽으로 펼치되 뷰포트를 넘지 않도록.
	let reactionPickerPos = $state<{
		left: number;
		top: number | null;
		bottom: number | null;
	} | null>(null);
	// BUG-131(admin 보고): "자동완성 팝업이랑 같은 방식" 이라고 해놓고 정작
	// scroll 추종(repositionWiki 에 해당하는 부분)을 안 만들었다 — 열 때 딱
	// 한 번만 위치를 계산해서, 목록을 스크롤하면 버튼은 움직이는데 팝업만
	// 그 자리에 고정돼 있었다. 트리거 버튼을 기억해뒀다가 scroll/resize 마다
	// 다시 계산 — wiki 팝업의 repositionWiki 와 동일 패턴.
	let reactionPickerBtn: HTMLElement | null = null;
	// BUG-132(admin 보고): 버튼이 스크롤로 화면 밖으로 완전히 나가면(wiki
	// 팝업의 placeWiki 가 `caretBottom < visTop || caretTop > visBottom` 일 때
	// null 반환해 숨기는 것과 동일 케이스) 팝업이 허공에 떠서 남아있으면 안
	// 됨 — 트리거가 안 보이면 팝업도 닫는다.
	function computeReactionPickerPos(btn: HTMLElement) {
		const rect = btn.getBoundingClientRect();
		const offscreen =
			rect.bottom <= 0 ||
			rect.top >= window.innerHeight ||
			rect.right <= 0 ||
			rect.left >= window.innerWidth;
		if (offscreen) {
			pickerOpenFor = null;
			reactionPickerPos = null;
			reactionPickerBtn = null;
			return;
		}
		const POPUP_W = 168; // .reaction-picker min-width(9rem=144px) + 여유.
		const left = Math.max(4, Math.min(rect.left, window.innerWidth - POPUP_W - 4));
		const spaceAbove = rect.top;
		const spaceBelow = window.innerHeight - rect.bottom;
		// 위쪽 공간이 좁고 아래가 더 넓으면 아래로 펼침(자동완성 팝업의 flip 과 동일 원리).
		if (spaceAbove < 220 && spaceBelow > spaceAbove) {
			reactionPickerPos = { left, top: rect.bottom + 4, bottom: null };
		} else {
			reactionPickerPos = { left, top: null, bottom: window.innerHeight - rect.top + 4 };
		}
	}
	function toggleReactionPicker(ev: MouseEvent, id: number) {
		if (pickerOpenFor === id) {
			pickerOpenFor = null;
			reactionPickerPos = null;
			reactionPickerBtn = null;
			return;
		}
		const btn = ev.currentTarget as HTMLElement;
		reactionPickerBtn = btn;
		computeReactionPickerPos(btn);
		pickerOpenFor = id;
		customEmojiInput = '';
		customEmojiError = null;
	}
	// 열려 있는 동안 스크롤/리사이즈 시마다 트리거 버튼의 새 위치로 재계산
	// (wiki 팝업의 repositionWiki 와 동일 — capture:true 로 조상 스크롤 컨테이너도 포착).
	$effect(() => {
		if (pickerOpenFor == null || !reactionPickerBtn) return;
		const btn = reactionPickerBtn;
		const onReposition = () => computeReactionPickerPos(btn);
		window.addEventListener('scroll', onReposition, true);
		window.addEventListener('resize', onReposition);
		return () => {
			window.removeEventListener('scroll', onReposition, true);
			window.removeEventListener('resize', onReposition);
		};
	});
	// DEV-108: reaction 항목 = "emoji" 또는 "emoji:author1|author2".
	// 누가 반응했는지 호버로 보여주기 위해 파싱.
	function parseReaction(r: string): { emoji: string; authors: string[] } {
		const idx = r.indexOf(':');
		if (idx < 0) return { emoji: r, authors: [] };
		return {
			emoji: r.slice(0, idx),
			authors: r
				.slice(idx + 1)
				.split('|')
				.map((a) => a.trim())
				.filter((a) => a.length > 0)
		};
	}
	function reactionsOf(e: CommentEntry): { emoji: string; authors: string[] }[] {
		return (e.reactions ?? []).map(parseReaction);
	}
	// 현재 사용자(=댓글 작성자 이름). 비어있으면 core 가 '(익명)' 처리.
	function currentAuthor(): string {
		return newAuthor.trim() || loadSavedAuthor();
	}
	function reactedByMe(authors: string[]): boolean {
		const me = currentAuthor().trim() || t('comment.anonymous', $locale);
		return authors.includes(me);
	}

	async function toggleReaction(id: number, emoji: string) {
		pickerOpenFor = null;
		reactionPickerBtn = null;
		try {
			const updated = await commentsApi.toggleReaction(slug, id, emoji, currentAuthor());
			entries = entries.map((e) => (e.id === id ? updated : e));
		} catch (e) {
			showToast(e instanceof Error ? e.message : 'reaction failed', 'error');
		}
	}

	// DEV-234: 상단 고정(pin) — root 댓글만 (버튼도 root 에만 노출). 목록
	// 정렬은 아래 orderedRoots derived 가 담당, 원래 순서(시간순)는 pin 안에서 유지.
	async function togglePinned(id: number) {
		try {
			const updated = await commentsApi.togglePinned(slug, id);
			entries = entries.map((e) => (e.id === id ? updated : e));
		} catch (e) {
			showToast(e instanceof Error ? e.message : 'pin toggle failed', 'error');
		}
	}

	// DEV-142: 토론(discussion) 플래그 토글. discussion 댓글이 미해결이면
	// 이 quest 를 완료 상태로 전환할 수 없다 (core 게이트).
	async function toggleDiscussion(id: number) {
		try {
			const updated = await commentsApi.toggleDiscussion(slug, id);
			entries = entries.map((e) => (e.id === id ? updated : e));
		} catch (e) {
			showToast(e instanceof Error ? e.message : 'discussion toggle failed', 'error');
		}
	}
	// DEV-142: discussion 댓글 resolve 토글.
	async function toggleResolved(id: number) {
		try {
			const updated = await commentsApi.toggleResolved(slug, id);
			entries = entries.map((e) => (e.id === id ? updated : e));
		} catch (e) {
			showToast(e instanceof Error ? e.message : 'resolve toggle failed', 'error');
		}
	}

	// DEV-129: 댓글 '내용' 접기 — entry 단위 본문 collapse. 답글 접기 (위)
	// 와 별개 — 본문만 가리고 head (작성자/번호/액션) 는 유지.
	let collapsedBodies = $state(new Set<number>());
	function toggleBodyCollapsed(id: number) {
		const next = new Set(collapsedBodies);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		collapsedBodies = next;
	}

	// DEV-190: 모든 댓글의 답글(collapsedRoots) + 본문(collapsedBodies)을 일괄
	// 접기/펼치기. 댓글 섹션 자체 접기(collapsed)와 별개. 모든 entry 본문이
	// 접혀있으면 '전체 펼치기', 아니면 '전체 접기'.
	let allCollapsed = $derived(
		entries.length > 0 && entries.every((e) => collapsedBodies.has(e.id))
	);
	function toggleCollapseAll() {
		if (allCollapsed) {
			collapsedRoots = new Set();
			collapsedBodies = new Set();
		} else {
			collapsedRoots = new Set(groups.roots.map((r) => r.id));
			collapsedBodies = new Set(entries.map((e) => e.id));
		}
	}
	// 접었을 때 보여줄 1줄 미리보기 — markdown 마커 대충 제거.
	function bodyPreview(body: string): string {
		const firstLine =
			body
				.split('\n')
				.map((l) => l.trim())
				.find((l) => l.length > 0) ?? '';
		const plain = firstLine.replace(/^#+\s*/, '').replace(/[*_`>]/g, '');
		return plain.length > 80 ? plain.slice(0, 80) + '…' : plain;
	}

	// DEV-136: 마지막 작성자 기억 — 비우면 "(이름 없음)" 으로 떠서 매번
	// 입력해야 하는 마찰 제거. localStorage prefill, 저장 성공 시 갱신.
	const AUTHOR_KEY = 'openguild.commentAuthor';
	function loadSavedAuthor(): string {
		try {
			return localStorage.getItem(AUTHOR_KEY) ?? '';
		} catch {
			return '';
		}
	}
	function saveAuthor(name: string) {
		try {
			const n = name.trim();
			if (n) localStorage.setItem(AUTHOR_KEY, n);
		} catch {
			/* 무시 */
		}
	}

	// 신규 top-level 작성 폼
	let newAuthor = $state(loadSavedAuthor());
	let newBody = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);
	// DEV-289: 각 댓글 입력창의 마크다운 편집기 토글 상태.
	let newRich = $state(false);
	let editRich = $state(false);
	let replyRich = $state(false);
	// BUG-157: 댓글 textarea 도 overlay 스크롤바로 통일.
	let newBodyEl = $state<HTMLTextAreaElement | null>(null);
	let editBodyEl = $state<HTMLTextAreaElement | null>(null);
	let replyBodyEl = $state<HTMLTextAreaElement | null>(null);

	// 개별 편집 — 한 번에 하나만.
	let editingId = $state<number | null>(null);
	let editBody = $state('');
	let editSaving = $state(false);
	let editError = $state<string | null>(null);

	// 답글 작성 — 한 번에 한 parent.
	let replyingTo = $state<number | null>(null);
	let replyAuthor = $state(loadSavedAuthor());
	let replyBody = $state('');
	let replySaving = $state(false);

	// DEV-153: 새 댓글에 입력했거나 편집/답글이 열려 있으면 미저장 — 이탈 가드 보고.
	let commentsDirty = $derived(newBody.trim() !== '' || editingId !== null || replyingTo !== null);
	$effect(() => setUnsaved(`comments:${scope}`, commentsDirty));
	onDestroy(() => setUnsaved(`comments:${scope}`, false));
	let replyError = $state<string | null>(null);

	async function load() {
		loading = true;
		loadError = null;
		try {
			const res = await commentsApi.listComments(slug);
			entries = res.entries ?? [];
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

	// DEV-296: 작업기록에서 댓글 활동을 클릭하면 `?comment=N` 으로 들어온다 —
	// 문서까지만 이동하고 끝나면 긴 문서에서 그 댓글을 다시 찾아야 했다.
	//
	// 댓글은 비동기로 로드되므로 URL 만 보고 스크롤하면 대상 노드가 아직 없다.
	// `entries` 가 채워진 뒤(= 앵커 `#comment-N` 이 렌더된 뒤) 스크롤한다.
	// 같은 id 에 대해 한 번만 — 댓글 추가/새로고침 때마다 다시 튀지 않도록.
	let scrolledToCommentId: number | null = $state(null);
	/** BUG-258: 진행 중인 딥링크 스크롤. 새 대상/언마운트 때 끊는다. */
	let cancelJumpScroll: (() => void) | null = null;
	onDestroy(() => cancelJumpScroll?.());
	$effect(() => {
		const raw = $page.url.searchParams.get('comment');
		let target = raw ? Number(raw) : NaN;
		if (entries.length === 0) return;
		// BUG-238: 홈의 "토론 댓글" 컨베이어는 퀘스트만 알고 어느 댓글인지는
		// 모른 채 보낸다(`?focus=discussion`). 홈이 퀘스트마다 댓글을 미리
		// 받아오게 하는 대신, 이미 로드된 entries 에서 첫 미해결 토론 댓글을
		// 여기서 고른다 — 추가 요청이 없다.
		if (!Number.isFinite(target) && $page.url.searchParams.get('focus') === 'discussion') {
			const first = entries.find((e) => e.discussion && !e.resolved);
			if (first) target = first.id;
		}
		if (!Number.isFinite(target)) return;
		if (scrolledToCommentId === target) return;
		// 접혀 있으면 펼쳐야 앵커가 보인다.
		if (collapsed) collapsed = false;
		// 앵커가 실제로 그려질 때까지 짧게 재시도한다. `entries` 가 채워진 직후
		// 한 번(tick)만 보면 놓친다 — 댓글은 groups 파생 → 중첩 snippet 순으로
		// 그려져 DOM 반영이 한 프레임 뒤일 수 있고, 접힌 스레드가 펼쳐지는 것도
		// 기다려야 한다. 실기에서 단발 tick 은 스크롤이 아예 안 걸렸다.
		// BUG-238: 재시도를 rAF 에서 타이머로 바꿨다. 숨겨진 문서
		// (`visibilityState: hidden`)에서는 `requestAnimationFrame` 이 아예
		// 발화하지 않아 재시도 루프가 첫 실패에서 영구히 멈춘다. GUI 는 자식
		// 창을 지원하므로 배경 창에서 문서를 열면 실제로 스크롤이 조용히 안
		// 걸린다. 32ms 는 보이는 창에서 체감상 rAF 2프레임과 같다.
		const deadline = Date.now() + 3000;
		const anchorId = `comment-${target}`;
		const tryScroll = () => {
			const el = document.getElementById(anchorId);
			if (!el) {
				if (Date.now() < deadline) setTimeout(tryScroll, 32);
				return;
			}
			scrolledToCommentId = target;
			const reduce = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
			// BUG-258: 앵커가 생겼다고 바로 스크롤하면 안 된다. 상세 페이지는
			// 본문 마크다운 → 첨부 → 서브퀘스트 → 댓글 순으로 늦게 레이아웃되고,
			// 부드러운 스크롤은 시작 시점의 목표 오프셋을 향해 가므로 그 사이
			// 위쪽이 자라면 엉뚱한 곳에 선다(admin 보고: "페이지 끝까지
			// 내려가버린다"). 높이가 잦아든 뒤 스크롤하고, 이후에도 변하면
			// 다시 맞춘다.
			cancelJumpScroll?.();
			cancelJumpScroll = scrollIntoViewWhenSettled(
				() => document.getElementById(anchorId),
				{
					smooth: !reduce,
					onScrolled: (node) => {
						// 어느 댓글로 왔는지 잠깐 강조 — 스크롤만으로는 눈에 안 띈다.
						// 기다린 뒤에 켠다: 먼저 켜면 스크롤이 닿기 전에 꺼질 수 있다.
						node.classList.add('jump-target');
						setTimeout(() => node.classList.remove('jump-target'), 2200);
					}
				}
			);
		};
		void tick().then(tryScroll);
	});

	// 화면 구조 (DEV-200: 2단 들여쓰기):
	//   root (parent_id == null)
	//   └ level-1 답글 (parent 가 root)
	//     └ level-2 답글 (parent 가 level-1 이하 — 더 깊은 체인은 가장 가까운
	//       level-1 조상 밑에 flatten, ↩ #id 링크가 실제 대상 표시)
	// 단일-패스로 구현:
	//   1. id → entry 맵.
	//   2. 각 entry 마다 root_id 찾기 (parent_id chain 따라가서 None 인 entry).
	//   3. root 가 사라진 reply 는 "orphan" 표시.
	let groups = $derived.by(() => {
		const byId = new Map<number, CommentEntry>();
		for (const e of entries) byId.set(e.id, e);
		const rootOf = (id: number): number | null => {
			let cur = byId.get(id);
			const visited = new Set<number>();
			while (cur && cur.parent_id != null) {
				if (visited.has(cur.id)) return null; // cycle 방어
				visited.add(cur.id);
				const next = byId.get(cur.parent_id);
				if (!next) return null; // orphan
				cur = next;
			}
			return cur ? cur.id : null;
		};
		const roots: CommentEntry[] = [];
		// 전체 자손 (답글 수/스레드 접기 기준 — 기존과 동일).
		const childrenByRoot = new Map<number, CommentEntry[]>();
		// DEV-200: 2단 트리 — root 직속(level-1) + level-1 별 하위(level-2, flatten).
		const level1ByRoot = new Map<number, CommentEntry[]>();
		const level2ByParent = new Map<number, CommentEntry[]>();
		// entry id → root id (답글 폼을 어느 스레드에 띄울지 — root 자신 포함).
		const rootIdOf = new Map<number, number>();
		// BUG-116: entry id → 그 entry 가 속한 level-1 조상 id (level-1 자신은
		// 자기 자신). 답글의 답글 작성 시 폼을 "스레드 맨 아래"가 아니라 그
		// level-1 그룹 바로 아래에 띄우기 위한 조회용 — root 에는 없음.
		const level1AncestorOf = new Map<number, number>();
		const orphans: CommentEntry[] = [];
		for (const e of entries) {
			if (e.parent_id == null) {
				roots.push(e);
				childrenByRoot.set(e.id, []);
				level1ByRoot.set(e.id, []);
				rootIdOf.set(e.id, e.id);
			}
		}
		for (const e of entries) {
			if (e.parent_id == null) continue;
			const r = rootOf(e.id);
			if (r == null) {
				orphans.push(e);
				continue;
			}
			childrenByRoot.get(r)?.push(e);
			rootIdOf.set(e.id, r);
			if (e.parent_id === r) {
				level1ByRoot.get(r)?.push(e);
				level1AncestorOf.set(e.id, e.id);
			} else {
				// 가장 가까운 level-1 조상 (parent_id === r 인 entry) 밑에 배치.
				let cur = byId.get(e.parent_id);
				while (cur && cur.parent_id != null && cur.parent_id !== r) {
					cur = byId.get(cur.parent_id);
				}
				if (cur && cur.parent_id === r) {
					const arr = level2ByParent.get(cur.id) ?? [];
					arr.push(e);
					level2ByParent.set(cur.id, arr);
					level1AncestorOf.set(e.id, cur.id);
				} else {
					// 방어 — 조상 해석 실패 시 level-1 로.
					level1ByRoot.get(r)?.push(e);
					level1AncestorOf.set(e.id, e.id);
				}
			}
		}
		return {
			roots,
			childrenByRoot,
			level1ByRoot,
			level2ByParent,
			rootIdOf,
			byId,
			orphans,
			level1AncestorOf
		};
	});

	// DEV-234: 고정된 root 댓글을 맨 위로 — 고정끼리/일반끼리는 원래 순서
	// (id 순, groups.roots 가 이미 그 순서) 유지. stable sort (Array#sort 는
	// stable 보장) 이라 안전.
	let orderedRoots = $derived.by(() => {
		const roots = groups.roots;
		if (!roots.some((r) => r.pinned)) return roots;
		return [...roots].sort((a, b) => (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0));
	});

	// DEV-213: 토론 댓글만 모아보기 (quest 전용 — discussion 은 quest 한정 기능).
	// 스레드 문맥 보존: root 카드는 "스레드 안에 토론이 하나라도 있으면" 표시하고,
	// 스레드 내부의 비토론 entry 는 숨기지 않고 dim 처리(대화 흐름 유지).
	let discussionOnly = $state(false);
	let discussionCount = $derived(entries.filter((e) => e.discussion).length);
	let unresolvedCount = $derived(entries.filter((e) => e.discussion && !e.resolved).length);
	// 토론을 포함한 스레드의 root id 집합.
	let discussionRoots = $derived.by(() => {
		const set = new Set<number>();
		for (const e of entries) {
			if (!e.discussion) continue;
			const r = e.parent_id == null ? e.id : groups.rootIdOf.get(e.id);
			if (r != null) set.add(r);
		}
		return set;
	});

	// DEV-214: 접힌 스레드 안(자손)에 토론이 있으면 상태 표시 — 접힘 때문에
	// 미해결 토론을 놓치는 문제 방지. root 자신은 접혀도 보이므로 자손만 집계.
	// 'unresolved' 가 하나라도 있으면 unresolved 우선.
	let threadDiscState = $derived.by(() => {
		const map = new Map<number, 'unresolved' | 'resolved'>();
		for (const [rootId, children] of groups.childrenByRoot) {
			let state: 'unresolved' | 'resolved' | null = null;
			for (const c of children) {
				if (!c.discussion) continue;
				if (!c.resolved) {
					state = 'unresolved';
					break;
				}
				state = 'resolved';
			}
			if (state) map.set(rootId, state);
		}
		return map;
	});

	// DEV-200: 답글 대상이 답글이어도 폼은 그 스레드(root 카드) 안에 표시.
	let replyFormRoot = $derived(
		replyingTo == null ? null : (groups.rootIdOf.get(replyingTo) ?? null)
	);
	let replyTarget = $derived(replyingTo == null ? null : (groups.byId.get(replyingTo) ?? null));
	// BUG-116: root 에 답글 쓰면 폼은 스레드 맨 아래(새 level-1 자리, null).
	// level-1/level-2 에 답글 쓰면 그 level-1 그룹 바로 아래 — 예전엔 항상
	// 스레드 맨 아래에 떠서 "답글의 답글" 쓸 때 입력창이 대상에서 멀리
	// 떨어져 보였다 (admin 보고).
	let replyFormLevel1 = $derived(
		replyingTo == null || replyTarget?.parent_id == null
			? null
			: (groups.level1AncestorOf.get(replyingTo) ?? null)
	);

	function formatTs(ts: string): string {
		if (!ts) return t('comment.unknownTime', $locale);
		try {
			const d = new Date(ts);
			if (Number.isNaN(d.getTime())) return ts;
			return d.toLocaleString();
		} catch {
			return ts;
		}
	}

	async function add() {
		if (!newBody.trim()) {
			saveError = t('comment.bodyRequired', $locale);
			return;
		}
		saving = true;
		saveError = null;
		try {
			const entry = await commentsApi.addComment(slug, newBody, newAuthor, null);
			saveAuthor(newAuthor); // DEV-136: 성공 시 기억.
			entries = [...entries, entry];
			newBody = '';
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'save failed';
		} finally {
			saving = false;
		}
	}

	function enterEdit(e: CommentEntry) {
		editingId = e.id;
		editBody = e.body;
		editError = null;
	}
	function cancelEdit() {
		editingId = null;
		editBody = '';
		editError = null;
	}

	async function saveEdit(id: number, keepEditing = false) {
		if (editSaving) return;
		if (!editBody.trim()) {
			editError = t('comment.bodyRequired', $locale);
			return;
		}
		editSaving = true;
		editError = null;
		try {
			const updated = await commentsApi.updateComment(slug, id, editBody);
			entries = entries.map((e) => (e.id === id ? updated : e));
			if (!keepEditing) cancelEdit();
		} catch (e) {
			editError = e instanceof Error ? e.message : 'save failed';
		} finally {
			editSaving = false;
		}
	}
	// DEV-118: 인앱 confirm 모달용 state.
	let confirmDeleteId = $state<number | null>(null);
	function askRemove(id: number) {
		confirmDeleteId = id;
	}
	async function remove() {
		const id = confirmDeleteId;
		if (id === null) return;
		confirmDeleteId = null;
		try {
			await commentsApi.deleteComment(slug, id);
			entries = entries.filter((e) => e.id !== id);
		} catch (e) {
			showToast(e instanceof Error ? e.message : 'delete failed', 'error');
		}
	}

	// DEV-120: 답글 폼 자동 focus + scroll.
	// 원 댓글이 길면 폼이 화면 밖에 나타나서 "↩ 답글" 클릭 후 아무 일도 안 일어난
	// 것처럼 보임. 폼이 mount 된 후 textarea focus + 화면 중앙으로 scroll.
	async function enterReply(parentId: number) {
		replyingTo = parentId;
		replyBody = '';
		replyError = null;
		// BUG-116: 대상이 root 가 아니면(level-1/level-2 답글) 폼이 그 level-1
		// 그룹 옆에 뜨는데, 스레드가 접혀 있으면 그 그룹 자체가 안 그려져
		// 폼도 같이 숨는다 — 먼저 펼침.
		const rootId = groups.rootIdOf.get(parentId);
		if (rootId != null && rootId !== parentId && collapsedRoots.has(rootId)) {
			const next = new Set(collapsedRoots);
			next.delete(rootId);
			collapsedRoots = next;
		}
		await tick();
		// 새로 mount 된 .reply-form 의 textarea — 한 번에 한 폼만 떠 있음.
		const form = document.querySelector<HTMLElement>('.reply-form');
		const ta = form?.querySelector<HTMLTextAreaElement>('textarea.body-input');
		if (!ta) return;
		try {
			ta.scrollIntoView({ behavior: 'smooth', block: 'center' });
		} catch {
			// 일부 환경에서 옵션 미지원 — fallback.
			ta.scrollIntoView();
		}
		ta.focus({ preventScroll: true });
	}
	function cancelReply() {
		replyingTo = null;
		replyBody = '';
		replyError = null;
	}

	async function submitReply(parentId: number) {
		if (!replyBody.trim()) {
			replyError = t('comment.bodyRequired', $locale);
			return;
		}
		replySaving = true;
		replyError = null;
		try {
			const entry = await commentsApi.addComment(slug, replyBody, replyAuthor, parentId);
			saveAuthor(replyAuthor); // DEV-136: 성공 시 기억.
			entries = [...entries, entry];
			cancelReply();
		} catch (e) {
			replyError = e instanceof Error ? e.message : 'save failed';
		} finally {
			replySaving = false;
		}
	}
</script>

{#snippet replyFormView(rootId: number)}
	<!-- BUG-116: root/level-1/level-2 어디에 답글 쓰든 같은 폼 마크업 —
	     호출 위치(스레드 하단 vs 특정 level-1 그룹 옆)만 다름. -->
	<div class="reply-form">
		<div class="reply-author">
			<input
				class="author-input"
				type="text"
				placeholder={t('comment.authorOpt', $locale)}
				bind:value={replyAuthor}
				disabled={replySaving}
			/>
			<button
				type="button"
				class="ce-toggle"
				class:active={replyRich}
				onclick={() => (replyRich = !replyRich)}
				title={t('comment.toggleEditor', $locale)}
				aria-pressed={replyRich}>M↓</button
			>
		</div>
		{#if replyRich}
			<MarkdownEditor
				bind:value={replyBody}
				onError={(m) => (replyError = m)}
				mediaOnly
				defaultHeight={160}
			/>
		{:else}
			<textarea
				use:tabInsert
				use:textareaAttach={{
					onError: (m) => (replyError = `${t('campaign.attachFailed', $locale)}: ${m}`),
					mediaOnly: true
				}}
				class="body-input"
				bind:this={replyBodyEl}
				bind:value={replyBody}
				oninput={onWikiInput}
				onkeyup={onWikiInput}
				onclick={onWikiInput}
				onkeydowncapture={onWikiKeydown}
				rows="3"
				placeholder={`↩ #${replyTarget?.id ?? rootId} ${replyTarget?.author || ''}${t('comment.replyToSuffix', $locale)}`}
				disabled={replySaving}
			></textarea>
			<OverlayScrollbar target={replyBodyEl ?? null} />
		{/if}
		{#if replyError}<p class="state err">{replyError}</p>{/if}
		<div class="actions">
			<button
				class="btn-save"
				onclick={() => submitReply(replyingTo ?? rootId)}
				disabled={replySaving || !replyBody.trim()}
			>
				{replySaving ? t('common.saving', $locale) : t('comment.addReply', $locale)}
			</button>
			<button class="btn-cancel" onclick={cancelReply} disabled={replySaving}>
				{t('common.cancel', $locale)}
			</button>
		</div>
	</div>
{/snippet}

{#snippet entryView(e: CommentEntry, isReply: boolean)}
	<!-- DEV-139: li → div — root + 답글을 하나의 카드 (entry-card) 로 감싸기 위해. -->
	<!-- DEV-213: 토론만 보기 모드에서 비토론 entry 는 숨기지 않고 dim (문맥 유지). -->
	<div
		class="entry"
		class:reply={isReply}
		class:dimmed={discussionOnly && !e.discussion}
		use:saveShortcut={{
			disabled: editingId !== e.id || editSaving,
			onSave: () => void saveEdit(e.id, true)
		}}
		class:pinned={isReply && e.pinned}
		id={`comment-${e.id}`}
	>
		<div class="entry-head">
			<!-- DEV-128 → DEV-139: 댓글 번호 — 클릭 시 본문 접기/펼치기 ('내용' 버튼 대체). -->
			<button
				class="entry-no"
				onclick={() => toggleBodyCollapsed(e.id)}
				aria-expanded={!collapsedBodies.has(e.id)}
				title={collapsedBodies.has(e.id)
					? `#${e.id} ${t('comment.expandBody', $locale)}`
					: `#${e.id} ${t('comment.collapseBody', $locale)}`}>#{e.id}</button
			>
			{#if e.parent_id != null}
				<a
					class="reply-to"
					href={`#comment-${e.parent_id}`}
					title={`#${e.parent_id} ${t('comment.jumpToComment', $locale)}`}>↩ #{e.parent_id}</a
				>
			{/if}
			<span class="author">{e.author || t('comment.noName', $locale)}</span>
			<span class="sep">·</span>
			<time class="ts" datetime={e.ts}>{formatTs(e.ts)}</time>
			<!-- DEV-182: 편집된 댓글 표시 — hover 시 편집 시각. -->
			{#if e.edited_at}
				<span
					class="edited-marker"
					title={`${t('comment.editedTitle', $locale)}${formatTs(e.edited_at)}`}
					>{t('comment.edited', $locale)}</span
				>
			{/if}
			<!-- DEV-142: 토론 댓글 상태 배지 — 미해결이면 완료 차단 (quest 한정).
			     클릭으로 resolve 토글. -->
			{#if scope === 'quest' && e.discussion}
				<button
					class="disc-badge"
					class:resolved={e.resolved}
					onclick={() => toggleResolved(e.id)}
					title={e.resolved
						? t('comment.resolvedTitle', $locale)
						: t('comment.unresolvedTitle', $locale)}
					>{e.resolved ? t('comment.resolved', $locale) : t('comment.unresolved', $locale)}</button
				>
			{/if}
			{#if editingId !== e.id}
				<div class="entry-actions">
					{#if scope === 'quest'}
						<button
							class="link-btn"
							class:on={e.discussion}
							onclick={() => toggleDiscussion(e.id)}
							title={e.discussion
								? t('comment.unmarkDiscussion', $locale)
								: t('comment.markDiscussion', $locale)}
							><Icon name="comment" size={12} /> {t('comment.discussion', $locale)}</button
						>
					{/if}
					<button class="link-btn" onclick={() => enterEdit(e)}
						>✎ {t('detail.edit', $locale)}</button
					>
					<button class="link-btn danger" onclick={() => askRemove(e.id)}
						>× {t('detail.delete', $locale)}</button
					>
				</div>
			{:else}
				<!-- BUG-232 후속: 편집기 토글도 작성자·시각과 같은 헤더 행에 둔다.
				     별도 ce-head 행이 본문 위에 빈 세로 간격을 만들던 원인이었다. -->
				<div class="entry-actions">
					<button
						type="button"
						class="ce-toggle"
						class:active={editRich}
						onclick={() => (editRich = !editRich)}
						title={t('comment.toggleEditor', $locale)}
						aria-pressed={editRich}>M↓</button
					>
				</div>
			{/if}
		</div>
		{#if editingId === e.id}
			{#if editRich}
				<MarkdownEditor
					bind:value={editBody}
					onError={(m) => (editError = m)}
					mediaOnly
					defaultHeight={200}
				/>
			{:else}
				<textarea
					use:tabInsert
					use:textareaAttach={{
						onError: (m) => (editError = `${t('campaign.attachFailed', $locale)}: ${m}`),
						mediaOnly: true
					}}
					class="body-input"
					bind:this={editBodyEl}
					bind:value={editBody}
					oninput={onWikiInput}
					onkeyup={onWikiInput}
					onclick={onWikiInput}
					onkeydowncapture={onWikiKeydown}
					rows="4"
					placeholder={t('comment.bodyMarkdown', $locale)}
				></textarea>
				<OverlayScrollbar target={editBodyEl ?? null} />
			{/if}
			{#if editError}<p class="state err">{editError}</p>{/if}
			<div class="actions">
				<button class="btn-save" onclick={() => saveEdit(e.id)} disabled={editSaving}>
					{editSaving ? t('common.saving', $locale) : t('common.save', $locale)}
				</button>
				<button class="btn-cancel" onclick={cancelEdit} disabled={editSaving}
					>{t('common.cancel', $locale)}</button
				>
			</div>
		{:else if collapsedBodies.has(e.id)}
			<!-- DEV-129: 접힌 본문 — 1줄 미리보기, 클릭으로 펼침.
			     DEV-214: 이 entry 자신이 토론이면 상태 글리프를 미리보기 앞에. -->
			<button
				class="body-collapsed"
				onclick={() => toggleBodyCollapsed(e.id)}
				title={t('comment.expandContent', $locale)}
			>
				{#if e.discussion}
					<span class="disc-flag" class:unresolved={!e.resolved} class:resolved={e.resolved}
						>{e.resolved ? '✓' : '✗'}</span
					>
				{/if}
				{bodyPreview(e.body)}
			</button>
		{:else}
			<div class="entry-body">
				<MarkdownView source={e.body} />
			</div>
		{/if}
		{#if editingId !== e.id}
			{@const reacts = reactionsOf(e)}
			<!-- DEV-139: 푸터 행 — 좌측 답글 컨트롤 / 우측 이모지 (slack 스타일). -->
			<div class="entry-foot">
				<div class="foot-left">
					{#if !isReply}
						{@const childCount = (groups.childrenByRoot.get(e.id) ?? []).length}
						{@const isThreadCollapsed = collapsedRoots.has(e.id)}
						{#if childCount > 0}
							<!-- 삼각형만 클릭 — '답글 n' 텍스트는 표시 전용. -->
							<button
								class="tri-btn"
								onclick={() => toggleRootCollapsed(e.id)}
								aria-expanded={!isThreadCollapsed}
								title={isThreadCollapsed
									? t('comment.expandReplies', $locale)
									: t('comment.collapseReplies', $locale)}>{isThreadCollapsed ? '▶' : '▼'}</button
							>
							<span class="reply-count">{t('comment.replies', $locale)} {childCount}</span>
							<!-- DEV-214: 접힌 답글 안에 토론 있으면 상태 글리프. -->
							{#if isThreadCollapsed}
								{@const disc = threadDiscState.get(e.id)}
								{#if disc === 'unresolved'}
									<span
										class="disc-flag unresolved"
										title={t('comment.collapsedHasUnresolved', $locale)}>✗</span
									>
								{:else if disc === 'resolved'}
									<span
										class="disc-flag resolved"
										title={t('comment.collapsedAllResolved', $locale)}>✓</span
									>
								{/if}
							{/if}
						{/if}
					{/if}
					<!-- DEV-200: 답글에도 답글 쓰기 — parent_id 로 대상 기록, 표시는 2단까지. -->
					<button class="reply-write-btn" onclick={() => enterReply(e.id)}
						>{t('comment.writeReply', $locale)}</button
					>
					<!-- DEV-132 후속(admin 요청): 이모지(반응 추가) 버튼을 답글 쓰기
					     버튼 오른쪽으로 이동 — foot-right 에서 여기로. -->
					<div class="picker-wrap">
						<button
							class="reaction-add"
							onclick={(ev) => toggleReactionPicker(ev, e.id)}
							aria-expanded={pickerOpenFor === e.id}
							title={t('comment.addReaction', $locale)}>☺+</button
						>
						{#if pickerOpenFor === e.id && reactionPickerPos}
							<div
								class="picker-ov"
								role="presentation"
								onclick={() => {
									pickerOpenFor = null;
									reactionPickerPos = null;
									reactionPickerBtn = null;
								}}
							></div>
							<div
								class="reaction-picker"
								role="menu"
								style:left="{reactionPickerPos.left}px"
								style:top={reactionPickerPos.top != null ? `${reactionPickerPos.top}px` : null}
								style:bottom={reactionPickerPos.bottom != null
									? `${reactionPickerPos.bottom}px`
									: null}
							>
								<div class="picker-row">
									{#each allReactions as emoji (emoji)}
										<div class="picker-item-wrap">
											<button
												class="picker-item"
												class:on={reactedByMe(reacts.find((x) => x.emoji === emoji)?.authors ?? [])}
												onclick={() => toggleReaction(e.id, emoji)}>{emoji}</button
											>
											<!-- DEV-132: 고정 4종은 제거 불가, 커스텀만 x 로 삭제. -->
											{#if !REACTION_SET.includes(emoji)}
												<button
													class="picker-item-rm"
													title={t('comment.removeCustomReaction', $locale)}
													aria-label="{emoji} {t('detail.delete', $locale)}"
													onclick={(ev) => {
														ev.stopPropagation();
														removeCustomReaction(emoji);
													}}>×</button
												>
											{/if}
										</div>
									{/each}
								</div>
								<!-- DEV-132: 직접 입력 — 고정 4종 외 임의 이모지 추가(길드 전체 재사용).
								     DEV-132 후속(admin 보고): 길이 제한 없이 임의 문자열이 그대로
								     들어갈 수 있던 문제 — 이모지 1개만 허용(addCustomReaction 검증). -->
								<div class="picker-add-row">
									<input
										class="picker-add-input"
										type="text"
										placeholder={t('comment.oneEmoji', $locale)}
										maxlength="16"
										bind:value={customEmojiInput}
										oninput={() => (customEmojiError = null)}
										onkeydown={(ev) => ev.key === 'Enter' && addCustomReaction(e.id)}
									/>
									<button
										class="picker-add-btn"
										disabled={!customEmojiInput.trim()}
										onclick={() => addCustomReaction(e.id)}>{t('common.add', $locale)}</button
									>
								</div>
								{#if customEmojiError}<p class="picker-add-err">{customEmojiError}</p>{/if}
							</div>
						{/if}
					</div>
				</div>
				<div class="foot-right">
					{#each reacts as r (r.emoji)}
						<!-- DEV-108: 호버하면 누가 반응했는지 (authors) 표시. -->
						<button
							class="reaction-pill"
							class:mine={reactedByMe(r.authors)}
							onclick={() => toggleReaction(e.id, r.emoji)}
							title={r.authors.length
								? `${r.authors.join(', ')} · ${t('comment.clickToggle', $locale)}`
								: t('comment.clickToggle', $locale)}
						>
							{r.emoji}{#if r.authors.length > 1}<span class="rc">{r.authors.length}</span>{/if}
						</button>
					{/each}
					<!-- DEV-234 후속(admin 요청): 상단 고정 버튼을 오른쪽 아래로 이동 —
					     foot-left 에서 여기로. 답글도 고정 가능(admin 요청) — root 만
					     스레드 정렬(orderedRoots)에 반영되고, 답글은 정렬 없이 강조
					     테두리(.entry.pinned)만. -->
					<button
						class="pin-btn"
						class:on={e.pinned}
						onclick={() => togglePinned(e.id)}
						title={e.pinned ? t('comment.unpin', $locale) : t('comment.pin', $locale)}
						><Icon name="pin" size={12} /></button
					>
				</div>
			</div>
		{/if}
	</div>
{/snippet}

<section class="comments-sec">
	<div class="section-head">
		<!-- DEV-107: 섹션 토글 — title 전체 클릭 가능. -->
		<button
			type="button"
			class="section-toggle"
			onclick={toggleCollapsed}
			aria-expanded={!collapsed}
			title={collapsed
				? t('comment.expandComments', $locale)
				: t('comment.collapseComments', $locale)}
		>
			<span class="toggle-icon" class:collapsed>▼</span>
			<h2 class="section-title">{t('comment.title', $locale)}</h2>
		</button>
		<span class="count">{entries.length}</span>
		<!-- DEV-214: 섹션이 접혀 있어도 미해결 토론은 보이게 (완료 차단과 직결). -->
		{#if collapsed && unresolvedCount > 0}
			<span
				class="disc-flag unresolved"
				title="{t('comment.unresolvedPre', $locale)}{unresolvedCount}{t(
					'comment.unresolvedPost',
					$locale
				)}">✗ {unresolvedCount}</span
			>
		{/if}
		<!-- DEV-213: 토론만 모아보기 — quest 전용, 토론 댓글이 있을 때만 노출. -->
		{#if scope === 'quest' && !collapsed && discussionCount > 0}
			<button
				class="disc-filter-btn"
				class:on={discussionOnly}
				onclick={() => (discussionOnly = !discussionOnly)}
				aria-pressed={discussionOnly}
				title={discussionOnly
					? t('comment.showAll', $locale)
					: t('comment.showDiscussionOnly', $locale)}
			>
				<Icon name="comment" size={12} />
				{t('comment.discussionOnly', $locale)}
				{discussionCount}{#if unresolvedCount > 0}&nbsp;({t('comment.unresolvedWord', $locale)}
					{unresolvedCount}){/if}
			</button>
		{/if}
		<!-- DEV-190: 전체 접기/펼치기 — 모든 댓글 답글+본문 일괄. 섹션 토글과 별개. -->
		{#if !collapsed && entries.length > 0}
			<button
				class="collapse-all-btn"
				onclick={toggleCollapseAll}
				title={allCollapsed
					? t('comment.expandAllTitle', $locale)
					: t('comment.collapseAllTitle', $locale)}
			>
				{allCollapsed ? t('comment.expandAll', $locale) : t('comment.collapseAll', $locale)}
			</button>
		{/if}
	</div>

	{#if !collapsed}
		{#if loading}
			<p class="state">Loading…</p>
		{:else if loadError}
			<p class="state err">{loadError}</p>
		{:else}
			{#if entries.length === 0}
				<p class="no-desc">{t('comment.empty', $locale)}</p>
			{:else}
				<ul class="entry-list">
					{#each orderedRoots as root (root.id)}
						{#if !discussionOnly || discussionRoots.has(root.id)}
							{@const childCount = (groups.childrenByRoot.get(root.id) ?? []).length}
							<!-- BUG-178: 예전엔 `!discussionOnly &&` 가 붙어 '토론만' 모드에서 접힘
							     상태가 무조건 무시됐다 — 전체접기를 눌러도 답글이 그대로 펼쳐져
							     있었다. dim(필터의 표현)과 접기(사용자가 누른 동작)는 다른 축이라
							     필터가 접기를 덮어쓸 이유가 없다(DEV-213 은 '숨기지 말 것'까지다). -->
							{@const isCollapsed = collapsedRoots.has(root.id)}
							<!-- DEV-139: root + 답글을 하나의 카드로 — 댓글 간 시각 구분.
							     DEV-234: pinned 면 강조 테두리(하이라이트) 도 함께. -->
							<li class="entry-card" class:pinned={root.pinned}>
								{@render entryView(root, false)}
								{#if (childCount > 0 && !isCollapsed) || replyFormRoot === root.id}
									<div class="thread">
										<div class="reply-list">
											{#if !isCollapsed}
												<!-- DEV-200: 2단 트리 — level-1 답글 + 그 밑에 level-2 (더 깊은
											     체인은 level-2 에 flatten, ↩ #id 가 실제 대상 표시). -->
												{#each groups.level1ByRoot.get(root.id) ?? [] as r (r.id)}
													{@render entryView(r, true)}
													{@const l2 = groups.level2ByParent.get(r.id) ?? []}
													{#if l2.length > 0}
														<div class="reply-list l2">
															{#each l2 as c (c.id)}
																{@render entryView(c, true)}
															{/each}
														</div>
													{/if}
													<!-- BUG-116: 이 level-1(또는 그 밑 level-2)에 쓰는 답글은
													     스레드 맨 아래가 아니라 여기(그 그룹 바로 옆)에. -->
													{#if replyFormRoot === root.id && replyFormLevel1 === r.id}
														{@render replyFormView(root.id)}
													{/if}
												{/each}
											{/if}
											<!-- root 자체에 쓰는 답글(새 level-1)만 스레드 맨 아래. -->
											{#if replyFormRoot === root.id && replyFormLevel1 === null}
												{@render replyFormView(root.id)}
											{/if}
										</div>
									</div>
								{/if}
							</li>
						{/if}
					{/each}
					{#if groups.orphans.length > 0}
						<li class="entry-card orphan-card">
							<span class="orphan-label">{t('comment.replyToDeleted', $locale)}</span>
							{#each groups.orphans as o (o.id)}
								{@render entryView(o, true)}
							{/each}
						</li>
					{/if}
				</ul>
			{/if}

			<!-- 새 top-level 댓글 -->
			<div class="new-form">
				<div class="new-row">
					<input
						class="author-input"
						type="text"
						placeholder={t('comment.authorOpt', $locale)}
						bind:value={newAuthor}
						disabled={saving}
					/>
					<button
						type="button"
						class="ce-toggle"
						class:active={newRich}
						onclick={() => (newRich = !newRich)}
						title={t('comment.toggleEditor', $locale)}
						aria-pressed={newRich}>M↓</button
					>
				</div>
				{#if newRich}
					<MarkdownEditor
						bind:value={newBody}
						onError={(m) => (saveError = m)}
						mediaOnly
						defaultHeight={160}
					/>
				{:else}
					<textarea
						use:tabInsert
						use:textareaAttach={{
							onError: (m) => (saveError = `${t('campaign.attachFailed', $locale)}: ${m}`),
							mediaOnly: true
						}}
						class="body-input"
						bind:this={newBodyEl}
						bind:value={newBody}
						oninput={onWikiInput}
						onkeyup={onWikiInput}
						onclick={onWikiInput}
						onkeydowncapture={onWikiKeydown}
						rows="3"
						placeholder={t('comment.writePlaceholder', $locale)}
						disabled={saving}
					></textarea>
					<OverlayScrollbar target={newBodyEl ?? null} />
				{/if}
				{#if saveError}<p class="state err">{saveError}</p>{/if}
				<div class="actions">
					<button class="btn-save" onclick={add} disabled={saving || !newBody.trim()}>
						{saving ? t('comment.adding', $locale) : t('comment.addComment', $locale)}
					</button>
				</div>
			</div>
		{/if}
	{/if}
</section>

<!-- DEV-118: 댓글 삭제 확인 모달. -->
<ConfirmDialog
	open={confirmDeleteId !== null}
	title={t('comment.deleteTitle', $locale)}
	message={t('comment.deleteMsg', $locale)}
	confirmLabel={t('detail.delete', $locale)}
	danger
	onconfirm={remove}
	oncancel={() => (confirmDeleteId = null)}
/>

<!-- DEV-171/172: cross-link 자동완성 팝업 — caret 위치에 떠서 실재 ID 후보 표시
     (MarkdownEditor 와 공유하는 WikiAutocompletePopup). -->
{#if wiki}
	<WikiAutocompletePopup
		items={wiki.items}
		left={wiki.left}
		top={wikiPlace?.top ?? wiki.caretBottom}
		bottom={wikiPlace?.bottom ?? null}
		maxH={wikiPlace?.maxH ?? 224}
		selectedIndex={wikiSel}
		onSelect={applyWiki}
		onHoverSelect={(i) => {
			wikiSel = i;
		}}
		bind:popupEl={wikiPopEl}
	/>
{/if}

<style>
	.comments-sec {
		margin-bottom: 1.5rem;
	}
	.section-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
		/* BUG-216: 좁은 화면에서 헤더 버튼(토론만 / 전체 접기)이 옆으로 밀려
		   페이지 전체에 가로 스크롤을 만들었다(admin 스크린샷). 줄로 흘린다. */
		flex-wrap: wrap;
		row-gap: 0.35rem;
	}
	/* 제목 자체도 줄어들 수 있어야 버튼이 다음 줄로 안 밀리고 먼저 좁아진다. */
	.section-toggle {
		min-width: 0;
	}
	/* DEV-107: 섹션 토글 — title 자체를 button 으로 만들어 클릭 가능. */
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
	.section-toggle:hover .section-title {
		color: var(--text);
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
		color: var(--accent);
		transition: color 0.12s;
	}
	.count {
		font-size: 0.8rem;
		color: var(--text-muted);
	}

	/* DEV-190: 전체 접기/펼치기 버튼 — 우측 정렬. */
	/* DEV-213: 토론만 모아보기 토글. */
	.disc-filter-btn {
		margin-left: auto;
		padding: 0.15rem 0.6rem;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--text-muted);
		font-size: 0.72rem;
		cursor: pointer;
	}
	.disc-filter-btn:hover {
		color: var(--text);
		border-color: var(--accent);
	}
	.disc-filter-btn.on {
		color: var(--accent);
		border-color: var(--accent);
	}
	/* 필터 버튼이 있으면 전체접기 버튼은 그 옆 (auto margin 은 필터 쪽). */
	.disc-filter-btn + .collapse-all-btn {
		margin-left: 0.4rem;
	}
	/* DEV-213: 토론만 보기에서 비토론 entry dim — 숨김 대신 문맥 유지. */
	.entry.dimmed {
		opacity: 0.45;
	}
	/* DEV-214: 접힘 지점(스레드/본문/섹션)의 토론 상태 글리프 — DEV-150 글리프 재사용. */
	.disc-flag {
		font-size: 0.72rem;
		font-weight: 700;
	}
	.disc-flag.unresolved {
		color: var(--danger);
	}
	.disc-flag.resolved {
		color: var(--success);
	}

	.collapse-all-btn {
		margin-left: auto;
		padding: 0.15rem 0.6rem;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--text-muted);
		font-size: 0.72rem;
		cursor: pointer;
	}
	.collapse-all-btn:hover {
		background: var(--bg-subtle);
		color: var(--text);
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

	.entry-list {
		list-style: none;
		margin: 0 0 1rem;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	/* DEV-139: root + 답글을 감싸는 카드 — 댓글 간 시각 구분.
	   본문 (entry-body 의 MarkdownView) 은 --bg 라 카드 배경과 한 단계 차이. */
	.entry-card {
		list-style: none;
		background: color-mix(in srgb, var(--bg-elevated) 65%, var(--bg));
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-lg);
		padding: 0.6rem 0.75rem;
	}
	/* DEV-234: 상단 고정된 댓글 — pin(위치 이동) 만으로는 눈에 안 띄어서
	   테두리로 하이라이트도 함께. */
	.entry-card.pinned {
		border-color: color-mix(in srgb, var(--accent) 55%, transparent);
		background: color-mix(
			in srgb,
			var(--accent) 6%,
			color-mix(in srgb, var(--bg-elevated) 65%, var(--bg))
		);
	}
	.entry {
		border-radius: var(--r-md);
	}

	/* DEV-296: 작업기록에서 점프해 온 댓글을 잠깐 강조 — 스크롤만으로는
	   어느 것인지 눈에 안 띈다. 2.2초 뒤 클래스가 제거된다. */
	.entry:global(.jump-target) {
		animation: jump-flash 2.2s ease-out;
	}
	@keyframes jump-flash {
		0%,
		35% {
			background: color-mix(in srgb, var(--accent) 18%, transparent);
			box-shadow: 0 0 0 2px var(--accent);
		}
		100% {
			background: transparent;
			box-shadow: none;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.entry:global(.jump-target) {
			animation: none;
			box-shadow: 0 0 0 2px var(--accent);
		}
	}
	/* DEV-234 후속(admin 요청): 답글도 고정 가능 — root 는 entry-card.pinned 로
	   이미 강조되니, 답글(.entry.reply)만 자체 테두리로 강조. */
	.entry.reply.pinned {
		border: var(--bw) solid color-mix(in srgb, var(--accent) 55%, transparent);
		background: color-mix(in srgb, var(--accent) 6%, transparent);
		padding: 0.3rem 0.5rem;
		margin: -0.3rem -0.5rem 0;
	}
	.thread {
		margin: 0;
		padding: 0;
	}
	.reply-list {
		margin: 0.25rem 0 0 1.5rem;
		padding-left: 0.75rem;
		border-left: 2px solid var(--bg-subtle);
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	/* DEV-200: level-2 (답글의 답글) — 한 단 더 들여쓰기. 그 이상 깊이는 여기에
	   flatten (↩ #id 링크가 실제 대상 표시). */
	.reply-list.l2 {
		margin: 0 0 0 1.25rem;
		padding-left: 0.75rem;
	}
	.reply-form {
		border: var(--bw) dashed var(--border);
		border-radius: var(--r-md);
		padding: 0.5rem 0.7rem;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.reply-author {
		display: flex;
		gap: 0.4rem;
	}
	.reply-author .author-input,
	.new-row .author-input {
		min-width: 0;
	}
	.reply-author .ce-toggle,
	.new-row .ce-toggle {
		flex-shrink: 0;
		align-self: center;
		margin-left: auto;
	}

	.orphan-card {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}
	.orphan-label {
		font-size: 0.72rem;
		color: var(--text-muted);
		font-style: italic;
	}

	.entry-head {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.78rem;
		color: var(--text-muted);
		margin-bottom: 0.4rem;
		/* BUG-195: 좁은 화면에서 한 줄에 안 들어가면 날짜가 "2026. 8." / "2. 오후" /
		   "7:08:32" 처럼 조각나 세 줄로 흩어졌고, 영어 UI 에선 액션(Discussion/
		   Edit/Delete)이 화면 밖으로 나가 가로 스크롤이 생겼다. 줄바꿈을 허용하고
		   각 조각은 붙여 둔다. */
		flex-wrap: wrap;
		row-gap: 0.25rem;
	}
	.entry-head > * {
		/* 날짜·작성자·번호가 조각나지 않게 — 줄바꿈은 항목 사이에서만. */
		white-space: nowrap;
	}

	.author {
		font-weight: 600;
		color: var(--text);
	}
	.sep {
		color: var(--text-faint);
	}
	.ts {
		color: var(--text-faint);
	}
	/* DEV-182: 편집됨 표시. */
	.edited-marker {
		color: var(--text-faint);
		font-size: 0.75rem;
		font-style: italic;
	}
	/* DEV-128 → DEV-139: 댓글 번호 — 클릭 시 본문 접기/펼치기 버튼. */
	.entry-no {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.72rem;
		color: var(--text-faint);
		background: transparent;
		cursor: pointer;
		padding: 0.05rem 0.35rem;
		border-radius: var(--r-sm);
		border: var(--bw) solid var(--border-muted);
	}
	.entry-no:hover {
		color: var(--accent);
		border-color: var(--accent);
	}
	.reply-to {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.7rem;
		color: var(--text-muted);
		text-decoration: none;
	}
	.reply-to:hover {
		color: var(--accent);
	}
	/* DEV-129: 접힌 본문 미리보기 — 1줄 ellipsis, 클릭으로 펼침. */
	.body-collapsed {
		display: block;
		width: 100%;
		text-align: left;
		background: none;
		border: none;
		border-left: 2px solid var(--border);
		padding: 0.15rem 0 0.15rem 0.6rem;
		color: var(--text-faint);
		font-size: 0.8rem;
		font-style: italic;
		cursor: pointer;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.body-collapsed:hover {
		color: var(--text-muted);
		border-left-color: var(--accent);
	}
	/* DEV-139: 푸터 행 — 좌측 답글 컨트롤 / 우측 이모지. */
	.entry-foot {
		display: flex;
		align-items: center;
		/* BUG-125(admin 보고): space-between 이면 foot-right(반응 pill + 고정
		   버튼)가 통째로 카드 오른쪽 끝에 붙어, 이모지 버튼이 foot-left 로
		   옮겨진 뒤에도 반응 pill 들은 여전히 오른쪽 끝에 몰려있는 것처럼
		   보였다 — foot-left 바로 옆에 왼쪽 정렬로 이어지게 하고, 고정
		   버튼(.pin-btn)에만 margin-left:auto 를 줘서 그것만 오른쪽 끝으로. */
		margin-top: 0.4rem;
		gap: 0.5rem;
	}
	.foot-left {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}
	/* 삼각형만 클릭 — 채운 삼각형 (▼/▶), 일반 글자색, 종전보다 큼. */
	.tri-btn {
		background: transparent;
		border: none;
		cursor: pointer;
		font-size: 0.85rem;
		line-height: 1;
		color: var(--text);
		padding: 0.1rem 0.2rem;
	}
	.tri-btn:hover {
		color: var(--accent);
	}
	.reply-count {
		font-size: 0.75rem;
		color: var(--text-muted);
		user-select: none;
	}
	/* '답글 쓰기' — 댓글번호 (#N) 와 같은 테두리 버튼 느낌. */
	.reply-write-btn {
		font-size: 0.72rem;
		color: var(--text-muted);
		background: transparent;
		cursor: pointer;
		padding: 0.1rem 0.5rem;
		border-radius: var(--r-sm);
		border: var(--bw) solid var(--border-muted);
	}
	.reply-write-btn:hover {
		color: var(--accent);
		border-color: var(--accent);
	}
	/* DEV-234: 상단 고정 버튼 — 꺼짐 상태는 흐리게, 켜지면 강조색 채움. */
	.pin-btn {
		font-size: 0.75rem;
		line-height: 1;
		color: var(--text-faint);
		background: transparent;
		cursor: pointer;
		padding: 0.15rem 0.35rem;
		border-radius: var(--r-sm);
		border: var(--bw) solid transparent;
		opacity: 0.6;
		/* BUG-125: 반응 pill 은 왼쪽 정렬로 남기고, 고정 버튼만 카드 오른쪽
		   끝(오른쪽 아래)으로 밀어냄. */
		margin-left: auto;
	}
	.pin-btn:hover {
		opacity: 1;
	}
	.pin-btn.on {
		color: var(--accent);
		border-color: color-mix(in srgb, var(--accent) 45%, transparent);
		opacity: 1;
	}
	/* 우측 — 활성 반응 pill + '+' popup (slack 스타일). */
	.foot-right {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		/* BUG-125: foot-left 옆으로 자연스레 이어지되(반응 pill 왼쪽 정렬),
		   남는 공간을 이 컨테이너가 차지해야 .pin-btn 의 margin-left:auto 가
		   카드 오른쪽 끝까지 닿는다. */
		flex: 1;
	}
	.reaction-pill {
		padding: 0.1rem 0.45rem;
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		border: var(--bw) solid color-mix(in srgb, var(--accent) 45%, transparent);
		border-radius: var(--r-xl);
		font-size: 0.78rem;
		cursor: pointer;
	}
	.reaction-pill:hover {
		border-color: var(--danger);
	}
	/* DEV-108: 내가 단 반응은 진한 테두리로 구분. */
	.reaction-pill.mine {
		background: color-mix(in srgb, var(--accent) 24%, transparent);
		border-color: var(--accent);
	}
	/* 반응 수 (2명 이상). */
	.reaction-pill .rc {
		margin-left: 0.2rem;
		font-size: 0.7rem;
		font-weight: 700;
		color: var(--text-muted);
	}
	.picker-wrap {
		position: relative;
	}
	.reaction-add {
		padding: 0.1rem 0.4rem;
		background: transparent;
		border: var(--bw) solid var(--border-muted);
		border-radius: var(--r-xl);
		font-size: 0.72rem;
		color: var(--text-faint);
		cursor: pointer;
	}
	.reaction-add:hover {
		color: var(--text);
		border-color: var(--text-faint);
	}
	.picker-ov {
		position: fixed;
		inset: 0;
		z-index: 90;
		background: transparent;
	}
	/* BUG-125: 버튼이 foot-left 로 옮겨진 뒤 CSS 만으로(right:0, picker-wrap
	   상대 위치) 펼치면 화면 밖으로 나갈 수 있어 — JS 로 뷰포트 기준 위치
	   계산(fixed) + clamp. 자동완성 팝업(.wiki-pop)과 동일한 접근. */
	.reaction-picker {
		position: fixed;
		z-index: 91;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		padding: 0.3rem 0.4rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: 0 6px 18px var(--shadow);
		min-width: 9rem;
	}
	.picker-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.2rem;
	}
	.picker-item-wrap {
		position: relative;
	}
	.picker-item {
		padding: 0.15rem 0.35rem;
		background: transparent;
		border: var(--bw) solid transparent;
		border-radius: var(--r-md);
		font-size: 0.95rem;
		cursor: pointer;
	}
	.picker-item:hover {
		background: var(--bg-subtle);
	}
	.picker-item.on {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		border-color: color-mix(in srgb, var(--accent) 45%, transparent);
	}
	/* DEV-132: 커스텀 반응 삭제 버튼 — 평소엔 숨기고 hover 시에만. */
	.picker-item-rm {
		position: absolute;
		top: -0.35rem;
		right: -0.35rem;
		width: 0.9rem;
		height: 0.9rem;
		padding: 0;
		display: none;
		align-items: center;
		justify-content: center;
		border: none;
		border-radius: 50%;
		background: color-mix(in srgb, var(--danger) 85%, transparent);
		color: white;
		font-size: 0.65rem;
		line-height: 1;
		cursor: pointer;
	}
	.picker-item-wrap:hover .picker-item-rm {
		display: flex;
	}
	.picker-add-row {
		display: flex;
		gap: 0.25rem;
		border-top: var(--bw) solid var(--border);
		padding-top: 0.3rem;
	}
	.picker-add-input {
		flex: 1;
		min-width: 0;
		padding: 0.2rem 0.4rem;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-sm);
		font-size: 0.8rem;
	}
	.picker-add-btn {
		padding: 0.2rem 0.5rem;
		background: transparent;
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-sm);
		font-size: 0.75rem;
		cursor: pointer;
	}
	.picker-add-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.picker-add-btn:hover:not(:disabled) {
		background: var(--bg-subtle);
	}
	/* DEV-132 후속: 이모지 1개 검증 실패 메시지. */
	.picker-add-err {
		margin: 0.2rem 0 0;
		font-size: 0.7rem;
		color: var(--danger);
	}
	.entry-actions {
		margin-left: auto;
		display: flex;
		gap: 0.5rem;
	}
	.link-btn {
		background: none;
		border: none;
		color: var(--accent);
		cursor: pointer;
		padding: 0;
		font: inherit;
		font-size: 0.78rem;
		text-decoration: underline;
	}
	.link-btn:hover {
		color: var(--accent);
	}
	.link-btn.danger {
		color: var(--danger);
	}
	.link-btn.danger:hover {
		color: var(--danger);
	}
	/* DEV-142: '토론' 토글 활성 표시. */
	.link-btn.on {
		color: var(--warning);
		font-weight: 700;
	}

	/* DEV-142: 토론 상태 배지 — 미해결(빨강) / 해결(초록). */
	.disc-badge {
		margin-left: 0.4rem;
		padding: 0.05rem 0.4rem;
		border-radius: var(--r-pill);
		border: var(--bw) solid color-mix(in srgb, var(--danger) 40%, transparent);
		background: color-mix(in srgb, var(--danger) 14%, transparent);
		color: var(--danger);
		font-size: 0.7rem;
		font-weight: 700;
		cursor: pointer;
		white-space: nowrap;
	}
	.disc-badge:hover {
		background: color-mix(in srgb, var(--danger) 22%, transparent);
	}
	.disc-badge.resolved {
		border-color: color-mix(in srgb, var(--success) 45%, transparent);
		background: color-mix(in srgb, var(--success) 14%, transparent);
		color: var(--success);
	}
	.disc-badge.resolved:hover {
		background: color-mix(in srgb, var(--success) 22%, transparent);
	}

	.entry-body :global(p) {
		margin: 0.25rem 0;
	}

	.new-form {
		border-top: var(--bw) dashed var(--bg-subtle);
		padding-top: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.new-row {
		display: flex;
		gap: 0.4rem;
	}
	.author-input {
		flex: 0 0 14rem;
		padding: 0.3rem 0.5rem;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-sm);
		font-size: 0.825rem;
	}
	.body-input {
		width: 100%;
		padding: 0.45rem 0.6rem;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-md);
		font-size: 0.825rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		resize: vertical;
		min-height: 4rem;
		/* BUG-157: native scrollbar 숨김 — OverlayScrollbar 가 대신 그린다. */
		scrollbar-width: none;
	}
	.body-input::-webkit-scrollbar {
		display: none;
	}
	/* DEV-289: 댓글 입력창 마크다운 편집기 토글 버튼. */
	.ce-toggle {
		font-size: 0.68rem;
		font-family: 'SFMono-Regular', Consolas, monospace;
		padding: 0.1rem 0.4rem;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		background: transparent;
		color: var(--text-muted);
		cursor: pointer;
	}
	.ce-toggle:hover {
		color: var(--text);
	}
	.ce-toggle.active {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
		border-color: var(--accent);
		color: var(--text);
	}
	.actions {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.35rem;
	}
	.btn-save {
		padding: 0.3rem 0.85rem;
		background: var(--btn-primary-bg);
		border: var(--bw) solid var(--btn-primary-border);
		color: var(--btn-primary-text);
		border-radius: var(--r-md);
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
		padding: 0.3rem 0.85rem;
		background: transparent;
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-md);
		cursor: pointer;
		font-size: 0.825rem;
	}
	.btn-cancel:hover:not(:disabled) {
		background: var(--bg-subtle);
	}
</style>
