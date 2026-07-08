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
	import { onMount, onDestroy, tick } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { setUnsaved } from '$lib/stores/unsaved';
	import { libraryApi, type Book, type LibraryFolder } from '$lib/api/library';
	import { buildLibraryTree, flattenFolderPaths, searchBooks } from '$lib/utils/library-tree';
	import LibraryFolderTree from '$lib/components/LibraryFolderTree.svelte';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import AttachmentSection from '$lib/components/AttachmentSection.svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	import { indentExtensions } from '$lib/utils/editor-indent';
	import { editorSettings } from '$lib/stores/editorSettings';
	// DEV-097 대체 결정: 도서관 문서가 공유 자료의 보금자리 — 첨부 지원 필수.
	import { attachmentExtension } from '$lib/utils/editor-attach';
	import { crossLinkAutocomplete } from '$lib/utils/editor-links';
	import { loadQuestIndex } from '$lib/stores/questIndex';
	// BUG-123(admin 재지적): 네이티브 alert() 대신 앱 공용 toast — 다른 페이지들과
	// 동일한 alert() 대체 컨벤션(+layout.svelte 주석 참고).
	import { showToast } from '$lib/stores/toast';

	let loading = $state(true);
	let error = $state<string | null>(null);
	let books = $state<Book[]>([]);
	let folders = $state<LibraryFolder[]>([]);
	let selectedId = $state<string | null>(null);

	const selected = $derived(books.find((b) => b.book_id === selectedId) ?? null);

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
	const tree = $derived(buildLibraryTree(folders, books));

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

	// DEV-238: 검색 — 활성화되면 폴더 구조/현재 탐색 위치와 무관하게 매칭
	// 문서만 평평하게 보여줌(검색 중엔 "어느 폴더에 있나"보다 "찾았나"가 우선).
	let searchQuery = $state('');
	const searchResults = $derived(searchBooks(books, searchQuery));

	let editMode = $state(false);
	$effect(() => setUnsaved('library-edit', editMode));
	onDestroy(() => setUnsaved('library-edit', false));
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
		editMode = true;
		saveError = null;
		await tick();
		initEditor(selected.body);
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

	function initEditor(doc: string) {
		if (!editorContainer) return;
		if (editorView) {
			editorView.destroy();
			editorView = null;
		}
		editorContainer.style.height = `${loadEditorHeight()}px`;
		editorView = new EditorView({
			doc,
			extensions: [
				basicSetup,
				markdown(),
				editorThemeCompartment.of(editorThemeExtension($theme)),
				// DEV-237: mediaOnly 제거 — 이미지/동영상 외 임의 파일은 attachToSection
				// 이 첨부 섹션에 등록(본문에 dead link 인라인 안 됨).
				attachmentExtension((msg) => (saveError = `첨부 업로드 실패: ${msg}`), attachToSection),
				crossLinkAutocomplete(),
				indentExtensions($editorSettings),
				EditorView.theme({
					'&': { fontSize: '0.875rem', borderRadius: '6px', height: '100%' },
					'.cm-editor': { borderRadius: '6px', height: '100%' },
					'.cm-scroller': { overflow: 'auto' }
				})
			],
			parent: editorContainer
		});
	}

	$effect(() => {
		const t = $theme;
		editorView?.dispatch({
			effects: editorThemeCompartment.reconfigure(editorThemeExtension(t))
		});
	});

	function cancelEdit() {
		editorView?.destroy();
		editorView = null;
		editMode = false;
		saveError = null;
	}

	async function save() {
		if (!selectedId) return;
		saving = true;
		saveError = null;
		try {
			const text = editorView ? editorView.state.doc.toString() : '';
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
			createError = '제목을 입력하세요.';
			return;
		}
		try {
			const created = await libraryApi.create(title, '', createPath);
			creating = false;
			await loadList(created.book_id);
		} catch (e) {
			createError = e instanceof Error ? e.message : '생성 실패';
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
			showToast(e instanceof Error ? e.message : '삭제 실패', 'error');
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
			retitleError = e instanceof Error ? e.message : '제목 변경 실패';
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
			moveError = e instanceof Error ? e.message : '이동 실패';
		}
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
			createFolderError = '폴더 이름을 입력하세요.';
			return;
		}
		const parent = viewMode === 'explorer' ? explorerPath : '';
		const path = parent ? `${parent}/${name}` : name;
		try {
			await libraryApi.folders.create(path);
			creatingFolder = false;
			await loadList(selectedId);
		} catch (e) {
			createFolderError = e instanceof Error ? e.message : '폴더 생성 실패';
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
			showToast(e instanceof Error ? e.message : '폴더 삭제 실패 — 비어 있는지 확인하세요', 'error');
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
			<h1 class="page-title">도서관</h1>
			<div class="view-toggle">
				<button class:on={isMode('tree')} onclick={() => setViewMode('tree')} title="트리 보기">☰</button>
				<button class:on={isMode('explorer')} onclick={() => setViewMode('explorer')} title="아이콘 보기">▦</button>
			</div>
			<button class="btn-new" onclick={openCreateFolder}>+ 폴더</button>
			<button class="btn-new" onclick={openCreate}>+ 신규</button>
		</div>
		<input
			class="search-input"
			type="search"
			placeholder="제목/본문 검색"
			bind:value={searchQuery}
		/>
		<div class="crumbs">
			<button class="crumb" onclick={() => gotoFolder('')}>도서관</button>
			{#each explorerPath ? explorerPath.split('/') : [] as _seg, i (i)}
				{@const partial = explorerPath.split('/').slice(0, i + 1).join('/')}
				<span class="crumb-sep">›</span>
				<button class="crumb" onclick={() => gotoFolder(partial)}>{_seg}</button>
			{/each}
			{#if explorerPath}
				<button class="btn-del-folder" onclick={() => askDeleteFolder(explorerPath)}>
					현재 폴더 삭제
				</button>
			{/if}
		</div>

		{#if creatingFolder}
			<div class="modal-inline">
				<input
					class="text-input"
					type="text"
					placeholder="새 폴더 이름"
					bind:value={createFolderPath}
					onkeydown={(e) => e.key === 'Enter' && submitCreateFolder()}
				/>
				{#if createFolderError}<p class="err">{createFolderError}</p>{/if}
				<div class="actions">
					<button class="btn-save" onclick={submitCreateFolder}>생성</button>
					<button class="btn-cancel" onclick={cancelCreateFolder}>취소</button>
				</div>
			</div>
		{/if}
		{#if creating}
			<div class="modal-inline">
				<input
					class="text-input"
					type="text"
					placeholder="문서 제목"
					bind:value={createTitle}
					onkeydown={(e) => e.key === 'Enter' && submitCreate()}
				/>
				{#if createError}<p class="err">{createError}</p>{/if}
				<div class="actions">
					<button class="btn-save" onclick={submitCreate}>생성</button>
					<button class="btn-cancel" onclick={cancelCreate}>취소</button>
				</div>
			</div>
		{/if}

		{#if searchResults}
			<!-- DEV-238: 검색 중엔 현재 폴더 위치 무시 — 매칭 문서만 평평하게. -->
			{#if searchResults.length === 0}
				<p class="empty-list">검색 결과 없음.</p>
			{:else}
				<div class="tile-grid">
					{#each searchResults as b (b.book_id)}
						<button class="tile" onclick={() => select(b.book_id)}>
							<span class="tile-icon" aria-hidden="true">📄</span>
							<span class="tile-label">{b.title}</span>
							<span class="tile-sub">{b.book_id}</span>
						</button>
					{/each}
				</div>
			{/if}
		{:else if explorerFolders.length === 0 && explorerDocs.length === 0}
			<p class="empty-list">비어 있음. "+ 신규" 또는 "+ 폴더" 로 만들기.</p>
		{:else}
			<div class="tile-grid">
				{#each explorerFolders as f (f.path)}
					<button class="tile" onclick={() => gotoFolder(f.path)}>
						<span class="tile-icon" aria-hidden="true">📁</span>
						<span class="tile-label">{f.name}</span>
					</button>
				{/each}
				{#each explorerDocs as b (b.book_id)}
					<button class="tile" onclick={() => select(b.book_id)}>
						<span class="tile-icon" aria-hidden="true">📄</span>
						<span class="tile-label">{b.title}</span>
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
						<h2>도서관</h2>
						<div class="view-toggle">
							<button class:on={isMode('tree')} onclick={() => setViewMode('tree')} title="트리 보기">☰</button>
							<button class:on={isMode('explorer')} onclick={() => setViewMode('explorer')} title="아이콘 보기">▦</button>
						</div>
					</div>
					<div class="sidebar-actions">
						<button class="btn-new" onclick={openCreateFolder} title="새 폴더">+ 폴더</button>
						<button class="btn-new" onclick={openCreate} title="신규 문서">+ 신규</button>
					</div>
					<input
						class="search-input"
						type="search"
						placeholder="제목/본문 검색"
						bind:value={searchQuery}
					/>
					{#if searchResults}
						<!-- DEV-238: 검색 중엔 폴더 구조 무시 — 매칭 문서만 평평하게. -->
						{#if searchResults.length === 0}
							<p class="empty-list">검색 결과 없음.</p>
						{:else}
							<div class="book-list">
								{#each searchResults as b (b.book_id)}
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
						<p class="empty-list">문서 없음. "+ 신규" 로 만들기.</p>
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
								/>
							{/each}
							{#each tree.rootDocs as b (b.book_id)}
								<button
									class="book-item"
									class:active={b.book_id === selectedId}
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
								placeholder="새 폴더 경로 (예: 아키텍처/서브)"
								bind:value={createFolderPath}
								onkeydown={(e) => e.key === 'Enter' && submitCreateFolder()}
							/>
							{#if createFolderError}<p class="err">{createFolderError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitCreateFolder}>생성</button>
								<button class="btn-cancel" onclick={cancelCreateFolder}>취소</button>
							</div>
						</div>
					{/if}
					{#if creating}
						<div class="modal-inline">
							<input
								class="text-input"
								type="text"
								placeholder="문서 제목"
								bind:value={createTitle}
								onkeydown={(e) => e.key === 'Enter' && submitCreate()}
							/>
							<select class="text-input" bind:value={createPath}>
								<option value="">(최상위)</option>
								{#each flattenFolderPaths(tree) as p (p)}
									<option value={p}>{p}</option>
								{/each}
							</select>
							{#if createError}<p class="err">{createError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitCreate}>생성</button>
								<button class="btn-cancel" onclick={cancelCreate}>취소</button>
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
							"+ 신규" 로 첫 문서를 만드세요.
						{:else}
							좌측에서 문서를 선택하세요.
						{/if}
					</div>
				{:else}
					<div class="top-bar">
						{#if viewMode === 'explorer'}
							<button class="btn-edit" onclick={explorerBack}>← 목록</button>
						{/if}
						<div class="view-toggle inline">
							<button class:on={viewMode === 'tree'} onclick={() => setViewMode('tree')} title="트리 보기">☰</button>
							<button class:on={viewMode === 'explorer'} onclick={() => setViewMode('explorer')} title="아이콘 보기">▦</button>
						</div>
						<h1 class="doc-title">
							<span class="doc-id">{selected.book_id}</span>
							{selected.title}
						</h1>
						{#if !editMode}
							<div class="top-actions">
								<button class="btn-edit" onclick={enterEdit}>
									{selected.body.trim() ? '✎ 편집' : '+ 작성'}
								</button>
								<button class="btn-edit" onclick={openRetitle}>제목 변경</button>
								<button class="btn-edit" onclick={openMove}>폴더 이동</button>
								<button class="btn-edit danger" onclick={askDeleteSelected}>삭제</button>
							</div>
						{/if}
					</div>

					{#if selected.path}
						<p class="doc-path">📁 {selected.path}</p>
					{/if}

					{#if retitling}
						<div class="modal-inline">
							<input
								class="text-input"
								type="text"
								placeholder="새 제목"
								bind:value={retitleText}
								onkeydown={(e) => e.key === 'Enter' && submitRetitle()}
							/>
							{#if retitleError}<p class="err">{retitleError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitRetitle}>변경</button>
								<button class="btn-cancel" onclick={cancelRetitle}>취소</button>
							</div>
						</div>
					{/if}

					{#if moving}
						<div class="modal-inline">
							<select class="text-input" bind:value={movePath}>
								<option value="">(최상위)</option>
								{#each flattenFolderPaths(tree) as p (p)}
									<option value={p}>{p}</option>
								{/each}
							</select>
							{#if moveError}<p class="err">{moveError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitMove}>이동</button>
								<button class="btn-cancel" onclick={cancelMove}>취소</button>
							</div>
						</div>
					{/if}

					{#if editMode}
						<div class="edit-form">
							<div class="field-label">
								<span>본문 (Markdown) — 첨부는 드래그&드랍 / Ctrl+V</span>
								<div class="editor-wrap" bind:this={editorContainer}></div>
							</div>
							<div class="actions">
								<button class="btn-save" onclick={save} disabled={saving}>
									{saving ? '저장…' : '저장'}
								</button>
								<button class="btn-cancel" onclick={cancelEdit} disabled={saving}> 취소 </button>
							</div>
							{#if saveError}<p class="err">{saveError}</p>{/if}
						</div>
					{:else if selected.body.trim()}
						<MarkdownView source={selected.body} />
					{:else}
						<div class="empty">
							아직 작성된 본문이 없습니다.
							<button class="link" onclick={enterEdit}>지금 작성</button>
						</div>
					{/if}

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
	title="문서 삭제"
	message={`'${confirmDeleteId ?? ''}' 문서를 삭제할까요? (번호는 재사용되지 않습니다)`}
	confirmLabel="삭제"
	danger
	onconfirm={deleteSelected}
	oncancel={() => (confirmDeleteId = null)}
/>

<!-- BUG-123: 폴더 삭제 확인도 네이티브 confirm() 대신 인앱 다이얼로그. -->
<ConfirmDialog
	open={confirmDeleteFolderPath !== null}
	title="폴더 삭제"
	message={`폴더 '${confirmDeleteFolderPath ?? ''}' 를 삭제할까요? (비어 있어야 삭제 가능)`}
	confirmLabel="삭제"
	danger
	onconfirm={deleteFolder}
	oncancel={() => (confirmDeleteFolderPath = null)}
/>

<ConfirmDialog
	open={confirmDiscardId !== null || pendingBackToGrid}
	title="편집중 이동"
	message="편집 중인 변경 사항이 있습니다. 버리고 이동할까요?"
	confirmLabel="버리고 이동"
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
	/* top-bar 안에서는 auto-margin 이 제목 위치를 어긋나게 함 — 그냥 나열. */
	.view-toggle.inline {
		margin-left: 0;
		flex: none;
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
	.tile-icon {
		font-size: 2rem;
	}
	.tile-label {
		font-size: 0.75rem;
		text-align: center;
		line-height: 1.3;
		word-break: break-word;
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
