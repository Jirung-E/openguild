<!--
  DEV-016 (multi-file): 길드 규칙 페이지 — 다중 파일 (`.guild/rules/{slug}.md`).

  레이아웃:
  - 좌측 sidebar: rule slug 목록 + 신규 / 삭제 / 이름변경 버튼.
  - 우측 panel: 선택된 rule 의 markdown view + CodeMirror 편집.

  legacy 단일 `.guild/rules.md` 은 첫 list 호출 시 자동으로
  `.guild/rules/general.md` 로 마이그레이션됨.
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	// BUG-104: 선택 규칙을 URL(?slug=) 에 반영 — 규칙간 링크 이동 + 뒤로가기 복원.
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	// DEV-153: 편집 중이면 이탈 가드에 보고 (라우트 이탈용. 같은 페이지 내 규칙
	// 전환 경고는 아래 confirmDiscardSlug 모달이 별도로 담당).
	import { setUnsaved } from '$lib/stores/unsaved';
	import { saveShortcut } from '$lib/utils/save-shortcut';
	import { rulesApi, type RuleEntry } from '$lib/api/rules';
	// DEV-205 모듈5: 규칙 페이지 i18n.
	import { locale, t } from '$lib/stores/locale';
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	import SidecarHistory from '$lib/components/SidecarHistory.svelte';
	import BacklinkSection from '$lib/components/BacklinkSection.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	// DEV-203: 편집기 셋업(테마/들여쓰기/첨부/자동완성/높이 영속)은 공통
	// MarkdownEditor 컴포넌트로 단일화.
	import MarkdownEditor from '$lib/components/MarkdownEditor.svelte';
	// DEV-173 후속: 규칙 생성/삭제/이름변경/저장(제목 변경) 시 cross-link 인덱스
	// 재적재 — 안 하면 방금 만든 규칙이 [[링크]] 에서 미존재(빨강)로 남음.
	import { loadQuestIndex } from '$lib/stores/questIndex';
	// DEV-243: 태그 — quest 와 동일한 정의(색/설명) registry 공유.
	import TagPills from '$lib/components/TagPills.svelte';
	import { adminApi } from '$lib/api/admin';
	import type { QuestTagDef } from '$lib/types';
	import { showToast } from '$lib/stores/toast';
	// DEV-182: 생성/변경 시각 표시 — quest 상세와 동일 포맷 유틸.
	import { formatTs, formatRelative } from '$lib/utils/datetime';
	import { filterRules } from '$lib/utils/rule-filter';
	// REQ-014: 발췌에서 걸린 부분 표시.
	import { highlightSegments } from '$lib/utils/highlight';

	let loading = $state(true);
	let error = $state<string | null>(null);
	let entries = $state<RuleEntry[]>([]);
	let selectedSlug = $state<string | null>(null);
	let selectedContent = $state<string | null>(null);

	let editMode = $state(false);
	// DEV-153: 편집 중이면 이탈 가드에 보고.
	$effect(() => setUnsaved('rules-edit', editMode));
	onDestroy(() => setUnsaved('rules-edit', false));
	let editText = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	// 신규 / 이름변경 모달.
	let creating = $state(false);
	let createSlug = $state('');
	let createError = $state<string | null>(null);

	let renaming = $state(false);
	let renameSlug = $state('');
	let renameError = $state<string | null>(null);

	async function loadList(preferSlug?: string | null, mutated = false) {
		loading = true;
		error = null;
		try {
			const res = await rulesApi.list();
			entries = res.entries ?? [];
			// 선택 유지 / 자동 선택.
			if (preferSlug && entries.some((e) => e.slug === preferSlug)) {
				selectedSlug = preferSlug;
			} else if (selectedSlug == null || !entries.some((e) => e.slug === selectedSlug)) {
				selectedSlug = entries[0]?.slug ?? null;
			}
			refreshSelectedContent();
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

	function refreshSelectedContent() {
		if (!selectedSlug) {
			selectedContent = null;
			return;
		}
		const e = entries.find((x) => x.slug === selectedSlug);
		selectedContent = e ? e.content : null;
	}

	// DEV-243: 태그 — entries 안의 RuleEntry.tags 를 그대로 파생.
	let tagDefs = $state<QuestTagDef[]>([]);
	onMount(async () => {
		tagDefs = await adminApi.listTagDefs().catch(() => [] as QuestTagDef[]);
	});
	const selectedTags = $derived(entries.find((e) => e.slug === selectedSlug)?.tags ?? []);
	// DEV-182: 생성/변경 시각.
	const selectedEntry = $derived(entries.find((e) => e.slug === selectedSlug) ?? null);
	async function setRuleTags(tags: string[]) {
		if (!selectedSlug) return;
		try {
			const updated = await rulesApi.setTags(selectedSlug, tags);
			entries = entries.map((e) => (e.slug === selectedSlug ? { ...e, tags: updated.tags } : e));
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('rules.tagSaveFailed', $locale), 'error');
		}
	}

	// DEV-243 후속(admin 지적): 태그는 달 수 있는데 태그로 찾을 방법이 없었음.
	// quest 의 DEV-068 tag 필터(AND, chip 클릭 토글)와 동일 패턴.
	// REQ-013: 규칙 검색 — 목록 응답이 본문을 싣고 오므로 서버 왕복 없이 즉시.
	let searchQuery = $state('');
	let filterTags = $state(new Set<string>());
	const allTagOptions = $derived.by(() => {
		const set = new Set<string>();
		for (const e of entries) for (const t of e.tags ?? []) set.add(t);
		return Array.from(set).sort();
	});
	const tagCounts = $derived.by(() => {
		const m = new Map<string, number>();
		for (const e of entries) for (const t of e.tags ?? []) m.set(t, (m.get(t) ?? 0) + 1);
		return m;
	});
	function toggleTagFilter(t: string) {
		const next = new Set(filterTags);
		if (next.has(t)) next.delete(t);
		else next.add(t);
		filterTags = next;
	}
	// REQ-013: 태그 AND 필터 + 검색어(slug/태그/본문)를 한 곳에서. 로직은
	// 단위 테스트가 붙은 utils 에 있다.
	const searchResults = $derived(filterRules(entries, searchQuery, filterTags));
	const filteredEntries = $derived(searchResults.map((r) => r.entry));
	/** slug → 매치 정보. 본문에서만 맞았을 때 이유를 보여주려고. */
	const matchInfo = $derived(new Map(searchResults.map((r) => [r.entry.slug, r])));

	onMount(() => {
		// DEV-173: cross-link 딥링크 — `/rules?slug=xxx` 로 진입 시 해당 규칙 선택.
		const slugParam = new URLSearchParams(window.location.search).get('slug');
		loadList(slugParam).then(() => {
			// BUG-126(admin 보고): slug 없이 진입(예: nav 의 "Rules" 링크)하면
			// loadList 가 entries[0] 을 자동 선택하지만 URL 엔 반영 안 됐음 —
			// 그 결과 "이 진입 지점" 자체가 history 상 "선택 없음" 으로 남아,
			// 여러 규칙을 거쳐 뒤로가기하면 이 지점에서 (원래 selectedSlug 였던)
			// 첫 규칙으로 안 돌아오고 직전 선택에 멈춰있는 것처럼 보였다 —
			// $effect 가 slug 없는 URL 은 무시하도록 짜여 있었기 때문(아래).
			// 진입 즉시 실제 선택을 URL 에 replaceState 로 명시해 이 지점부터
			// 항상 URL == 상태가 성립하게 만든다.
			if (selectedSlug && selectedSlug !== slugParam) {
				const cur = new URLSearchParams(window.location.search);
				cur.set('slug', selectedSlug);
				goto(`/rules?${cur.toString()}`, {
					replaceState: true,
					keepFocus: true,
					noScroll: true
				});
			}
		});
	});

	// BUG-104: URL(?slug=) 을 선택 상태의 진리원으로.
	//
	// (1) 규칙 본문의 다른 규칙 [[링크]] 클릭 — 같은 라우트 내 이동이라 onMount 가
	//     재실행되지 않아 선택이 안 바뀌던 문제: $page.url 반응형 구독으로 해결.
	// (2) 퀘스트/캠페인 링크를 따라갔다 뒤로가기 — 선택이 컴포넌트 state 뿐이라
	//     첫 규칙으로 초기화되던 문제: 선택 시 URL 에 기록해 해결
	//     (복귀 시 history 의 ?slug= 를 onMount 가 읽음).
	// BUG-126 후속: slug 가 사라진 경우(뒤로가기로 slug 없는 최초 진입 지점에
	// 복귀 / nav 의 "Rules" 링크를 같은 라우트에서 다시 클릭)도 첫 규칙으로
	// 리셋 — 이전엔 무시해서 selectedSlug 가 직전 값에 멈춰있었다.
	$effect(() => {
		const slug = $page.url.searchParams.get('slug');
		if (slug && slug !== selectedSlug && entries.some((e) => e.slug === slug)) {
			select(slug);
		} else if (!slug && entries.length > 0 && selectedSlug !== entries[0].slug) {
			selectedSlug = entries[0].slug;
			refreshSelectedContent();
		}
	});

	function syncUrl(slug: string) {
		const cur = new URLSearchParams(window.location.search).get('slug');
		if (cur === slug) return;
		// BUG-120: pushState(기본) — 규칙 선택마다 새 history entry 를 쌓아
		// 브라우저 뒤로가기로 이전에 본 규칙을 한 단계씩 되짚어갈 수 있게 한다.
		goto(`/rules?slug=${encodeURIComponent(slug)}`, {
			keepFocus: true,
			noScroll: true
		});
	}

	// DEV-119: 편집중 다른 slug 선택 시 native confirm 대신 인앱 모달.
	let confirmDiscardSlug = $state<string | null>(null);

	function select(slug: string) {
		if (editMode) {
			confirmDiscardSlug = slug;
			return;
		}
		selectedSlug = slug;
		refreshSelectedContent();
		syncUrl(slug);
	}

	function applyPendingSelect() {
		const slug = confirmDiscardSlug;
		confirmDiscardSlug = null;
		if (!slug) return;
		cancelEdit();
		selectedSlug = slug;
		refreshSelectedContent();
		syncUrl(slug); // BUG-104
	}

	// ─── 편집 ───
	// DEV-203: 편집기 생성/파괴는 MarkdownEditor 컴포넌트가 {#if editMode}
	// 수명주기로 자동 처리 — initEditor/destroy 보일러플레이트 제거.
	function enterEdit() {
		if (!selectedSlug) return;
		editText = selectedContent ?? '';
		editMode = true;
		saveError = null;
	}

	function cancelEdit() {
		editMode = false;
		saveError = null;
	}

	async function save(keepEditing = false) {
		if (!selectedSlug || saving) return;
		saving = true;
		saveError = null;
		try {
			const text = editText;
			const res = await rulesApi.set(selectedSlug, text);
			selectedContent = res.content;
			// 메모리 목록도 갱신 — 페이지 reload 안 해도 sidebar 정합.
			entries = entries.map((e) => (e.slug === selectedSlug ? { ...e, content: text } : e));
			// DEV-173 후속: 제목(첫 # 헤딩)이 바뀌었을 수 있음 — 인덱스 재적재.
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
		createSlug = '';
		createError = null;
	}
	function cancelCreate() {
		creating = false;
		createError = null;
	}
	async function submitCreate() {
		const slug = createSlug.trim();
		if (!slug) {
			createError = t('rules.slugRequired', $locale);
			return;
		}
		try {
			await rulesApi.create(slug, '');
			creating = false;
			await loadList(slug, true); // 새로 만든 것 자동 선택.
		} catch (e) {
			createError = e instanceof Error ? e.message : t('rules.createFailed', $locale);
		}
	}

	// ─── 삭제 (DEV-118: 인앱 확인 모달) ───
	let confirmDeleteSlug = $state<string | null>(null);
	function askDeleteSelected() {
		if (!selectedSlug) return;
		confirmDeleteSlug = selectedSlug;
	}
	async function deleteSelected() {
		const target = confirmDeleteSlug;
		confirmDeleteSlug = null;
		if (!target) return;
		try {
			await rulesApi.delete(target);
			if (selectedSlug === target) {
				selectedSlug = null;
				selectedContent = null;
			}
			await loadList(null, true);
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('rules.deleteFailed', $locale), 'error');
		}
	}

	// ─── 이름변경 ───
	function openRename() {
		if (!selectedSlug) return;
		renaming = true;
		renameSlug = selectedSlug;
		renameError = null;
	}
	function cancelRename() {
		renaming = false;
		renameError = null;
	}
	async function submitRename() {
		if (!selectedSlug) return;
		const newSlug = renameSlug.trim();
		if (!newSlug || newSlug === selectedSlug) {
			cancelRename();
			return;
		}
		try {
			await rulesApi.rename(selectedSlug, newSlug);
			renaming = false;
			await loadList(newSlug, true);
		} catch (e) {
			renameError = e instanceof Error ? e.message : t('rules.renameFailed', $locale);
		}
	}
</script>

<div class="page">
	{#if loading}
		<div class="state">Loading…</div>
	{:else if error}
		<div class="state err">{error}</div>
	{:else}
		<div class="layout">
			<!-- 좌측 sidebar -->
			<aside class="sidebar">
				<div class="sidebar-head">
					<h2>{t('rules.listHeading', $locale)}</h2>
					<button class="btn-new" onclick={openCreate} title={t('rules.newRule', $locale)}
						>{t('rules.newBtn', $locale)}</button
					>
				</div>
				{#if allTagOptions.length > 0}
					<div class="tag-filter-row" aria-label={t('rules.tagFilter', $locale)}>
						{#each allTagOptions as tag (tag)}
							<button
								class="tag-filter-chip"
								class:active={filterTags.has(tag)}
								onclick={() => toggleTagFilter(tag)}
								title={filterTags.has(tag)
									? `${tag}${t('questList.filterRemoveSuffix', $locale)}`
									: `${tag}${t('questList.filterAddSuffix', $locale)}`}
							>
								{tag}
								<span class="tag-chip-count">{tagCounts.get(tag) ?? 0}</span>
							</button>
						{/each}
						{#if filterTags.size > 0}
							<button
								class="tag-clear"
								onclick={() => (filterTags = new Set())}
								title={t('rules.clearTagFilters', $locale)}
							>
								{t('questList.clearAllBtn', $locale)}
							</button>
						{/if}
					</div>
				{/if}
				<!-- REQ-013: slug 뿐 아니라 본문까지 — 규칙이 늘어나면 slug 만으로는
				     원하는 문서를 못 찾는다. 본문은 이미 목록에 실려 와 즉시 필터된다. -->
				<input
					class="rule-search"
					type="search"
					placeholder={t('rules.searchPlaceholder', $locale)}
					bind:value={searchQuery}
					data-testid="rule-search"
				/>
				{#if entries.length === 0}
					<p class="empty-list">{t('rules.emptyList', $locale)}</p>
				{:else if filteredEntries.length === 0}
					<p class="empty-list">
						{searchQuery.trim()
							? t('rules.emptySearch', $locale)
							: t('rules.emptyFiltered', $locale)}
					</p>
				{:else}
					<ul class="rule-list">
						{#each filteredEntries as e (e.slug)}
							{@const m = matchInfo.get(e.slug)}
							<li>
								<button
									class="rule-item"
									class:active={e.slug === selectedSlug}
									onclick={() => select(e.slug)}
								>
									<span class="rule-slug">{e.slug}</span>
									<!-- slug 에 없는데 나왔다면 왜 나왔는지 보여준다 — 긴 본문에서
									     검색어를 눈으로 못 찾는 상황을 막는다. -->
									{#if m && m.matchedIn.length > 0 && !m.matchedIn.includes('slug')}
										<span class="rule-why">
											{m.matchedIn.includes('body')
												? t('rules.matchedInBody', $locale)
												: t('rules.matchedInTag', $locale)}
										</span>
									{/if}
									{#if m?.excerpt && !m.matchedIn.includes('slug')}
										<span class="rule-excerpt">
											{#each highlightSegments(m.excerpt, searchQuery) as seg, si (si)}
												{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}
											{/each}
										</span>
									{/if}
								</button>
							</li>
						{/each}
					</ul>
				{/if}

				{#if creating}
					<div class="modal-inline">
						<input
							class="text-input"
							type="text"
							placeholder={t('rules.slugPlaceholder', $locale)}
							bind:value={createSlug}
							onkeydown={(e) => e.key === 'Enter' && submitCreate()}
						/>
						{#if createError}<p class="err">{createError}</p>{/if}
						<div class="actions">
							<button class="btn-save" onclick={submitCreate}>{t('rules.create', $locale)}</button>
							<button class="btn-cancel" onclick={cancelCreate}
								>{t('common.cancel', $locale)}</button
							>
						</div>
					</div>
				{/if}
			</aside>

			<!-- 우측 panel -->
			<section class="panel">
				{#if !selectedSlug}
					<div class="empty">
						{#if entries.length === 0}
							{t('rules.emptyCreateFirst', $locale)}
						{:else}
							{t('rules.emptySelect', $locale)}
						{/if}
					</div>
				{:else}
					<div class="top-bar">
						<h1 class="slug-title"># {selectedSlug}</h1>
						{#if !editMode}
							<div class="top-actions">
								<button class="btn-edit" onclick={enterEdit}>
									{selectedContent && selectedContent.trim()
										? `✎ ${t('detail.edit', $locale)}`
										: t('rules.writeBtn', $locale)}
								</button>
								<button class="btn-edit" onclick={openRename}>{t('rules.rename', $locale)}</button>
								<button class="btn-edit danger" onclick={askDeleteSelected}
									>{t('detail.delete', $locale)}</button
								>
							</div>
						{/if}
					</div>

					<!-- DEV-182: 생성 / 변경 시각. -->
					{#if selectedEntry}
						<div class="meta-times">
							<span class="meta-item">
								<span class="meta-label">{t('common.created', $locale)}</span>
								<time
									class="meta-val"
									datetime={selectedEntry.created_at}
									title={formatTs(selectedEntry.created_at)}
								>
									{formatTs(selectedEntry.created_at)}
								</time>
							</span>
							<span class="meta-sep">·</span>
							<span class="meta-item">
								<span class="meta-label">{t('common.updated', $locale)}</span>
								<time
									class="meta-val"
									datetime={selectedEntry.updated_at}
									title={formatTs(selectedEntry.updated_at)}
								>
									{formatRelative(selectedEntry.updated_at, undefined, $locale)}
								</time>
							</span>
						</div>
					{/if}

					{#if renaming}
						<div class="modal-inline">
							<input
								class="text-input"
								type="text"
								placeholder={t('rules.newSlugPlaceholder', $locale)}
								bind:value={renameSlug}
								onkeydown={(e) => e.key === 'Enter' && submitRename()}
							/>
							{#if renameError}<p class="err">{renameError}</p>{/if}
							<div class="actions">
								<button class="btn-save" onclick={submitRename}
									>{t('common.change', $locale)}</button
								>
								<button class="btn-cancel" onclick={cancelRename}
									>{t('common.cancel', $locale)}</button
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
								<span>{t('rules.bodyLabel', $locale)}</span>
								<MarkdownEditor
									bind:value={editText}
									mediaOnly
									onError={(msg) =>
										(saveError = `${t('rules.attachUploadFailed', $locale)}: ${msg}`)}
								/>
							</div>
							<div class="actions">
								<button class="btn-save" onclick={() => save()} disabled={saving}>
									{saving ? t('common.saving', $locale) : t('common.save', $locale)}
								</button>
								<button class="btn-cancel" onclick={cancelEdit} disabled={saving}>
									{t('common.cancel', $locale)}
								</button>
							</div>
							{#if saveError}<p class="err">{saveError}</p>{/if}
						</div>
					{:else if selectedContent && selectedContent.trim()}
						<MarkdownView source={selectedContent} />
					{:else}
						<div class="empty">
							{t('rules.noBodyYet', $locale)}
							<button class="link" onclick={enterEdit}>{t('rules.writeNow', $locale)}</button>
						</div>
					{/if}
					{#if !editMode}
						<!-- DEV-243: 태그. -->
						<TagPills tags={selectedTags} {tagDefs} onSetTags={setRuleTags} />
						<!-- DEV-290: 규칙 변경 이력. -->
						{#if selectedSlug}
							<!-- REQ-008: 이 문서를 참조하는 문서. -->
							<BacklinkSection kind="rule" id={selectedSlug} />
							<SidecarHistory kind="rule" id={selectedSlug} />
						{/if}
					{/if}
				{/if}
			</section>
		</div>
	{/if}
</div>

<!-- DEV-118: 규칙 삭제 확인 모달. -->
<ConfirmDialog
	open={confirmDeleteSlug !== null}
	title={t('rules.deleteTitle', $locale)}
	message={`${t('rules.deleteMsgPre', $locale)}${confirmDeleteSlug ?? ''}${t('rules.deleteMsgPost', $locale)}`}
	confirmLabel={t('detail.delete', $locale)}
	danger
	onconfirm={deleteSelected}
	oncancel={() => (confirmDeleteSlug = null)}
/>

<!-- DEV-119: 편집중 다른 slug 선택 시 미저장 확인 모달. -->
<ConfirmDialog
	open={confirmDiscardSlug !== null}
	title={t('rules.discardEditTitle', $locale)}
	message={t('rules.discardEditMsg', $locale)}
	confirmLabel={t('rules.discardAndLeave', $locale)}
	danger
	onconfirm={applyPendingSelect}
	oncancel={() => (confirmDiscardSlug = null)}
/>

<style>
	.page {
		padding: 1.25rem 1.5rem 2rem;
		max-width: var(--content-max-width, 1200px);
		margin: 0 auto;
	}
	.layout {
		display: grid;
		grid-template-columns: 240px 1fr;
		gap: 1.25rem;
		min-height: 70vh;
	}
	.sidebar {
		border-right: 1px solid var(--bg-subtle);
		padding-right: 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	/* BUG-200 후속 감사: 이 미디어 블록이 기본 `.sidebar` 규칙보다 **앞**에 있어
	   border-right / padding-right override 가 순서에서 지고 있었다(좁은 화면에서도
	   우측 테두리·여백이 남음). 기본 규칙 뒤로 옮긴다.
	   DEV-257(사용자 보고): 도서관과 동일 — 좁은 화면에서 240px 고정 열이
	   화면 대부분을 먹고 컨텐츠가 화면 밖으로 밀려 가로 스크롤/메뉴바 잘림이
	   생겼다. 한 열로 쌓고 sidebar 는 자체 스크롤로 높이 제한. */
	@media (max-width: 640px) {
		.layout {
			grid-template-columns: 1fr;
		}
		.sidebar {
			border-right: none;
			border-bottom: 1px solid var(--bg-subtle);
			padding-right: 0;
			padding-bottom: 0.75rem;
			max-height: 40vh;
			overflow-y: auto;
		}
	}
	.sidebar-head {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
	.sidebar-head h2 {
		font-size: 0.8rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-muted);
		margin: 0;
	}
	.btn-new {
		margin-left: auto;
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
	.rule-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
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
	.rule-item {
		width: 100%;
		text-align: left;
		padding: 0.35rem 0.5rem;
		background: transparent;
		border: none;
		border-radius: 4px;
		color: var(--text);
		font-size: 0.85rem;
		cursor: pointer;
	}
	.rule-item:hover {
		background: var(--bg-elevated);
	}
	/* REQ-013: 검색 입력 — 도서관(.search-input)과 같은 치수·색. */
	.rule-search {
		width: 100%;
		padding: 0.3rem 0.55rem;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 4px;
		font-size: 0.82rem;
		margin: 0.4rem 0;
		box-sizing: border-box;
	}
	.rule-slug {
		display: block;
	}
	/* slug 가 아닌 곳에서 맞았을 때만 보이는 이유 표시. */
	.rule-why {
		display: inline-block;
		margin-top: 0.15rem;
		padding: 0 0.3rem;
		border-radius: 3px;
		background: color-mix(in srgb, var(--accent) 14%, transparent);
		color: var(--accent-secondary);
		font-size: 0.62rem;
	}
	.rule-excerpt mark {
		/* REQ-014: 검색 일치 표시. 기본 mark 의 노란 배경은 다크 테마에서 튀므로
		   테마별 토큰을 쓴다(global.css 의 --search-hit-*). */
		background: var(--search-hit-bg);
		color: var(--search-hit-text);
		border-radius: 2px;
		padding: 0 1px;
	}
	.rule-excerpt {
		/* 발췌는 두 줄까지 — 목록 행이 길어지면 훑기가 나빠진다. */
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
		margin-top: 0.1rem;
		color: var(--text-muted);
		font-size: 0.68rem;
		line-height: 1.35;
	}
	.rule-item.active {
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent-secondary);
	}

	.panel {
		min-width: 0;
	}

	.top-bar {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}
	.slug-title {
		font-size: 1.1rem;
		font-weight: 600;
		color: var(--text);
		margin: 0;
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
