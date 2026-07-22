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
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { setUnsaved } from '$lib/stores/unsaved';
	import { libraryApi, type Book, type LibraryFolder } from '$lib/api/library';
	import { buildLibraryTree, flattenFolderPaths, searchLibrary } from '$lib/utils/library-tree';
	import LibraryFolderTree from '$lib/components/LibraryFolderTree.svelte';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
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
	const explorerFolders = $derived(explorerPath === '' ? tree.roots : (explorerNode?.children ?? []));
	const explorerDocs = $derived(explorerPath === '' ? tree.rootDocs : (explorerNode?.docs ?? []));

	// DEV-238 → BUG-127 후속(admin 보고): 예전엔 항상 전역 검색이었음 — 이제
	// explorer(아이콘) 모드는 현재 폴더(explorerPath) + 하위만 검색(탐색기의
	// "여기서 찾기" 감각). tree 모드는 "현재 폴더" 개념 자체가 없어(트리
	// 전체를 항상 다 보여줌) 전역 유지. 폴더 이름도 매칭 대상에 포함.
	let searchQuery = $state('');
	const searchScope = $derived(viewMode === 'explorer' ? explorerPath : '');
	const searchResults = $derived(searchLibrary(tree, sortedBooks, searchQuery, searchScope));

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

	async function loadList(preferId?: string | null) {
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

	async function save() {
		if (!selectedId) return;
		saving = true;
		saveError = null;
		try {
			const text = editText;
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
			await loadList(created.book_id);
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
			await loadList();
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
			await loadList(selectedId);
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
			await loadList(selectedId);
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
				<button class:on={isMode('tree')} onclick={() => setViewMode('tree')} title={t('library.treeView', $locale)}>☰</button>
				<button class:on={isMode('explorer')} onclick={() => setViewMode('explorer')} title={t('library.iconView', $locale)}>▦</button>
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
			<div class="tag-filter-row" aria-label={t('library.tagFilterAria', $locale)}>
				{#each allTagOptions as tag (tag)}
					<button
						class="tag-filter-chip"
						class:active={filterTags.has(tag)}
						onclick={() => toggleTagFilter(tag)}
						title={filterTags.has(tag) ? `${tag}${t('library.tagFilterOffPost', $locale)}` : `${tag}${t('library.tagFilterOnPost', $locale)}`}
					>
						{tag}
						<span class="tag-chip-count">{tagCounts.get(tag) ?? 0}</span>
					</button>
				{/each}
				{#if filterTags.size > 0}
					<button class="tag-clear" onclick={() => (filterTags = new Set())} title={t('library.clearTagFiltersTitle', $locale)}>
						{t('library.clearTagFilters', $locale)}
					</button>
				{/if}
			</div>
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
				⬆
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
				{@const partial = explorerPath.split('/').slice(0, i + 1).join('/')}
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
					<button class="btn-save" onclick={submitCreateFolder}>{t('library.create', $locale)}</button>
					<button class="btn-cancel" onclick={cancelCreateFolder}>{t('library.cancel', $locale)}</button>
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
							<span class="tile-icon" aria-hidden="true">📁</span>
							<span class="tile-label" title={f.name}>{f.name}</span>
						</button>
					{/each}
					{#each searchResults.books as b (b.book_id)}
						<button
							class="tile"
							draggable="true"
							ondragstart={(e) => e.dataTransfer?.setData('text/plain', b.book_id)}
							onclick={() => select(b.book_id)}
						>
							<span class="tile-icon" aria-hidden="true">📄</span>
							<span class="tile-label" title={b.title}>{b.title}</span>
							<span class="tile-sub">{b.book_id}</span>
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
						<span class="tile-icon" aria-hidden="true">📁</span>
						<span class="tile-label" title={f.name}>{f.name}</span>
					</button>
				{/each}
				{#each explorerDocs as b (b.book_id)}
					<button
						class="tile"
						draggable="true"
						ondragstart={(e) => e.dataTransfer?.setData('text/plain', b.book_id)}
						onclick={() => select(b.book_id)}
					>
						<span class="tile-icon" aria-hidden="true">📄</span>
						<span class="tile-label" title={b.title}>{b.title}</span>
						<span class="tile-sub">{b.book_id}</span>
					</button>
				{/each}
			</div>
		{/if}
	{:else}
		<div class="layout" class:single={viewMode === 'explorer'}>
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
							<button class:on={isMode('tree')} onclick={() => setViewMode('tree')} title={t('library.treeView', $locale)}>☰</button>
							<button class:on={isMode('explorer')} onclick={() => setViewMode('explorer')} title={t('library.iconView', $locale)}>▦</button>
						</div>
					</div>
					<div class="sidebar-actions">
						<button class="btn-new" onclick={openCreateFolder} title={t('library.newFolder', $locale)}>{t('library.newFolder', $locale)}</button>
						<button class="btn-new" onclick={openCreate} title={t('library.newDoc', $locale)}>{t('library.newDoc', $locale)}</button>
					</div>
					<div class="search-row">
						<input
							class="search-input"
							type="search"
							placeholder={t('library.searchPlaceholder', $locale)}
							bind:value={searchQuery}
						/>
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
						<div class="tag-filter-row" aria-label={t('library.tagFilterAria', $locale)}>
							{#each allTagOptions as tag (tag)}
								<button
									class="tag-filter-chip"
									class:active={filterTags.has(tag)}
									onclick={() => toggleTagFilter(tag)}
									title={filterTags.has(tag) ? `${tag}${t('library.tagFilterOffPost', $locale)}` : `${tag}${t('library.tagFilterOnPost', $locale)}`}
								>
									{tag}
									<span class="tag-chip-count">{tagCounts.get(tag) ?? 0}</span>
								</button>
							{/each}
							{#if filterTags.size > 0}
								<button
									class="tag-clear"
									onclick={() => (filterTags = new Set())}
									title={t('library.clearTagFiltersTitle', $locale)}
								>
									{t('library.clearTagFilters', $locale)}
								</button>
							{/if}
						</div>
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
								<button class="btn-save" onclick={submitCreateFolder}>{t('library.create', $locale)}</button>
								<button class="btn-cancel" onclick={cancelCreateFolder}>{t('library.cancel', $locale)}</button>
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
								<button class="btn-save" onclick={submitCreate}>{t('library.create', $locale)}</button>
								<button class="btn-cancel" onclick={cancelCreate}>{t('library.cancel', $locale)}</button>
							</div>
						</div>
					{/if}
				</aside>
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
							<button class="btn-edit" onclick={explorerBack}>{t('library.backToList', $locale)}</button>
						{/if}
						<!-- BUG-127(admin 요청): 뷰모드 토글은 트리/탐색기 목록 쪽에만 —
						     여기(문서 상세)에 중복으로 있을 필요 없음. 아이콘 뷰에서 상세로
						     들어가면 토글이 안 보이게 되는데, 그건 의도된 동작(admin 확인). -->
						<h1 class="doc-title">
							<span class="doc-id">{selected.book_id}</span>
							{selected.title}
						</h1>
						{#if !editMode}
							<div class="top-actions">
								<button class="btn-edit" onclick={enterEdit}>
									{selected.body.trim() ? t('library.editDoc', $locale) : t('library.writeDoc', $locale)}
								</button>
								<button class="btn-edit" onclick={openRetitle}>{t('library.retitle', $locale)}</button>
								<button class="btn-edit" onclick={openMove}>{t('library.moveFolder', $locale)}</button>
								<button class="btn-edit danger" onclick={askDeleteSelected}>{t('library.delete', $locale)}</button>
							</div>
						{/if}
					</div>

					{#if selected.path}
						<p class="doc-path">📁 {selected.path}</p>
					{/if}

					<!-- DEV-182: 생성 / 변경 시각. -->
					<div class="meta-times">
						<span class="meta-item">
							<span class="meta-label">{t('library.created', $locale)}</span>
							<time class="meta-val" datetime={selected.created_at} title={formatTs(selected.created_at)}>
								{formatTs(selected.created_at)}
							</time>
						</span>
						<span class="meta-sep">·</span>
						<span class="meta-item">
							<span class="meta-label">{t('library.updated', $locale)}</span>
							<time class="meta-val" datetime={selected.updated_at} title={formatTs(selected.updated_at)}>
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
								<button class="btn-save" onclick={submitRetitle}>{t('library.change', $locale)}</button>
								<button class="btn-cancel" onclick={cancelRetitle}>{t('library.cancel', $locale)}</button>
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
								<button class="btn-cancel" onclick={cancelMove}>{t('library.cancel', $locale)}</button>
							</div>
						</div>
					{/if}

					{#if editMode}
						<div class="edit-form">
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
								<button class="btn-save" onclick={save} disabled={saving}>
									{saving ? t('worklogPage.saving', $locale) : t('worklogPage.save', $locale)}
								</button>
								<button class="btn-cancel" onclick={cancelEdit} disabled={saving}> {t('library.cancel', $locale)} </button>
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
		grid-template-columns: 260px 1fr;
		gap: 1.25rem;
		min-height: 70vh;
	}
	.layout.single {
		grid-template-columns: 1fr;
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
	}
	.sidebar-head h2 {
		font-size: 0.8rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-muted);
		margin: 0;
		border-radius: 4px;
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
		border: 1px solid var(--border);
		border-radius: 4px;
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
		border: 1px solid var(--border);
		border-radius: 4px;
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
		border-radius: 4px;
		outline: 1px dashed var(--accent);
	}
	.btn-del-folder {
		margin-left: auto;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 4px;
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
		grid-template-columns: repeat(auto-fill, 92px);
		justify-content: start;
		gap: 0.4rem;
	}
	.tile {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.35rem;
		padding: 0.6rem 0.25rem;
		border-radius: 6px;
		border: 1px solid transparent;
		background: transparent;
		width: 92px;
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
		word-break: break-word;
		/* BUG-153: align-items:center 인 column flex 에선 자식이 content 폭을
		   그대로 가져 긴 제목이 타일(92px) 밖 옆 타일 영역까지 침범했다 —
		   타일 폭으로 제한하면 word-break 로 줄바꿈된다. 줄 수 제한(line-clamp)은
		   두지 않아 제목 전체가 보이게 한다(말줄임으로 잘리던 문제 — 사용자 재지적).
		   긴 제목은 타일이 세로로 길어질 뿐 옆은 침범하지 않는다. */
		max-width: 100%;
	}
	.tile-sub {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.62rem;
		color: var(--text-muted);
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
	/* DEV-238: 검색창 — sidebar/explorer 툴바 양쪽에서 재사용. */
	.search-input {
		width: 100%;
		padding: 0.3rem 0.55rem;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 4px;
		font-size: 0.82rem;
		margin: 0.4rem 0;
	}
	.book-path {
		font-size: 0.68rem;
		color: var(--text-muted);
	}

	/* DEV-251: 검색 + 정렬 한 줄 배치. */
	.search-row {
		display: flex;
		gap: 0.35rem;
		align-items: center;
	}
	.search-row .search-input {
		flex: 1;
		min-width: 0;
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
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.78rem;
		cursor: pointer;
	}
	.sort-dir {
		width: 1.7rem;
		height: 1.7rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.85rem;
		cursor: pointer;
	}
	.sort-dir:hover {
		color: var(--text);
		border-color: var(--text-faint);
	}

	/* DEV-243 후속: 태그 필터 chip — quest QuestList.svelte 와 동일 패턴. */
	.tag-filter-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
		align-items: center;
		margin-bottom: 0.4rem;
	}
	.tag-filter-chip {
		padding: 0.15rem 0.65rem;
		background: color-mix(in srgb, var(--warning) 8%, transparent);
		border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
		border-radius: 20px;
		color: var(--warning);
		font-size: 0.72rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		cursor: pointer;
		transition:
			background 0.1s,
			border-color 0.1s;
	}
	.tag-filter-chip:hover {
		background: color-mix(in srgb, var(--warning) 18%, transparent);
	}
	.tag-filter-chip.active {
		background: color-mix(in srgb, var(--warning) 28%, transparent);
		border-color: color-mix(in srgb, var(--warning) 70%, transparent);
		color: color-mix(in srgb, var(--warning) 60%, white);
	}
	.tag-chip-count {
		display: inline-block;
		margin-left: 0.4rem;
		padding: 0 0.4rem;
		min-width: 1.1rem;
		text-align: center;
		font-size: 0.65rem;
		color: var(--text-muted);
		background: var(--bg-subtle);
		border-radius: 10px;
	}
	.tag-clear {
		padding: 0.15rem 0.55rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 20px;
		color: var(--text-muted);
		font-size: 0.7rem;
		cursor: pointer;
	}
	.tag-clear:hover {
		background: var(--bg-subtle);
		color: var(--text);
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
