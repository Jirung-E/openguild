<!--
  DEV-217: 도서관 페이지 — `.guild/library/{BOOK-NNN}.md` 목록/편집.
  DEV-239: 폴더(계층) + 보기 방식(트리 / 탐색기) 토글 추가.

  rules 페이지 패턴 재사용:
  - tree 모드: 좌측 sidebar(폴더 트리 + 문서 목록) + 우측 panel(항상 표시).
  - explorer 모드: 폴더/문서를 큰 고정 크기 아이콘 그리드로 보여주다가,
    문서를 클릭하면 그리드 대신 전체 폭 문서 상세로 전환 ("← 목록"으로 복귀).
    admin 이 승인한 두 목업(트리+미리보기 / 아이콘 탐색기)을 토글로 겸용.

  rules 와의 차이: 식별자가 slug 가 아니라 자동 부여 BOOK 번호 — 신규 모달은
  제목만 입력, "이름 변경" 대신 "제목 변경"(번호는 불변). 딥링크는 ?id=BOOK-NNN
  (cross-link 대상 — DEV-218).
-->
<script lang="ts">
	import PaneResizer from '$lib/components/PaneResizer.svelte';
	import { paneWidth } from '$lib/stores/paneWidth';
	import Icon from '$lib/components/Icon.svelte';
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { setUnsaved } from '$lib/stores/unsaved';
	import { saveShortcut } from '$lib/utils/save-shortcut';
	import { libraryApi, type Book, type LibraryFolder } from '$lib/api/library';
	import { searchApi } from '$lib/api/search';
	import { buildLibraryTree, flattenFolderPaths, searchLibrary } from '$lib/utils/library-tree';
	import LibraryFolderTree from '$lib/components/LibraryFolderTree.svelte';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	import SidecarHistory from '$lib/components/SidecarHistory.svelte';
	import BacklinkSection from '$lib/components/BacklinkSection.svelte';
	// DEV-297: 잘린 타일 제목은 커스텀 팝업으로 전체 표시.
	import { titlePopup } from '$lib/actions/title-popup';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import AttachmentSection from '$lib/components/AttachmentSection.svelte';
	// DEV-203: 편집기 셋업(테마/들여쓰기/첨부/자동완성/redo/높이/overlay 스크롤)은
	// 공통 MarkdownEditor 컴포넌트로 단일화.
	import MarkdownEditor from '$lib/components/MarkdownEditor.svelte';
	import { loadQuestIndex } from '$lib/stores/questIndex';
	// BUG-123(admin 재지적): 네이티브 alert() 대신 앱 공용 toast — 다른 페이지들과
	// 동일한 alert() 대체 컨벤션(+layout.svelte 주석 참고).
	import { showToast } from '$lib/stores/toast';
	// DEV-243: 태그 — quest 와 동일한 정의(색/설명) registry 공유.
	import TagPills from '$lib/components/TagPills.svelte';
	import { adminApi } from '$lib/api/admin';
	import type { QuestTagDef } from '$lib/types';
	// DEV-182: 생성/변경 시각 표시 — quest 상세와 동일 포맷 유틸.
	import { formatTs, formatRelative } from '$lib/utils/datetime';
	// DEV-205(2차): i18n.
	import { locale, t } from '$lib/stores/locale';
	// REQ-019: 태그 필터 줄 — 접기 포함 공통 컴포넌트.
	import TagFilterRow from '$lib/components/TagFilterRow.svelte';

	// REQ-015: 사이드바 폭 — 구분선 드래그로 조절, rem 이라 배율을 따라간다.
	const sidebarW = paneWidth('library');

	let loading = $state(true);
	let error = $state<string | null>(null);
	let books = $state<Book[]>([]);
	let folders = $state<LibraryFolder[]>([]);
	let selectedId = $state<string | null>(null);

	const selected = $derived(books.find((b) => b.book_id === selectedId) ?? null);

	// BUG-133(admin 보고) → BUG-134 후속(admin 지적): 문서를 열 때 quest 상세
	// 페이지(`/quests/[id]` — 매번 questsApi.getBySlug 로 서버에서 새로 조회)와
	// 달리, 도서관은 애초에 최초 `list()` 스냅샷을 그대로 보여주기만 하고
	// 재조회가 없었다 — "인덱스(list 캐시) 내용을 읽는" 상태. quest 처럼
	// 상세는 매번 서버에서 새로 불러오는 게 맞고, list 캐시가 본문까지 들고
	// 있을 필요는 없다(검색용 캐시일 뿐 — 상세 표시는 이 재조회가 진리원).
	// 선택이 바뀔 때마다 libraryApi.get() 으로 title/body/path/attachments
	// 전부를 서버에서 다시 받아 덮어쓴다.
	$effect(() => {
		const id = selectedId;
		if (!id) return;
		libraryApi
			.get(id)
			.then((full) => {
				// 응답 도착 전에 다른 문서를 선택했으면 무시(경쟁 상태 방지).
				if (selectedId !== id) return;
				books = books.map((b) => (b.book_id === id ? full : b));
			})
			.catch(() => {
				/* 보조 기능 — 실패해도 list() 스냅샷은 표시됨 */
			});
	});

	// BUG-123(admin 보고): tree 모드 폴더 접기 — 이전엔 아예 구현이 안 돼 있어
	// 하위 폴더/문서가 항상 펼쳐진 채였음. 세션 한정(일회성 토글, 재진입 시
	// 기본 펼침) — 댓글 섹션 전체 접기(DEV-107 fix1)와 동일한 정책.
	let collapsedFolders = $state(new Set<string>());
	function toggleFolderCollapsed(path: string) {
		const next = new Set(collapsedFolders);
		if (next.has(path)) next.delete(path);
		else next.add(path);
		collapsedFolders = next;
	}
	// BUG-127: tree 모드 검색 결과의 폴더를 클릭하면 그 폴더 + 조상 전부를
	// 펼치고(collapsedFolders 에서 제거) 검색을 닫아 트리에서 바로 보이게.
	function revealFolder(path: string) {
		const next = new Set(collapsedFolders);
		const parts = path.split('/');
		for (let i = 1; i <= parts.length; i++) {
			next.delete(parts.slice(0, i).join('/'));
		}
		collapsedFolders = next;
		searchQuery = '';
	}
	// DEV-243 후속(admin 지적): 태그는 달 수 있는데 태그로 찾을 방법이 없었음.
	// quest 의 DEV-068 tag 필터(AND, chip 클릭 토글)와 동일 패턴.
	let filterTags = $state(new Set<string>());
	const allTagOptions = $derived.by(() => {
		const set = new Set<string>();
		for (const b of books) for (const t of b.tags ?? []) set.add(t);
		return Array.from(set).sort();
	});
	const tagCounts = $derived.by(() => {
		const m = new Map<string, number>();
		for (const b of books) for (const t of b.tags ?? []) m.set(t, (m.get(t) ?? 0) + 1);
		return m;
	});
	function toggleTagFilter(t: string) {
		const next = new Set(filterTags);
		if (next.has(t)) next.delete(t);
		else next.add(t);
		filterTags = next;
	}
	const tagFilteredBooks = $derived(
		filterTags.size === 0
			? books
			: books.filter((b) => {
					const bTags = new Set(b.tags ?? []);
					for (const t of filterTags) if (!bTags.has(t)) return false;
					return true;
				})
	);
	// DEV-251: 문서 정렬 기준 선택 — 번호/이름/수정순 + 방향. localStorage 영속.
	type DocSortKey = 'number' | 'title' | 'updated';
	const DOC_SORT_KEY = 'openguild.librarySort';
	function loadDocSort(): { key: DocSortKey; desc: boolean } {
		try {
			const raw = JSON.parse(localStorage.getItem(DOC_SORT_KEY) ?? '');
			if (
				raw &&
				['number', 'title', 'updated'].includes(raw.key) &&
				typeof raw.desc === 'boolean'
			) {
				return raw;
			}
		} catch {
			/* ignore */
		}
		return { key: 'title', desc: false }; // 기존 기본(이름순)과 동일.
	}
	let docSortKey = $state<DocSortKey>(loadDocSort().key);
	let docSortDesc = $state(loadDocSort().desc);
	$effect(() => {
		try {
			localStorage.setItem(DOC_SORT_KEY, JSON.stringify({ key: docSortKey, desc: docSortDesc }));
		} catch {
			/* ignore */
		}
	});
	// DEV-205(2차): locale 반응이어야 해서 const 맵 대신 $derived.
	const DOC_SORT_LABELS = $derived<Record<DocSortKey, string>>({
		number: t('library.sort.number', $locale),
		title: t('library.sort.title', $locale),
		updated: t('library.sort.updated', $locale)
	});
	const sortedBooks = $derived.by(() => {
		const cmp = (a: Book, b: Book): number => {
			switch (docSortKey) {
				case 'number':
					return a.number - b.number;
				case 'updated':
					return a.updated_at.localeCompare(b.updated_at);
				default:
					return a.title.localeCompare(b.title);
			}
		};
		const arr = [...tagFilteredBooks].sort(cmp);
		if (docSortDesc) arr.reverse();
		return arr;
	});
	const tree = $derived(buildLibraryTree(folders, sortedBooks, { preserveDocOrder: true }));

	// ─── 보기 방식 (tree / explorer) — localStorage 로 유지 ───
	type ViewMode = 'tree' | 'explorer';
	const VIEW_MODE_KEY = 'openguild.libraryViewMode';
	function loadViewMode(): ViewMode {
		try {
			const v = localStorage.getItem(VIEW_MODE_KEY);
			if (v === 'tree' || v === 'explorer') return v;
		} catch {
			/* ignore */
		}
		return 'tree';
	}
	let viewMode = $state<ViewMode>(loadViewMode());
	function setViewMode(m: ViewMode) {
		viewMode = m;
		try {
			localStorage.setItem(VIEW_MODE_KEY, m);
		} catch {
			/* ignore */
		}
	}
	// svelte-check 는 `{#if viewMode === 'explorer'}` 블록 안에서 다시
	// `viewMode === 'tree'` 를 비교하면 (그 블록 안에선 타입이 'explorer' 로
	// 좁혀져) "겹치지 않는 타입 비교"로 오탐한다 — 토글 버튼 두 개가 항상 같은
	// 블록 안에 있어 발생. 함수 호출로 감싸 좁히기를 우회.
	function isMode(m: ViewMode): boolean {
		return viewMode === m;
	}
	// explorer 모드 전용 — 현재 탐색 중인 폴더 경로("" = 최상위).
	let explorerPath = $state('');
	const explorerNode = $derived(explorerPath ? (tree.nodeMap.get(explorerPath) ?? null) : null);
	const explorerFolders = $derived(
		explorerPath === '' ? tree.roots : (explorerNode?.children ?? [])
	);
	const explorerDocs = $derived(explorerPath === '' ? tree.rootDocs : (explorerNode?.docs ?? []));

	// DEV-238 → BUG-127 후속(admin 보고): 예전엔 항상 전역 검색이었음 — 이제
	// explorer(아이콘) 모드는 현재 폴더(explorerPath) + 하위만 검색(탐색기의
	// "여기서 찾기" 감각). tree 모드는 "현재 폴더" 개념 자체가 없어(트리
	// 전체를 항상 다 보여줌) 전역 유지. 폴더 이름도 매칭 대상에 포함.
	let searchQuery = $state('');
	// REQ-011: 첨부 **이름** 검색. 도서관 문서엔 댓글이 없어 대상은 첨부뿐이고,
	// 첨부 이름은 목록 응답에 없어 클라이언트가 알 수 없다 — 켰을 때만 서버에
	// 판정을 맡긴다(REQ-009 의 /api/search 를 그대로 재사용).
	let searchAttachments = $state(false);
	let attachMatchIds = $state<Set<string> | null>(null);
	let attachSeq = 0;
	let attachTimer: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		const q = searchQuery.trim();
		const on = searchAttachments;
		if (!on || !q) {
			attachMatchIds = null;
			return;
		}
		if (attachTimer) clearTimeout(attachTimer);
		const seq = ++attachSeq;
		attachTimer = setTimeout(() => {
			backlinksSearchAttachments(q)
				.then((ids) => {
					// 늦게 온 응답이 최신 결과를 덮으면 안 된다.
					if (seq !== attachSeq) return;
					attachMatchIds = ids;
				})
				.catch(() => {
					if (seq !== attachSeq) return;
					attachMatchIds = null; // 실패 시 기존 동작으로.
				});
		}, 250);
	});

	/** REQ-009 의 강화 검색에서 **도서관 문서**만 추려 book_id 집합을 만든다. */
	async function backlinksSearchAttachments(q: string): Promise<Set<string>> {
		const hits = await searchApi.enhanced(q, ['attachment']);
		return new Set(hits.filter((h) => h.kind === 'book').map((h) => h.id));
	}
	const searchScope = $derived(viewMode === 'explorer' ? explorerPath : '');
	const searchResults = $derived(
		searchLibrary(tree, sortedBooks, searchQuery, searchScope, attachMatchIds ?? undefined)
	);

	let editMode = $state(false);
	$effect(() => setUnsaved('library-edit', editMode));
	onDestroy(() => setUnsaved('library-edit', false));
	let editText = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	// 신규(제목 입력) / 제목 변경 모달.
	let creating = $state(false);
	let createTitle = $state('');
	let createPath = $state('');
	let createError = $state<string | null>(null);

	let retitling = $state(false);
	let retitleText = $state('');
	let retitleError = $state<string | null>(null);

	// DEV-239: 폴더 이동 모달.
	let moving = $state(false);
	let movePath = $state('');
	let moveError = $state<string | null>(null);

	// DEV-239: 새 폴더 모달.
	let creatingFolder = $state(false);
	let createFolderPath = $state('');
	let createFolderError = $state<string | null>(null);

	async function loadList(preferId?: string | null, mutated = false) {
		loading = true;
		error = null;
		try {
			const [b, f] = await Promise.all([libraryApi.list(), libraryApi.folders.list()]);
			books = b;
			folders = f;
			if (preferId && books.some((b2) => b2.book_id === preferId)) {
				selectedId = preferId;
			} else if (selectedId == null || !books.some((b2) => b2.book_id === selectedId)) {
				selectedId = viewMode === 'tree' ? (books[0]?.book_id ?? null) : null;
			}
			// BUG-210: 예전엔 여기서 무조건 `loadQuestIndex(true)` 를 불렀다.
			// 이 함수는 **생성/삭제/이름변경 후에도, 페이지 진입 때도** 호출되는데
			// force 는 memo 를 무시하므로 페이지에 들어갈 때마다 quests/campaigns/
			// rules/library 4종을 통째로 다시 받았다(퀘스트 531건 기준 /api/quests
			// 응답만 1.1MB, 라우트 이동 1회당 힙 +2.5MB). 실제로 목록이 바뀐
			// 호출에서만 force 한다.
			loadQuestIndex(mutated);
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		// 딥링크 — `/library?id=BOOK-NNN&path=폴더` 진입 시 해당 문서/폴더 위치 복원.
		const sp = new URLSearchParams(window.location.search);
		explorerPath = sp.get('path') ?? '';
		loadList(sp.get('id'));
	});

	// DEV-243: 태그 정의(색/설명) — quest 상세와 동일한 registry.
	let tagDefs = $state<QuestTagDef[]>([]);
	onMount(async () => {
		tagDefs = await adminApi.listTagDefs().catch(() => [] as QuestTagDef[]);
	});
	async function setDocTags(tags: string[]) {
		if (!selectedId) return;
		try {
			const updated = await libraryApi.setTags(selectedId, tags);
			books = books.map((b) => (b.book_id === selectedId ? updated : b));
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('library.tagSaveFail', $locale), 'error');
		}
	}

	// BUG-104 + admin 보고(2026-07-07): 문서/폴더를 여러 번 옮겨다닌 뒤 브라우저
	// 뒤로가기를 눌러도 도서관 밖으로 그냥 나가버렸음 — syncUrl 이 매번
	// replaceState 라 히스토리가 안 쌓였던 게 원인(원래 "선택은 페이지 이동
	// 아님"이라는 다른 페이지들의 의도된 절충이었는데, 도서관처럼 폴더를 옮겨
	// 다니며 탐색하는 화면엔 안 맞는다는 피드백). id/path 변경마다 새 history
	// entry 를 쌓도록 pushState 로 전환 + explorerPath 도 URL 에 반영.
	$effect(() => {
		const sp = $page.url.searchParams;
		const id = sp.get('id');
		const path = sp.get('path') ?? '';
		if (path !== explorerPath) {
			explorerPath = path;
		}
		if (id && id !== selectedId && books.some((b) => b.book_id === id)) {
			select(id);
		} else if (!id && selectedId !== null) {
			// 뒤로가기로 ?id= 가 사라짐(문서 상세 → 목록) — 상태만 반영, 다시
			// syncUrl 로 되돌리지 않음(이미 URL 이 진실).
			selectedId = null;
		}
	});

	/** 현재 selectedId/explorerPath 를 URL 에 반영 — 새 history entry 를 쌓아
	 *  브라우저 뒤로/앞으로가기가 문서·폴더 탐색을 단계적으로 되돌릴 수 있게. */
	function syncUrl() {
		const cur = new URLSearchParams(window.location.search);
		const curId = cur.get('id');
		const curPath = cur.get('path') ?? '';
		if (curId === selectedId && curPath === explorerPath) return;
		const next = new URLSearchParams();
		if (explorerPath) next.set('path', explorerPath);
		if (selectedId) next.set('id', selectedId);
		const qs = next.toString();
		goto(qs ? `/library?${qs}` : '/library', { keepFocus: true, noScroll: true });
	}

	/** explorer 모드 폴더 이동 — state 갱신 + history entry. */
	function gotoFolder(path: string) {
		explorerPath = path;
		syncUrl();
	}

	// BUG-127(admin 요청): 아이콘 뷰에서 상위 폴더로 바로 이동하는 버튼.
	function parentOf(path: string): string {
		return path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
	}

	let confirmDiscardId = $state<string | null>(null);
	// DEV-239: explorer 모드 "← 목록" 도 편집중이면 같은 확인 다이얼로그를 타되,
	// confirmDiscardId(빈 문자열은 falsy라 select id 와 혼동됨)와 별개 플래그로.
	let pendingBackToGrid = $state(false);

	function select(id: string) {
		if (editMode) {
			confirmDiscardId = id;
			return;
		}
		selectedId = id;
		syncUrl();
	}

	function applyPendingSelect() {
		const id = confirmDiscardId;
		confirmDiscardId = null;
		if (pendingBackToGrid) {
			pendingBackToGrid = false;
			cancelEdit();
			selectedId = null;
			syncUrl();
			return;
		}
		if (!id) return;
		cancelEdit();
		selectedId = id;
		syncUrl();
	}

	// explorer 모드: 그리드로 복귀 (선택 해제, 폴더 위치는 유지).
	function explorerBack() {
		if (editMode) {
			pendingBackToGrid = true;
			return;
		}
		selectedId = null;
		syncUrl();
	}

	// ─── 편집 ───
	async function enterEdit() {
		if (!selected) return;
		const id = selected.book_id;
		saveError = null;
		// BUG-134 후속(admin 지적 — 위 effect 만으로는 여전히 안 고쳐짐): 선택
		// 시 백그라운드로 도는 재조회(위 effect)는 비동기라, 문서를 선택하자마자
		// 곧바로 "편집"을 누르면(흔한 동선) 그 fetch 가 아직 안 끝난 상태 —
		// `selected.body` 는 여전히 최초 list() 스냅샷(구버전)이었다. 편집
		// 진입 자체에서 직접 재조회해, 배경 fetch 타이밍과 무관하게 항상 최신
		// 본문으로 시작하도록 보장.
		let body = selected.body;
		try {
			const full = await libraryApi.get(id);
			if (selectedId === id) {
				books = books.map((b) => (b.book_id === id ? full : b));
			}
			body = full.body;
		} catch {
			/* 재조회 실패 — list() 스냅샷으로 폴백(기존 동작). */
		}
		// DEV-203: MarkdownEditor 는 마운트 시점의 editText 를 초기 doc 으로
		// 쓰므로, 최신 본문 확보 후에 editMode 진입(BUG-134 의 fresh-body 보장).
		editText = body;
		editMode = true;
	}

	// DEV-237: 편집기 '첨부' 버튼 / 비미디어 paste·drop → 본문 인라인 대신 첨부
	// 섹션. quests/campaigns 의 attachToSection 과 동일 시맨틱 — 다만 여기는
	// api.post(transport 경유)를 써서 브라우저 모드에서도 동작(quests/campaigns
	// 는 Tauri invoke 직접 호출이라 브라우저 모드 미지원인 기존 한계가 있음).
	async function attachToSection(rel: string, name: string) {
		if (!selected) return;
		try {
			selected.attachments = await libraryApi.addAttachment(selected.book_id, rel, name);
		} catch (e) {
			saveError = `첨부 실패: ${e instanceof Error ? e.message : String(e)}`;
		}
	}

	// DEV-203: 편집기 생성/파괴/테마·설정 반응은 MarkdownEditor 가 자동 처리.
	function cancelEdit() {
		editMode = false;
		saveError = null;
	}

	async function save(keepEditing = false) {
		if (!selectedId || saving) return;
		saving = true;
		saveError = null;
		try {
			const text = editText;
			const updated = await libraryApi.update(selectedId, { body: text });
			books = books.map((b) => (b.book_id === selectedId ? updated : b));
			loadQuestIndex(true);
			if (!keepEditing) cancelEdit();
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
		createPath = viewMode === 'explorer' ? explorerPath : '';
		createError = null;
	}
	function cancelCreate() {
		creating = false;
		createError = null;
	}
	async function submitCreate() {
		const title = createTitle.trim();
		if (!title) {
			createError = t('library.titleRequired', $locale);
			return;
		}
		try {
			const created = await libraryApi.create(title, '', createPath);
			creating = false;
			await loadList(created.book_id, true);
		} catch (e) {
			createError = e instanceof Error ? e.message : t('library.createFail', $locale);
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
			await loadList(null, true);
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('library.deleteFail', $locale), 'error');
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
			retitleError = e instanceof Error ? e.message : t('library.retitleFail', $locale);
		}
	}

	// ─── DEV-239: 폴더 이동 ───
	function openMove() {
		if (!selected) return;
		moving = true;
		movePath = selected.path;
		moveError = null;
	}
	function cancelMove() {
		moving = false;
		moveError = null;
	}
	async function submitMove() {
		if (!selectedId) return;
		try {
			const updated = await libraryApi.update(selectedId, { path: movePath });
			books = books.map((b) => (b.book_id === selectedId ? updated : b));
			moving = false;
		} catch (e) {
			moveError = e instanceof Error ? e.message : t('library.moveFail', $locale);
		}
	}

	// BUG-127(admin 요청): 드래그&드랍 — 문서를 폴더(또는 상위/루트)에 드롭하면
	// 그 경로로 이동. 기존 "폴더 이동" 버튼(submitMove)과 같은 PATCH, 대상만 다름.
	async function moveDocTo(bookId: string, targetPath: string) {
		const b = books.find((x) => x.book_id === bookId);
		if (!b || b.path === targetPath) return;
		try {
			const updated = await libraryApi.update(bookId, { path: targetPath });
			books = books.map((x) => (x.book_id === bookId ? updated : x));
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('library.moveFail', $locale), 'error');
		}
	}
	// explorer 모드 타일 드롭 시각 강조 + 핸들러.
	let dragOverFolder = $state<string | null>(null);
	function onTileDrop(e: DragEvent, targetPath: string) {
		e.preventDefault();
		dragOverFolder = null;
		const id = e.dataTransfer?.getData('text/plain');
		if (id) moveDocTo(id, targetPath);
	}

	// ─── DEV-239: 새 폴더 ───
	function openCreateFolder() {
		creatingFolder = true;
		createFolderPath = '';
		createFolderError = null;
	}
	function cancelCreateFolder() {
		creatingFolder = false;
		createFolderError = null;
	}
	async function submitCreateFolder() {
		const name = createFolderPath.trim();
		if (!name) {
			createFolderError = t('library.folderNameRequired', $locale);
			return;
		}
		const parent = viewMode === 'explorer' ? explorerPath : '';
		const path = parent ? `${parent}/${name}` : name;
		try {
			await libraryApi.folders.create(path);
			creatingFolder = false;
			await loadList(selectedId, true);
		} catch (e) {
			createFolderError = e instanceof Error ? e.message : t('library.createFolderFail', $locale);
		}
	}

	// BUG-123(admin 재지적 — 저번에도 커스텀 다이얼로그 요청 있었음): 네이티브
	// confirm()/alert() 대신 인앱 ConfirmDialog + toast.
	let confirmDeleteFolderPath = $state<string | null>(null);
	function askDeleteFolder(path: string) {
		confirmDeleteFolderPath = path;
	}
	async function deleteFolder() {
		const path = confirmDeleteFolderPath;
		confirmDeleteFolderPath = null;
		if (!path) return;
		try {
			await libraryApi.folders.delete(path);
			if (explorerPath === path || explorerPath.startsWith(`${path}/`)) {
				explorerPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
				syncUrl();
			}
			await loadList(selectedId, true);
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('library.deleteFolderFail', $locale), 'error');
		}
	}
