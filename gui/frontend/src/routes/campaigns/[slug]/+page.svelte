<!--
  DEV-011: Campaign 상세 페이지 (/campaigns/[slug]).
   - 제목 / 기간 / status (active|done 토글) / display_order
   - 본문 markdown 편집 (DEV-066 패턴 — DB sync)
   - 체크리스트 (본문의 GFM task list 와 자동 sync) — 토글 / 추가 / 삭제
   - 연결된 quest 표시 + 추가 / 제거
-->
<script lang="ts">
	import { modalScrollLock } from '$lib/actions/modal-scroll-lock';
	// BUG-257: 스크롤 컨테이너는 문서가 아니라 `<main>` 이다.
	import {
		pageScrollTop,
		scrollPageTo,
		onPageScroll,
		pageViewportHeight
	} from '$lib/utils/page-scroll';
	import Icon from '$lib/components/Icon.svelte';
	import { onMount, onDestroy } from 'svelte';
	// DEV-153: 편집 중이면 이탈 가드에 보고.
	import { setUnsaved } from '$lib/stores/unsaved';
	import { saveShortcut } from '$lib/utils/save-shortcut';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	// DEV-192: 스크롤 위치 복원용 snapshot 타입 + 견고 복원 유틸.
	import type { Snapshot } from './$types';
	import { restoreScroll } from '$lib/utils/scroll-restore';
	import { campaignsApi } from '$lib/api/campaigns';
	// DEV-205 / REQ-001: 상세 공통 액션 라벨을 퀘스트 상세와 같은 i18n 사전으로.
	import { locale, t } from '$lib/stores/locale';
	// DEV-259: alert() 잔재 제거 — 앱 공용 toast 로 통일.
	import { showToast } from '$lib/stores/toast';
	// DEV-255: 자식윈도우(검색 팔레트 "새 창으로 열기")에선 뒤로가기 버튼 숨김.
	// DEV-015: status 표시 이름 — 언어 반응.
	import { questStatusLabel } from '$lib/utils/status-label';
	// DEV-205: 언어 반응 날짜 입력(네이티브 date 대체).
	import DateField from '$lib/components/DateField.svelte';
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
	import CampaignHistory from '$lib/components/CampaignHistory.svelte';
	import BacklinkSection from '$lib/components/BacklinkSection.svelte';
	// DEV-203: 편집기 셋업(테마/들여쓰기/첨부/자동완성/redo/높이/overlay 스크롤)은
	// 공통 MarkdownEditor 컴포넌트로 단일화.
	import MarkdownEditor from '$lib/components/MarkdownEditor.svelte';

	let slug = $derived($page.params.slug ?? '');
	let detail = $state<CampaignDetail | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// DEV-233: 링크 퀘스트 진행바 hover 시 상태별 stacked + 카운트 팝업 —
	// CampaignCard 와 동일 UX(기본은 단일 채움, hover 시에만 전환).
	let questBarEl = $state<HTMLDivElement | null>(null);
	let questBarHover = $state(false);
	let tooltipTop = $state(0);
	let tooltipLeft = $state(0);
	function onQuestBarEnter() {
		if (!detail || detail.quest_done === detail.quest_total || !questBarEl) return;
		const r = questBarEl.getBoundingClientRect();
		tooltipTop = r.top;
		tooltipLeft = r.left;
		questBarHover = true;
	}
	function onQuestBarLeave() {
		questBarHover = false;
	}
	const showQuestStack = $derived(
		questBarHover &&
			!!detail &&
			detail.quest_done !== detail.quest_total &&
			(detail.quest_status_counts?.length ?? 0) > 0
	);

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
			error = `${t('campaign.attachFailed', $locale)}: ${e}`;
		}
	}
	let titleEdit = $state('');
	let startedEdit = $state('');
	let endedEdit = $state('');
	let bodyEdit = $state('');
	let saving = $state(false);

	// DEV-203: 편집기 생성/파괴/높이 영속/설정·테마 반응은 MarkdownEditor
	// 컴포넌트가 {#if editMode} 수명주기로 자동 처리.

	// 체크리스트 추가 입력
	let newChecklistText = $state('');

	// quest 연결
	let allQuests = $state<Quest[]>([]);
	// BUG-023: datalist input → QuestCombobox 모달.
	let comboOpen = $state(false);

	async function load() {
		loading = true;
		try {
			const [d, qs] = await Promise.all([campaignsApi.get(slug), questsApi.list(true)]);
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
		// DEV-191: anchor 가 보이는 밴드 [0, 1.1vh] 밖이면 표시 — 아래(내려가기)·
		// 위(top<0, 올라가기) 양방향. 메모 영역에서도 '댓글로'가 뜬다.
		const cTop = commentsAnchorEl?.getBoundingClientRect().top ?? null;
		showCommentsJump = cTop !== null && (cTop > vh * 1.1 || cTop < 0);
		const mTop = memoAnchorEl?.getBoundingClientRect().top ?? null;
		showMemoJump = mTop !== null && (mTop > vh * 1.1 || mTop < 0);
		// BUG-257: '한 화면' 의 기준은 창이 아니라 **스크롤 컨테이너** 다 —
		// `pageScrollTop()` 이 컨테이너 기준이므로 높이도 같이 맞춘다
		// (위 anchor 비교는 viewport 좌표라 `vh` 그대로가 맞다).
		showTopJump = pageScrollTop() > pageViewportHeight() * 0.8;
	}
	function jumpToComments() {
		commentsAnchorEl?.scrollIntoView({ behavior: 'smooth', block: 'start' });
	}
	function jumpToMemo() {
		memoAnchorEl?.scrollIntoView({ behavior: 'smooth', block: 'start' });
	}
	function jumpToTop() {
		scrollPageTo(0, true);
	}

	// DEV-192: 스크롤 위치 복원 (퀘스트 상세와 동일). detail 은 onMount 후 async
	// 로드 → 복원값을 보관했다가 로드 + 레이아웃(2 rAF) 후 instant 적용. back 버튼은
	// 이미 history.back() 이라 popstate 복귀 시 snapshot.restore 가 발동한다.
	let pendingScroll: number | null = null;
	function applyPendingScroll() {
		if (pendingScroll == null || !detail) return;
		const y = pendingScroll;
		pendingScroll = null;
		restoreScroll(y);
	}
	export const snapshot: Snapshot<number> = {
		capture: () => pageScrollTop(),
		restore: (y) => {
			pendingScroll = y;
			applyPendingScroll();
		}
	};
	$effect(() => {
		void detail;
		if (detail) applyPendingScroll();
	});
	// BUG-257: 퀘스트 상세와 같은 이유로 `onPageScroll` — 컨테이너 스크롤은
	// window 로 버블하지 않는다.
	onMount(() => {
		const handler = () => checkJumpVisibility();
		const offScroll = onPageScroll(handler);
		window.addEventListener('resize', handler);
		checkJumpVisibility();
		return () => {
			offScroll();
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
		if (!a && !b) return t('campaignList.periodUndefined', $locale);
		if (a && !b) return `${a} ~`;
		if (!a && b) return `~ ${b}`;
		return `${a} ~ ${b}`;
	}

	// BUG-033: editMeta + editBody → 단일 editMode. Quest Detail 패턴 그대로.
	function enterEditMode() {
		if (!detail) return;
		titleEdit = detail.title;
		startedEdit = detail.started_at ?? '';
		endedEdit = detail.ended_at ?? '';
		bodyEdit = detail.description ?? '';
		editMode = true;
	}
	function exitEditMode() {
		editMode = false;
	}
	async function saveEdit(keepEditing = false) {
		if (!detail || saving) return;
		saving = true;
		try {
			const desc = bodyEdit;
			const updated = await campaignsApi.update(detail.campaign_slug, {
				title: titleEdit.trim() || detail.title,
				started_at: startedEdit,
				ended_at: endedEdit,
				description: desc
			});
			if (keepEditing) {
				// load()는 loading 분기에서 편집기를 unmount해 커서·undo를 잃는다.
				// 응답 필드만 현재 detail에 합쳐 편집기는 그대로 유지한다.
				detail = { ...detail, ...updated };
			} else {
				exitEditMode();
				await load();
			}
		} catch (e) {
			showToast(e instanceof Error ? e.message : 'failed', 'error');
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
			showToast(e instanceof Error ? e.message : 'failed', 'error');
		}
	}

	// ── DEV-087: 배너 이미지 — 파일 선택 + assets/ 복사 ──
	//
	// BUG-255: 예전엔 `detectEnvironment() === 'tauri'` 로 버튼을 숨겨 브라우저·
	// 원격에서는 배너를 아예 못 건드렸다. 게다가 원격 제외(`!getRemoteServerUrl()`)
	// 가 빠져 있어, 데스크톱이 원격 길드에 접속한 상태에서는 버튼이 보이는데
	// `invoke` 가 **로컬** Store 를 건드렸다 — 보는 길드와 쓰는 대상이 갈렸다.
	// 이제 판별은 `isLocalTauri` 하나로 통일하고, 버튼은 항상 보인다.
	import { isLocalTauri } from '$lib/api/transport';
	const BANNER_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'];
	let bannerBusy = $state(false);

	/** 브라우저/원격 — 숨은 file input. `editor-attach.ts` 의 것과 같은 패턴. */
	function pickImageViaInput(): Promise<File | null> {
		return new Promise((resolve) => {
			const input = document.createElement('input');
			input.type = 'file';
			input.accept = BANNER_EXTS.map((e) => `.${e}`).join(',');
			input.style.display = 'none';
			// 취소를 눌러도 promise 가 남지 않도록 cancel 도 함께 처리.
			input.oncancel = () => {
				input.remove();
				resolve(null);
			};
			input.onchange = () => {
				const f = input.files?.[0] ?? null;
				input.remove();
				resolve(f);
			};
			document.body.appendChild(input);
			input.click();
		});
	}

	async function pickBanner() {
		if (!detail) return;
		bannerBusy = true;
		try {
			if (isLocalTauri()) {
				// 로컬 데스크톱은 경로 기반 — bytes 를 IPC 로 보내지 않는다.
				const { open } = await import('@tauri-apps/plugin-dialog');
				const picked = await open({
					multiple: false,
					directory: false,
					filters: [{ name: t('campaign.imageFilter', $locale), extensions: BANNER_EXTS }]
				});
				if (typeof picked === 'string' && picked) {
					await campaignsApi.setBannerFromPath(detail.campaign_slug, picked);
					await load();
				}
			} else {
				const file = await pickImageViaInput();
				if (file) {
					await campaignsApi.setBannerFromFile(detail.campaign_slug, file);
					await load();
				}
			}
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('campaign.bannerSetFailed', $locale), 'error');
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
			showToast(
				e instanceof Error ? e.message : t('campaign.bannerRemoveFailed', $locale),
				'error'
			);
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
			showToast(e instanceof Error ? e.message : 'failed', 'error');
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
			showToast(e instanceof Error ? e.message : 'failed', 'error');
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
			showToast(e instanceof Error ? e.message : 'failed', 'error');
		}
	}

	// ── Quest 연결 (BUG-023: QuestCombobox 모달) ──
	let linkableQuests = $derived(
		allQuests.filter((q) => !(detail?.linked_quests ?? []).some((lq) => lq.id === q.id))
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
			status_name_ko: q.status_name_ko,
			status_color: q.status_color
		};
		detail.linked_quests.push(linked);
		try {
			await campaignsApi.linkQuest(detail.campaign_slug, q.quest_id);
		} catch (e) {
			const i = detail.linked_quests.findIndex((x) => x.quest_id === q.quest_id);
			if (i >= 0) detail.linked_quests.splice(i, 1);
			showToast(e instanceof Error ? e.message : 'failed', 'error');
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
			showToast(e instanceof Error ? e.message : 'failed', 'error');
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
			showToast(e instanceof Error ? e.message : 'failed', 'error');
		}
	}
</script>

<div class="page">
	<div class="top">
		<!-- DEV-370: '뒤로' 버튼 제거 — 타이틀바 앞/뒤와 중복. 자식창에도
		     타이틀바 앞/뒤가 노출된다. -->
		{#if detail}
			<button
				class="pill status-badge status-{detail.status}"
				onclick={toggleStatus}
				title={t('campaign.statusToggle', $locale)}
			>
				{detail.status}
			</button>
			<!-- BUG-035: Quest Detail 의 top-bar 패턴 — 우측에 편집/삭제 묶음. -->
			{#if !editMode}
				<div class="top-actions">
					<!-- DEV-087: 배너 이미지. BUG-255 전까지 Tauri 전용이었다. -->
					<button class="btn-edit" onclick={pickBanner} disabled={bannerBusy}
						><Icon name="image" /> {t('campaign.banner', $locale)}
					</button>
					{#if detail.image_path}
						<button
							class="btn-edit"
							onclick={removeBanner}
							disabled={bannerBusy}
							title={t('campaign.bannerRemove', $locale)}
						>
							<Icon name="image" /> ×
						</button>
					{/if}
					<button class="btn-edit" onclick={enterEditMode}>✎ {t('detail.edit', $locale)}</button>
					<button class="btn-delete" onclick={askDeleteCampaign}
						><Icon name="trash" /> {t('detail.delete', $locale)}</button
					>
				</div>
			{/if}
		{/if}
	</div>

	{#if loading}
		<div class="state">Loading…</div>
	{:else if error || !detail}
		<div class="state error">{error ?? t('campaign.notFound', $locale)}</div>
	{:else}
		<!-- BUG-033: 메타 + 본문 통합 편집 (Quest Detail 패턴). 단일 편집 버튼,
		     단일 저장 / 취소. -->
		<section
			class="meta"
			use:saveShortcut={{
				disabled: !editMode || saving || !titleEdit.trim(),
				onSave: () => void saveEdit(true)
			}}
		>
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
						<span class="lbl">{t('campaign.start', $locale)}</span>
						<DateField bind:value={startedEdit} disabled={saving} />
					</label>
					<span class="dash">~</span>
					<label>
						<span class="lbl">{t('campaign.end', $locale)}</span>
						<DateField bind:value={endedEdit} disabled={saving} />
					</label>
				</div>
			{:else}
				<!-- DEV-079: 종료 기한 지난 캠페인 (status != done) 이면 period 빨강. -->
				<div
					class="period"
					class:overdue={isDateOverdue(detail.ended_at, detail.status === 'done' ? 'done' : null)}
				>
					{fmtPeriod()}
				</div>
			{/if}
			<!-- BUG-033: 캠페인도 생성 / 변경 시각 표시 (Quest Detail 과 동일). -->
			<div class="meta-times">
				<span class="meta-item">
					<span class="meta-label">{t('common.created', $locale)}</span>
					<time class="meta-val" datetime={detail.created_at} title={formatTs(detail.created_at)}
						>{formatTs(detail.created_at)}</time
					>
				</span>
				<span class="meta-sep">·</span>
				<span class="meta-item">
					<span class="meta-label">{t('common.updated', $locale)}</span>
					<time class="meta-val" datetime={detail.updated_at} title={formatTs(detail.updated_at)}
						>{formatRelative(detail.updated_at, undefined, $locale)}</time
					>
				</span>
			</div>
		</section>

		<!-- 본문 markdown — editMode 면 같은 form 안의 editor. -->
		<section
			class="body"
			use:saveShortcut={{
				disabled: !editMode || saving || !titleEdit.trim(),
				onSave: () => void saveEdit(true)
			}}
		>
			{#if editMode}
				<!-- CodeMirror 가 div 안에 textarea 를 동적 생성 — svelte 정적
				     분석으로는 label 의 associated control 미확인. ignore. -->
				<!-- DEV-202: 편집기 위 '첨부' 버튼 제거 — 아래 첨부 섹션과 중복.
				     이미지·동영상·파일은 드래그&드랍 / Ctrl+V 로 첨부(attachmentExtension). -->
				<div class="field-label">
					<span>{t('campaign.bodyLabel', $locale)}</span>
					<MarkdownEditor
						bind:value={bodyEdit}
						onError={(msg) => (error = `${t('campaign.attachUploadFailed', $locale)}: ${msg}`)}
						onAttach={attachToSection}
					/>
				</div>
				<div class="actions">
					<button
						class="btn-save"
						onclick={() => saveEdit()}
						disabled={saving || !titleEdit.trim()}
					>
						{saving ? t('common.saving', $locale) : t('common.save', $locale)}
					</button>
					<button class="btn-cancel" onclick={exitEditMode} disabled={saving}
						>{t('common.cancel', $locale)}</button
					>
				</div>
			{:else if detail.description && detail.description.trim()}
				<MarkdownView source={detail.description ?? ''} />
			{:else}
				<div class="empty">
					{t('campaign.noBody', $locale)}
					<button class="link" onclick={enterEditMode}>{t('campaign.addBody', $locale)}</button>
				</div>
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
				{t('campaign.checklist', $locale)} ({detail.checklists.filter((c) => c.checked)
					.length}/{detail.checklists.length})
				{#if detail.checklists.length > 0 && detail.checklists.every((c) => c.checked)}
					<span class="done-mark"> {t('common.doneMark', $locale)}</span>
				{/if}
			</h2>
			{#if detail.checklists.length === 0}
				<p class="empty">{t('campaign.noItems', $locale)}</p>
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
							<button
								class="rm"
								title={t('detail.delete', $locale)}
								onclick={() => askRemoveChecklist(idx)}>×</button
							>
						</li>
					{/each}
				</ul>
			{/if}
			<div class="add-row">
				<input
					type="text"
					bind:value={newChecklistText}
					placeholder={t('campaign.newChecklistItem', $locale)}
					onkeydown={(e) => e.key === 'Enter' && addChecklist()}
				/>
				<button onclick={addChecklist} disabled={!newChecklistText.trim()}
					>{t('common.add', $locale)}</button
				>
			</div>
		</section>

		<!-- 연결된 Quest -->
		<section>
			<h2 class:done={(detail.quest_total ?? 0) > 0 && detail.quest_done === detail.quest_total}>
				{t('campaign.linkedQuests', $locale)}
				{#if (detail.quest_total ?? 0) > 0}
					({detail.quest_done}/{detail.quest_total}, {Math.round(
						(detail.quest_progress ?? 0) * 100
					)}%)
				{:else}
					({detail.linked_quests.length})
				{/if}
				{#if (detail.quest_total ?? 0) > 0 && detail.quest_done === detail.quest_total}
					<span class="done-mark"> {t('common.doneMark', $locale)}</span>
				{/if}
			</h2>
			{#if (detail.quest_total ?? 0) > 0}
				<!-- DEV-093: progress bar — 체크리스트 옆 같은 시각. DEV-233: hover 시 상태별 stacked. -->
				<div
					class="quest-progress-bar"
					bind:this={questBarEl}
					role="img"
					aria-label={`${detail.quest_done}/${detail.quest_total}`}
					onmouseenter={onQuestBarEnter}
					onmouseleave={onQuestBarLeave}
				>
					{#if showQuestStack}
						{#each detail.quest_status_counts ?? [] as sc (sc.status_slug)}
							<div
								class="quest-progress-seg"
								style:width={`${(sc.count / (detail.quest_total ?? 1)) * 100}%`}
								style:background={sc.status_color}
							></div>
						{/each}
					{:else}
						<div
							class="quest-progress-fill"
							class:done={detail.quest_done === detail.quest_total}
							style:width={`${Math.round((detail.quest_progress ?? 0) * 100)}%`}
						></div>
					{/if}
				</div>
			{/if}
			{#if showQuestStack}
				<div
					class="quest-status-tooltip"
					style:top={`${tooltipTop}px`}
					style:left={`${tooltipLeft}px`}
				>
					{#each detail.quest_status_counts ?? [] as sc (sc.status_slug)}
						<div class="tooltip-row">
							<span class="tooltip-dot" style:background={sc.status_color}></span>
							<span class="tooltip-name">{questStatusLabel(sc, $locale)}</span>
							<span class="tooltip-count"
								>{sc.count}{t('common.countSuffix', $locale)} ({Math.round(
									(sc.count / (detail.quest_total ?? 1)) * 100
								)}%)</span
							>
						</div>
					{/each}
				</div>
			{/if}
			{#if detail.linked_quests.length === 0}
				<p class="empty">{t('campaign.noLinkedQuests', $locale)}</p>
			{:else}
				<ul class="linked">
					{#each detail.linked_quests as q (q.id)}
						<li>
							<a
								href={`/quests/${encodeURIComponent(q.quest_id)}?from=campaign:${detail.campaign_slug}`}
							>
								<span class="badge type" style:--c={q.type_color}>{q.quest_id}</span>
								<span class="qtitle">{q.title}</span>
								<span class="badge status" style:--c={q.status_color}
									>{questStatusLabel(q, $locale)}</span
								>
							</a>
							<button
								class="rm"
								title={t('campaign.unlinkQuest', $locale)}
								onclick={() => unlinkQuest(q.quest_id)}>×</button
							>
						</li>
					{/each}
				</ul>
			{/if}
			<div class="add-row">
				<!-- BUG-023: QuestCombobox 모달 (Quest Detail 과 동일 UI) -->
				<button class="link-add-btn" onclick={() => (comboOpen = true)}
					>{t('campaign.linkQuest', $locale)}</button
				>
			</div>
		</section>

		<!-- DEV-100: 캠페인 댓글 + 메모 — quest 와 동일 컴포넌트, scope 만 다름. -->
		<!-- DEV-144: floating 버튼 점프 anchor. -->
		<div bind:this={commentsAnchorEl} id="campaign-comments-anchor"></div>
		<QuestCommentsSection slug={detail.campaign_slug} scope="campaign" />
		<div bind:this={memoAnchorEl} id="campaign-memo-anchor"></div>
		<QuestNoteSection slug={detail.campaign_slug} mode="memo" scope="campaign" />

		<!-- DEV-226: 변경 이력. -->
		<!-- REQ-008: 이 문서를 참조하는 문서. -->
		<BacklinkSection kind="campaign" id={detail.campaign_slug} />
		<CampaignHistory campaignSlug={detail.campaign_slug} />
	{/if}
</div>

<!-- DEV-144: 우하단 floating 점프 버튼 cluster (quest 상세 패턴). -->
{#if detail && (showTopJump || showCommentsJump || showMemoJump)}
	<div class="jump-cluster">
		{#if showTopJump}
			<button
				class="jump-btn"
				onclick={jumpToTop}
				title={t('common.jumpTop', $locale)}
				aria-label={t('common.jumpTop', $locale)}
			>
				<span class="jb-icon">↑</span><span class="jb-label"
					>{t('common.jumpTopShort', $locale)}</span
				>
			</button>
		{/if}
		{#if showCommentsJump}
			<button
				class="jump-btn"
				onclick={jumpToComments}
				title={t('common.jumpComments', $locale)}
				aria-label={t('common.jumpComments', $locale)}
			>
				<span class="jb-icon"><Icon name="comment" /></span><span class="jb-label"
					>{t('common.jumpCommentsShort', $locale)}</span
				>
			</button>
		{/if}
		{#if showMemoJump}
			<button
				class="jump-btn"
				onclick={jumpToMemo}
				title={t('common.jumpMemo', $locale)}
				aria-label={t('common.jumpMemo', $locale)}
			>
				<span class="jb-icon"><Icon name="memo" /></span><span class="jb-label"
					>{t('common.jumpMemoShort', $locale)}</span
				>
			</button>
		{/if}
	</div>
{/if}

<!-- BUG-023: Quest 연결 콤보 모달 (Quest Detail 패턴 그대로) -->
{#if comboOpen && detail}
	<!-- BUG-160: 바깥(백드롭) 클릭으로 닫기 — ConfirmDialog 와 동일 패턴.
	     e.target === e.currentTarget 가드로 내부 클릭 버블링은 제외. -->
	<div
		class="ov"
		use:modalScrollLock
		role="presentation"
		onclick={(e) => {
			if (e.target === e.currentTarget) comboOpen = false;
		}}
	>
		<div class="modal-sm modal-combo" role="dialog" aria-modal="true" tabindex="-1">
			<div class="modal-head">
				<h3>{t('campaign.linkQuestTitle', $locale)}</h3>
				<button class="x" onclick={() => (comboOpen = false)}>×</button>
			</div>
			<QuestCombobox
				quests={linkableQuests}
				placeholder={t('campaign.searchPlaceholder', $locale)}
				onselect={pickQuestToLink}
				oncancel={() => (comboOpen = false)}
			/>
		</div>
	</div>
{/if}

<!-- DEV-118: 캠페인 / 체크리스트 삭제 확인 모달. -->
<ConfirmDialog
	open={confirmDeleteCampaign}
	title={t('campaign.deleteTitle', $locale)}
	message={detail
		? `${t('campaign.deleteMsg1', $locale)}${detail.title}${t('campaign.deleteMsg2', $locale)}`
		: ''}
	confirmLabel={t('detail.delete', $locale)}
	danger
	onconfirm={deleteCampaign}
	oncancel={() => (confirmDeleteCampaign = false)}
/>
<ConfirmDialog
	open={confirmDeleteChecklistIdx !== null}
	title={t('campaign.checklistDeleteTitle', $locale)}
	message={t('campaign.checklistDeleteMsg', $locale)}
	confirmLabel={t('detail.delete', $locale)}
	danger
	onconfirm={removeChecklist}
	oncancel={() => (confirmDeleteChecklistIdx = null)}
/>

<style>
	.page {
		padding: 1.25rem 1.5rem;
		max-width: var(--content-max-width, 880px);
		margin: 0 auto;
	}
	.top {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 1rem;
	}
	.btn-delete,
	.btn-edit {
		/* BUG-254 후속: 아이콘 + 글자 버튼의 세로 정렬.
		   `Icon` 은 인라인 SVG 라 기본적으로 글자 **기준선** 위에 얹힌다 —
		   아이콘만 위로 떠 보였다(실측 200% 에서 2px). flex 로 두 자식의
		   박스를 중앙에 맞추면 어긋남이 사라진다(실측 0.23px). */
		display: inline-flex;
		align-items: center;
		gap: 0.3em;
		font-size: 0.825rem;
		padding: 0.3rem 0.7rem;
		border-radius: var(--r-md);
		cursor: pointer;
		background: transparent;
		border: var(--bw) solid var(--border);
		color: var(--text);
		font-family: inherit;
	}
	.btn-edit:hover {
		background: var(--bg-subtle);
	}
	/* BUG-035: 단독 margin-left 제거 — top-actions wrapper 가 push right. */
	.btn-delete {
		color: var(--danger);
		border-color: color-mix(in srgb, var(--danger) 35%, transparent);
	}
	.btn-delete:hover {
		background: color-mix(in srgb, var(--danger) 18%, transparent);
	}
	.top-actions {
		display: flex;
		gap: 0.4rem;
		margin-left: auto;
	}

	/* DEV-364: 모양은 global.css 의 `.pill` 이 정본.
	   예전엔 버튼(`.btn-edit/.btn-delete`)과 상자를 공유하면서 곡률만
	   `20px !important` 로 덮어쓰고 있었다 — pill 이 아니라 '버튼처럼 생긴
	   것을 알약으로 우겨넣은' 상태였다. 공용 규칙에서 뺐다. */
	.status-badge {
		text-transform: uppercase;
		font-weight: 600;
	}
	.status-badge.status-active {
		--c: var(--success);
	}
	.status-badge.status-done {
		--c: var(--text-muted);
	}

	.state {
		color: var(--text-faint);
		padding: 1.5rem 0;
		font-size: 0.875rem;
	}
	.state.error {
		color: var(--danger);
	}

	section {
		margin-bottom: 1.75rem;
	}
	.section-head {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
		margin-bottom: 0.4rem;
	}
	h1 {
		font-size: 1.4rem;
		color: var(--text);
		margin: 0;
	}
	h2 {
		font-size: 1rem;
		color: var(--text);
		margin: 0 0 0.4rem 0;
	}
	/* BUG-025: 체크리스트 100% 달성 시 헤더 초록 */
	h2.done {
		color: var(--success);
	}
	h2 .done-mark {
		font-weight: 700;
		color: var(--success);
		font-size: 0.85rem;
		margin-left: 0.25rem;
	}

	.title-row {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
	}
	/* BUG-035: title-row 안 편집 버튼 제거 — top-bar 로 이동. */
	.slug {
		font-size: 0.8rem;
		font-family: var(--font-mono);
		color: var(--text-muted);
	}
	.period {
		color: var(--text-muted);
		font-size: 0.875rem;
	}
	/* DEV-079: 종료 기한 지난 캠페인 (status != done) 의 period 빨강 강조. */
	.period.overdue {
		color: var(--danger);
		font-weight: 600;
	}

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
	.meta-times .meta-label {
		color: var(--text-muted);
		margin-right: 0.25rem;
	}
	.meta-times .meta-val {
		color: var(--text);
	}
	.meta-times .meta-sep {
		color: var(--border);
	}

	/* BUG-033: editMode 에 묶인 기간 입력에 라벨 추가. */
	.period-row label {
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}
	.period-row .lbl {
		font-size: 0.75rem;
		color: var(--text-muted);
	}
	.period-row .dash {
		color: var(--text-faint);
	}

	/* BUG-033: 본문 editor 라벨 (Quest Detail .field-label 와 동일 스타일). */
	.field-label {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
	.field-label > span {
		font-size: 0.8rem;
		color: var(--text-muted);
	}

	.title-input {
		background: var(--bg);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-md);
		padding: 0.4rem 0.6rem;
		font-size: 1.2rem;
		width: 100%;
		margin-bottom: 0.5rem;
	}
	.period-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
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
	.btn-save:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
	.btn-save:disabled {
		opacity: 0.5;
		cursor: not-allowed;
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

	/* BUG-021: textarea 는 CodeMirror 로 교체. CSS 미사용 selector 정리. */

	/* DEV-203: .editor-wrap CSS 는 공통 MarkdownEditor 컴포넌트로 이동. */

	/* BUG-021 fix1: .md CSS 는 공유 컴포넌트 MarkdownView 로 이동. */

	.empty {
		color: var(--text-faint);
		font-size: 0.875rem;
	}
	.link {
		background: none;
		border: none;
		color: var(--accent);
		cursor: pointer;
		padding: 0;
	}

	.checklist,
	.linked {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}
	.checklist li,
	.linked li {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.45rem 0.7rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-md);
	}
	.checklist li label {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex: 1;
		cursor: pointer;
	}
	.checklist li span.checked {
		text-decoration: line-through;
		color: var(--text-muted);
	}

	.linked li a {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex: 1;
		text-decoration: none;
		color: inherit;
	}
	.qtitle {
		color: var(--text);
		flex: 1;
	}

	.rm {
		background: transparent;
		border: var(--bw) solid transparent;
		color: var(--text-muted);
		cursor: pointer;
		border-radius: var(--r-sm);
		width: 1.5rem;
		height: 1.5rem;
	}
	.rm:hover {
		color: var(--danger);
		border-color: color-mix(in srgb, var(--danger) 35%, transparent);
	}

	.add-row {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
	.add-row input {
		flex: 1;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-md);
		padding: 0.35rem 0.6rem;
		font-size: 0.875rem;
	}
	.add-row button {
		padding: 0.35rem 0.85rem;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-md);
		cursor: pointer;
		font-size: 0.825rem;
	}
	.add-row button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.add-row button:hover:not(:disabled) {
		background: var(--bg-subtle);
	}

	/* BUG-023: 모달 (Quest Detail 패턴) */
	.ov {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 100;
		display: flex;
		/* BUG-199 후속: 화면보다 긴 모달은 오버레이가 스크롤을 맡는다. 가운데
		   정렬이면 넘칠 때 위쪽(닫기/제목)이 잘려 손댈 수 없다 — 위 정렬 +
		   modal 의 margin:auto 로, 짧으면 가운데처럼 보이고 길면 위부터 보인다. */
		align-items: flex-start;
		overflow-y: auto;
		overscroll-behavior: contain;
		justify-content: center;
		padding: 1rem;
	}
	.modal-sm {
		/* BUG-199 후속(admin 보고: "지나치게 위로 치우쳐져 있음"): 오버레이
		   `.ov` 가 `align-items: flex-start` 인데 여기에 짝이 되는
		   `margin: auto` 가 빠져 있어 팝업이 화면 위에 붙었다(실측 900px
		   화면에서 위 16px / 아래 771px).

		   `.ov` 주석이 요구하는 패턴 그대로다 — flex 컨테이너에서 `margin: auto`
		   는 남는 공간을 위아래로 나눠 가지므로, **짧으면 가운데**에 오고
		   **길어서 넘치면** flex-start 가 이겨 위부터 보인다. 그래서 긴 팝업의
		   제목·닫기 버튼이 잘리지 않는다. */
		margin: auto;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-xl);
		width: 100%;
		/* BUG-160: 뷰포트보다 크면 창을 따라 줄어들도록 vw 상한 + 높이 상한. */
		max-width: min(calc(30rem * var(--popup-scale, 1)), 92vw); /* BUG-064 */
		max-height: 92vh;
		max-height: 92dvh;
		overflow-y: auto;
		padding: 1rem 1.25rem 1rem;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
	}
	/* BUG-160: 콤보박스 팝업 넓은 변형 — quests/[id] 와 동일 수치. */
	.modal-combo {
		max-width: min(calc(56rem * var(--popup-scale, 1)), 92vw);
	}
	.modal-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.85rem;
	}
	.modal-head h3 {
		margin: 0;
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.x {
		background: none;
		border: none;
		color: var(--text-faint);
		font-size: 1.2rem;
		line-height: 1;
		cursor: pointer;
		padding: 0 0.25rem;
	}
	.x:hover {
		color: var(--text);
	}
	.link-add-btn {
		padding: 0.35rem 0.85rem;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		color: var(--text);
		border-radius: var(--r-md);
		cursor: pointer;
		font-size: 0.825rem;
	}
	.link-add-btn:hover {
		background: var(--bg-subtle);
	}

	/* BUG-021: linked quest 의 type/status badge 도 Quest List pill 패턴. */
	.badge {
		flex-shrink: 0;
		padding: 0.15rem 0.55rem;
		border-radius: var(--r-pill);
		font-size: 0.75rem;
		font-weight: 500;
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: var(--bw) solid color-mix(in srgb, var(--c) 40%, transparent);
	}

	/* DEV-093: 연결된 퀘스트 진행률 bar — CampaignCard 의 progress-bar 와 같은 패턴. */
	.quest-progress-bar {
		/* BUG-254 계열(admin 보고): 바 두께가 px 이라 UI 배율을 안 따라갔다.
		   곡률(`--r-xs`)은 rem 이라 배율에서 함께 커지는데 두께만 그대로여서,
		   배율을 올리면 바가 과하게 둥근 실선처럼 보였다. 16px 기준 환산이라
		   기본 배율에서 두께는 그대로다. */
		height: 0.375rem;
		background: var(--bg-subtle);
		border-radius: var(--r-xs);
		overflow: hidden;
		margin: 0 0 0.75rem;
		display: flex;
	}
	.quest-progress-fill {
		height: 100%;
		background: var(--accent);
		transition:
			width 0.2s,
			background 0.2s;
	}
	.quest-progress-fill.done {
		background: var(--success-strong);
	}
	/* DEV-233: hover 시 상태별 stacked 세그먼트 — CampaignCard 와 동일 패턴. */
	.quest-progress-seg {
		height: 100%;
	}
	.quest-status-tooltip {
		position: fixed;
		transform: translateY(-100%) translateY(-6px);
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		padding: 0.4rem 0.6rem;
		font-size: 0.72rem;
		z-index: 50;
		box-shadow: 0 4px 12px color-mix(in srgb, black 25%, transparent);
		pointer-events: none;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		white-space: nowrap;
	}
	.tooltip-row {
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}
	.tooltip-dot {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 50%;
		flex: none;
	}
	.tooltip-name {
		color: var(--text);
		font-weight: 600;
	}
	.tooltip-count {
		color: var(--text-muted);
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
		border: var(--bw) solid var(--border);
		border-radius: var(--r-pill);
		color: var(--text);
		font-size: 0.85rem;
		font-weight: 500;
		cursor: pointer;
		box-shadow: 0 4px 14px rgba(0, 0, 0, 0.18);
		transition:
			background 0.12s,
			border-color 0.12s,
			transform 0.12s;
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

	/* admin 요청: 좁은 화면에서 `SLUG 배지들` / `제목` 2단으로.
	   한 줄에 다 넣으면 배지들이 자리를 먼저 가져가 제목이 몇 글자만 남는다.
	   마크업은 그대로 두고 wrap + order 로만 바꾼다 — 제목만 마지막 순서로
	   보내고 폭을 100% 로 주면 자기 줄을 통째로 쓴다.
	   (미디어 쿼리는 기본 규칙보다 **뒤**에 둔다 — 특이성이 같으면 순서가
	    이긴다. BUG-200 에서 이걸 놓쳐 수정이 통째로 무효였다.) */
	@media (max-width: 640px) {
		.linked li a {
			flex-wrap: wrap;
			row-gap: 0.15rem;
		}
		.qtitle {
			order: 10;
			flex: 1 1 100%;
			min-width: 0;
		}
	}
</style>
