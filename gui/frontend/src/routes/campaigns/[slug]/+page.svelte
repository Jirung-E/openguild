<!--
  DEV-011: Campaign 상세 페이지 (/campaigns/[slug]).
   - 제목 / 기간 / status (active|done 토글) / display_order
   - 본문 markdown 편집 (DEV-066 패턴 — DB sync)
   - 체크리스트 (본문의 GFM task list 와 자동 sync) — 토글 / 추가 / 삭제
   - 연결된 quest 표시 + 추가 / 제거
-->
<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	// DEV-153: 편집 중이면 이탈 가드에 보고.
	import { setUnsaved } from '$lib/stores/unsaved';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { campaignsApi } from '$lib/api/campaigns';
	import { questsApi } from '$lib/api/quests';
	import type { CampaignDetail, CampaignLinkedQuest, Quest } from '$lib/types';
	// BUG-021 fix1: 공유 컴포넌트로 Quest Detail / Campaign Detail 의 markdown
	// 프리뷰 통일.
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	// DEV-156: 본문 아래 첨부 섹션 (Jira 식).
	import AttachmentSection from '$lib/components/AttachmentSection.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	// BUG-033: 생성 / 변경 시각 표시용 — Quest Detail 과 동일 헬퍼.
	import { formatTs, formatRelative, isDateOverdue } from '$lib/utils/datetime';
	// BUG-023: Quest Detail 의 QuestCombobox 와 같은 UI 로 통일.
	import QuestCombobox from '$lib/components/QuestCombobox.svelte';
	// DEV-100: 캠페인 댓글 / 메모 — quest 컴포넌트 재사용 (scope prop).
	import QuestCommentsSection from '$lib/components/QuestCommentsSection.svelte';
	import QuestNoteSection from '$lib/components/QuestNoteSection.svelte';
	// BUG-021: Quest Detail 과 동일한 CodeMirror editor (라인 번호 + markdown
	// syntax highlighting) 로 통일.
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown } from '@codemirror/lang-markdown';
	// BUG: 편집창 띄운 채 다크/라이트 전환 시 테마 안 바뀌던 문제 — Compartment 로 라이브 교체.
	import { theme } from '$lib/stores/theme';
	import { editorThemeCompartment, editorThemeExtension } from '$lib/utils/editor-theme';
	// DEV-130: Tab = 들여쓰기 — indentExtensions 가 Tab 키맵 포함 (focus 이동 X).
	// DEV-069: 편집기 첨부 — 클립보드 이미지 paste / 파일 drag&drop 업로드.
	import { attachmentExtension, pickAndAttach } from '$lib/utils/editor-attach';
	// DEV-140: 본문 cross-link — XXX-NNN 타이핑 시 [[...]] 링크 자동완성.
	import { crossLinkAutocomplete } from '$lib/utils/editor-links';
	// DEV-130: 편집기 들여쓰기 설정 (tab/space + 2/4칸).
	import { indentExtensions } from '$lib/utils/editor-indent';
	import { editorSettings } from '$lib/stores/editorSettings';

	let slug = $derived($page.params.slug ?? '');
	let detail = $state<CampaignDetail | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// BUG-033: edit mode 통합 — Quest Detail 과 동일하게 단일 editMode 가
	// 제목 / 기간 / 본문 모두 묶음. 이전엔 editMeta / editBody 분리되어 통일감 X.
	let editMode = $state(false);
	// DEV-153: 편집 중이면 이탈 가드에 보고.
	$effect(() => setUnsaved('campaign-edit', editMode));
	onDestroy(() => setUnsaved('campaign-edit', false));

	// DEV-156: 편집기 '첨부' 버튼 / 비미디어 paste·drop → 본문 인라인 대신 첨부 섹션.
	async function attachToSection(rel: string, name: string) {
		if (!detail) return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			detail.attachments = await invoke('add_campaign_attachment', {
				slug: detail.campaign_slug,
				path: rel,
				name
			});
		} catch (e) {
			error = `첨부 실패: ${e}`;
		}
	}
	let titleEdit = $state('');
	let startedEdit = $state('');
	let endedEdit = $state('');
	let bodyEdit = $state('');
	let saving = $state(false);

	// BUG-021: CodeMirror editor (Quest Detail 패턴 그대로).
	// EDITOR_HEIGHT_KEY 는 Quest Detail 과 공유 — 일관 사용자 경험.
	const EDITOR_HEIGHT_KEY = 'openguild.questEditorHeight';
	let editorContainer: HTMLDivElement | undefined = $state(undefined);
	let editorView: EditorView | null = null;
	let editorResizeObserver: ResizeObserver | null = null;
	let editorHeightSaveTimer: ReturnType<typeof setTimeout> | null = null;
	function loadEditorHeight(): number {
		try {
			const raw = localStorage.getItem(EDITOR_HEIGHT_KEY);
			const n = raw ? parseInt(raw, 10) : NaN;
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
	function initEditor() {
		if (!editorContainer) return;
		if (editorView) {
			editorView.destroy();
			editorView = null;
		}
		editorContainer.style.height = `${loadEditorHeight()}px`;
		editorView = new EditorView({
			doc: bodyEdit,
			extensions: [
				basicSetup,
				markdown(),
				// 테마 — Compartment 로 다크/라이트 라이브 전환.
				editorThemeCompartment.of(editorThemeExtension($theme)),
				// DEV-130: tab/space + 2/4칸 들여쓰기 — Tab 키맵 + indentUnit/tabSize.
				indentExtensions($editorSettings),
				// DEV-069: 클립보드 이미지 paste / 파일 drag&drop → 첨부 업로드.
				attachmentExtension((msg) => (error = `첨부 업로드 실패: ${msg}`), attachToSection),
				// DEV-140: XXX-NNN 타이핑 → [[...]] cross-link 자동완성.
				crossLinkAutocomplete(),
				EditorView.theme({
					'&': { fontSize: '0.875rem', borderRadius: '6px', height: '100%' },
					'.cm-editor': { borderRadius: '6px', height: '100%' },
					'.cm-scroller': { overflow: 'auto' }
				})
			],
			parent: editorContainer
		});
		editorResizeObserver?.disconnect();
		editorResizeObserver = new ResizeObserver((entries) => {
			for (const entry of entries) {
				scheduleEditorHeightSave(entry.contentRect.height);
			}
		});
		editorResizeObserver.observe(editorContainer);
	}
	function destroyEditor() {
		editorView?.destroy();
		editorView = null;
		editorResizeObserver?.disconnect();
		editorResizeObserver = null;
	}
	// 테마 변경 시 editor 재생성 없이 테마 확장만 교체 (커서/스크롤/undo 보존).
	$effect(() => {
		const t = $theme;
		editorView?.dispatch({
			effects: editorThemeCompartment.reconfigure(editorThemeExtension(t))
		});
	});

	// 체크리스트 추가 입력
	let newChecklistText = $state('');

	// quest 연결
	let allQuests = $state<Quest[]>([]);
	// BUG-023: datalist input → QuestCombobox 모달.
	let comboOpen = $state(false);

	async function load() {
		loading = true;
		try {
			const [d, qs] = await Promise.all([
				campaignsApi.get(slug),
				questsApi.list()
			]);
			detail = d;
			allQuests = qs;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	}

	// DEV-144: quest 상세(DEV-109/123/127) 의 우하단 floating 점프 cluster 를
	// 캠페인 상세에도. 댓글/메모 anchor 기준 노출 + 맨 위로.
	let commentsAnchorEl: HTMLDivElement | undefined = $state(undefined);
	let memoAnchorEl: HTMLDivElement | undefined = $state(undefined);
	let showCommentsJump = $state(false);
	let showMemoJump = $state(false);
	let showTopJump = $state(false);
	function checkJumpVisibility() {
		const vh = window.innerHeight;
		showCommentsJump = commentsAnchorEl
			? commentsAnchorEl.getBoundingClientRect().top > vh * 1.1
			: false;
		showMemoJump = memoAnchorEl
			? memoAnchorEl.getBoundingClientRect().top > vh * 1.1
			: false;
		showTopJump = window.scrollY > vh * 0.8;
	}
	function jumpToComments() {
		commentsAnchorEl?.scrollIntoView({ behavior: 'smooth', block: 'start' });
	}
	function jumpToMemo() {
		memoAnchorEl?.scrollIntoView({ behavior: 'smooth', block: 'start' });
	}
	function jumpToTop() {
		window.scrollTo({ top: 0, behavior: 'smooth' });
	}
	onMount(() => {
		const handler = () => checkJumpVisibility();
		window.addEventListener('scroll', handler, { passive: true });
		window.addEventListener('resize', handler);
		checkJumpVisibility();
		return () => {
			window.removeEventListener('scroll', handler);
			window.removeEventListener('resize', handler);
		};
	});
	// detail 로드/변경 시 anchor 위치 재측정.
	$effect(() => {
		void detail;
		queueMicrotask(() => checkJumpVisibility());
	});

	onMount(load);
	$effect(() => {
		// slug 가 바뀌면 (다른 캠페인으로 navigate) 재로드
		void slug;
		if (slug) load();
	});

	function fmtPeriod(): string {
		if (!detail) return '';
		const a = detail.started_at?.trim() || '';
		const b = detail.ended_at?.trim() || '';
		if (!a && !b) return '기간 미정';
		if (a && !b) return `${a} ~`;
		if (!a && b) return `~ ${b}`;
		return `${a} ~ ${b}`;
	}

	// BUG-033: editMeta + editBody → 단일 editMode. Quest Detail 패턴 그대로.
	async function enterEditMode() {
		if (!detail) return;
		titleEdit = detail.title;
		startedEdit = detail.started_at ?? '';
		endedEdit = detail.ended_at ?? '';
		bodyEdit = detail.description ?? '';
		editMode = true;
		// CodeMirror 컨테이너는 {#if editMode} 가 true 되어야 mount → tick 후 init.
		await tick();
		initEditor();
	}
	function exitEditMode() {
		destroyEditor();
		editMode = false;
	}
	async function saveEdit() {
		if (!detail) return;
		saving = true;
		try {
			const desc = editorView ? editorView.state.doc.toString() : bodyEdit;
			await campaignsApi.update(detail.campaign_slug, {
				title: titleEdit.trim() || detail.title,
				started_at: startedEdit,
				ended_at: endedEdit,
				description: desc
			});
			exitEditMode();
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		} finally {
			saving = false;
		}
	}

	async function toggleStatus() {
		if (!detail) return;
		const next = detail.status === 'active' ? 'done' : 'active';
		try {
			await campaignsApi.update(detail.campaign_slug, { status: next });
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}

	// ── DEV-087: 배너 이미지 (Tauri 전용 — 파일 picker + assets/ 복사) ──
	import { detectEnvironment } from '$lib/api/transport';
	const isTauri = detectEnvironment() === 'tauri';
	let bannerBusy = $state(false);

	async function pickBanner() {
		if (!detail) return;
		bannerBusy = true;
		try {
			const { open } = await import('@tauri-apps/plugin-dialog');
			const picked = await open({
				multiple: false,
				directory: false,
				filters: [{ name: '이미지', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'] }]
			});
			if (typeof picked === 'string' && picked) {
				await campaignsApi.setBanner(detail.campaign_slug, picked);
				await load();
			}
		} catch (e) {
			alert(e instanceof Error ? e.message : '배너 설정 실패');
		} finally {
			bannerBusy = false;
		}
	}

	async function removeBanner() {
		if (!detail) return;
		bannerBusy = true;
		try {
			await campaignsApi.clearBanner(detail.campaign_slug);
			await load();
		} catch (e) {
			alert(e instanceof Error ? e.message : '배너 제거 실패');
		} finally {
			bannerBusy = false;
		}
	}

	// ── 체크리스트 ──
	async function addChecklist() {
		if (!detail) return;
		const t = newChecklistText.trim();
		if (!t) return;
		try {
			// BUG-046: load() 대신 응답 row 만 push — scroll 보존.
			const added = await campaignsApi.addChecklist(detail.campaign_slug, t);
			detail.checklists.push(added);
			newChecklistText = '';
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}
	// BUG-046: 체크리스트 토글 / 삭제 시 `load()` 전체 reload 가 detail 객체를
	// 새로 만들어 `{#each ...}` 모든 item 의 DOM 참조가 swap → 브라우저가 scroll
	// anchor 잃고 페이지 최상단으로 점프. optimistic update 로 scroll 보존 +
	// 즉시 반응. 실패 시 revert.
	async function toggleChecklist(idx: number, currentlyChecked: boolean) {
		if (!detail) return;
		const next = !currentlyChecked;
		const target = detail.checklists[idx];
		if (!target) return;
		// 즉시 UI 갱신 — Svelte 5 의 deep reactivity 가 prop 변경 감지.
		target.checked = next;
		try {
			await campaignsApi.setChecklist(detail.campaign_slug, idx + 1, next);
		} catch (e) {
			// 실패 시 원복 — 사용자에게 알림.
			target.checked = currentlyChecked;
			alert(e instanceof Error ? e.message : 'failed');
		}
	}
	// DEV-118: 인앱 확인 모달. 체크리스트 항목 삭제 / 캠페인 삭제.
	let confirmDeleteChecklistIdx = $state<number | null>(null);
	let confirmDeleteCampaign = $state(false);
	function askRemoveChecklist(idx: number) {
		confirmDeleteChecklistIdx = idx;
	}
	async function removeChecklist() {
		const idx = confirmDeleteChecklistIdx;
		confirmDeleteChecklistIdx = null;
		if (idx === null || !detail) return;
		// BUG-046: load() 대신 splice — 같은 array 안 단일 row 제거라 다른 item
		// 의 (item.id) key 가 안정. scroll 보존.
		const removed = detail.checklists[idx];
		if (!removed) return;
		detail.checklists.splice(idx, 1);
		try {
			await campaignsApi.removeChecklist(detail.campaign_slug, idx + 1);
		} catch (e) {
			// 실패 시 원복 — 같은 위치에 다시.
			detail.checklists.splice(idx, 0, removed);
			alert(e instanceof Error ? e.message : 'failed');
		}
	}

	// ── Quest 연결 (BUG-023: QuestCombobox 모달) ──
	let linkableQuests = $derived(
		allQuests.filter(
			(q) => !(detail?.linked_quests ?? []).some((lq) => lq.id === q.id)
		)
	);
	// BUG-046 와 동일 유형: `load()` 전체 reload 는 detail 객체를 새로 만들어
	// `{#each linked_quests}` 의 DOM 참조를 통째로 swap → 브라우저가 scroll
	// anchor 를 잃고 페이지 최상단으로 점프. 체크리스트(removeChecklist)는 이미
	// optimistic splice 로 고쳤지만 quest 연결/해제 경로엔 미적용이었음.
	// optimistic update 로 scroll 보존 + 즉시 반응. 실패 시 revert.
	async function pickQuestToLink(questId: number) {
		if (!detail) return;
		const q = allQuests.find((x) => x.id === questId);
		if (!q) return;
		comboOpen = false;
		const linked: CampaignLinkedQuest = {
			id: q.id,
			quest_id: q.quest_id,
			title: q.title,
			type_prefix: q.type_prefix,
			type_color: q.type_color,
			status_slug: q.status_slug,
			status_name_en: q.status_name_en,
			status_color: q.status_color
		};
		detail.linked_quests.push(linked);
		try {
			await campaignsApi.linkQuest(detail.campaign_slug, q.quest_id);
		} catch (e) {
			const i = detail.linked_quests.findIndex((x) => x.quest_id === q.quest_id);
			if (i >= 0) detail.linked_quests.splice(i, 1);
			alert(e instanceof Error ? e.message : 'failed');
		}
	}
	async function unlinkQuest(qSlug: string) {
		if (!detail) return;
		const idx = detail.linked_quests.findIndex((q) => q.quest_id === qSlug);
		if (idx < 0) return;
		const removed = detail.linked_quests[idx];
		detail.linked_quests.splice(idx, 1);
		try {
			await campaignsApi.unlinkQuest(detail.campaign_slug, qSlug);
		} catch (e) {
			detail.linked_quests.splice(idx, 0, removed);
			alert(e instanceof Error ? e.message : 'failed');
		}
	}

	function askDeleteCampaign() {
		if (!detail) return;
		confirmDeleteCampaign = true;
	}
	async function deleteCampaign() {
		confirmDeleteCampaign = false;
		if (!detail) return;
		try {
			await campaignsApi.delete(detail.campaign_slug);
			goto('/campaigns');
		} catch (e) {
			alert(e instanceof Error ? e.message : 'failed');
		}
	}

</script>

<div class="page">
	<div class="top">
		<button class="back" onclick={() => history.back()}>← 뒤로</button>
		{#if detail}
			<button class="status-badge status-{detail.status}" onclick={toggleStatus} title="클릭하여 상태 토글">
				{detail.status}
			</button>
			<!-- BUG-035: Quest Detail 의 top-bar 패턴 — 우측에 편집/삭제 묶음. -->
			{#if !editMode}
				<div class="top-actions">
					{#if isTauri}
						<!-- DEV-087: 배너 이미지 — Tauri 전용 (파일 picker). -->
						<button class="btn-edit" onclick={pickBanner} disabled={bannerBusy}>
							🖼 배너
						</button>
						{#if detail.image_path}
							<button class="btn-edit" onclick={removeBanner} disabled={bannerBusy} title="배너 제거">
								🖼 ×
							</button>
						{/if}
					{/if}
					<button class="btn-edit" onclick={enterEditMode}>✎ 편집</button>
					<button class="btn-delete" onclick={askDeleteCampaign}>🗑 삭제</button>
				</div>
			{/if}
		{/if}
	</div>

	{#if loading}
		<div class="state">Loading…</div>
	{:else if error || !detail}
		<div class="state error">{error ?? '캠페인 없음'}</div>
	{:else}
		<!-- BUG-033: 메타 + 본문 통합 편집 (Quest Detail 패턴). 단일 편집 버튼,
		     단일 저장 / 취소. -->
		<section class="meta">
			<!-- BUG-035: 편집 버튼은 top-bar 로 이동 — title-row 에서 제거. -->
			<div class="title-row">
				<span class="slug">{detail.campaign_slug}</span>
				{#if editMode}
					<input class="title-input" bind:value={titleEdit} disabled={saving} />
				{:else}
					<h1>{detail.title}</h1>
				{/if}
			</div>
			{#if editMode}
				<div class="period-row">
					<label>
						<span class="lbl">시작</span>
						<input type="date" bind:value={startedEdit} disabled={saving} />
					</label>
					<span class="dash">~</span>
					<label>
						<span class="lbl">종료</span>
						<input type="date" bind:value={endedEdit} disabled={saving} />
					</label>
				</div>
			{:else}
				<!-- DEV-079: 종료 기한 지난 캠페인 (status != done) 이면 period 빨강. -->
				<div
					class="period"
					class:overdue={isDateOverdue(detail.ended_at, detail.status === 'done' ? 'done' : null)}
				>{fmtPeriod()}</div>
			{/if}
			<!-- BUG-033: 캠페인도 생성 / 변경 시각 표시 (Quest Detail 과 동일). -->
			<div class="meta-times">
				<span class="meta-item">
					<span class="meta-label">생성</span>
					<time
						class="meta-val"
						datetime={detail.created_at}
						title={formatTs(detail.created_at)}
					>{formatTs(detail.created_at)}</time>
				</span>
				<span class="meta-sep">·</span>
				<span class="meta-item">
					<span class="meta-label">변경</span>
					<time
						class="meta-val"
						datetime={detail.updated_at}
						title={formatTs(detail.updated_at)}
					>{formatRelative(detail.updated_at)}</time>
				</span>
			</div>
		</section>

		<!-- 본문 markdown — editMode 면 같은 form 안의 editor. -->
		<section class="body">
			{#if editMode}
				<!-- CodeMirror 가 div 안에 textarea 를 동적 생성 — svelte 정적
				     분석으로는 label 의 associated control 미확인. ignore. -->
				<!-- BUG: editor 섹션은 <label> 금지 — 안의 '📎 첨부' 버튼(labelable)이
				     라벨 클릭마다 활성화돼 파일창이 뜬다(admin #13). div 로. -->
				<div class="field-label">
					<span>본문 (Markdown)</span>
					<!-- DEV-069: 첨부 — 버튼/드래그&드랍/Ctrl+V 동일 업로드. -->
					<div class="editor-toolbar">
						<button
							type="button"
							class="btn-attach"
							onclick={() =>
								editorView &&
								pickAndAttach(editorView, (msg) => (error = `첨부 업로드 실패: ${msg}`), attachToSection)}
							title="이미지·동영상·파일 첨부 (드래그&드랍 / Ctrl+V 도 가능)"
						>📎 첨부</button>
					</div>
					<div class="editor-wrap" bind:this={editorContainer}></div>
				</div>
				<div class="actions">
					<button class="btn-save" onclick={saveEdit} disabled={saving || !titleEdit.trim()}>
						{saving ? '저장…' : '저장'}
					</button>
					<button class="btn-cancel" onclick={exitEditMode} disabled={saving}>취소</button>
				</div>
			{:else if detail.description && detail.description.trim()}
				<MarkdownView source={detail.description ?? ''} />
			{:else}
				<div class="empty">본문 없음. <button class="link" onclick={enterEditMode}>본문 추가</button></div>
			{/if}
		</section>

		<!-- DEV-156: 본문 아래 첨부 섹션 (Jira 식). -->
		<section class="body">
			<AttachmentSection
				slug={detail.campaign_slug}
				scope="campaign"
				bind:attachments={detail.attachments}
			/>
		</section>

		<!-- 체크리스트 -->
		<section>
			<h2 class:done={detail.checklists.length > 0 && detail.checklists.every((c) => c.checked)}>
				체크리스트 ({detail.checklists.filter((c) => c.checked).length}/{detail.checklists.length})
				{#if detail.checklists.length > 0 && detail.checklists.every((c) => c.checked)}
					<span class="done-mark"> ✓ 완료</span>
				{/if}
			</h2>
			{#if detail.checklists.length === 0}
				<p class="empty">항목 없음.</p>
			{:else}
				<ul class="checklist">
					{#each detail.checklists as item, idx (item.id)}
						<li>
							<label>
								<input
									type="checkbox"
									checked={item.checked}
									onchange={() => toggleChecklist(idx, item.checked)}
								/>
								<span class:checked={item.checked}>{item.text}</span>
							</label>
							<button class="rm" title="삭제" onclick={() => askRemoveChecklist(idx)}>×</button>
						</li>
					{/each}
				</ul>
			{/if}
			<div class="add-row">
				<input
					type="text"
					bind:value={newChecklistText}
					placeholder="새 체크리스트 항목..."
					onkeydown={(e) => e.key === 'Enter' && addChecklist()}
				/>
				<button onclick={addChecklist} disabled={!newChecklistText.trim()}>추가</button>
			</div>
		</section>

		<!-- 연결된 Quest -->
		<section>
			<h2 class:done={(detail.quest_total ?? 0) > 0 && detail.quest_done === detail.quest_total}>
				연결된 퀘스트
				{#if (detail.quest_total ?? 0) > 0}
					({detail.quest_done}/{detail.quest_total}, {Math.round((detail.quest_progress ?? 0) * 100)}%)
				{:else}
					({detail.linked_quests.length})
				{/if}
				{#if (detail.quest_total ?? 0) > 0 && detail.quest_done === detail.quest_total}
					<span class="done-mark"> ✓ 완료</span>
				{/if}
			</h2>
			{#if (detail.quest_total ?? 0) > 0}
				<!-- DEV-093: progress bar — 체크리스트 옆 같은 시각. -->
				<div class="quest-progress-bar">
					<div
						class="quest-progress-fill"
						class:done={detail.quest_done === detail.quest_total}
						style:width={`${Math.round((detail.quest_progress ?? 0) * 100)}%`}
					></div>
				</div>
			{/if}
			{#if detail.linked_quests.length === 0}
				<p class="empty">연결된 퀘스트 없음.</p>
			{:else}
				<ul class="linked">
					{#each detail.linked_quests as q (q.id)}
						<li>
							<a href={`/quests/${encodeURIComponent(q.quest_id)}?from=campaign:${detail.campaign_slug}`}>
								<span class="badge type" style:--c={q.type_color}>{q.quest_id}</span>
								<span class="qtitle">{q.title}</span>
								<span class="badge status" style:--c={q.status_color}>{q.status_name_en}</span>
							</a>
							<button class="rm" title="연결 해제" onclick={() => unlinkQuest(q.quest_id)}>×</button>
						</li>
					{/each}
				</ul>
			{/if}
			<div class="add-row">
				<!-- BUG-023: QuestCombobox 모달 (Quest Detail 과 동일 UI) -->
				<button class="link-add-btn" onclick={() => (comboOpen = true)}>+ 퀘스트 연결</button>
			</div>
		</section>

		<!-- DEV-100: 캠페인 댓글 + 메모 — quest 와 동일 컴포넌트, scope 만 다름. -->
		<!-- DEV-144: floating 버튼 점프 anchor. -->
		<div bind:this={commentsAnchorEl} id="campaign-comments-anchor"></div>
		<QuestCommentsSection slug={detail.campaign_slug} scope="campaign" onAttach={attachToSection} />
		<div bind:this={memoAnchorEl} id="campaign-memo-anchor"></div>
		<QuestNoteSection slug={detail.campaign_slug} mode="memo" scope="campaign" />
	{/if}
</div>

<!-- DEV-144: 우하단 floating 점프 버튼 cluster (quest 상세 패턴). -->
{#if detail && (showTopJump || showCommentsJump || showMemoJump)}
	<div class="jump-cluster">
		{#if showTopJump}
			<button class="jump-btn" onclick={jumpToTop} title="맨 위로" aria-label="맨 위로">
				<span class="jb-icon">↑</span><span class="jb-label">위</span>
			</button>
		{/if}
		{#if showCommentsJump}
			<button class="jump-btn" onclick={jumpToComments} title="댓글로 이동" aria-label="댓글로 이동">
				<span class="jb-icon">💬</span><span class="jb-label">댓글</span>
			</button>
		{/if}
		{#if showMemoJump}
			<button class="jump-btn" onclick={jumpToMemo} title="메모로 이동" aria-label="메모로 이동">
				<span class="jb-icon">📝</span><span class="jb-label">메모</span>
			</button>
		{/if}
	</div>
{/if}

<!-- BUG-023: Quest 연결 콤보 모달 (Quest Detail 패턴 그대로) -->
{#if comboOpen && detail}
	<div class="ov" role="presentation">
		<div class="modal-sm" role="dialog" aria-modal="true" tabindex="-1">
			<div class="modal-head">
				<h3>퀘스트 연결</h3>
				<button class="x" onclick={() => (comboOpen = false)}>×</button>
			</div>
			<QuestCombobox
				quests={linkableQuests}
				placeholder="ID 또는 제목으로 검색"
				onselect={pickQuestToLink}
				oncancel={() => (comboOpen = false)}
			/>
		</div>
	</div>
{/if}

<!-- DEV-118: 캠페인 / 체크리스트 삭제 확인 모달. -->
<ConfirmDialog
	open={confirmDeleteCampaign}
	title="캠페인 삭제"
	message={detail ? `캠페인 "${detail.title}" 을(를) 삭제할까요?\n(soft delete — restore 가능)` : ''}
	confirmLabel="삭제"
	danger
	onconfirm={deleteCampaign}
	oncancel={() => (confirmDeleteCampaign = false)}
/>
<ConfirmDialog
	open={confirmDeleteChecklistIdx !== null}
	title="체크리스트 항목 삭제"
	message="이 체크리스트 항목을 삭제할까요?"
	confirmLabel="삭제"
	danger
	onconfirm={removeChecklist}
	oncancel={() => (confirmDeleteChecklistIdx = null)}
/>

<style>
	.page { padding: 1.25rem 1.5rem; max-width: var(--content-max-width, 880px); margin: 0 auto; }
	.top {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 1rem;
	}
	.back, .btn-delete, .status-badge, .btn-edit {
		font-size: 0.825rem;
		padding: 0.3rem 0.7rem;
		border-radius: 6px;
		cursor: pointer;
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text);
		font-family: inherit;
	}
	.back:hover, .btn-edit:hover { background: var(--bg-subtle); }
	/* BUG-035: 단독 margin-left 제거 — top-actions wrapper 가 push right. */
	.btn-delete { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 35%, transparent); }
	.btn-delete:hover { background: color-mix(in srgb, var(--danger) 18%, transparent); }
	.top-actions {
		display: flex;
		gap: 0.4rem;
		margin-left: auto;
	}

	/* BUG-021: pill 스타일 통일 (Quest List 패턴). */
	.status-badge {
		text-transform: uppercase;
		font-weight: 600;
		border-radius: 20px !important;
	}
	.status-badge.status-active {
		--c: var(--success);
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	.status-badge.status-done {
		--c: var(--text-muted);
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}

	.state { color: var(--text-faint); padding: 1.5rem 0; font-size: 0.875rem; }
	.state.error { color: var(--danger); }

	section { margin-bottom: 1.75rem; }
	.section-head { display: flex; align-items: baseline; gap: 0.75rem; margin-bottom: 0.4rem; }
	h1 { font-size: 1.4rem; color: var(--text); margin: 0; }
	h2 { font-size: 1rem; color: var(--text); margin: 0 0 0.4rem 0; }
	/* BUG-025: 체크리스트 100% 달성 시 헤더 초록 */
	h2.done { color: var(--success); }
	h2 .done-mark { font-weight: 700; color: var(--success); font-size: 0.85rem; margin-left: 0.25rem; }

	.title-row { display: flex; align-items: baseline; gap: 0.75rem; }
	/* BUG-035: title-row 안 편집 버튼 제거 — top-bar 로 이동. */
	.slug {
		font-size: 0.8rem;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		color: var(--text-muted);
	}
	.period { color: var(--text-muted); font-size: 0.875rem; }
	/* DEV-079: 종료 기한 지난 캠페인 (status != done) 의 period 빨강 강조. */
	.period.overdue { color: var(--danger); font-weight: 600; }

	/* BUG-033: 생성 / 변경 시각 표시 — Quest Detail 의 .meta-times 와 동일 톤. */
	.meta-times {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
		margin-top: 0.4rem;
		font-size: 0.75rem;
		color: var(--text-faint);
	}
	.meta-times .meta-label { color: var(--text-muted); margin-right: 0.25rem; }
	.meta-times .meta-val { color: var(--text); }
	.meta-times .meta-sep { color: var(--border); }

	/* BUG-033: editMode 에 묶인 기간 입력에 라벨 추가. */
	.period-row label { display: flex; align-items: center; gap: 0.35rem; }
	.period-row .lbl { font-size: 0.75rem; color: var(--text-muted); }
	.period-row .dash { color: var(--text-faint); }

	/* BUG-033: 본문 editor 라벨 (Quest Detail .field-label 와 동일 스타일). */
	.field-label { display: flex; flex-direction: column; gap: 0.4rem; margin-top: 0.5rem; }
	.field-label > span { font-size: 0.8rem; color: var(--text-muted); }

	.title-input {
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 6px;
		padding: 0.4rem 0.6rem;
		font-size: 1.2rem;
		width: 100%;
		margin-bottom: 0.5rem;
	}
	.period-row { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem; }
	.period-row input {
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 6px;
		padding: 0.3rem 0.5rem;
	}

	.actions { display: flex; gap: 0.4rem; margin-top: 0.5rem; }
	.btn-save {
		padding: 0.35rem 0.85rem;
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		color: var(--btn-primary-text);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.btn-save:hover:not(:disabled) { background: var(--btn-primary-bg-hover); border-color: var(--btn-primary-border-hover); }
	.btn-save:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-cancel {
		padding: 0.35rem 0.85rem;
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}

	/* BUG-021: textarea 는 CodeMirror 로 교체. CSS 미사용 selector 정리. */

	/* BUG-021: CodeMirror editor (Quest Detail 패턴 — DEV-057 의 height 영속). */
	/* DEV-069: 편집기 위 첨부 툴바. */
	.editor-toolbar {
		display: flex;
		gap: 0.4rem;
		margin: 0.25rem 0;
	}
	.btn-attach {
		font-size: 0.8rem;
		padding: 0.2rem 0.6rem;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: var(--bg-subtle);
		color: var(--text);
		cursor: pointer;
	}
	.btn-attach:hover {
		background: var(--bg-elevated);
	}
	.editor-wrap {
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
		min-height: 200px;
		max-height: 90vh;
		resize: vertical;
	}
	.editor-wrap :global(.cm-editor) { outline: none; }
	.editor-wrap :global(.cm-editor.cm-focused) { outline: none; border: none; }

	/* BUG-021 fix1: .md CSS 는 공유 컴포넌트 MarkdownView 로 이동. */

	.empty { color: var(--text-faint); font-size: 0.875rem; }
	.link { background: none; border: none; color: var(--accent); cursor: pointer; padding: 0; }

	.checklist, .linked { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 2px; }
	.checklist li, .linked li {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.45rem 0.7rem;
		background: var(--bg-elevated);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
	}
	.checklist li label { display: flex; align-items: center; gap: 0.5rem; flex: 1; cursor: pointer; }
	.checklist li span.checked { text-decoration: line-through; color: var(--text-muted); }

	.linked li a { display: flex; align-items: center; gap: 0.5rem; flex: 1; text-decoration: none; color: inherit; }
	.qtitle { color: var(--text); flex: 1; }

	.rm {
		background: transparent;
		border: 1px solid transparent;
		color: var(--text-muted);
		cursor: pointer;
		border-radius: 4px;
		width: 1.5rem;
		height: 1.5rem;
	}
	.rm:hover { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 35%, transparent); }

	.add-row {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
	.add-row input {
		flex: 1;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 6px;
		padding: 0.35rem 0.6rem;
		font-size: 0.875rem;
	}
	.add-row button {
		padding: 0.35rem 0.85rem;
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.add-row button:disabled { opacity: 0.5; cursor: not-allowed; }
	.add-row button:hover:not(:disabled) { background: var(--bg-subtle); }

	/* BUG-023: 모달 (Quest Detail 패턴) */
	.ov {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 100;
		display: flex; align-items: center; justify-content: center;
		padding: 1rem;
	}
	.modal-sm {
		background: var(--bg-elevated);
		border: 1px solid var(--border); border-radius: 10px;
		width: 100%; max-width: calc(30rem * var(--popup-scale, 1)); /* BUG-064 */
		padding: 1rem 1.25rem 1rem;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
	}
	.modal-head {
		display: flex; align-items: center; justify-content: space-between;
		margin-bottom: 0.85rem;
	}
	.modal-head h3 {
		margin: 0; font-size: 0.95rem; font-weight: 600; color: var(--text-strong);
	}
	.x {
		background: none; border: none; color: var(--text-faint);
		font-size: 1.2rem; line-height: 1; cursor: pointer; padding: 0 4px;
	}
	.x:hover { color: var(--text); }
	.link-add-btn {
		padding: 0.35rem 0.85rem;
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		color: var(--text);
		border-radius: 6px;
		cursor: pointer;
		font-size: 0.825rem;
	}
	.link-add-btn:hover { background: var(--bg-subtle); }

	/* BUG-021: linked quest 의 type/status badge 도 Quest List pill 패턴. */
	.badge {
		flex-shrink: 0;
		padding: 0.15rem 0.55rem;
		border-radius: 20px;
		font-size: 0.75rem;
		font-weight: 500;
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}

	/* DEV-093: 연결된 퀘스트 진행률 bar — CampaignCard 의 progress-bar 와 같은 패턴. */
	.quest-progress-bar {
		height: 6px;
		background: var(--bg-subtle);
		border-radius: 3px;
		overflow: hidden;
		margin: 0 0 0.75rem;
	}
	.quest-progress-fill {
		height: 100%;
		background: var(--accent);
		transition: width 0.2s, background 0.2s;
	}
	.quest-progress-fill.done {
		background: var(--success-strong);
	}

	/* DEV-144: 우하단 floating 점프 cluster (quest 상세와 동일). */
	.jump-cluster {
		position: fixed;
		right: 1.5rem;
		bottom: 1.5rem;
		z-index: 80;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		align-items: flex-end;
	}
	.jump-btn {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.55rem 1rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 999px;
		color: var(--text);
		font-size: 0.85rem;
		font-weight: 500;
		cursor: pointer;
		box-shadow: 0 4px 14px rgba(0, 0, 0, 0.18);
		transition: background 0.12s, border-color 0.12s, transform 0.12s;
	}
	.jump-btn:hover {
		background: var(--bg-subtle);
		border-color: var(--accent);
		transform: translateY(-2px);
	}
	.jump-btn .jb-icon {
		font-size: 1rem;
		line-height: 1;
		color: var(--accent);
	}
	.jump-btn .jb-label {
		line-height: 1;
	}
</style>