</script>

<div class="page">
	{#if loading}
		<div class="state">Loading…</div>
	{:else if error}
		<div class="state err">{error}</div>
	{:else if viewMode === 'explorer' && !selected}
		<!-- ─── explorer 모드: 폴더/문서 그리드 (문서 선택 전) ─── -->
		<div class="explorer-toolbar">
			<h1 class="page-title">{t('library.title', $locale)}</h1>
			<div class="view-toggle">
				<button
					class:on={isMode('tree')}
					onclick={() => setViewMode('tree')}
					title={t('library.treeView', $locale)}>☰</button
				>
				<button
					class:on={isMode('explorer')}
					onclick={() => setViewMode('explorer')}
					title={t('library.iconView', $locale)}>▦</button
				>
			</div>
			<button class="btn-new" onclick={openCreateFolder}>{t('library.newFolder', $locale)}</button>
			<button class="btn-new" onclick={openCreate}>{t('library.newDoc', $locale)}</button>
		</div>
		<div class="search-row">
			<input
				class="search-input"
				type="search"
				placeholder={t('library.searchPlaceholder', $locale)}
				bind:value={searchQuery}
			/>
			<!-- REQ-011: 첨부 이름까지 검색. 켜지 않으면 예전과 동일. -->
			<label class="search-opt">
				<input
					type="checkbox"
					bind:checked={searchAttachments}
					data-testid="library-search-attachments"
				/>
				<span>{t('library.searchAttachments', $locale)}</span>
			</label>
			<!-- DEV-251: 문서 정렬 — quest list 의 sort-group 과 동일 패턴. -->
			<div class="sort-group" aria-label={t('library.sortAria', $locale)}>
				<select class="sort-sel" bind:value={docSortKey} title={t('library.sortTitle', $locale)}>
					{#each Object.entries(DOC_SORT_LABELS) as [k, label] (k)}
						<option value={k}>{label}</option>
					{/each}
				</select>
				<button
					class="sort-dir"
					onclick={() => (docSortDesc = !docSortDesc)}
					title={docSortDesc ? t('library.sortDesc', $locale) : t('library.sortAsc', $locale)}
					aria-label={t('library.sortDirAria', $locale)}>{docSortDesc ? '↓' : '↑'}</button
				>
			</div>
		</div>
		{#if allTagOptions.length > 0}
			<TagFilterRow
				tags={allTagOptions}
				counts={tagCounts}
				selected={filterTags}
				ontoggle={toggleTagFilter}
				onclear={() => (filterTags = new Set())}
				storageKey="library"
			/>
		{/if}
		<div class="crumbs">
			<!-- BUG-127(admin 요청): 현재 위치 왼쪽에 상위 폴더 이동 버튼.
			     BUG-127: 상위/도서관/각 경로 조각도 드롭 대상 — 문서를 여기로 끌어다
			     놓으면 그 조상 경로로 이동. -->
			<button
				class="btn-up"
				class:drag-over={dragOverFolder === parentOf(explorerPath)}
				onclick={() => gotoFolder(parentOf(explorerPath))}
				disabled={!explorerPath}
				title={t('library.upToParent', $locale)}
				ondragover={(e) => {
					if (!explorerPath) return;
					e.preventDefault();
					dragOverFolder = parentOf(explorerPath);
				}}
				ondragleave={() => (dragOverFolder = null)}
				ondrop={(e) => explorerPath && onTileDrop(e, parentOf(explorerPath))}
			>
				<Icon name="up" />
			</button>
			<button
				class="crumb"
				class:drag-over={dragOverFolder === ''}
				onclick={() => gotoFolder('')}
				ondragover={(e) => {
					e.preventDefault();
					dragOverFolder = '';
				}}
				ondragleave={() => (dragOverFolder = null)}
				ondrop={(e) => onTileDrop(e, '')}
			>
				{t('library.title', $locale)}
			</button>
			{#each explorerPath ? explorerPath.split('/') : [] as _seg, i (i)}
				{@const partial = explorerPath
					.split('/')
					.slice(0, i + 1)
					.join('/')}
				<span class="crumb-sep">›</span>
				<button
					class="crumb"
					class:drag-over={dragOverFolder === partial}
					onclick={() => gotoFolder(partial)}
					ondragover={(e) => {
						e.preventDefault();
						dragOverFolder = partial;
					}}
					ondragleave={() => (dragOverFolder = null)}
					ondrop={(e) => onTileDrop(e, partial)}
				>
					{_seg}
				</button>
			{/each}
			{#if explorerPath}
				<button class="btn-del-folder" onclick={() => askDeleteFolder(explorerPath)}>
					{t('library.deleteCurrentFolder', $locale)}
				</button>
			{/if}
		</div>

		{#if creatingFolder}
			<div class="modal-inline">
				<input
					class="text-input"
					type="text"
					placeholder={t('library.newFolderPlaceholder', $locale)}
					bind:value={createFolderPath}
					onkeydown={(e) => e.key === 'Enter' && submitCreateFolder()}
				/>
				{#if createFolderError}<p class="err">{createFolderError}</p>{/if}
				<div class="actions">
					<button class="btn-save" onclick={submitCreateFolder}
						>{t('library.create', $locale)}</button
					>
					<button class="btn-cancel" onclick={cancelCreateFolder}
						>{t('library.cancel', $locale)}</button
					>
				</div>
			</div>
		{/if}
		{#if creating}
			<div class="modal-inline">
				<input
					class="text-input"
					type="text"
					placeholder={t('library.docTitlePlaceholder', $locale)}
					bind:value={createTitle}
					onkeydown={(e) => e.key === 'Enter' && submitCreate()}
				/>
				{#if createError}<p class="err">{createError}</p>{/if}
				<div class="actions">
					<button class="btn-save" onclick={submitCreate}>{t('library.create', $locale)}</button>
					<button class="btn-cancel" onclick={cancelCreate}>{t('library.cancel', $locale)}</button>
				</div>
			</div>
		{/if}

		{#if searchResults}
			<!-- BUG-127: 검색은 현재 폴더(explorerPath) + 하위로 스코프, 폴더 이름도 매칭. -->
			{#if searchResults.folders.length === 0 && searchResults.books.length === 0}
				<p class="empty-list">{t('library.noSearchResults', $locale)}</p>
			{:else}
				<div class="tile-grid">
					{#each searchResults.folders as f (f.path)}
						<button
							class="tile"
							class:drag-over={dragOverFolder === f.path}
							onclick={() => gotoFolder(f.path)}
							ondragover={(e) => {
								e.preventDefault();
								dragOverFolder = f.path;
							}}
							ondragleave={() => (dragOverFolder = null)}
							ondrop={(e) => onTileDrop(e, f.path)}
						>
							<!-- BUG-153: 문서 타일은 아이콘 위에 BOOK 번호(.tile-sub)가 있어서, 폴더에
							     같은 자리가 비면 아이콘 높이가 서로 어긋난다(사용자 지적). 빈
							     자리표시자로 높이를 맞춘다. -->
							<span class="tile-sub" aria-hidden="true"></span>
							<!-- emoji-ok: DEV-326 admin 결정 — 도서관 타일은 이전(이모지) 모양 유지 -->
							<span class="tile-icon" aria-hidden="true">📁</span>
							<span class="tile-label" use:titlePopup={f.name}>{f.name}</span>
						</button>
					{/each}
					{#each searchResults.books as b (b.book_id)}
						<button
							class="tile"
							draggable="true"
							ondragstart={(e) => e.dataTransfer?.setData('text/plain', b.book_id)}
							onclick={() => select(b.book_id)}
						>
							<span class="tile-sub">{b.book_id}</span>
							<!-- emoji-ok: DEV-326 admin 결정 — 도서관 타일은 이전(이모지) 모양 유지 -->
							<span class="tile-icon" aria-hidden="true">📄</span>
							<span class="tile-label" use:titlePopup={b.title}>{b.title}</span>
						</button>
					{/each}
				</div>
			{/if}
		{:else if explorerFolders.length === 0 && explorerDocs.length === 0}
			<p class="empty-list">{t('library.emptyFolder', $locale)}</p>
		{:else}
			<div class="tile-grid">
				{#each explorerFolders as f (f.path)}
					<button
						class="tile"
						class:drag-over={dragOverFolder === f.path}
						onclick={() => gotoFolder(f.path)}
						ondragover={(e) => {
							e.preventDefault();
							dragOverFolder = f.path;
						}}
						ondragleave={() => (dragOverFolder = null)}
						ondrop={(e) => onTileDrop(e, f.path)}
					>
						<!-- BUG-153: 문서 타일은 아이콘 위에 BOOK 번호(.tile-sub)가 있어서, 폴더에
						     같은 자리가 비면 아이콘 높이가 서로 어긋난다(사용자 지적). 빈
						     자리표시자로 높이를 맞춘다. -->
						<span class="tile-sub" aria-hidden="true"></span>
						<!-- emoji-ok: DEV-326 admin 결정 — 도서관 타일은 이전(이모지) 모양 유지 -->
						<span class="tile-icon" aria-hidden="true">📁</span>
						<span class="tile-label" use:titlePopup={f.name}>{f.name}</span>
					</button>
				{/each}
				{#each explorerDocs as b (b.book_id)}
					<button
						class="tile"
						draggable="true"
						ondragstart={(e) => e.dataTransfer?.setData('text/plain', b.book_id)}
						onclick={() => select(b.book_id)}
					>
						<span class="tile-sub">{b.book_id}</span>
						<!-- emoji-ok: DEV-326 admin 결정 — 도서관 타일은 이전(이모지) 모양 유지 -->
						<span class="tile-icon" aria-hidden="true">📄</span>
						<span class="tile-label" use:titlePopup={b.title}>{b.title}</span>
					</button>
				{/each}
			</div>
		{/if}
	{:else}
		<div class="layout" class:single={viewMode === 'explorer'} style:--pane-w={`${$sidebarW}rem`}>
			{#if viewMode === 'tree'}
				<!-- 좌측 sidebar -->
				<aside class="sidebar">
					<div class="sidebar-head">
						<!-- BUG-127: 최상위(루트)로 이동 — 문서를 여기로 끌어다 놓으면 path=''. -->
						<h2
							class:drag-over={dragOverFolder === ''}
							role="presentation"
							ondragover={(e) => {
								e.preventDefault();
								dragOverFolder = '';
							}}
							ondragleave={() => (dragOverFolder = null)}
							ondrop={(e) => onTileDrop(e, '')}
						>
							{t('library.title', $locale)}
						</h2>
						<div class="view-toggle">
							<button
								class:on={isMode('tree')}
								onclick={() => setViewMode('tree')}
								title={t('library.treeView', $locale)}>☰</button
							>
							<button
								class:on={isMode('explorer')}
								onclick={() => setViewMode('explorer')}
								title={t('library.iconView', $locale)}>▦</button
							>
						</div>
					</div>
					<div class="sidebar-actions">
						<button
							class="btn-new"
							onclick={openCreateFolder}
							title={t('library.newFolder', $locale)}>{t('library.newFolder', $locale)}</button
						>
						<button class="btn-new" onclick={openCreate} title={t('library.newDoc', $locale)}
							>{t('library.newDoc', $locale)}</button
						>
					</div>
					<div class="search-row">
						<input
							class="search-input"
							type="search"
							placeholder={t('library.searchPlaceholder', $locale)}
							bind:value={searchQuery}
						/>
						<!-- REQ-011: 첨부 이름까지 검색. 위쪽 뷰의 검색줄과 같은 옵션. -->
						<label class="search-opt">
							<input
								type="checkbox"
								bind:checked={searchAttachments}
								data-testid="library-search-attachments"
							/>
							<span>{t('library.searchAttachments', $locale)}</span>
						</label>
						<!-- DEV-251: 문서 정렬 — quest list 의 sort-group 과 동일 패턴. -->
						<div class="sort-group" aria-label={t('library.sortAria', $locale)}>
							<select
								class="sort-sel"
								bind:value={docSortKey}
								title={t('library.sortTitle', $locale)}
							>
								{#each Object.entries(DOC_SORT_LABELS) as [k, label] (k)}
									<option value={k}>{label}</option>
								{/each}
							</select>
							<button
								class="sort-dir"
								onclick={() => (docSortDesc = !docSortDesc)}
								title={docSortDesc ? t('library.sortDesc', $locale) : t('library.sortAsc', $locale)}
								aria-label={t('library.sortDirAria', $locale)}>{docSortDesc ? '↓' : '↑'}</button
							>
						</div>
					</div>
					{#if allTagOptions.length > 0}
						<TagFilterRow
							tags={allTagOptions}
							counts={tagCounts}
							selected={filterTags}
							ontoggle={toggleTagFilter}
							onclear={() => (filterTags = new Set())}
							storageKey="library"
						/>
					{/if}
					{#if searchResults}
						<!-- BUG-127: tree 모드는 "현재 폴더" 개념이 없어 전역 검색 유지,
						     폴더 이름도 매칭 — 클릭하면 그 폴더를 펼쳐서 보여줌. -->
						{#if searchResults.folders.length === 0 && searchResults.books.length === 0}
							<p class="empty-list">{t('library.noSearchResults', $locale)}</p>
						{:else}
							<div class="book-list">
								{#each searchResults.folders as f (f.path)}
									<button class="book-item" onclick={() => revealFolder(f.path)}>
										<!-- emoji-ok: DEV-326 admin 결정 — 도서관 타일은 이전(이모지) 모양 유지 -->
										<span class="book-id">📁</span>
										<span class="book-title">{f.name}</span>
										<span class="book-path">{f.path}</span>
									</button>
								{/each}
								{#each searchResults.books as b (b.book_id)}
									<button
										class="book-item"
										class:active={b.book_id === selectedId}
										onclick={() => select(b.book_id)}
									>
										<span class="book-id">{b.book_id}</span>
										<span class="book-title">{b.title}</span>
										{#if b.path}<span class="book-path">{b.path}</span>{/if}
									</button>
								{/each}
							</div>
						{/if}
					{:else if books.length === 0 && folders.length === 0}
						<p class="empty-list">{t('library.emptyRoot', $locale)}</p>
					{:else}
						<div class="book-list">
							{#each tree.roots as node (node.path)}
								<LibraryFolderTree
									{node}
									depth={0}
									{selectedId}
									{collapsedFolders}
									onSelectDoc={select}
									onDeleteFolder={askDeleteFolder}
									onToggleCollapse={toggleFolderCollapsed}
									onMoveDoc={moveDocTo}
								/>
							{/each}
							{#each tree.rootDocs as b (b.book_id)}
								<button
									class="book-item"
									class:active={b.book_id === selectedId}
									draggable="true"
									ondragstart={(e) => e.dataTransfer?.setData('text/plain', b.book_id)}
									onclick={() => select(b.book_id)}
								>
									<span class="book-id">{b.book_id}</span>
									<span class="book-title">{b.title}</span>
								</button>
							{/each}
						</div>
					{/if}

					{#if creatingFolder}
						<div class="modal-inline">
							<input
								class="text-input"
								type="text"
								placeholder={t('library.newFolderPathPlaceholder', $locale)}
								bind:value={createFolderPath}
								onkeydown={(e) => e.key === 'Enter' && submitCreateFolder()}
							/>
							{#if createFolderError}<p class="err">{createFolderError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitCreateFolder}
									>{t('library.create', $locale)}</button
								>
								<button class="btn-cancel" onclick={cancelCreateFolder}
									>{t('library.cancel', $locale)}</button
								>
							</div>
						</div>
					{/if}
					{#if creating}
						<div class="modal-inline">
							<input
								class="text-input"
								type="text"
								placeholder={t('library.docTitlePlaceholder', $locale)}
								bind:value={createTitle}
								onkeydown={(e) => e.key === 'Enter' && submitCreate()}
							/>
							<select class="text-input" bind:value={createPath}>
								<option value="">{t('library.topLevel', $locale)}</option>
								{#each flattenFolderPaths(tree) as p (p)}
									<option value={p}>{p}</option>
								{/each}
							</select>
							{#if createError}<p class="err">{createError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitCreate}
									>{t('library.create', $locale)}</button
								>
								<button class="btn-cancel" onclick={cancelCreate}
									>{t('library.cancel', $locale)}</button
								>
							</div>
						</div>
					{/if}
				</aside>

				<!-- REQ-015: 구분선 = 드래그 핸들. 사이드바가 있을 때만(트리 보기). -->
				<PaneResizer pane="library" />
			{/if}

			<!-- 우측 panel -->
			<section class="panel">
				{#if !selected}
					<div class="empty">
						{#if books.length === 0}
							{t('library.pickFirstDoc', $locale)}
						{:else}
							{t('library.pickDocFromList', $locale)}
						{/if}
					</div>
				{:else}
					<div class="top-bar">
						{#if viewMode === 'explorer'}
							<button class="btn-edit" onclick={explorerBack}
								>{t('library.backToList', $locale)}</button
							>
						{/if}
						<!-- BUG-127(admin 요청): 뷰모드 토글은 트리/탐색기 목록 쪽에만 —
						     여기(문서 상세)에 중복으로 있을 필요 없음. 아이콘 뷰에서 상세로
						     들어가면 토글이 안 보이게 되는데, 그건 의도된 동작(admin 확인). -->
						<!-- admin 요청: `SLUG 제목` 한 줄 → slug 위, 제목 아래 2단
						     (모바일·PC 공통). 제목 텍스트는 스타일을 걸 수 있도록
						     span 으로 감싼다 — 벌거벗은 텍스트 노드는 익명 flex
						     item 이 되어 폭/줄바꿈 제어가 어렵다. -->
						<h1 class="doc-title">
							<span class="doc-id">{selected.book_id}</span>
							<span class="doc-title-text">{selected.title}</span>
						</h1>
						{#if !editMode}
							<div class="top-actions">
								<button class="btn-edit" onclick={enterEdit}>
									{selected.body.trim()
										? t('library.editDoc', $locale)
										: t('library.writeDoc', $locale)}
								</button>
								<button class="btn-edit" onclick={openRetitle}
									>{t('library.retitle', $locale)}</button
								>
								<button class="btn-edit" onclick={openMove}
									>{t('library.moveFolder', $locale)}</button
								>
								<button class="btn-edit danger" onclick={askDeleteSelected}
									>{t('library.delete', $locale)}</button
								>
							</div>
						{/if}
					</div>

					{#if selected.path}
						<!-- emoji-ok: DEV-326 admin 결정 — 도서관 타일은 이전(이모지) 모양 유지 -->
						<p class="doc-path">📁 {selected.path}</p>
					{/if}

					<!-- DEV-182: 생성 / 변경 시각. -->
					<div class="meta-times">
						<span class="meta-item">
							<span class="meta-label">{t('library.created', $locale)}</span>
							<time
								class="meta-val"
								datetime={selected.created_at}
								title={formatTs(selected.created_at)}
							>
								{formatTs(selected.created_at)}
							</time>
						</span>
						<span class="meta-sep">·</span>
						<span class="meta-item">
							<span class="meta-label">{t('library.updated', $locale)}</span>
							<time
								class="meta-val"
								datetime={selected.updated_at}
								title={formatTs(selected.updated_at)}
							>
								{formatRelative(selected.updated_at, undefined, $locale)}
							</time>
						</span>
					</div>

					{#if retitling}
						<div class="modal-inline">
							<input
								class="text-input"
								type="text"
								placeholder={t('library.newTitlePlaceholder', $locale)}
								bind:value={retitleText}
								onkeydown={(e) => e.key === 'Enter' && submitRetitle()}
							/>
							{#if retitleError}<p class="err">{retitleError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitRetitle}
									>{t('library.change', $locale)}</button
								>
								<button class="btn-cancel" onclick={cancelRetitle}
									>{t('library.cancel', $locale)}</button
								>
							</div>
						</div>
					{/if}

					{#if moving}
						<div class="modal-inline">
							<select class="text-input" bind:value={movePath}>
								<option value="">{t('library.topLevel', $locale)}</option>
								{#each flattenFolderPaths(tree) as p (p)}
									<option value={p}>{p}</option>
								{/each}
							</select>
							{#if moveError}<p class="err">{moveError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitMove}>{t('library.move', $locale)}</button>
								<button class="btn-cancel" onclick={cancelMove}
									>{t('library.cancel', $locale)}</button
								>
							</div>
						</div>
					{/if}

					{#if editMode}
						<div
							class="edit-form"
							use:saveShortcut={{ disabled: saving, onSave: () => void save(true) }}
						>
							<div class="field-label">
								<span>{t('library.bodyHint', $locale)}</span>
								<!-- DEV-237: 비미디어 파일은 attachToSection 이 첨부 섹션에 등록. -->
								<MarkdownEditor
									bind:value={editText}
									onError={(msg) => (saveError = `${t('library.attachUploadFail', $locale)}${msg}`)}
									onAttach={attachToSection}
								/>
							</div>
							<div class="actions">
								<button class="btn-save" onclick={() => save()} disabled={saving}>
									{saving ? t('worklogPage.saving', $locale) : t('worklogPage.save', $locale)}
								</button>
								<button class="btn-cancel" onclick={cancelEdit} disabled={saving}>
									{t('library.cancel', $locale)}
								</button>
							</div>
							{#if saveError}<p class="err">{saveError}</p>{/if}
						</div>
					{:else if selected.body.trim()}
						<MarkdownView source={selected.body} />
					{:else}
						<div class="empty">
							{t('library.noBodyYet', $locale)}
							<button class="link" onclick={enterEdit}>{t('library.writeNow', $locale)}</button>
						</div>
					{/if}

					<!-- DEV-243: 태그. -->
					<TagPills tags={selected.tags} {tagDefs} onSetTags={setDocTags} />

					<!-- DEV-237: 첨부 섹션 — 이미지/동영상 외 임의 파일. -->
					<AttachmentSection
						slug={selected.book_id}
						scope="library"
						bind:attachments={selected.attachments}
					/>

					<!-- DEV-290: BOOK 변경 이력. -->
					<!-- REQ-008: 이 문서를 참조하는 문서. -->
					<BacklinkSection kind="book" id={selected.book_id} />
					<SidecarHistory kind="book" id={selected.book_id} />
				{/if}
			</section>
		</div>
	{/if}
</div>

<ConfirmDialog
	open={confirmDeleteId !== null}
	title={t('library.deleteDocTitle', $locale)}
	message={`'${confirmDeleteId ?? ''}' ${t('library.deleteDocMessage', $locale)}`}
	confirmLabel={t('library.delete', $locale)}
	danger
	onconfirm={deleteSelected}
	oncancel={() => (confirmDeleteId = null)}
/>

<!-- BUG-123: 폴더 삭제 확인도 네이티브 confirm() 대신 인앱 다이얼로그. -->
<ConfirmDialog
	open={confirmDeleteFolderPath !== null}
	title={t('library.deleteFolderTitle', $locale)}
	message={`${t('library.deleteFolderMessagePre', $locale)}${confirmDeleteFolderPath ?? ''}${t('library.deleteFolderMessagePost', $locale)}`}
	confirmLabel={t('library.delete', $locale)}
	danger
	onconfirm={deleteFolder}
	oncancel={() => (confirmDeleteFolderPath = null)}
/>

<ConfirmDialog
	open={confirmDiscardId !== null || pendingBackToGrid}
	title={t('library.editingMoveTitle', $locale)}
	message={t('library.editingMoveMessage', $locale)}
	confirmLabel={t('library.discardAndMove', $locale)}
	danger
	onconfirm={applyPendingSelect}
	oncancel={() => {
		confirmDiscardId = null;
		pendingBackToGrid = false;
	}}
/>

<style>
	.page {
		padding: 1.25rem 1.5rem 2rem;
		max-width: var(--content-max-width, 1200px);
		margin: 0 auto;
	}
	.layout {
		display: grid;
		/* BUG-254: 사이드바 폭이 px 고정이라 배율을 올리면 안의 버튼·태그·제목이
		   경계를 넘었다(admin 보고 + 스크린샷). 내용이 커지면 칸도 같이 커져야 한다.
		   REQ-015: 그 폭을 사용자가 드래그로 정한다. `--pane-w` 는 store 가 rem
		   으로 주므로 배율을 계속 따라간다. 기본값은 예전 16.25rem 그대로.
		   가운데 열이 예전 `gap: 1.25rem` 자리를 그대로 차지하는 드래그 핸들
		   이라 기존 간격·배치가 안 바뀐다. */
		grid-template-columns: var(--pane-w, 16.25rem) 1.25rem 1fr;
		gap: 0;
		min-height: 70vh;
	}
	/* 아이콘 보기(explorer)는 사이드바가 없어 조절할 것도 없다. */
	.layout.single {
		grid-template-columns: 1fr;
	}
	/* DEV-257(사용자 보고): 375px 급 화면에서 260px 고정 sidebar 가 화면의
	   2/3 를 먹고, 남은 `1fr` 열은 min-content 아래로 못 줄어들어 컨텐츠가
	   화면 밖으로 밀렸다 — 페이지에 가로 스크롤이 생기고 sticky 메뉴바까지
	   잘려 보였다. 좁은 화면에서는 위/아래로 쌓고, sidebar 는 자체 스크롤
	   영역으로 높이를 제한해 컨텐츠가 화면 아래로 밀려나지 않게 한다. */
	.sidebar {
		border-right: var(--bw) solid var(--bg-subtle);
		padding-right: 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	/* BUG-200 후속 감사(규칙 페이지와 동일): 이 블록이 기본 `.sidebar` 규칙보다
	   **앞**에 있어 border-right / padding-right override 가 순서에서 지고 있었다.
	   기본 규칙 뒤로 옮긴다 — 같은 우선순위면 뒤가 이긴다. */
	@media (max-width: 640px) {
		.layout,
		.layout.single {
			grid-template-columns: 1fr;
		}
		/* 한 열로 쌓이면 좌우 구분선이 없어져 조절할 대상이 사라진다. */
		.layout :global(.pane-resizer) {
			display: none;
		}
		/* 두 열이 한 열로 쌓이면 세로 구분선이 의미를 잃는다. */
		.sidebar {
			border-right: none;
			border-bottom: var(--bw) solid var(--bg-subtle);
			padding-right: 0;
			padding-bottom: 0.75rem;
			max-height: 45vh;
			max-height: 45dvh;
			overflow-y: auto;
		}
	}
	.sidebar-head {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}
	.sidebar-head h2 {
		font-size: 0.8rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-muted);
		margin: 0;
		border-radius: var(--r-sm);
	}
	/* BUG-127: 루트로 드래그&드롭 이동 강조. */
	.sidebar-head h2.drag-over {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		outline: 1px dashed var(--accent);
		color: var(--text);
	}
	.sidebar-actions {
		display: flex;
		gap: 0.35rem;
		margin-bottom: 0.25rem;
	}
	.view-toggle {
		margin-left: auto;
		display: flex;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		overflow: hidden;
	}
	.view-toggle button {
		padding: 0.15rem 0.4rem;
		background: transparent;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.75rem;
	}
	.view-toggle button.on {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		color: var(--text);
	}
	.btn-new {
		padding: 0.15rem 0.55rem;
		background: transparent;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
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
		border-radius: var(--r-sm);
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
		font-family: var(--font-mono);
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

	.explorer-toolbar {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		margin-bottom: 0.75rem;
	}
	.page-title {
		font-size: 1.1rem;
		font-weight: 600;
		margin: 0;
	}
	.crumbs {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		margin-bottom: 1rem;
		font-size: 0.82rem;
	}
	.crumb {
		background: transparent;
		border: none;
		color: var(--text-accent, var(--accent));
		cursor: pointer;
		font-size: 0.82rem;
		padding: 0;
	}
	.crumb-sep {
		color: var(--text-muted);
	}
	.btn-up {
		background: transparent;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.78rem;
		line-height: 1;
		padding: 0.2rem 0.4rem;
	}
	.btn-up:hover:not(:disabled) {
		color: var(--text);
		border-color: var(--text-faint);
	}
	.btn-up:disabled {
		opacity: 0.35;
		cursor: default;
	}
	/* BUG-127: 상위 폴더 버튼/경로 조각도 드롭 대상. */
	.btn-up.drag-over,
	.crumb.drag-over {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		border-radius: var(--r-sm);
		outline: 1px dashed var(--accent);
	}
	.btn-del-folder {
		margin-left: auto;
		background: transparent;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text-muted);
		font-size: 0.72rem;
		padding: 0.1rem 0.5rem;
		cursor: pointer;
	}
	.btn-del-folder:hover {
		color: var(--danger);
	}

	/* DEV-239: 탐색기 타일 — 개수와 무관하게 고정 크기, 좌상단부터 채움. */
	.tile-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, 5.75rem);
		justify-content: start;
		gap: 0.4rem;
	}
	.tile {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.35rem;
		padding: 0.6rem 0.25rem;
		border-radius: var(--r-md);
		border: var(--bw) solid transparent;
		background: transparent;
		width: 5.75rem;
		cursor: pointer;
		color: var(--text);
	}
	.tile:hover {
		background: var(--bg-elevated);
	}
	/* BUG-127: 드래그 중인 문서가 이 폴더 타일 위에 있을 때 강조. */
	.tile.drag-over {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		border-color: var(--accent);
	}
	.tile-icon {
		font-size: 2rem;
	}
	.tile-label {
		font-size: 0.75rem;
		text-align: center;
		line-height: 1.3;
		/* ★ BUG-153 (진짜 원인, 사용자 검증 완료): global.css 의
		   `button { white-space: nowrap }`(BUG-143 — CJK 버튼 라벨이 두 줄 되는
		   것 방지)이 `.tile`(=<button>)에서 이 라벨로 **상속**돼 줄바꿈이 원천
		   차단돼 있었다. nowrap 이면 폭을 어떻게 확정해도(max-width/width/
		   align-self) 절대 안 꺾이고 overflow:hidden 이 한 줄로 자른다 — 앞선
		   시도들이 전부 이것 때문에 실패. 그 전역 규칙 주석이 안내한 대로
		   여기서 opt-out 한다.
		   폭은 정렬 방식에 의존하지 않는 퍼센트로 확정(버튼의 anonymous content
		   box 때문에 stretch 만으로는 불안정), 3줄은 max-height(1.3em × 3). */
		white-space: normal;
		width: 100%;
		max-width: 100%;
		box-sizing: border-box;
		min-width: 0;
		display: block;
		word-break: break-word;
		overflow-wrap: anywhere;
		max-height: 3.9em;
		overflow: hidden;
	}
	.tile-sub {
		font-family: var(--font-mono);
		font-size: 0.62rem;
		color: var(--text-muted);
		/* BUG-153: 폴더 타일은 내용이 빈 자리표시자 — 빈 인라인 요소는 높이가
		   0 이라 문서/폴더 아이콘 높이가 어긋난다. 한 줄 높이를 확보. */
		min-height: 1.2em;
	}

	.top-bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
		/* BUG-197: 좁은 화면에서 제목과 버튼 4개가 한 줄을 다투면 제목이 0 폭까지
		   눌려 **글자마다 줄바꿈**됐다(세로 한 줄로 보임). 줄바꿈을 허용하고
		   제목에 최소 폭을 줘서, 자리가 모자라면 버튼이 다음 줄로 내려가게 한다. */
		flex-wrap: wrap;
	}
	.doc-title {
		font-size: 1.1rem;
		font-weight: 600;
		color: var(--text);
		margin: 0;
		display: flex;
		/* admin 요청: slug 위 / 제목 아래. 화면 폭과 무관하게 항상 2단. */
		flex-direction: column;
		align-items: flex-start;
		gap: 0.1rem;
		/* BUG-197: min-width:0 만 있으면 "얼마든지 줄어도 된다"는 뜻이라 한 글자
		   폭까지 눌린다. 최소 폭 + 늘어남으로 바꿔 버튼 쪽이 밀려나게. */
		flex: 1 1 14rem;
		min-width: 10rem;
		overflow-wrap: anywhere;
	}
	.doc-title-text {
		line-height: 1.3;
	}
	.doc-id {
		font-family: var(--font-mono);
		font-size: 0.85rem;
		color: var(--accent);
		flex: none;
	}
	.doc-path {
		color: var(--text-muted);
		font-size: 0.78rem;
		margin: -0.5rem 0 0.75rem;
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
		flex: none;
	}
	.btn-edit {
		padding: 0.3rem 0.7rem;
		background: transparent;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
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
		border: var(--bw) dashed var(--border);
		border-radius: var(--r-md);
		margin: 0.5rem 0;
	}
	.text-input {
		padding: 0.35rem 0.55rem;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-sm);
		font-size: 0.85rem;
	}
	/* DEV-238: 검색창 — sidebar/explorer 툴바 양쪽에서 재사용. */
	.search-input {
		width: 100%;
		padding: 0.3rem 0.55rem;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-sm);
		font-size: 0.82rem;
		margin: 0.4rem 0;
	}
	.book-path {
		font-size: 0.68rem;
		color: var(--text-muted);
	}

	/* DEV-251: 검색 + 정렬 배치.
	   REQ-016: 한 줄이었는데 입력란이 옵션에 눌려 못 쓸 만큼 좁았다
	   (실측 243px 폭의 줄에서 입력란 51.6px / 체크박스 89px / 정렬 91.2px).
	   입력란에 한 줄을 통째로 주고 나머지는 아래로 내린다 — 총 2줄. */
	.search-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
		align-items: center;
	}
	/* BUG-266: 탐색기 보기에서 이 줄과 바로 뒤 태그 필터 줄이 딱 붙어 있었다
	   (admin 보고). `.page` 는 일반 블록이라 자식들이 각자 margin 으로 간격을
	   만드는데(`.explorer-toolbar` 0.75rem, `.crumbs` 1rem) 이 줄에만 없었다.
	   사이드바/규칙/퀘스트 목록에서 안 보였던 건 그쪽 부모가 flex `gap` 을
	   주기 때문이다 — 그래서 `TagFilterRow` 에 `margin-top` 을 다는 대신
	   여기만 채운다(그쪽은 gap 과 더해져 안 하던 곳까지 벌어진다).

	   **자식 결합자가 중요하다.** `.search-row` 는 이 파일에서 두 번 쓰인다
	   (탐색기 / 사이드바). 사이드바 쪽은 `.layout > .sidebar` 안이라 여기
	   걸리지 않는다. 값은 사이드바의 `gap` 과 같게 맞춰 두 보기의 간격이
	   같아 보이게 한다. */
	.page > .search-row {
		margin-bottom: 0.5rem;
	}
	.search-row .search-input {
		/* flex-basis 100% → 항상 자기 줄을 독차지하고 뒤 요소를 밀어낸다. */
		flex: 1 1 100%;
		min-width: 0;
	}
	/* 둘째 줄 — 체크박스는 왼쪽, 정렬 컨트롤은 오른쪽 끝. */
	.search-row .sort-group {
		margin-left: auto;
	}
	/* DEV-251: 정렬 select + 방향 토글 — QuestList 의 sort-group 과 동일 패턴. */
	.sort-group {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		flex: none;
	}
	.sort-sel {
		padding: 0.25rem 0.5rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text);
		font-size: 0.78rem;
		cursor: pointer;
	}
	.sort-dir {
		width: 1.7rem;
		height: 1.7rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text-muted);
		font-size: 0.85rem;
		cursor: pointer;
	}
	.sort-dir:hover {
		color: var(--text);
		border-color: var(--text-faint);
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
	/* DEV-203: .editor-wrap CSS 는 공통 MarkdownEditor 컴포넌트로 이동. */

	.actions {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
	.btn-save {
		padding: 0.35rem 0.85rem;
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
		padding: 0.35rem 0.85rem;
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
	/* REQ-011: 검색 영역 확장 체크박스 — quest 목록 필터의 같은 이름 클래스와
	   같은 모양으로 맞춘다. */
	.search-opt {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		flex: none;
		font-size: 0.75rem;
		color: var(--text-muted);
		white-space: nowrap;
		cursor: pointer;
	}
</style>
