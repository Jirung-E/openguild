<script lang="ts">
	import { page } from '$app/stores';
	import { onMount, onDestroy } from 'svelte';
	// DEV-153: 편집 중이면 이탈 가드에 보고.
	import { setUnsaved } from '$lib/stores/unsaved';
	import { goto } from '$app/navigation';
	// DEV-192: 스크롤 위치 복원용 snapshot 타입 + 견고 복원 유틸.
	import type { Snapshot } from './$types';
	import { restoreScroll } from '$lib/utils/scroll-restore';
	import { questsApi } from '$lib/api/quests';
	import { metaApi } from '$lib/api/meta';
	// DEV-205 / REQ-001: 상세 화면 라벨을 i18n 사전으로 — 캠페인 상세와 언어 통일.
	import { locale, t } from '$lib/stores/locale';
	// DEV-255: 자식윈도우(검색 팔레트 "새 창으로 열기")에선 뒤로가기 버튼 숨김.
	import { isChildWindow } from '$lib/stores/windowKind';
	// DEV-015: status 표시 이름 — 언어 반응(ko 면 name_ko 우선, 빈 값이면 en).
	import { statusLabel, questStatusLabel } from '$lib/utils/status-label';
	// DEV-205: 언어 반응 날짜 입력(네이티브 date 대체).
	import DateField from '$lib/components/DateField.svelte';
	import { campaignsApi } from '$lib/api/campaigns';
	// DEV-068: `.guild/tags/{slug}.toml` 정의 — Tag pill 색칠용.
	import { adminApi } from '$lib/api/admin';
	import type { QuestTagDef } from '$lib/types';
	// DEV-074 fix17: 삭제 cascade 모달의 sub-quest list overlay scrollbar.
	import OverlayScrollbar from '$lib/components/OverlayScrollbar.svelte';
	// BUG-021 fix1: marked 직접 호출 대신 공유 컴포넌트 MarkdownView 사용.
	import MarkdownView from '$lib/components/MarkdownView.svelte';
	// DEV-156: 본문 아래 첨부 섹션 (Jira 식).
	import AttachmentSection from '$lib/components/AttachmentSection.svelte';
	// DEV-203: 편집기 셋업(테마/들여쓰기/첨부/자동완성/redo/높이/overlay 스크롤)은
	// 공통 MarkdownEditor 컴포넌트로 단일화.
	import MarkdownEditor from '$lib/components/MarkdownEditor.svelte';
	// alert() 대신 통일된 toast (UI 일관성 — DEV-142 완료 차단 경고 등).
	import { showToast } from '$lib/stores/toast';
	import {
		URGENCY_LABEL,
		urgencyColor,
		urgencyLabel,
		urgencyOutOfRange,
		type CandidateRelation,
		type Quest,
		type QuestDetail,
		type QuestStatus,
		type QuestType
	} from '$lib/types';
	import NewQuestModal from '$lib/components/NewQuestModal.svelte';
	import QuestCombobox from '$lib/components/QuestCombobox.svelte';
	// BUG-030: 캠페인 연결 콤보박스 (QuestCombobox 와 동일 톤).
	import CampaignCombobox from '$lib/components/CampaignCombobox.svelte';
	import QuestHistory from '$lib/components/QuestHistory.svelte';
	// DEV-012: 공개 댓글 + 비공개 메모 섹션.
	import QuestNoteSection from '$lib/components/QuestNoteSection.svelte';
	// DEV-094: 댓글은 entry 단위 컴포넌트.
	import QuestCommentsSection from '$lib/components/QuestCommentsSection.svelte';
	import { formatTs, formatRelative, isDateOverdue } from '$lib/utils/datetime';
	// 상태순서 통일: 보드 레인 순서(localStorage laneOrder)를 status 드롭다운에도
	// 반영. 없으면 sort_order fallback. (보드와 같은 공유 헬퍼/키 사용.)
	import { loadLaneOrder, orderStatusesByLane } from '$lib/utils/lane-order';
	import { resolveGuildKeyPrefix } from '$lib/utils/guild-storage';

	let slug = $derived($page.params.id ?? '');
	// BUG-015 fix1: parent / sub / prereq link 가 같은 origin 을 propagate 해서
	// 같은 quest 안에서 다른 quest 클릭 후 back 도 list/board 로 가게 함.
	let fromSuffix = $derived.by(() => {
		const f = $page.url.searchParams.get('from');
		return f ? `?from=${f}` : '';
	});
	let detail = $state<QuestDetail | null>(null);
	// DEV-055: types 도 노출 — type 변경 UI 에서 사용.
	let types = $state<QuestType[]>([]);
	let statuses = $state<QuestStatus[]>([]);
	// DEV-011: 이 quest 가 속한 캠페인 목록.
	let linkedCampaigns = $state<import('$lib/types').Campaign[]>([]);
	let allCampaigns = $state<import('$lib/types').Campaign[]>([]);
	// BUG-030: 콤보박스 모달 표시 여부. true 면 모달 노출.
	let showCampaignCombo = $state(false);
	let campaignLinkError = $state<string | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// 편집 모드
	let editMode = $state(false);
	// DEV-153: 편집 모드 = 미저장 가능 → 이탈 가드에 보고. 저장/취소/네비 reset 시
	// editMode=false 가 되어 자동 해제. 컴포넌트 파기 시에도 안전하게 정리.
	$effect(() => setUnsaved('quest-edit', editMode));
	onDestroy(() => setUnsaved('quest-edit', false));

	// DEV-156: 편집기의 '첨부' 버튼 / 비미디어 paste·drop 이 본문 인라인 대신 이
	// 콜백으로 '첨부 섹션'에 추가. detail.attachments 가 단일 소스 → 섹션 갱신.
	async function attachToSection(rel: string, name: string) {
		if (!detail) return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			detail.attachments = await invoke('add_quest_attachment', {
				slug: detail.quest_id,
				path: rel,
				name
			});
		} catch (e) {
			saveError = `${t('qd.attachFailed', $locale)}: ${e}`;
		}
	}
	let editTitle = $state('');
	let editUrgency = $state(3);
	let editDescription = $state('');
	// DEV-076: 기한 — YYYY-MM-DD 또는 빈 문자열 (= 미설정 / 해제).
	let editDesiredDue = $state('');
	let editRequiredDue = $state('');
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

	// 콤보박스 / 후보 (DEV-124: succ 추가)
	type ComboMode = 'sub' | 'prereq' | 'succ';
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

	// DEV-074 fix17: 삭제 cascade 모달의 sub-quest list overlay scrollbar.
	let delSubListEl: HTMLUListElement | undefined = $state(undefined);

	// DEV-109 / DEV-123 / DEV-127: floating button cluster — 본문이 길 때 점프.
	// 각 anchor 의 viewport 위치 기준으로 노출 결정.
	let commentsAnchorEl: HTMLDivElement | undefined = $state(undefined);
	let memoAnchorEl: HTMLDivElement | undefined = $state(undefined);
	let showCommentsJump = $state(false);
	let showMemoJump = $state(false);
	let showTopJump = $state(false);
	function checkJumpVisibility() {
		const vh = window.innerHeight;
		// DEV-191: anchor 가 보이는 밴드 [0, 1.1vh] 밖이면 표시 — 아래(top>1.1vh,
		// 내려가기)·위(top<0, 올라가기) 양방향. 메모 영역에서도 '댓글로'가 뜬다.
		if (commentsAnchorEl) {
			const top = commentsAnchorEl.getBoundingClientRect().top;
			showCommentsJump = top > vh * 1.1 || top < 0;
		} else {
			showCommentsJump = false;
		}
		// 메모 (DEV-123 → DEV-191): 댓글과 동일하게 양방향.
		if (memoAnchorEl) {
			const top = memoAnchorEl.getBoundingClientRect().top;
			showMemoJump = top > vh * 1.1 || top < 0;
		} else {
			showMemoJump = false;
		}
		// 맨 위로 (DEV-127): 스크롤이 한 화면 이상 내려가 있으면 표시.
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

	// DEV-192: 스크롤 위치 복원. SvelteKit snapshot 으로 떠날 때 scrollY 를 캡쳐하고
	// 뒤로/앞으로 복귀 시 복원. 단 detail 은 mount 후 async 로 로드되고 마크다운/
	// 이미지/첨부가 이어서 레이아웃되므로, 값을 보관했다가 detail 로드 후
	// restoreScroll(높이 안정까지 재적용) 로 정확히 복원한다.
	let pendingScroll: number | null = null;
	function applyPendingScroll() {
		if (pendingScroll == null || !detail) return;
		const y = pendingScroll;
		pendingScroll = null;
		restoreScroll(y);
	}
	export const snapshot: Snapshot<number> = {
		capture: () => window.scrollY,
		restore: (y) => {
			pendingScroll = y;
			applyPendingScroll();
		}
	};
	// detail 이 (재)로드되면 대기 중인 복원 적용.
	$effect(() => {
		void detail;
		if (detail) applyPendingScroll();
	});

	// 보드에서 정한 레인 순서 우선, 없는 status 는 sort_order 로 뒤에.
	let laneOrder = $state<string[]>([]);
	let sortedStatuses = $derived(
		orderStatusesByLane(statuses, laneOrder, (a, b) => a.sort_order - b.sort_order)
	);

	// DEV-068: tag 정의 — slug → (color, description) lookup.
	let tagDefs = $state<QuestTagDef[]>([]);
	let tagDefMap = $derived(new Map(tagDefs.map((d) => [d.slug, d])));

	function tagStyle(t: string): string {
		const d = tagDefMap.get(t);
		if (!d || !d.color) return '';
		// hex → rgba 변환 (배경 12% / 테두리 40%).
		const c = d.color.trim();
		const hex = c.startsWith('#') ? c.slice(1) : c;
		if (!/^[0-9a-fA-F]{6}$/.test(hex)) return `color: ${c}`;
		const r = parseInt(hex.slice(0, 2), 16);
		const g = parseInt(hex.slice(2, 4), 16);
		const b = parseInt(hex.slice(4, 6), 16);
		return `background: rgba(${r},${g},${b},0.12); border-color: rgba(${r},${g},${b},0.4); color: ${c};`;
	}

	function tagTitle(t: string): string {
		const d = tagDefMap.get(t);
		return d?.description || t;
	}

	// 메타(타입/상태)는 마운트 시 한 번만
	onMount(async () => {
		try {
			const [t, s, td] = await Promise.all([
				metaApi.getQuestTypes(),
				metaApi.getQuestStatuses(),
				adminApi.listTagDefs().catch(() => [] as QuestTagDef[])
			]);
			types = t;
			statuses = s;
			tagDefs = td;
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		}
		// 보드 레인 순서 로드 (길드별 localStorage). 실패/web 이면 빈 배열 → sort_order.
		laneOrder = loadLaneOrder(await resolveGuildKeyPrefix());
	});

	// DEV-109/123/127: window 스크롤 / resize 추적 → 점프 버튼 cluster 노출.
	onMount(() => {
		const handler = () => checkJumpVisibility();
		window.addEventListener('scroll', handler, { passive: true });
		window.addEventListener('resize', handler);
		// 초기 1회.
		checkJumpVisibility();
		return () => {
			window.removeEventListener('scroll', handler);
			window.removeEventListener('resize', handler);
		};
	});

	// detail 이 로드되거나 변경되면 (다른 quest 로 이동 등) 다시 측정.
	$effect(() => {
		void detail;
		// tick 후 DOM 안정화 — anchor 위치 정확.
		queueMicrotask(() => checkJumpVisibility());
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
			.then(async (d) => {
				// 효과 실행 도중 다시 slug 가 바뀌었을 수 있으므로 ID 비교 후 적용
				if (slug !== currentSlug) return;
				detail = d;
				// DEV-011: 연결된 캠페인 + 전체 캠페인 (link UI 자동완성용) 동시 로드.
				try {
					const [linked, all] = await Promise.all([
						campaignsApi.forQuest(d.id),
						campaignsApi.list()
					]);
					if (slug === currentSlug) {
						linkedCampaigns = linked;
						allCampaigns = all;
					}
				} catch {
					/* 무시 — 캠페인 없거나 backend 미지원이어도 detail 자체는 표시 */
				}
			})
			.catch((e) => {
				if (slug === currentSlug) error = e instanceof Error ? e.message : 'failed to load';
			})
			.finally(() => {
				if (slug === currentSlug) loading = false;
			});
	});

	// --- 편집 모드 ---

	// DEV-203: 편집기 생성/파괴/높이 영속/설정·테마 반응은 MarkdownEditor
	// 컴포넌트가 {#if editMode} 수명주기로 자동 처리.
	function enterEditMode() {
		if (!detail) return;
		editTitle = detail.title;
		editUrgency = detail.urgency;
		editDescription = detail.description ?? '';
		// DEV-076: null / undefined → 빈 문자열 (input value).
		editDesiredDue = detail.desired_due ?? '';
		editRequiredDue = detail.required_due ?? '';
		editMode = true;
	}

	function exitEditMode() {
		editMode = false;
		saveError = null;
	}

	async function saveEdit() {
		if (!detail) return;
		saving = true;
		saveError = null;
		try {
			const desc = editDescription;
			await questsApi.update(detail.id, {
				title: editTitle.trim() || detail.title,
				description: desc || undefined,
				urgency: editUrgency
			});
			// DEV-076: 기한 — 빈 문자열 → null (해제), 값 → 설정.
			// 변경 사항이 있을 때만 PATCH 호출 (no-op 절약).
			const wantDesired = editDesiredDue.trim() || null;
			const wantRequired = editRequiredDue.trim() || null;
			const haveDesired = detail.desired_due ?? null;
			const haveRequired = detail.required_due ?? null;
			if (wantDesired !== haveDesired || wantRequired !== haveRequired) {
				const body: { desired_due?: string | null; required_due?: string | null } = {};
				if (wantDesired !== haveDesired) body.desired_due = wantDesired;
				if (wantRequired !== haveRequired) body.required_due = wantRequired;
				await questsApi.setDueDates(detail.id, body);
			}
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
			setTimeout(() => {
				if (statusFlashId === statusId) statusFlashId = null;
			}, 600);
		} catch (e) {
			showToast(e instanceof Error ? e.message : 'status change failed', 'error');
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
			// slug 바뀜 → 새 slug 의 URL 로 navigate. BUG-015 fix1: from 보존.
			await goto(`/quests/${updated.quest_id}${fromSuffix}`, { replaceState: true });
		} catch (e) {
			showToast(e instanceof Error ? e.message : 'type change failed', 'error');
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
			// DEV-124: succ 도 동일 API endpoint (server / Tauri 가 그대로 통과).
			const relation: CandidateRelation = mode;
			candidates = await questsApi.candidates(detail.id, relation);
		} catch (e) {
			comboError = e instanceof Error ? e.message : t('qd.candidateFailed', $locale);
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
			} else if (mode === 'prereq') {
				await questsApi.addPrerequisite(detail.id, questId);
			} else {
				// DEV-124: succ — 이 quest 를 candidate 의 prereq 로 추가.
				await questsApi.addPrerequisite(questId, detail.id);
			}
			detail = await questsApi.getBySlug(slug);
			closeCombo();
		} catch (e) {
			comboError = e instanceof Error ? e.message : t('qd.addFailed', $locale);
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
			showToast(e instanceof Error ? e.message : t('qd.detachFailed', $locale), 'error');
		}
	}

	async function removePrerequisite(prereqId: number) {
		if (!detail) return;
		try {
			await questsApi.removePrerequisite(detail.id, prereqId);
			detail = await questsApi.getBySlug(slug);
		} catch (e) {
			showToast(e instanceof Error ? e.message : 'failed', 'error');
		}
	}

	// --- DEV-068: 태그 ---
	let tagInputOpen = $state(false);
	let newTagText = $state('');
	async function addTagFromInput(e: Event) {
		e.preventDefault();
		if (!detail) return;
		const tokens = newTagText
			.split(/\s+/)
			.map((s) => s.trim())
			.filter((s) => s.length > 0);
		if (tokens.length === 0) return;
		const existing = detail.tags ?? [];
		const merged = [...existing];
		for (const t of tokens) {
			if (!merged.includes(t)) merged.push(t);
		}
		try {
			await questsApi.setTags(detail.id, merged);
			detail = await questsApi.getBySlug(slug);
			newTagText = '';
			tagInputOpen = false;
		} catch (err) {
			showToast(err instanceof Error ? err.message : 'failed', 'error');
		}
	}
	async function removeTag(t: string) {
		if (!detail) return;
		const after = (detail.tags ?? []).filter((x) => x !== t);
		try {
			await questsApi.setTags(detail.id, after);
			detail = await questsApi.getBySlug(slug);
		} catch (err) {
			showToast(err instanceof Error ? err.message : 'failed', 'error');
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
			// BUG-044: origin 으로 복귀 — 하드코딩된 '/' 대신 goBack() 이 ?from
			// query param 분기 (list / board / home / campaign).
			goBack();
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('qd.deleteFailed', $locale), 'error');
			deleting = false;
		}
	}

	// BUG-015 (fix1) + DEV-011: query parameter `?from=list|board|home|campaign:<slug>`
	// 로 명시 origin 추적. SvelteKit / Tauri WebView 의 history stack 동작
	// 불확실 → URL query 가 신뢰 가능.
	function goBack() {
		// DEV-192 (B-2a): 앱 내 히스토리가 있으면 실제 뒤로가기 → 직전 페이지의
		// snapshot(스크롤) 복원이 발동한다. goto(push) 는 새 엔트리라 복원 안 됨.
		// 직접 진입(히스토리 없음, length<=1) 은 아래 ?from 분기로 fallback.
		if (window.history.length > 1) {
			window.history.back();
			return;
		}
		const from = $page.url.searchParams.get('from');
		if (from === 'list') {
			goto('/?view=list');
		} else if (from === 'board') {
			goto('/?view=board');
		} else if (from === 'home') {
			goto('/');
		} else if (from && from.startsWith('campaign:')) {
			const slug = from.slice('campaign:'.length);
			goto(`/campaigns/${encodeURIComponent(slug)}`);
		} else {
			// 외부 link 직접 진입 / parent 추적 안 된 경우 — Home 으로.
			goto('/');
		}
	}

	/* BUG-021 fix1: renderMarkdown 직접 호출 제거 — MarkdownView 컴포넌트 사용. */

	// DEV-011: Campaign 연결 / 해제
	// BUG-030: native datalist 입력 → 콤보박스 모달 (sub/prereq 와 동일 패턴).
	function openCampaignCombo() {
		campaignLinkError = null;
		showCampaignCombo = true;
	}
	function closeCampaignCombo() {
		showCampaignCombo = false;
		campaignLinkError = null;
	}
	// 콤보박스 후보 — 이미 연결된 캠페인은 제외 (선택해도 의미 없음).
	let campaignCandidates = $derived.by(() => {
		const linkedIds = new Set(linkedCampaigns.map((c) => c.id));
		return allCampaigns.filter((c) => !linkedIds.has(c.id));
	});

	async function linkCampaign(slug: string) {
		if (!detail) return;
		try {
			await campaignsApi.linkQuest(slug, detail.quest_id);
			linkedCampaigns = await campaignsApi.forQuest(detail.id);
			closeCampaignCombo();
		} catch (e) {
			campaignLinkError = e instanceof Error ? e.message : t('qd.campaignLinkFailed', $locale);
		}
	}

	async function unlinkCampaign(campaignSlug: string) {
		if (!detail) return;
		try {
			await campaignsApi.unlinkQuest(campaignSlug, detail.quest_id);
			linkedCampaigns = await campaignsApi.forQuest(detail.id);
		} catch (e) {
			showToast(e instanceof Error ? e.message : t('qd.campaignUnlinkFailed', $locale), 'error');
		}
	}
