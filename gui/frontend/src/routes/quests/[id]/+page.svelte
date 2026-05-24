<script lang="ts">
	import { page } from '$app/stores';
	import { onMount, tick } from 'svelte';
	import { goto } from '$app/navigation';
	import { questsApi } from '$lib/api/quests';
	import { metaApi } from '$lib/api/meta';
	import { marked } from 'marked';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	import { oneDark } from '@codemirror/theme-one-dark';
	import {
		URGENCY_COLOR,
		URGENCY_LABEL,
		type CandidateRelation,
		type Quest,
		type QuestDetail,
		type QuestStatus,
		type QuestType
	} from '$lib/types';
	import NewQuestModal from '$lib/components/NewQuestModal.svelte';
	import QuestCombobox from '$lib/components/QuestCombobox.svelte';
	import QuestHistory from '$lib/components/QuestHistory.svelte';
	import { formatTs, formatRelative } from '$lib/utils/datetime';

	let slug = $derived($page.params.id ?? '');
	let detail = $state<QuestDetail | null>(null);
	// DEV-055: types 도 노출 — type 변경 UI 에서 사용.
	let types = $state<QuestType[]>([]);
	let statuses = $state<QuestStatus[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// 편집 모드
	let editMode = $state(false);
	let editTitle = $state('');
	let editUrgency = $state(3);
	let editDescription = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	// 상태 변경 피드백
	let statusFlashId = $state<number | null>(null); // 방금 변경된 상태 버튼 (체크 아이콘)
	let badgePulse = $state(0); // 헤더 상태 뱃지 펄스 트리거 (값이 바뀌면 한 번 펄스)
	let changingStatus = $state(false);

	// DEV-055: type 변경.
	let confirmTypeChange = $state<QuestType | null>(null);
	let changingType = $state(false);

	// 변경이력 — 상태 변경 후 새로고침 트리거 (DEV-038).
	let historyVersion = $state(0);

	// 콤보박스 / 후보
	type ComboMode = 'sub' | 'prereq';
	let comboMode = $state<ComboMode | null>(null);
	let candidates = $state<Quest[]>([]);
	let candidatesLoading = $state(false);
	let comboError = $state<string | null>(null);

	// 서브퀘스트 신규 생성 모달
	let showNewSubQuest = $state(false);

	// 삭제 모달
	let deleteModal = $state(false);
	let deleting = $state(false);
	let cascadeSet = $state<Set<number>>(new Set());

	// CodeMirror
	let editorContainer: HTMLDivElement | undefined = $state(undefined);
	let editorView: EditorView | null = null;

	let sortedStatuses = $derived([...statuses].sort((a, b) => a.sort_order - b.sort_order));

	// 메타(타입/상태)는 마운트 시 한 번만
	onMount(async () => {
		try {
			const [t, s] = await Promise.all([metaApi.getQuestTypes(), metaApi.getQuestStatuses()]);
			types = t;
			statuses = s;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		}
	});

	// slug 가 바뀌면(다른 quest 페이지로 navigate) detail 을 다시 로드.
	// SvelteKit 은 같은 라우트 안에서 컴포넌트를 재사용하므로 onMount 만으론 부족.
	$effect(() => {
		const currentSlug = slug;
		if (!currentSlug) return;
		// 편집/모달 상태는 페이지 변경 시 모두 닫는다 (이전 quest 의 상태가 새 quest 에 누수되지 않도록)
		editMode = false;
		comboMode = null;
		showNewSubQuest = false;
		deleteModal = false;
		loading = true;
		error = null;
		questsApi
			.getBySlug(currentSlug)
			.then((d) => {
				// 효과 실행 도중 다시 slug 가 바뀌었을 수 있으므로 ID 비교 후 적용
				if (slug === currentSlug) detail = d;
			})
			.catch((e) => {
				if (slug === currentSlug)
					error = e instanceof Error ? e.message : 'failed to load';
			})
			.finally(() => {
				if (slug === currentSlug) loading = false;
			});
	});

	// --- 편집 모드 ---

	async function enterEditMode() {
		if (!detail) return;
		editTitle = detail.title;
		editUrgency = detail.urgency;
		editDescription = detail.description ?? '';
		editMode = true;
		await tick();
		initEditor();
	}

	// DEV-057: 편집창 사용자 크기 영속화.
	const EDITOR_HEIGHT_KEY = 'openguild.questEditorHeight';
	let editorHeightSaveTimer: ReturnType<typeof setTimeout> | null = null;
	function loadEditorHeight(): number {
		try {
			const raw = localStorage.getItem(EDITOR_HEIGHT_KEY);
			const n = raw ? parseInt(raw, 10) : NaN;
			// 합리적 범위만 — 200~2000px.
			if (Number.isFinite(n) && n >= 200 && n <= 2000) return n;
		} catch {
			/* 무시 */
		}
		return 480;
	}
	function scheduleEditorHeightSave(px: number) {
		if (editorHeightSaveTimer) clearTimeout(editorHeightSaveTimer);
		editorHeightSaveTimer = setTimeout(() => {
			try {
				localStorage.setItem(EDITOR_HEIGHT_KEY, String(Math.round(px)));
			} catch {
				/* 무시 */
			}
		}, 250);
	}
	let editorResizeObserver: ResizeObserver | null = null;

	function initEditor() {
		if (!editorContainer) return;
		if (editorView) { editorView.destroy(); editorView = null; }
		// DEV-057: parent (.editor-wrap) 가 height 결정. cm-scroller 는 fill.
		// 이전엔 cm-scroller maxHeight 480px 가 고정 한계 — parent resize 시 의미 없음.
		editorContainer.style.height = `${loadEditorHeight()}px`;
		editorView = new EditorView({
			doc: editDescription,
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
		// 사용자가 resize 핸들로 크기 바꿀 때마다 영속화.
		editorResizeObserver?.disconnect();
		editorResizeObserver = new ResizeObserver((entries) => {
			for (const entry of entries) {
				scheduleEditorHeightSave(entry.contentRect.height);
			}
		});
		editorResizeObserver.observe(editorContainer);
	}

	function exitEditMode() {
		editorView?.destroy();
		editorView = null;
		editorResizeObserver?.disconnect();
		editorResizeObserver = null;
		editMode = false;
		saveError = null;
	}

	async function saveEdit() {
		if (!detail) return;
		saving = true;
		saveError = null;
		try {
			const desc = editorView ? editorView.state.doc.toString() : editDescription;
			await questsApi.update(detail.id, {
				title: editTitle.trim() || detail.title,
				description: desc || undefined,
				urgency: editUrgency
			});
			detail = await questsApi.getBySlug(slug);
			exitEditMode();
		} catch (e) {
			saveError = e instanceof Error ? e.message : 'save failed';
		} finally {
			saving = false;
		}
	}

	// --- 상태 변경 (다이얼로그 없음, 시각 피드백) ---

	// DEV-048: status 변경 API 가 slug 전용. statusId (number) 는 UI feedback 용,
	// statusSlug (string) 은 backend 전송용.
	async function changeStatus(statusId: number, statusSlug: string) {
		if (!detail || statusId === detail.status_id || changingStatus) return;
		changingStatus = true;
		try {
			await questsApi.changeStatus(detail.id, { status_slug: statusSlug });
			detail = await questsApi.getBySlug(slug);
			// 피드백: 버튼 체크 + 헤더 뱃지 펄스
			statusFlashId = statusId;
			badgePulse += 1;
			// 새 history 행을 보이도록 reload 트리거.
			historyVersion += 1;
			setTimeout(() => { if (statusFlashId === statusId) statusFlashId = null; }, 600);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'status change failed');
		} finally {
			changingStatus = false;
		}
	}

	// --- DEV-055: type 변경 ---
	//
	// slug 가 바뀌어서 URL 도 새 slug 로 navigate. 다른 quest 본문의 mention 은
	// 자동 갱신 안 됨 (사용자 책임) — confirm 모달에서 안내.

	function askChangeType(t: QuestType) {
		if (!detail || t.id === detail.quest_type_id) return;
		confirmTypeChange = t;
	}

	async function doChangeType() {
		const target = confirmTypeChange;
		if (!detail || !target || changingType) return;
		confirmTypeChange = null;
		changingType = true;
		try {
			const updated = await questsApi.changeType(detail.id, {
				new_type_prefix: target.prefix
			});
			// slug 바뀜 → 새 slug 의 URL 로 navigate.
			await goto(`/quests/${updated.quest_id}`, { replaceState: true });
		} catch (e) {
			alert(e instanceof Error ? e.message : 'type change failed');
		} finally {
			changingType = false;
		}
	}

	// --- 콤보박스 (sub / prereq 추가) ---

	async function openCombo(mode: ComboMode) {
		if (!detail) return;
		comboError = null;
		comboMode = mode;
		candidatesLoading = true;
		candidates = [];
		try {
			const relation: CandidateRelation = mode === 'sub' ? 'sub' : 'prereq';
			candidates = await questsApi.candidates(detail.id, relation);
		} catch (e) {
			comboError = e instanceof Error ? e.message : '후보 조회 실패';
		} finally {
			candidatesLoading = false;
		}
	}

	function closeCombo() {
		comboMode = null;
		candidates = [];
		comboError = null;
	}

	async function pickCandidate(questId: number) {
		if (!detail || !comboMode) return;
		const mode = comboMode;
		try {
			if (mode === 'sub') {
				// 기존 퀘스트를 이 퀘스트의 서브로 지정 = 그 퀘스트의 부모를 이 퀘스트로
				await questsApi.changeParent(questId, { parent_quest_id: detail.id });
			} else {
				await questsApi.addPrerequisite(detail.id, questId);
			}
			detail = await questsApi.getBySlug(slug);
			closeCombo();
		} catch (e) {
			comboError = e instanceof Error ? e.message : '추가 실패';
		}
	}

	// --- 서브퀘스트 생성 모달 ---

	async function onSubQuestCreated() {
		// onclose 가 직접 호출돼도 reload 하도록 (oncreated 가 있어도 닫힘)
		showNewSubQuest = false;
		if (detail) detail = await questsApi.getBySlug(slug);
	}

	// --- 서브퀘스트 분리 (× 버튼) ---

	async function detachSubQuest(subId: number) {
		if (!detail) return;
		try {
			await questsApi.changeParent(subId, { parent_quest_id: null });
			detail = await questsApi.getBySlug(slug);
		} catch (e) {
			alert(e instanceof Error ? e.message : '분리 실패');
		}
	}

	async function removePrerequisite(prereqId: number) {
		if (!detail) return;
		try {
			await questsApi.removePrerequisite(detail.id, prereqId);
			detail = await questsApi.getBySlug(slug);
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}

	// --- 삭제 모달 ---

	function openDeleteModal() {
		cascadeSet = new Set();
		deleteModal = true;
	}

	function toggleCascade(id: number) {
		const next = new Set(cascadeSet);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		cascadeSet = next;
	}

	function toggleAllCascade() {
		if (!detail) return;
		// 모두 선택돼있으면 전체 해제, 아니면 전체 선택
		if (cascadeSet.size === detail.sub_quests.length) {
			cascadeSet = new Set();
		} else {
			cascadeSet = new Set(detail.sub_quests.map((s) => s.id));
		}
	}

	async function confirmDelete() {
		if (!detail) return;
		deleting = true;
		try {
			const ids = Array.from(cascadeSet);
			await questsApi.delete(detail.id, ids.length > 0 ? ids : undefined);
			goto('/');
		} catch (e) {
			alert(e instanceof Error ? e.message : '삭제 실패');
			deleting = false;
		}
	}

	function renderMarkdown(src: string): string {
		return marked(src, { async: false }) as string;
	}
</script>

<div class="container">
	<div class="top-bar">
		<a href="/" class="back">← Back</a>
		{#if detail && !editMode}
			<div class="top-actions">
				<button class="btn-edit" onclick={enterEditMode}>✎ Edit</button>
				<button class="btn-delete" onclick={openDeleteModal}>🗑 Delete</button>
			</div>
		{/if}
	</div>

	{#if loading}
		<div class="state-msg">Loading...</div>
	{:else if error}
		<div class="state-msg error">{error}</div>
	{:else if detail}
		<!-- 헤더 뱃지 -->
		<div class="header">
			<span class="badge type" style:--c={detail.type_color}>{detail.quest_id}</span>
			<span class="badge urgency" style:--c={URGENCY_COLOR[detail.urgency]}>
				{URGENCY_LABEL[detail.urgency]}
			</span>
			{#key badgePulse}
				<span class="badge status pulsing" style:--c={detail.status_color}>
					{detail.status_name_en}
				</span>
			{/key}
		</div>

		<!-- 생성 / 변경 시각 -->
		<div class="meta-times">
			<span class="meta-item">
				<span class="meta-label">생성</span>
				<time
					class="meta-val"
					datetime={detail.created_at}
					title={formatTs(detail.created_at)}
					data-testid="created-at"
				>
					{formatTs(detail.created_at)}
				</time>
			</span>
			<span class="meta-sep">·</span>
			<span class="meta-item">
				<span class="meta-label">변경</span>
				<time
					class="meta-val"
					datetime={detail.updated_at}
					title={formatTs(detail.updated_at)}
					data-testid="updated-at"
				>
					{formatRelative(detail.updated_at)}
				</time>
			</span>
		</div>

		{#if editMode}
			<div class="edit-form">
				<label class="field-label">
					<span>제목</span>
					<input class="edit-title" type="text" bind:value={editTitle} />
				</label>

				<label class="field-label">
					<span>긴급도</span>
					<select class="edit-select" bind:value={editUrgency}>
						{#each [1, 2, 3, 4] as u}
							<option value={u}>{URGENCY_LABEL[u]}</option>
						{/each}
					</select>
				</label>

				<!-- CodeMirror 가 div 안에 textarea 를 동적으로 생성 — svelte 가 정적
				     분석으로는 control 미포함으로 판단. ignore. -->
				<!-- svelte-ignore a11y_label_has_associated_control -->
				<label class="field-label">
					<span>설명 (Markdown)</span>
					<div class="editor-wrap" bind:this={editorContainer}></div>
				</label>

				{#if saveError}<p class="save-error">{saveError}</p>{/if}

				<div class="edit-actions">
					<button class="btn-save" onclick={saveEdit} disabled={saving}>
						{saving ? '저장 중…' : '저장'}
					</button>
					<button class="btn-cancel" onclick={exitEditMode} disabled={saving}>취소</button>
				</div>
			</div>
		{:else}
			<h1 class="title">{detail.title}</h1>

			<!-- 권장 브랜치명 -->
			<div class="branch-row">
				<span class="branch-label">Branch</span>
				<code class="branch-name">{detail.type_prefix}-{String(detail.number).padStart(3, '0')}</code>
				<button class="copy-btn" onclick={() => navigator.clipboard.writeText(`${detail!.type_prefix}-${String(detail!.number).padStart(3, '0')}`)}>복사</button>
			</div>

			<!-- 상태 변경 -->
			<div class="status-row">
				<span class="branch-label">상태 변경</span>
				<div class="status-btns">
					{#each sortedStatuses as s}
						<button
							class="status-btn"
							class:active={s.id === detail.status_id}
							class:flash={s.id === statusFlashId}
							style:--c={s.color}
							onclick={() => changeStatus(s.id, s.slug)}
							disabled={changingStatus || s.id === detail.status_id}
							data-testid="status-btn-{s.id}"
						>
							{#if s.id === statusFlashId}✓ {/if}{s.name_en}
						</button>
					{/each}
				</div>
			</div>

			<!-- DEV-055: type 변경 (slug 가 바뀜, confirm 모달 후 진행) -->
			<div class="status-row">
				<span class="branch-label">타입 변경</span>
				<div class="status-btns">
					{#each types as t}
						<button
							class="status-btn"
							class:active={t.id === detail.quest_type_id}
							style:--c={t.color}
							onclick={() => askChangeType(t)}
							disabled={changingType || t.id === detail.quest_type_id}
							title={t.id === detail.quest_type_id
								? '현재 타입'
								: `${t.prefix} 로 변경 — slug 바뀜`}
						>
							{t.prefix}
						</button>
					{/each}
				</div>
			</div>

			{#if detail.description}
				<div class="md-body">{@html renderMarkdown(detail.description)}</div>
			{:else}
				<p class="no-desc">No description. <button class="link-btn" onclick={enterEditMode}>설명 추가하기</button></p>
			{/if}
		{/if}

		<!-- 부모 퀘스트 (DEV-050) -->
		{#if detail.parent}
			<section>
				<div class="section-head">
					<h2 class="section-title parent-label">Parent</h2>
				</div>
				<ul class="quest-list">
					<li>
						<div class="prereq-row">
							<a href="/quests/{detail.parent.quest_id}" class="prereq-link">
								<span class="badge type" style:--c={detail.parent.type_color}>{detail.parent.quest_id}</span>
								<span class="ql-title">{detail.parent.title}</span>
								<span class="badge status" style:--c={detail.parent.status_color}>{detail.parent.status_name_en}</span>
							</a>
						</div>
					</li>
				</ul>
			</section>
		{/if}

		<!-- 서브퀘스트 -->
		<section>
			<div class="section-head">
				<h2 class="section-title sub-label">Sub-Quests</h2>
				{#if !editMode}
					<button class="sec-add-btn" onclick={() => (showNewSubQuest = true)}>+ 신규</button>
					<button class="sec-add-btn" onclick={() => openCombo('sub')}>+ 기존 지정</button>
				{/if}
			</div>
			{#if detail.sub_quests.length > 0}
				<ul class="quest-list">
					{#each detail.sub_quests as sq (sq.id)}
						<li>
							<div class="prereq-row">
								<a href="/quests/{sq.quest_id}" class="prereq-link">
									<span class="badge type" style:--c={sq.type_color}>{sq.quest_id}</span>
									<span class="ql-title">{sq.title}</span>
									<span class="badge status" style:--c={sq.status_color}>{sq.status_name_en}</span>
								</a>
								{#if !editMode}
									<button class="prereq-rm" title="부모에서 분리" onclick={() => detachSubQuest(sq.id)}>×</button>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="no-desc">서브퀘스트 없음.</p>
			{/if}
		</section>

		<!-- 선행 퀘스트 -->
		<section>
			<div class="section-head">
				<h2 class="section-title prereq-label">Prerequisites</h2>
				{#if !editMode}
					<button class="sec-add-btn" onclick={() => openCombo('prereq')}>+ 추가</button>
				{/if}
			</div>

			{#if detail.prerequisites.length > 0}
				<ul class="quest-list">
					{#each detail.prerequisites as pq (pq.id)}
						<li>
							<div class="prereq-row">
								<a href="/quests/{pq.quest_id}" class="prereq-link">
									<span class="badge type" style:--c={pq.type_color}>{pq.quest_id}</span>
									<span class="ql-title">{pq.title}</span>
									<span class="badge status" style:--c={pq.status_color}>{pq.status_name_en}</span>
								</a>
								{#if !editMode}
									<button class="prereq-rm" title="선행 퀘스트 제거" onclick={() => removePrerequisite(pq.id)}>×</button>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="no-desc">선행 퀘스트 없음.</p>
			{/if}
		</section>

		<!-- 변경 이력 (DEV-038) -->
		{#key `${detail.id}:${historyVersion}`}
			<QuestHistory questId={detail.id} {statuses} />
		{/key}
	{/if}
</div>

<!-- 콤보박스 모달 -->
{#if comboMode && detail}
	<div class="ov" role="presentation">
		<div class="modal-sm" role="dialog" aria-modal="true" tabindex="-1">
			<div class="modal-head">
				<h3>{comboMode === 'sub' ? '기존 퀘스트를 서브퀘스트로 지정' : '선행 퀘스트 추가'}</h3>
				<button class="x" onclick={closeCombo}>×</button>
			</div>
			{#if candidatesLoading}
				<div class="combo-state">후보 조회 중…</div>
			{:else}
				<QuestCombobox
					quests={candidates}
					placeholder="ID 또는 제목으로 검색"
					onselect={pickCandidate}
					oncancel={closeCombo}
				/>
			{/if}
			{#if comboError}<p class="combo-err">{comboError}</p>{/if}
		</div>
	</div>
{/if}

<!-- 서브퀘스트 신규 생성 모달 -->
{#if showNewSubQuest && detail}
	<NewQuestModal
		parentQuestId={detail.id}
		onclose={() => (showNewSubQuest = false)}
		oncreated={onSubQuestCreated}
	/>
{/if}

<!-- DEV-055: type 변경 확인 모달 -->
{#if confirmTypeChange && detail}
	{@const target = confirmTypeChange}
	<div class="ov" role="presentation">
		<div class="modal-sm" role="dialog" aria-modal="true" tabindex="-1">
			<div class="modal-head">
				<h3 class="del-title">타입 변경</h3>
				<button class="x" onclick={() => (confirmTypeChange = null)} disabled={changingType}>×</button>
			</div>
			<p class="del-msg">
				<code>{detail.quest_id}</code> 의 타입을 <strong>{target.prefix}</strong> 로
				변경합니다. 슬러그(quest_id) 가 바뀌어
				<code>{target.prefix}-NNN</code> 형태의 새 번호가 부여됩니다.
			</p>
			<p class="del-prereq">
				⚠ 다른 퀘스트 본문 안에 <code>{detail.quest_id}</code> 를 직접 언급(예 "참조") 한 부분은
				자동으로 갱신되지 않습니다. 필요하면 검색해서 직접 수정하세요.
				부모/자식/선행 관계의 auto-block 메타는 자동 갱신됩니다.
			</p>
			<div class="del-actions">
				<button class="btn-del-yes" onclick={doChangeType} disabled={changingType}>
					{changingType ? '변경 중…' : '변경'}
				</button>
				<button class="btn-del-no" onclick={() => (confirmTypeChange = null)} disabled={changingType}>
					취소
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- 삭제 모달 -->
{#if deleteModal && detail}
	<div class="ov" role="presentation">
		<div class="modal-sm" role="dialog" aria-modal="true" tabindex="-1">
			<div class="modal-head">
				<h3 class="del-title">{detail.quest_id} 삭제</h3>
				<button class="x" onclick={() => (deleteModal = false)} disabled={deleting}>×</button>
			</div>
			<p class="del-msg">이 퀘스트를 삭제합니다. 되돌릴 수 없습니다.</p>
			{#if detail.sub_quests.length > 0}
				<div class="del-sub">
					<div class="del-sub-head">
						<p class="del-sub-title">서브퀘스트 처리:</p>
						<label class="del-sub-all">
							<input
								type="checkbox"
								checked={cascadeSet.size === detail.sub_quests.length}
								indeterminate={cascadeSet.size > 0 && cascadeSet.size < detail.sub_quests.length}
								onchange={toggleAllCascade}
								data-testid="cascade-all"
							/>
							<span>전체 선택</span>
						</label>
					</div>
					<p class="del-sub-help">체크한 항목은 함께 삭제됩니다. 체크하지 않은 항목은 부모에서 분리됩니다.</p>
					<ul class="del-sub-list">
						{#each detail.sub_quests as sq (sq.id)}
							<li>
								<label>
									<input
										type="checkbox"
										checked={cascadeSet.has(sq.id)}
										onchange={() => toggleCascade(sq.id)}
										data-testid="cascade-{sq.id}"
									/>
									<span class="badge type" style:--c={sq.type_color}>{sq.quest_id}</span>
									<span class="del-sub-title-text">{sq.title}</span>
								</label>
							</li>
						{/each}
					</ul>
				</div>
			{/if}
			<p class="del-prereq">선행 퀘스트들은 별도의 퀘스트이므로 영향받지 않습니다.</p>
			<div class="del-actions">
				<button class="btn-del-yes" onclick={confirmDelete} disabled={deleting} data-testid="confirm-delete">
					{deleting ? '삭제 중…' : '삭제'}
				</button>
				<button class="btn-del-no" onclick={() => (deleteModal = false)} disabled={deleting}>취소</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.container {
		max-width: 800px;
		margin: 0 auto;
		padding: 1.5rem;
	}

	.top-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 1.5rem;
	}

	.back { font-size: 0.875rem; color: #8b949e; text-decoration: none; }
	.back:hover { color: #c9d1d9; }

	.top-actions { display: flex; align-items: center; gap: 0.5rem; }

	.btn-edit {
		padding: 0.3rem 0.9rem;
		border: 1px solid #30363d; border-radius: 6px;
		background: #21262d; color: #8b949e;
		font-size: 0.8rem; cursor: pointer;
		transition: background 0.1s, color 0.1s;
	}
	.btn-edit:hover { background: #30363d; color: #c9d1d9; }

	.btn-delete {
		padding: 0.3rem 0.9rem;
		border: 1px solid #3a1f22; border-radius: 6px;
		background: transparent; color: #e94f4f;
		font-size: 0.8rem; cursor: pointer;
		transition: background 0.1s;
	}
	.btn-delete:hover { background: rgba(233,79,79,0.1); }

	.state-msg {
		display: flex; align-items: center; justify-content: center;
		height: 60vh; color: #484f58; font-size: 0.9rem;
	}
	.state-msg.error { color: #e94f4f; }

	.header {
		display: flex; gap: 0.5rem; flex-wrap: wrap;
		margin-bottom: 0.75rem;
	}

	.meta-times {
		display: flex; align-items: center; flex-wrap: wrap;
		gap: 0.4rem;
		font-size: 0.72rem; color: #6e7681;
		margin-bottom: 0.85rem;
	}
	.meta-item { display: inline-flex; gap: 0.3rem; align-items: baseline; }
	.meta-label { color: #484f58; text-transform: uppercase; letter-spacing: 0.05em; }
	.meta-val { color: #8b949e; font-variant-numeric: tabular-nums; }
	.meta-sep { color: #30363d; }

	.title {
		font-size: 1.4rem; font-weight: 600; color: #e6edf3;
		margin: 0 0 1rem; line-height: 1.4;
	}

	.branch-row {
		display: flex; align-items: center; gap: 0.75rem;
		margin-bottom: 0.75rem;
		padding: 0.5rem 0.75rem;
		background: #161b22; border: 1px solid #21262d; border-radius: 6px;
	}
	.branch-label { font-size: 0.75rem; color: #8b949e; flex-shrink: 0; }
	.branch-name {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.85rem; color: #79c0ff; flex: 1;
	}
	.copy-btn {
		padding: 0.15rem 0.6rem;
		border: 1px solid #30363d; border-radius: 4px;
		background: transparent; color: #8b949e;
		font-size: 0.72rem; cursor: pointer;
		transition: background 0.1s, color 0.1s;
	}
	.copy-btn:hover { background: #21262d; color: #c9d1d9; }

	.status-row {
		display: flex; align-items: center; gap: 0.75rem;
		flex-wrap: wrap; margin-bottom: 1.25rem;
		padding: 0.5rem 0.75rem;
		background: #161b22; border: 1px solid #21262d; border-radius: 6px;
	}
	.status-btns { display: flex; gap: 0.4rem; flex-wrap: wrap; }
	.status-btn {
		padding: 0.15rem 0.7rem;
		border-radius: 20px;
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
		background: transparent;
		color: color-mix(in srgb, var(--c) 70%, #8b949e);
		font-size: 0.75rem; cursor: pointer;
		transition: background 0.12s, color 0.12s, transform 0.12s;
	}
	.status-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--c) 15%, transparent);
		color: var(--c);
	}
	.status-btn.active {
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c); font-weight: 600; cursor: default;
	}
	.status-btn:disabled:not(.active) { opacity: 0.5; cursor: default; }
	.status-btn.flash {
		background: color-mix(in srgb, var(--c) 32%, transparent);
		color: var(--c); font-weight: 600;
		transform: scale(1.04);
	}

	/* 헤더 상태 뱃지 펄스 */
	.badge.pulsing { animation: pulseBadge 0.8s ease-out; }
	@keyframes pulseBadge {
		0%   { box-shadow: 0 0 0 0 var(--c); }
		60%  { box-shadow: 0 0 0 6px color-mix(in srgb, var(--c) 0%, transparent); }
		100% { box-shadow: 0 0 0 0 transparent; }
	}

	.md-body {
		font-size: 0.9rem; color: #c9d1d9; line-height: 1.7;
		margin: 0 0 1.5rem; padding: 1rem 1.25rem;
		background: #161b22; border: 1px solid #21262d; border-radius: 6px;
	}
	.md-body :global(h1), .md-body :global(h2), .md-body :global(h3) {
		color: #e6edf3; margin: 1em 0 0.4em;
	}
	.md-body :global(h1) { font-size: 1.2rem; }
	.md-body :global(h2) { font-size: 1.05rem; }
	.md-body :global(h3) { font-size: 0.95rem; }
	.md-body :global(p) { margin: 0.5em 0; }
	.md-body :global(ul), .md-body :global(ol) { padding-left: 1.5rem; margin: 0.4em 0; }
	.md-body :global(code) {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.85em; background: #0d1117;
		padding: 0.1em 0.35em; border-radius: 3px; color: #79c0ff;
	}
	.md-body :global(pre) {
		background: #0d1117; border: 1px solid #21262d; border-radius: 6px;
		padding: 0.75rem 1rem; overflow-x: auto;
	}
	.md-body :global(pre code) { background: none; padding: 0; color: #c9d1d9; }
	.md-body :global(blockquote) {
		border-left: 3px solid #30363d; margin: 0.5em 0;
		padding: 0.25em 0.75em; color: #8b949e;
	}
	.md-body :global(a) { color: #58a6ff; }
	.md-body :global(hr) { border: none; border-top: 1px solid #21262d; margin: 1em 0; }
	.md-body :global(table) { border-collapse: collapse; width: 100%; font-size: 0.875rem; }
	.md-body :global(th), .md-body :global(td) {
		border: 1px solid #21262d; padding: 0.4em 0.7em; text-align: left;
	}
	.md-body :global(th) { background: #0d1117; color: #8b949e; font-weight: 600; }

	.no-desc { color: #484f58; font-size: 0.9rem; margin: 0 0 1.5rem; }
	.link-btn {
		background: none; border: none; color: #58a6ff;
		font-size: 0.9rem; cursor: pointer; padding: 0;
		text-decoration: underline;
	}

	.edit-form { display: flex; flex-direction: column; gap: 0.5rem; margin-bottom: 1.5rem; }
	/* BUG-010: text-transform / letter-spacing 을 label 전체가 아닌 라벨 텍스트
	   span 에만 적용 — 그렇지 않으면 자식 input / CodeMirror 까지 캐스케이드
	   되어 입력값이 대문자로 보임. */
	.field-label {
		font-size: 0.75rem; font-weight: 600; color: #8b949e;
		margin-top: 0.5rem;
	}
	.field-label > span:first-child {
		text-transform: uppercase; letter-spacing: 0.05em;
	}
	.edit-title {
		padding: 0.5rem 0.75rem;
		background: #161b22; border: 1px solid #30363d; border-radius: 6px;
		color: #e6edf3; font-size: 1rem; outline: none;
		width: 100%; box-sizing: border-box;
	}
	.edit-title:focus { border-color: #58a6ff; }
	.edit-select {
		padding: 0.4rem 0.6rem;
		background: #161b22; border: 1px solid #30363d; border-radius: 6px;
		color: #c9d1d9; font-size: 0.875rem; outline: none; width: 160px;
	}
	.edit-select:focus { border-color: #58a6ff; }
	.editor-wrap {
		/* DEV-057: 사용자 drag 로 height 조절. CodeMirror 의 cm-scroller 는
		   parent height 100% 따라가서 늘어남. ResizeObserver 가 변경 감지 →
		   localStorage 영속. */
		border: 1px solid #30363d; border-radius: 6px;
		overflow: hidden; min-height: 200px; max-height: 90vh;
		resize: vertical;
	}
	.editor-wrap :global(.cm-editor) { outline: none; }
	.editor-wrap :global(.cm-editor.cm-focused) { outline: none; border: none; }
	.save-error { color: #e94f4f; font-size: 0.8rem; margin: 0; }
	.edit-actions { display: flex; gap: 0.5rem; margin-top: 0.5rem; }
	.btn-save {
		padding: 0.4rem 1.2rem;
		background: #238636; border: 1px solid #2ea043; border-radius: 6px;
		color: #fff; font-size: 0.875rem; cursor: pointer;
	}
	.btn-save:hover:not(:disabled) { background: #2ea043; }
	.btn-save:disabled { opacity: 0.5; cursor: default; }
	.btn-cancel {
		padding: 0.4rem 1rem;
		background: transparent; border: 1px solid #30363d; border-radius: 6px;
		color: #8b949e; font-size: 0.875rem; cursor: pointer;
	}
	.btn-cancel:hover:not(:disabled) { background: #21262d; }

	.section-head {
		display: flex; align-items: center; gap: 0.75rem;
		margin-bottom: 0.5rem;
	}
	.section-title {
		font-size: 0.8rem; font-weight: 600; color: #8b949e;
		text-transform: uppercase; letter-spacing: 0.05em; margin: 0;
	}
	/* DEV-050: 라벨별 색 — QuestBoard 하이라이트 / CLI 의 quest show 와 일치. */
	.section-title.parent-label { color: #7ee787; }
	.section-title.sub-label { color: #3dc9b0; }
	.section-title.prereq-label { color: #a371f7; }
	.sec-add-btn {
		padding: 0.15rem 0.6rem;
		border: 1px solid #30363d; border-radius: 4px;
		background: transparent; color: #8b949e;
		font-size: 0.72rem; cursor: pointer;
	}
	.sec-add-btn:hover { background: #21262d; color: #c9d1d9; }

	section { margin-bottom: 1.5rem; }

	.quest-list {
		list-style: none; padding: 0; margin: 0;
		border: 1px solid #21262d; border-radius: 6px; overflow: hidden;
	}
	.quest-list li + li { border-top: 1px solid #21262d; }

	.prereq-row { display: flex; align-items: center; padding: 0; }
	.prereq-link {
		display: flex; align-items: center; gap: 0.6rem;
		flex: 1; padding: 0.55rem 1rem;
		text-decoration: none;
		transition: background 0.1s;
	}
	.prereq-link:hover { background: #161b22; }
	.prereq-rm {
		padding: 0.35rem 0.75rem;
		background: none; border: none; color: #484f58;
		font-size: 1rem; cursor: pointer;
		transition: color 0.1s; flex-shrink: 0;
	}
	.prereq-rm:hover { color: #e94f4f; }

	.ql-title {
		flex: 1; font-size: 0.875rem; color: #c9d1d9;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.badge {
		flex-shrink: 0;
		padding: 0.15rem 0.55rem; border-radius: 20px;
		font-size: 0.75rem; font-weight: 500;
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}

	/* --- 모달 (콤보박스 / 삭제) --- */
	.ov {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 100;
		display: flex; align-items: center; justify-content: center;
		padding: 1rem;
	}
	.modal-sm {
		background: #161b22;
		border: 1px solid #30363d; border-radius: 10px;
		width: 100%; max-width: 480px;
		padding: 1rem 1.25rem 1rem;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
	}
	.modal-head {
		display: flex; align-items: center; justify-content: space-between;
		margin-bottom: 0.85rem;
	}
	.modal-head h3 {
		margin: 0; font-size: 0.95rem; font-weight: 600; color: #e6edf3;
	}
	.x {
		background: none; border: none; color: #484f58;
		font-size: 1.2rem; line-height: 1; cursor: pointer; padding: 0 4px;
	}
	.x:hover { color: #c9d1d9; }

	.combo-state { color: #484f58; font-size: 0.85rem; padding: 0.6rem 0; }
	.combo-err { color: #e94f4f; font-size: 0.8rem; margin: 0.5rem 0 0; }

	.del-title { color: #e94f4f; }
	.del-msg { color: #c9d1d9; font-size: 0.875rem; margin: 0 0 0.85rem; }
	.del-sub {
		background: #0d1117; border: 1px solid #21262d; border-radius: 6px;
		padding: 0.6rem 0.8rem; margin-bottom: 0.85rem;
	}
	.del-sub-head {
		display: flex; align-items: center; justify-content: space-between;
		gap: 0.5rem; margin-bottom: 0.3rem;
	}
	.del-sub-all {
		display: flex; align-items: center; gap: 0.3rem;
		font-size: 0.75rem; color: #8b949e; cursor: pointer;
	}
	.del-sub-all:hover { color: #c9d1d9; }
	.del-sub-title { margin: 0; font-size: 0.8rem; color: #c9d1d9; font-weight: 600; }
	.del-sub-help { margin: 0 0 0.5rem; font-size: 0.75rem; color: #8b949e; }
	.del-sub-list { list-style: none; padding: 0; margin: 0; max-height: 180px; overflow-y: auto; }
	.del-sub-list li { padding: 0.25rem 0; }
	.del-sub-list label {
		display: flex; align-items: center; gap: 0.45rem;
		cursor: pointer; font-size: 0.85rem; color: #c9d1d9;
	}
	.del-sub-list .badge { padding: 0.05rem 0.45rem; font-size: 0.7rem; }
	.del-sub-title-text { flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
	.del-prereq { font-size: 0.75rem; color: #8b949e; margin: 0 0 0.85rem; font-style: italic; }
	.del-actions { display: flex; gap: 0.5rem; justify-content: flex-end; }
	.btn-del-yes {
		padding: 0.4rem 1.1rem;
		background: rgba(233,79,79,0.15);
		border: 1px solid #e94f4f; border-radius: 6px;
		color: #e94f4f; font-size: 0.875rem; cursor: pointer;
	}
	.btn-del-yes:hover:not(:disabled) { background: rgba(233,79,79,0.25); }
	.btn-del-yes:disabled { opacity: 0.5; cursor: default; }
	.btn-del-no {
		padding: 0.4rem 1rem;
		background: transparent; border: 1px solid #30363d; border-radius: 6px;
		color: #8b949e; font-size: 0.875rem; cursor: pointer;
	}
	.btn-del-no:hover:not(:disabled) { background: #21262d; }
</style>