</script>

<div class="container">
	<div class="top-bar">
		<!-- BUG-015: history.back() 으로 직전 페이지 (List 또는 Board) 복귀.
		     history 가 비어있으면 (외부 link 직접 진입) Board 로 fallback.
		     DEV-255: 자식윈도우(단일 문서 보기)는 돌아갈 곳이 없음 — 숨김. -->
		{#if !$isChildWindow}
			<button class="back" type="button" onclick={goBack}>← {t('detail.back', $locale)}</button>
		{/if}
		{#if detail && !editMode}
			<div class="top-actions">
				<button class="btn-edit" onclick={enterEditMode}>✎ {t('detail.edit', $locale)}</button>
				<button class="btn-delete" onclick={openDeleteModal}>🗑 {t('detail.delete', $locale)}</button>
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
			<span class="badge urgency" style:--c={urgencyColor(detail.urgency)}>
				{urgencyLabel(detail.urgency)}
			</span>
			{#if urgencyOutOfRange(detail.urgency)}
				<!-- BUG-060 후속: 원본 urgency 가 범위(1-4) 밖 — clamp 표시 + 경고. -->
				<span
					class="urgency-warn"
					title={`${t('qd.urgencyClampPre', $locale)}${detail.urgency}${t('qd.urgencyClampPost', $locale)}`}
					>{t('qd.outOfRange', $locale)}</span
				>
			{/if}
			{#key badgePulse}
				<span class="badge status pulsing" style:--c={detail.status_color}>
					{questStatusLabel(detail, $locale)}
				</span>
			{/key}
		</div>

		<!-- 생성 / 변경 시각 -->
		<div class="meta-times">
			<span class="meta-item">
				<span class="meta-label">{t('common.created', $locale)}</span>
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
				<span class="meta-label">{t('common.updated', $locale)}</span>
				<time
					class="meta-val"
					datetime={detail.updated_at}
					title={formatTs(detail.updated_at)}
					data-testid="updated-at"
				>
					{formatRelative(detail.updated_at, undefined, $locale)}
				</time>
			</span>
			{#if detail.desired_due || detail.required_due}
				<span class="meta-sep">·</span>
				<!-- DEV-076: 기한 표시 — desired / required 둘 다 있으면 둘 다. -->
				<!-- DEV-079: 기한 지났으면 빨강 강조 (done/cancelled 제외). -->
				{#if detail.required_due}
					<span class="meta-item">
						<span class="meta-label">{t('qd.requiredDue', $locale)}</span>
						<span
							class="meta-val due-required"
							class:overdue={isDateOverdue(detail.required_due, detail.status_slug)}
							>{detail.required_due}</span
						>
					</span>
				{/if}
				{#if detail.desired_due}
					<span class="meta-item">
						<span class="meta-label">{t('qd.desiredDue', $locale)}</span>
						<span
							class="meta-val due-desired"
							class:overdue={isDateOverdue(detail.desired_due, detail.status_slug)}
							>{detail.desired_due}</span
						>
					</span>
				{/if}
			{/if}
		</div>

		{#if editMode}
			<div class="edit-form">
				<label class="field-label">
					<span>{t('qd.titleLabel', $locale)}</span>
					<input class="edit-title" type="text" bind:value={editTitle} />
				</label>

				<div class="field-label">
					<span>{t('filter.urgency', $locale)}</span>
					<!-- DEV-287: 드롭다운 대신 pill 버튼 — 타입 변경 UI 와 통일.
					     선택만 바꾸고 적용은 기존대로 저장 시. -->
					<div class="status-btns">
						{#each [1, 2, 3, 4] as u}
							<button
								type="button"
								class="status-btn"
								class:active={u === editUrgency}
								style:--c={urgencyColor(u)}
								onclick={() => (editUrgency = u)}
							>
								{URGENCY_LABEL[u]}
							</button>
						{/each}
					</div>
				</div>

				<!-- DEV-055 → DEV-133: 타입 변경 — 편집 모드에서만 노출 (사용자 요청).
				     slug 가 바뀌는 무거운 동작이라 일반 보기에서 한 클릭 거리는 과함.
				     기존과 동일하게 confirm 모달 후 즉시 적용 (저장 버튼과 무관). -->
				<div class="field-label">
					<span>{t('qd.typeChange', $locale)} <span class="hint">{t('qd.typeChangeHint', $locale)}</span></span>
					<div class="status-btns">
						{#each types as ty}
							<button
								class="status-btn"
								class:active={ty.id === detail.quest_type_id}
								style:--c={ty.color}
								onclick={() => askChangeType(ty)}
								disabled={changingType || ty.id === detail.quest_type_id}
								title={ty.id === detail.quest_type_id
									? t('qd.currentType', $locale)
									: `${ty.prefix}${t('qd.changeToSuffix', $locale)}`}
							>
								{ty.prefix}
							</button>
						{/each}
					</div>
				</div>

				<!-- DEV-076: 희망 / 필수 기한. 빈 값 = 미설정 / 해제. -->
				<div class="due-row">
					<label class="field-label">
						<span>{t('qd.desiredDue', $locale)} <span class="hint">{t('qd.desiredDueHint', $locale)}</span></span>
						<DateField bind:value={editDesiredDue} />
					</label>
					<label class="field-label">
						<span>{t('qd.requiredDue', $locale)} <span class="hint">{t('qd.requiredDueHint', $locale)}</span></span>
						<DateField bind:value={editRequiredDue} />
					</label>
				</div>

				<!-- CodeMirror 가 div 안에 textarea 를 동적으로 생성 — svelte 가 정적
				     분석으로는 control 미포함으로 판단. ignore. -->
				<!-- DEV-202: 편집기 위 '첨부' 버튼 제거 — 아래 첨부 섹션과 중복.
				     이미지·동영상·파일은 드래그&드랍 / Ctrl+V 로 첨부(attachmentExtension). -->
				<div class="field-label">
					<span>{t('qd.descLabel', $locale)}</span>
					<MarkdownEditor
						bind:value={editDescription}
						onError={(msg) => (saveError = `${t('qd.attachUploadFailed', $locale)}: ${msg}`)}
						onAttach={attachToSection}
					/>
				</div>

				{#if saveError}<p class="save-error">{saveError}</p>{/if}

				<div class="edit-actions">
					<button class="btn-save" onclick={saveEdit} disabled={saving}>
						{saving ? t('common.saving', $locale) : t('common.save', $locale)}
					</button>
					<button class="btn-cancel" onclick={exitEditMode} disabled={saving}>{t('common.cancel', $locale)}</button>
				</div>
			</div>
		{:else}
			<h1 class="title">{detail.title}</h1>

			<!-- 상태 변경 -->
			<div class="status-row">
				<span class="branch-label">{t('qd.statusChange', $locale)}</span>
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
							{#if s.id === statusFlashId}✓
							{/if}{statusLabel(s, $locale)}
						</button>
					{/each}
				</div>
			</div>

			<!-- BUG-031: description 블록 / no-desc 와 아래 sections (Parent /
			     Sub / Prereq / Campaigns) 사이가 너무 좁아 시각적으로 겹쳐 보임.
			     wrapper 로 명확한 간격 부여. -->
			<div class="description-block">
				{#if detail.description}
					<MarkdownView source={detail.description} />
				{:else}
					<p class="no-desc">
						{t('qd.noDescription', $locale)} <button class="link-btn" onclick={enterEditMode}>{t('qd.addDescription', $locale)}</button>
					</p>
				{/if}
			</div>
		{/if}

		<!-- DEV-156: 본문 아래 첨부 섹션 (Jira 식). -->
		<AttachmentSection slug={detail.quest_id} scope="quest" bind:attachments={detail.attachments} />

		<!-- 부모 퀘스트 (DEV-050) -->
		{#if detail.parent}
			<section>
				<div class="section-head">
					<h2 class="section-title parent-label">{t('quest.section.parent', $locale)}</h2>
				</div>
				<ul class="quest-list">
					<li>
						<div class="prereq-row">
							<a href="/quests/{detail.parent.quest_id}{fromSuffix}" class="prereq-link">
								<span class="badge type" style:--c={detail.parent.type_color}
									>{detail.parent.quest_id}</span
								>
								<span class="ql-title">{detail.parent.title}</span>
								<span class="badge status" style:--c={detail.parent.status_color}
									>{questStatusLabel(detail.parent, $locale)}</span
								>
							</a>
						</div>
					</li>
				</ul>
			</section>
		{/if}

		<!-- 서브퀘스트 -->
		<section>
			<div class="section-head">
				<h2 class="section-title sub-label">{t('quest.section.subQuests', $locale)}</h2>
				{#if !editMode}
					<button class="sec-add-btn" onclick={() => (showNewSubQuest = true)}>{t('qd.newSub', $locale)}</button>
					<button class="sec-add-btn" onclick={() => openCombo('sub')}>{t('qd.assignExisting', $locale)}</button>
				{/if}
			</div>
			{#if detail.sub_quests.length > 0}
				<ul class="quest-list">
					{#each detail.sub_quests as sq (sq.id)}
						<li>
							<div class="prereq-row">
								<a href="/quests/{sq.quest_id}{fromSuffix}" class="prereq-link">
									<span class="badge type" style:--c={sq.type_color}>{sq.quest_id}</span>
									<span class="ql-title">{sq.title}</span>
									<span class="badge status" style:--c={sq.status_color}>{questStatusLabel(sq, $locale)}</span>
								</a>
								{#if !editMode}
									<button
										class="prereq-rm"
										title={t('qd.detachFromParent', $locale)}
										onclick={() => detachSubQuest(sq.id)}>×</button
									>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="no-desc">{t('qd.noSubQuests', $locale)}</p>
			{/if}
		</section>

		<!-- 선행 퀘스트 -->
		<section>
			<div class="section-head">
				<h2 class="section-title prereq-label">{t('quest.section.prerequisites', $locale)}</h2>
				{#if !editMode}
					<button class="sec-add-btn" onclick={() => openCombo('prereq')}>{t('qd.addBtn', $locale)}</button>
				{/if}
			</div>

			{#if detail.prerequisites.length > 0}
				<ul class="quest-list">
					{#each detail.prerequisites as pq (pq.id)}
						<li>
							<div class="prereq-row">
								<a href="/quests/{pq.quest_id}{fromSuffix}" class="prereq-link">
									<span class="badge type" style:--c={pq.type_color}>{pq.quest_id}</span>
									<span class="ql-title">{pq.title}</span>
									<span class="badge status" style:--c={pq.status_color}>{questStatusLabel(pq, $locale)}</span>
								</a>
								{#if !editMode}
									<button
										class="prereq-rm"
										title={t('qd.removePrereq', $locale)}
										onclick={() => removePrerequisite(pq.id)}>×</button
									>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="no-desc">{t('qd.noPrereqs', $locale)}</p>
			{/if}
		</section>

		<!-- DEV-070: 후속 퀘스트 — 본 quest 를 선행으로 가진 quest 들 (역방향
			참조). DEV-124: 추가 버튼. -->
		<section>
			<div class="section-head">
				<h2 class="section-title succ-label">{t('quest.section.successors', $locale)}</h2>
				<span class="sec-hint">{t('quest.section.successorsHint', $locale)}</span>
				{#if !editMode}
					<button class="sec-add-btn" onclick={() => openCombo('succ')} title={t('qd.addSuccessor', $locale)}>
						{t('qd.addBtn', $locale)}
					</button>
				{/if}
			</div>
			{#if (detail.successors ?? []).length > 0}
				<ul class="quest-list">
					{#each detail.successors ?? [] as sq (sq.id)}
						<li>
							<div class="prereq-row">
								<a href="/quests/{sq.quest_id}{fromSuffix}" class="prereq-link">
									<span class="badge type" style:--c={sq.type_color}>{sq.quest_id}</span>
									<span class="ql-title">{sq.title}</span>
									<span class="badge status" style:--c={sq.status_color}>{questStatusLabel(sq, $locale)}</span>
								</a>
							</div>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="no-desc">{t('qd.noSuccessors', $locale)}</p>
			{/if}
		</section>

		<!-- DEV-011: 연결된 캠페인 -->
		<section>
			<div class="section-head">
				<h2 class="section-title campaign-label">{t('quest.section.campaigns', $locale)}</h2>
				<!-- BUG-031: 버튼 배치를 sub-quest / prereq 와 동일하게 — title 옆 -->
				{#if !editMode}
					<button
						class="sec-add-btn"
						onclick={openCampaignCombo}
						disabled={campaignCandidates.length === 0}
						title={campaignCandidates.length === 0
							? t('qd.noLinkableCampaigns', $locale)
							: t('qd.selectCampaign', $locale)}>{t('qd.linkBtn', $locale)}</button
					>
				{/if}
			</div>
			{#if linkedCampaigns.length > 0}
				<ul class="quest-list">
					{#each linkedCampaigns as c (c.id)}
						<li>
							<div class="prereq-row">
								<a href={`/campaigns/${encodeURIComponent(c.campaign_slug)}`} class="prereq-link">
									<span class="badge type campaign-badge">{c.campaign_slug}</span>
									<span class="ql-title">{c.title}</span>
									<span class="badge status status-{c.status}">{c.status}</span>
								</a>
								{#if !editMode}
									<button
										class="prereq-rm"
										title={t('qd.unlinkCampaign', $locale)}
										onclick={() => unlinkCampaign(c.campaign_slug)}>×</button
									>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="no-desc">{t('qd.noLinkedCampaigns', $locale)}</p>
			{/if}
		</section>

		<!-- DEV-068: 태그 — frontmatter 가 진리원. inline 편집 가능.
		     DEV-205: 관계 섹션(선행/후속) 사이에 끼어 있던 위치를 관계 섹션
		     뒤(메타)로 이동. 루프 변수는 번역 t() 와 충돌 않게 tag 로. -->
		<section>
			<div class="section-head">
				<h2 class="section-title tag-label">{t('quest.section.tags', $locale)}</h2>
				{#if !editMode}
					<button class="sec-add-btn" onclick={() => (tagInputOpen = !tagInputOpen)}>
						{tagInputOpen ? t('common.cancel', $locale) : t('quest.tags.add', $locale)}
					</button>
				{/if}
			</div>
			{#if (detail.tags ?? []).length > 0}
				<ul class="tag-pills">
					{#each detail.tags ?? [] as tag (tag)}
						<li>
							<span class="tag-pill" style={tagStyle(tag)} title={tagTitle(tag)}>
								{tag}
								{#if !editMode}
									<button
										class="tag-rm"
										title={t('quest.tags.remove', $locale)}
										onclick={() => removeTag(tag)}
										aria-label={`${t('quest.tags.remove', $locale)}: ${tag}`}>×</button
									>
								{/if}
							</span>
						</li>
					{/each}
				</ul>
			{:else if !tagInputOpen}
				<p class="no-desc">{t('quest.tags.none', $locale)}</p>
			{/if}
			{#if tagInputOpen && !editMode}
				<form class="tag-add-form" onsubmit={addTagFromInput}>
					<input
						type="text"
						bind:value={newTagText}
						placeholder={t('quest.tags.placeholder', $locale)}
						aria-label={t('quest.tags.newAria', $locale)}
					/>
					<button type="submit" disabled={!newTagText.trim()}>{t('quest.tags.addSubmit', $locale)}</button>
				</form>
			{/if}
		</section>

		<!-- DEV-012: 공개 댓글 + 비공개 메모. quest slug 기준. -->
		<!-- DEV-109: 본문이 길 때 floating 버튼이 이 anchor 로 점프. -->
		<div bind:this={commentsAnchorEl} id="comments-anchor"></div>
		<QuestCommentsSection slug={detail.quest_id} />
		<!-- DEV-123: 메모 점프 anchor. -->
		<div bind:this={memoAnchorEl} id="memo-anchor"></div>
		<QuestNoteSection slug={detail.quest_id} mode="memo" />

		<!-- 변경 이력 (DEV-038) -->
		{#key `${detail.id}:${historyVersion}`}
			<QuestHistory questId={detail.id} {statuses} />
		{/key}
	{/if}
</div>

<!-- 콤보박스 모달 -->
{#if comboMode && detail}
	<!-- BUG-160: 바깥(백드롭) 클릭으로 닫기 — ConfirmDialog 와 동일 패턴.
	     e.target === e.currentTarget 가드가 핵심: 모달 내부 클릭이 버블링돼
	     닫히는 걸 막는다. -->
	<div class="ov" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) closeCombo(); }}>
		<div class="modal-sm modal-combo" role="dialog" aria-modal="true" tabindex="-1">
			<div class="modal-head">
				<h3>
					{#if comboMode === 'sub'}{t('qd.comboSub', $locale)}{:else if comboMode === 'prereq'}{t('qd.comboPrereq', $locale)}{:else}{t('qd.comboSuccessor', $locale)}{/if}
				</h3>
				<button class="x" onclick={closeCombo}>×</button>
			</div>
			{#if candidatesLoading}
				<div class="combo-state">{t('qd.loadingCandidates', $locale)}</div>
			{:else}
				<QuestCombobox
					quests={candidates}
					placeholder={t('qd.searchByIdTitle', $locale)}
					onselect={pickCandidate}
					oncancel={closeCombo}
				/>
			{/if}
			{#if comboError}<p class="combo-err">{comboError}</p>{/if}
		</div>
	</div>
{/if}

<!-- BUG-030: 캠페인 연결 콤보박스 모달 (sub/prereq 와 동일 패턴) -->
{#if showCampaignCombo && detail}
	<div class="ov" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) closeCampaignCombo(); }}>
		<div class="modal-sm modal-combo" role="dialog" aria-modal="true" tabindex="-1">
			<div class="modal-head">
				<h3>{t('qd.linkCampaignTitle', $locale)}</h3>
				<button class="x" onclick={closeCampaignCombo}>×</button>
			</div>
			<CampaignCombobox
				campaigns={campaignCandidates}
				placeholder={t('qd.campaignSearchPlaceholder', $locale)}
				onselect={linkCampaign}
				oncancel={closeCampaignCombo}
			/>
			{#if campaignLinkError}<p class="combo-err">{campaignLinkError}</p>{/if}
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
	<!-- BUG-160: 바깥 클릭 = 취소. 단 변경이 진행 중이면 닫지 않는다
	     (요청은 계속 날아가는데 UI 만 사라져 결과를 못 보는 상태 방지). -->
	<div class="ov" role="presentation" onclick={(e) => { if (e.target === e.currentTarget && !changingType) confirmTypeChange = null; }}>
		<div class="modal-sm" role="dialog" aria-modal="true" tabindex="-1">
			<div class="modal-head">
				<h3 class="del-title">{t('qd.changeTypeTitle', $locale)}</h3>
				<button class="x" onclick={() => (confirmTypeChange = null)} disabled={changingType}
					>×</button
				>
			</div>
			<p class="del-msg">
				<code>{detail.quest_id}</code>{t('qd.changeTypeMsg1', $locale)}<strong>{target.prefix}</strong>{t('qd.changeTypeMsg2', $locale)}
				<code>{target.prefix}-NNN</code>{t('qd.changeTypeMsg3', $locale)}
			</p>
			<p class="del-prereq">
				{t('qd.changeTypeWarnPre', $locale)}<code>{detail.quest_id}</code>{t('qd.changeTypeWarnPost', $locale)}
				{t('qd.autoBlockNote', $locale)}
				{t('qd.autoUpdated', $locale)}
			</p>
			<!-- DEV-133: 타입 변경이 편집 모드 안으로 이동 — 즉시 적용 + 새 slug
			     로 navigate 되므로 저장 안 한 제목/설명 편집은 유지되지 않음. -->
			{#if editMode}
				<p class="del-prereq">
					{t('qd.immediateNavWarn', $locale)}<strong
						>{t('qd.unsavedWarnStrong', $locale)}</strong
					>{t('qd.unsavedWarnRest', $locale)}
				</p>
			{/if}
			<div class="del-actions">
				<button class="btn-del-yes" onclick={doChangeType} disabled={changingType}>
					{changingType ? t('qd.changing', $locale) : t('common.change', $locale)}
				</button>
				<button
					class="btn-del-no"
					onclick={() => (confirmTypeChange = null)}
					disabled={changingType}
				>
					{t('common.cancel', $locale)}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- 삭제 모달 -->
{#if deleteModal && detail}
	<!-- BUG-160: 바깥 클릭 = 취소. 삭제 진행 중엔 닫지 않음(위와 동일 이유). -->
	<div class="ov" role="presentation" onclick={(e) => { if (e.target === e.currentTarget && !deleting) deleteModal = false; }}>
		<div class="modal-sm" role="dialog" aria-modal="true" tabindex="-1">
			<div class="modal-head">
				<h3 class="del-title">{detail.quest_id} {t('detail.delete', $locale)}</h3>
				<button class="x" onclick={() => (deleteModal = false)} disabled={deleting}>×</button>
			</div>
			<p class="del-msg">{t('quest.delete.msg', $locale)}</p>
			{#if detail.sub_quests.length > 0}
				<div class="del-sub">
					<div class="del-sub-head">
						<p class="del-sub-title">{t('quest.delete.subTitle', $locale)}</p>
						<label class="del-sub-all">
							<input
								type="checkbox"
								checked={cascadeSet.size === detail.sub_quests.length}
								indeterminate={cascadeSet.size > 0 && cascadeSet.size < detail.sub_quests.length}
								onchange={toggleAllCascade}
								data-testid="cascade-all"
							/>
							<span>{t('quest.delete.selectAll', $locale)}</span>
						</label>
					</div>
					<p class="del-sub-help">
						{t('quest.delete.subHelp', $locale)}
					</p>
					<ul class="del-sub-list" bind:this={delSubListEl}>
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
					{#if delSubListEl}
						<OverlayScrollbar target={delSubListEl} />
					{/if}
				</div>
			{/if}
			<p class="del-prereq">{t('quest.delete.prereqNote', $locale)}</p>
			<!-- 버튼 순서: [취소][삭제] — ConfirmDialog(캠페인 상세 등)와 통일. -->
			<div class="del-actions">
				<button class="btn-del-no" onclick={() => (deleteModal = false)} disabled={deleting}
					>{t('common.cancel', $locale)}</button
				>
				<button
					class="btn-del-yes"
					onclick={confirmDelete}
					disabled={deleting}
					data-testid="confirm-delete"
				>
					{deleting ? t('quest.delete.deleting', $locale) : t('detail.delete', $locale)}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- DEV-109/123/127: 우하단 floating 점프 버튼 cluster. -->
{#if detail && (showTopJump || showCommentsJump || showMemoJump)}
	<div class="jump-cluster">
		{#if showTopJump}
			<button class="jump-btn" onclick={jumpToTop} title={t('common.jumpTop', $locale)} aria-label={t('common.jumpTop', $locale)}>
				<span class="jb-icon">↑</span>
				<span class="jb-label">{t('common.jumpTopShort', $locale)}</span>
			</button>
		{/if}
		{#if showCommentsJump}
			<button
				class="jump-btn"
				onclick={jumpToComments}
				title={t('common.jumpComments', $locale)}
				aria-label={t('common.jumpComments', $locale)}
			>
				<span class="jb-icon">💬</span>
				<span class="jb-label">{t('common.jumpCommentsShort', $locale)}</span>
			</button>
		{/if}
		{#if showMemoJump}
			<button class="jump-btn" onclick={jumpToMemo} title={t('common.jumpMemo', $locale)} aria-label={t('common.jumpMemo', $locale)}>
				<span class="jb-icon">📝</span>
				<span class="jb-label">{t('common.jumpMemoShort', $locale)}</span>
			</button>
		{/if}
	</div>
{/if}

<style>
	.container {
		max-width: var(--content-max-width, 800px);
		margin: 0 auto;
		padding: 1.5rem;
	}

	.top-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 1.5rem;
	}

	/* BUG-015: anchor → button 으로 변경. button 기본 스타일 제거. */
	.back {
		font-size: 0.875rem;
		color: var(--text-muted);
		text-decoration: none;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		font-family: inherit;
	}
	.back:hover {
		color: var(--text);
	}

	.top-actions {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.btn-edit {
		padding: 0.3rem 0.9rem;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg-subtle);
		color: var(--text-muted);
		font-size: 0.8rem;
		cursor: pointer;
		transition:
			background 0.1s,
			color 0.1s;
	}
	.btn-edit:hover {
		background: var(--border);
		color: var(--text);
	}

	.btn-delete {
		padding: 0.3rem 0.9rem;
		border: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
		border-radius: 6px;
		background: transparent;
		color: var(--danger);
		font-size: 0.8rem;
		cursor: pointer;
		transition: background 0.1s;
	}
	.btn-delete:hover {
		background: rgba(233, 79, 79, 0.1);
	}

	.state-msg {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 60vh;
		color: var(--text-faint);
		font-size: 0.9rem;
	}
	.state-msg.error {
		color: var(--danger);
	}

	.header {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
		margin-bottom: 0.75rem;
	}

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

	.title {
		font-size: 1.4rem;
		font-weight: 600;
		color: var(--text-strong);
		margin: 0 0 1rem;
		line-height: 1.4;
	}

	.branch-label {
		font-size: 0.75rem;
		color: var(--text-muted);
		flex-shrink: 0;
	}

	.status-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
		margin-bottom: 1.25rem;
		padding: 0.5rem 0.75rem;
		background: var(--bg-elevated);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
	}
	.status-btns {
		display: flex;
		gap: 0.4rem;
		flex-wrap: wrap;
	}
	.status-btn {
		padding: 0.15rem 0.7rem;
		border-radius: 20px;
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
		background: transparent;
		color: color-mix(in srgb, var(--c) 70%, var(--text-muted));
		font-size: 0.75rem;
		cursor: pointer;
		transition:
			background 0.12s,
			color 0.12s,
			transform 0.12s;
	}
	.status-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--c) 15%, transparent);
		color: var(--c);
	}
	.status-btn.active {
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		font-weight: 600;
		cursor: default;
	}
	.status-btn:disabled:not(.active) {
		opacity: 0.5;
		cursor: default;
	}
	.status-btn.flash {
		background: color-mix(in srgb, var(--c) 32%, transparent);
		color: var(--c);
		font-weight: 600;
		transform: scale(1.04);
	}

	/* 헤더 상태 뱃지 펄스 */
	.badge.pulsing {
		animation: pulseBadge 0.8s ease-out;
	}
	@keyframes pulseBadge {
		0% {
			box-shadow: 0 0 0 0 var(--c);
		}
		60% {
			box-shadow: 0 0 0 6px color-mix(in srgb, var(--c) 0%, transparent);
		}
		100% {
			box-shadow: 0 0 0 0 transparent;
		}
	}

	/* BUG-021 fix1: .md-body CSS 는 공유 컴포넌트 MarkdownView 로 이동.
	   캠페인과 동일 스타일 — 헤더 사이즈 = 브라우저 기본 (헤더 명확 구분). */

	.no-desc {
		color: var(--text-faint);
		font-size: 0.9rem;
		margin: 0 0 1.5rem;
	}
	.link-btn {
		background: none;
		border: none;
		color: var(--accent);
		font-size: 0.9rem;
		cursor: pointer;
		padding: 0;
		text-decoration: underline;
	}

	.edit-form {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin-bottom: 1.5rem;
	}
	/* BUG-010: text-transform / letter-spacing 을 label 전체가 아닌 라벨 텍스트
	   span 에만 적용 — 그렇지 않으면 자식 input / CodeMirror 까지 캐스케이드
	   되어 입력값이 대문자로 보임. */
	.field-label {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--text-muted);
		margin-top: 0.5rem;
	}
	.field-label > span:first-child {
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.edit-title {
		padding: 0.5rem 0.75rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-strong);
		font-size: 1rem;
		outline: none;
		width: 100%;
		box-sizing: border-box;
	}
	.edit-title:focus {
		border-color: var(--accent);
	}
	.edit-select {
		padding: 0.4rem 0.6rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.875rem;
		outline: none;
		width: 160px;
	}
	.edit-select:focus {
		border-color: var(--accent);
	}
	/* DEV-076: 기한 입력. select 와 동일 스타일. */
	.due-row {
		display: flex;
		gap: 1rem;
		flex-wrap: wrap;
	}
	.due-row .field-label {
		flex: 1 1 200px;
	}
	.edit-date {
		padding: 0.4rem 0.6rem;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.875rem;
		outline: none;
		font-family: inherit;
		/* BUG-031: color-scheme: dark 를 추가하면 picker icon 이 흰색 렌더 →
		   global.css 의 filter:invert(0.85) 가 다시 검정으로 invert 함. 즉
		   글로벌 fix 와 충돌. 어두운 입력 배경은 background 색만으로 충분 — 별도
		   color-scheme 지정 금지. */
	}
	.edit-date:focus {
		border-color: var(--accent);
	}
	.field-label .hint {
		color: var(--text-faint);
		font-weight: 400;
		font-size: 0.8em;
	}
	.due-required {
		color: var(--orange);
		font-weight: 600;
	}
	.due-desired {
		color: var(--accent);
		font-weight: 500;
	}
	/* DEV-079: overdue 는 강한 빨강 + 굵게. desired / required 공통. */
	.due-required.overdue,
	.due-desired.overdue {
		color: var(--danger);
		font-weight: 700;
	}
	/* DEV-203: .editor-wrap CSS 는 공통 MarkdownEditor 컴포넌트로 이동. */
	.save-error {
		color: var(--danger);
		font-size: 0.8rem;
		margin: 0;
	}
	.edit-actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
	.btn-save {
		padding: 0.4rem 1.2rem;
		background: var(--btn-primary-bg);
		border: 1px solid var(--btn-primary-border);
		border-radius: 6px;
		color: var(--btn-primary-text);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-save:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
	.btn-save:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.btn-cancel {
		padding: 0.4rem 1rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-cancel:hover:not(:disabled) {
		background: var(--bg-subtle);
	}

	.section-head {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 0.5rem;
	}
	.section-title {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin: 0;
	}
	/* DEV-050: 라벨별 색 — QuestBoard 하이라이트 / CLI 의 quest show 와 일치. */
	.section-title.parent-label {
		color: var(--success);
	}
	.section-title.sub-label {
		color: var(--hl-sub);
	}
	.section-title.prereq-label {
		color: var(--hl-pre);
	}
	/* DEV-070/DEV-205: 후속 퀘스트 — QuestBoard/CLI 의 successor(--hl-next) 색.
	   이전엔 prereq-label 을 재사용해 선행과 색이 같았다(사용자 보고). */
	.section-title.succ-label {
		color: var(--hl-next);
	}
	/* DEV-070: section header 옆의 부가 설명 hint. */
	.sec-hint {
		font-size: 0.75rem;
		color: var(--text-faint);
		font-style: italic;
	}

	/* DEV-068: 태그 섹션. */
	.section-title.tag-label {
		color: var(--warning);
	}
	.tag-pills {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
	}
	.tag-pill {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.15rem 0.6rem;
		background: rgba(198, 144, 38, 0.12);
		border: 1px solid rgba(198, 144, 38, 0.4);
		border-radius: 20px;
		font-size: 0.75rem;
		color: var(--warning);
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		letter-spacing: 0.02em;
	}
	.tag-rm {
		border: none;
		background: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 1rem;
		line-height: 1;
		padding: 0 0 0 2px;
	}
	.tag-rm:hover {
		color: var(--danger);
	}
	.tag-add-form {
		display: flex;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}
	.tag-add-form input {
		flex: 1;
		padding: 0.3rem 0.6rem;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.85rem;
	}
	.tag-add-form button {
		padding: 0.3rem 0.85rem;
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text);
		font-size: 0.8rem;
		cursor: pointer;
	}
	.tag-add-form button:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
	.tag-add-form button:hover:not(:disabled) {
		background: var(--border);
	}
	/* DEV-011: Campaign section */
	.section-title.campaign-label {
		color: var(--accent);
	}
	/* BUG-021: campaign slug badge — quest type badge 와 동일 pill 패턴 (color-mix). */
	.campaign-badge {
		--c: var(--accent);
	}
	.badge.status.status-active {
		--c: var(--success);
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	.badge.status.status-done {
		--c: var(--text-muted);
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
	}
	/* BUG-030 + BUG-031: campaign-add wrapper 도 제거됨 — 버튼은 .section-head
	   안으로 이동 (sub-quest / prereq 와 동일 배치). */
	.sec-add-btn {
		padding: 0.15rem 0.6rem;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: transparent;
		color: var(--text-muted);
		font-size: 0.72rem;
		cursor: pointer;
	}
	.sec-add-btn:hover {
		background: var(--bg-subtle);
		color: var(--text);
	}

	section {
		margin-bottom: 1.5rem;
	}
	/* BUG-031 → BUG-033: 본문과 첫 section (Parent / Sub-Quests) 사이가 여전히
	   좁다는 피드백. margin → padding 으로 변경 (collapse 회피) + border-top
	   으로 시각 구분선. 첫 section 윗쪽에도 padding-top 동시 적용. */
	/* BUG-031 후속: 본문 아래에 첨부 섹션이 들어오며 이 큰 padding 이 본문↔첨부
	   간격만 과하게 벌렸음. 첨부 섹션이 자체 구분선/여백을 가지므로 본문 아래는 좁게. */
	.description-block {
		padding-bottom: 0.5rem;
		margin-bottom: 0;
	}
	.description-block + section,
	.description-block ~ section:first-of-type {
		padding-top: 0;
	}

	.quest-list {
		list-style: none;
		padding: 0;
		margin: 0;
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		overflow: hidden;
	}
	.quest-list li + li {
		border-top: 1px solid var(--bg-subtle);
	}

	.prereq-row {
		display: flex;
		align-items: center;
		padding: 0;
	}
	.prereq-link {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		flex: 1;
		padding: 0.55rem 1rem;
		text-decoration: none;
		transition: background 0.1s;
	}
	.prereq-link:hover {
		background: var(--bg-elevated);
	}
	.prereq-rm {
		padding: 0.35rem 0.75rem;
		background: none;
		border: none;
		color: var(--text-faint);
		font-size: 1rem;
		cursor: pointer;
		transition: color 0.1s;
		flex-shrink: 0;
	}
	.prereq-rm:hover {
		color: var(--danger);
	}

	.ql-title {
		flex: 1;
		font-size: 0.875rem;
		color: var(--text);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

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
	/* BUG-060 후속: 범위 밖 urgency 경고 배지. */
	.urgency-warn {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		padding: 0.15rem 0.55rem;
		border-radius: 20px;
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--danger);
		background: color-mix(in srgb, var(--danger) 14%, transparent);
		border: 1px solid color-mix(in srgb, var(--danger) 40%, transparent);
		cursor: help;
	}

	/* --- 모달 (콤보박스 / 삭제) --- */
	.ov {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 100;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1rem;
	}
	.modal-sm {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 10px;
		width: 100%;
		/* BUG-160: 뷰포트보다 크면 창을 따라 줄어들도록 vw 상한 + 높이 상한 —
		   작은 창에서 팝업이 그대로 커서 화면에 꽉 차던 문제. */
		max-width: min(calc(30rem * var(--popup-scale, 1)), 92vw); /* BUG-064 */
		max-height: 92vh;
		overflow-y: auto;
		padding: 1rem 1.25rem 1rem;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
	}
	/* BUG-160: 콤보박스 팝업은 "목록"이 본문이라 확인 모달용 30rem 로는 좁다 —
	   퀘스트 제목이 잘리고 후보가 스크롤 뒤로 숨었다(사용자 지적). 넓은 변형을
	   따로 둬서 삭제/타입변경 등 다른 modal-sm 사용처는 그대로 유지. */
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
		padding: 0 4px;
	}
	.x:hover {
		color: var(--text);
	}

	.combo-state {
		color: var(--text-faint);
		font-size: 0.85rem;
		padding: 0.6rem 0;
	}
	.combo-err {
		color: var(--danger);
		font-size: 0.8rem;
		margin: 0.5rem 0 0;
	}

	.del-title {
		color: var(--danger);
	}
	.del-msg {
		color: var(--text);
		font-size: 0.875rem;
		margin: 0 0 0.85rem;
	}
	.del-sub {
		background: var(--bg);
		border: 1px solid var(--bg-subtle);
		border-radius: 6px;
		padding: 0.6rem 0.8rem;
		margin-bottom: 0.85rem;
	}
	.del-sub-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		margin-bottom: 0.3rem;
	}
	.del-sub-all {
		display: flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.75rem;
		color: var(--text-muted);
		cursor: pointer;
	}
	.del-sub-all:hover {
		color: var(--text);
	}
	.del-sub-title {
		margin: 0;
		font-size: 0.8rem;
		color: var(--text);
		font-weight: 600;
	}
	.del-sub-help {
		margin: 0 0 0.5rem;
		font-size: 0.75rem;
		color: var(--text-muted);
	}
	/* DEV-074 fix17: native scrollbar 숨김 — OverlayScrollbar 가 대신 그림. */
	.del-sub-list {
		list-style: none;
		padding: 0;
		margin: 0;
		max-height: 180px;
		overflow-y: auto;
		scrollbar-width: none;
	}
	.del-sub-list::-webkit-scrollbar {
		display: none;
	}
	.del-sub-list li {
		padding: 0.25rem 0;
	}
	.del-sub-list label {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		cursor: pointer;
		font-size: 0.85rem;
		color: var(--text);
	}
	.del-sub-list .badge {
		padding: 0.05rem 0.45rem;
		font-size: 0.7rem;
	}
	.del-sub-title-text {
		flex: 1;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.del-prereq {
		font-size: 0.75rem;
		color: var(--text-muted);
		margin: 0 0 0.85rem;
		font-style: italic;
	}
	.del-actions {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}
	.btn-del-yes {
		padding: 0.4rem 1.1rem;
		background: rgba(233, 79, 79, 0.15);
		border: 1px solid var(--danger);
		border-radius: 6px;
		color: var(--danger);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-del-yes:hover:not(:disabled) {
		background: rgba(233, 79, 79, 0.25);
	}
	.btn-del-yes:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.btn-del-no {
		padding: 0.4rem 1rem;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-del-no:hover:not(:disabled) {
		background: var(--bg-subtle);
	}

	/* DEV-109/123/127: 우하단 floating 점프 cluster — 위/댓글/메모. */
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
</style>
