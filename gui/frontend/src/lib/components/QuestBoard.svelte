<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { SvelteMap, SvelteSet } from 'svelte/reactivity';
	import { get } from 'svelte/store';
	import { goto } from '$app/navigation';
	// DEV-205 모듈3: Quest Board 문자열 i18n.
	import { locale, t } from '$lib/stores/locale';
	// DEV-015: status 표시 이름 — 언어 반응(ko 면 name_ko 우선, 빈 값이면 en).
	import { statusLabel, questStatusLabel } from '$lib/utils/status-label';
	import { questsApi } from '$lib/api/quests';
	// DEV-142 후속: 상태 변경 실패(미해결 토론 등) 시 통일된 toast 경고.
	import { showToast } from '$lib/stores/toast';
	import { metaApi } from '$lib/api/meta';
	import { detectEnvironment } from '$lib/api/transport';
	// BUG-034: 유효 기한 (퀘스트 required_due vs 연결 캠페인 ended_at) 계산 헬퍼.
	import { effectiveQuestDue } from '$lib/utils/quest-node-svg';
	import {
		BOARD_NODE_HEIGHT,
		BOARD_NODE_WIDTH,
		BoardGraph,
		BoardNode,
		type BoardElementDefinition,
		type BoardPoint
	} from '$lib/utils/quest-board-model';
	import { boardEdgePath, parallelEdgeBends } from '$lib/utils/quest-board-render';
	import { isBoardPanSurfaceTarget } from '$lib/utils/quest-board-input';
	import {
		boardPointToCanonical,
		canonicalGridBaseY,
		canonicalToBoardPoint,
		laneIndexAtCrossCoordinate,
		rowGridBaseX,
		rowLaneHeight,
		type BoardOrientation,
		type BoardOrientationMetrics
	} from '$lib/utils/quest-board-orientation';
	import {
		boardLodForZoom,
		isPerformanceMonitorShortcut,
		screenGridColumnCenters,
		screenGridMetrics,
		summarizeBoardFrames,
		type BoardFrameStats,
		type BoardLod
	} from '$lib/utils/quest-board-viewport';
	import Icon from './Icon.svelte';
	import {
		loadLaneOrder as loadLaneOrderShared,
		saveLaneOrder as saveLaneOrderShared
	} from '$lib/utils/lane-order';
	import { flashQuestId } from '$lib/stores';
	import {
		urgencyColor,
		urgencyLabel,
		urgencyOutOfRange,
		urgencyBgFor,
		type Quest,
		type QuestDependency,
		type QuestPosition,
		type QuestStatus,
		type QuestType
	} from '$lib/types';
	// DEV-135: 보드에서도 필터 설정 — '보드 설정' 모달에 List 와 동일한 필터 UI.
	import QuestListFilter from './QuestListFilter.svelte';

	// DEV-084: New Quest 버튼이 toolbar 로 이동 — 클릭 시 부모 (+page) 의 모달 오픈.
	let { onNewQuest }: { onNewQuest?: () => void } = $props();

	const NODE_W = BOARD_NODE_WIDTH;
	const NODE_H = BOARD_NODE_HEIGHT;
	/// 정렬 animate 의 duration (ms). 가드 (`arranging`) 가 이 시간만큼 유지되어
	/// 빠른 더블클릭이 진행 중 animate 중간에 새 animate 를 trigger 하지 않도록.
	const ARRANGE_ANIM_MS = 200;

	const NODE_GAP = 28;
	const LANE_PAD_X = 20; // lane 양쪽 가장자리 여백 (한쪽)
	const LANE_W = NODE_W * 3 + NODE_GAP * 2 + LANE_PAD_X * 2; // 948px
	const LANE_GAP = 36; // lane 사이 시각적 간격
	const LANE_STRIDE = LANE_W + LANE_GAP; // 한 lane 의 X 단위 (다음 lane 시작점까지)
	// DEV-105: collapsed lane 의 좁은 폭 — 세로 라벨 한 줄 들어갈 정도.
	const LANE_COLLAPSED_W = 40;
	/**
	 * DEV-334: 화면과 내용에 맞춘 최소 zoom.
	 *
	 * "전체가 화면에 들어오는 zoom" 보다 조금 더(70%) 축소할 수 있게 잡는다 —
	 * 전체 보기 + 주변 여유까지는 항상 가능해야 한다. 레인 수, 한 레인에 쌓인
	 * 퀘스트 수, 화면 크기가 바뀌어도 자동으로 따라간다.
	 *
	 * **가로만 보면 안 된다** — 실측에서 375px 화면의 제약은 세로였다(보드
	 * 6,504 x 17,364). 그래서 cy 가 있으면 실제 bounding box 로 계산하고,
	 * 아직 없으면(최초 생성 시점) 레인 폭으로 어림잡는다.
	 *
	 * 상한은 기존 하한(0.25) — 넓은 화면에서 쓸데없이 깊게 축소되지 않게.
	 * 하한 0.02 는 안전장치.
	 */
	const LANE_TOP = 52;
	const ORIENTATION_METRICS: BoardOrientationMetrics = {
		nodeWidth: NODE_W,
		nodeHeight: NODE_H,
		nodeGap: NODE_GAP,
		lanePadding: LANE_PAD_X,
		columnLaneWidth: LANE_W,
		columnLaneStride: LANE_STRIDE,
		laneHeaderSize: LANE_TOP
	};
	const ROW_LANE_H = rowLaneHeight(ORIENTATION_METRICS);
	let boardOrientation = $state<BoardOrientation>('columns');

	function computeMinZoom(): number {
		const vw = container?.clientWidth || window.innerWidth || 1;
		const vh = container?.clientHeight || window.innerHeight || 1;
		let fitZoom: number;
		const bb = cy?.elements().nonempty() ? cy.elements().boundingBox() : null;
		if (bb && bb.w > 0 && bb.h > 0) {
			fitZoom = Math.min(vw / bb.w, vh / bb.h);
		} else {
			fitZoom =
				boardOrientation === 'columns'
					? vw / (LANE_STRIDE * Math.max(1, sorted.length))
					: vh / ((ROW_LANE_H + LANE_GAP) * Math.max(1, sorted.length));
		}
		return Math.max(0.02, Math.min(0.25, fitZoom * 0.7));
	}
	const CARD_W = 300;
	const MAX_HISTORY = 50;

	// ── 이력 타입 ────────────────────────────────────────────────

	interface SingleMove {
		type: 'single';
		questId: number;
		from: { x: number; y: number; statusId: number };
		to: { x: number; y: number; statusId: number };
	}
	interface BatchMove {
		type: 'batch';
		items: {
			questId: number;
			from: { x: number; y: number; statusId: number };
			to: { x: number; y: number; statusId: number };
		}[];
	}
	type HistoryRecord = SingleMove | BatchMove;

	// ── DOM refs ─────────────────────────────────────────────────

	let container: HTMLDivElement;
	let boardWrapEl: HTMLDivElement;
	let worldViewportEl: HTMLDivElement;
	let worldEl: HTMLDivElement;
	let lanesEl: HTMLDivElement;
	let gridLanesEl: HTMLDivElement;
	let headersEl: HTMLDivElement;
	// BUG(admin 보고): 레인 설정 팝오버를 toolbar 위로 올리려고 헤더 레이어
	// 전체(z:3 → 11)를 올렸더니 **레인 제목까지** toolbar·새 퀘스트 버튼을
	// 덮었다. 헤더 레이어는 z-index 가 있어 stacking context 라, 안쪽 팝오버만
	// 따로 올릴 수가 없다. 팝오버를 toolbar 와 같은 층의 별도 레이어로 빼고
	// 위치는 JS 로 버튼에 맞춘다 — 제목은 원래 자리(toolbar 아래)에 남는다.
	let lanePopLayerEl: HTMLDivElement;
	let cy: BoardGraph | null = null;
	let boardLod = $state<BoardLod>('detail');

	let performanceVisible = $state(false);
	// import.meta.env.DEV는 브라우저 dev server용. 패키징된 Tauri debug는
	// Rust의 is_debug_build를 onMount에서 조회해 true로 바꾼다.
	let performanceEnabled = $state(import.meta.env.DEV);
	let performanceStats = $state<BoardFrameStats>({
		rafHz: 0,
		medianMs: 0,
		p95Ms: 0,
		missed120Percent: 0
	});
	let performanceViewportHz = $state(0);
	let performanceZoom = $state(1);
	let performancePageState = $state('visible/focus');
	let performanceRaf: number | null = null;
	let performanceLastFrame = 0;
	let performanceWindowStarted = 0;
	let performanceViewportStart = 0;
	let viewportVisualUpdateCount = 0;
	let performanceIntervals: number[] = [];
	let gridZoomActive = false;
	let gridZoomEndTimer: ReturnType<typeof setTimeout> | null = null;

	function resetPerformanceWindow(now: number) {
		performanceLastFrame = now;
		performanceWindowStarted = now;
		performanceViewportStart = viewportVisualUpdateCount;
		performanceIntervals = [];
	}

	function samplePerformanceFrame(now: number) {
		if (!performanceVisible) {
			performanceRaf = null;
			return;
		}
		if (performanceWindowStarted === 0) resetPerformanceWindow(now);
		else if (performanceLastFrame > 0) performanceIntervals.push(now - performanceLastFrame);
		performanceLastFrame = now;

		const elapsed = now - performanceWindowStarted;
		if (elapsed >= 1000) {
			performanceStats = summarizeBoardFrames(performanceIntervals);
			performanceViewportHz =
				((viewportVisualUpdateCount - performanceViewportStart) * 1000) / Math.max(elapsed, 1);
			performanceZoom = cy?.zoom() ?? 1;
			performancePageState = `${document.visibilityState}/${document.hasFocus() ? 'focus' : 'blur'}`;
			resetPerformanceWindow(now);
		}
		performanceRaf = requestAnimationFrame(samplePerformanceFrame);
	}

	function togglePerformanceMonitor() {
		if (!performanceEnabled) return;
		performanceVisible = !performanceVisible;
		if (performanceVisible) {
			resetPerformanceWindow(performance.now());
			performanceRaf = requestAnimationFrame(samplePerformanceFrame);
		} else if (performanceRaf !== null) {
			cancelAnimationFrame(performanceRaf);
			performanceRaf = null;
		}
	}

	// DEV-317: lane, edge, node 를 한 DOM world 에 두고 같은 transform 을 적용한다.
	// BoardGraph 는 렌더러가 아닌 위치/선택/viewport 상태 모델이다.
	interface DomNodeView {
		id: number;
		quest: Quest;
		x: number;
		y: number;
		urgencyColor: string;
		urgencyBg: string;
		highlightType: HighlightType | 'dim' | '';
		active: boolean;
		selected: boolean;
		fdim: boolean;
		flash: boolean;
		hidden: boolean;
		zIndex: number;
	}
	interface DomEdgeView {
		id: string;
		sourceId: number;
		targetId: number;
		path: string;
		bend: number;
		etype: 'pre' | 'sub';
		dimmed: boolean;
		fdim: boolean;
		hidden: boolean;
	}
	let domNodes = $state<DomNodeView[]>([]);
	let domEdges = $state<DomEdgeView[]>([]);
	let domGraphRaf: number | null = null;
	let fullDomSyncPending = false;
	const dirtyDomNodeIds = new SvelteSet<number>();

	function dueState(date: string | null): 'overdue' | 'soon' | 'normal' {
		if (!date) return 'normal';
		const dueMs = new Date(`${date}T23:59:59`).getTime();
		if (Number.isNaN(dueMs)) return 'normal';
		const daysLeft = Math.floor((dueMs - Date.now()) / (24 * 60 * 60 * 1000));
		if (daysLeft < 0) return 'overdue';
		if (daysLeft <= 7) return 'soon';
		return 'normal';
	}

	function syncDomGraphNow() {
		fullDomSyncPending = false;
		dirtyDomNodeIds.clear();
		if (!cy) {
			domNodes = [];
			domEdges = [];
			return;
		}
		const positions = new SvelteMap<number, { x: number; y: number; hidden: boolean }>();
		domNodes = cy
			.nodes('[questId]')
			.toArray()
			.map((raw) => {
				const n = raw as BoardNode;
				const id = n.data('questId') as number;
				const pos = n.position();
				const hidden = n.style('display') === 'none';
				positions.set(id, { ...pos, hidden });
				const quest = allQuests.find((q) => q.id === id)!;
				return {
					id,
					quest,
					x: pos.x,
					y: pos.y,
					urgencyColor: n.data('urgencyColor') as string,
					urgencyBg: n.data('urgencyBg') as string,
					highlightType: (n.data('highlightType') ?? '') as DomNodeView['highlightType'],
					active: Boolean(n.data('active')),
					selected: n.selected(),
					fdim: Boolean(n.data('fdim')),
					flash: Boolean(n.data('flash')),
					hidden,
					zIndex: Number(n.style('z-index')) || 10
				};
			});
		const rawEdges = cy.edges().toArray();
		const bends = parallelEdgeBends(
			rawEdges.map((raw) => ({
				id: raw.id(),
				sourceId: raw.source().data('questId') as number,
				targetId: raw.target().data('questId') as number
			}))
		);
		domEdges = rawEdges.map((raw) => {
			const sourceId = raw.source().data('questId') as number;
			const targetId = raw.target().data('questId') as number;
			const source = positions.get(sourceId);
			const target = positions.get(targetId);
			const id = raw.id();
			const bend = bends.get(id) ?? 0;
			return {
				id,
				sourceId,
				targetId,
				path:
					source && target
						? boardEdgePath(source.x, source.y, target.x, target.y, bend, NODE_W, NODE_H)
						: '',
				bend,
				etype: raw.data('etype') as 'pre' | 'sub',
				dimmed: Boolean(raw.data('dimmed')),
				fdim: Boolean(raw.data('fdim')),
				hidden: !source || !target || source.hidden || target.hidden
			};
		});
	}

	function flushDomGraphSync() {
		domGraphRaf = null;
		if (fullDomSyncPending) {
			syncDomGraphNow();
			return;
		}
		if (!cy || dirtyDomNodeIds.size === 0) return;
		const moved = new SvelteMap<number, { x: number; y: number }>();
		for (const id of dirtyDomNodeIds) {
			const node = cy.getElementById(`q-${id}`) as BoardNode;
			if (node.length > 0) moved.set(id, { ...node.position() });
		}
		dirtyDomNodeIds.clear();
		domNodes = domNodes.map((node) => {
			const pos = moved.get(node.id);
			return pos ? { ...node, ...pos } : node;
		});
		domEdges = domEdges.map((edge) => {
			if (!moved.has(edge.sourceId) && !moved.has(edge.targetId)) return edge;
			const source = moved.get(edge.sourceId) ?? domNodes.find((n) => n.id === edge.sourceId);
			const target = moved.get(edge.targetId) ?? domNodes.find((n) => n.id === edge.targetId);
			return source && target
				? {
						...edge,
						path: boardEdgePath(source.x, source.y, target.x, target.y, edge.bend, NODE_W, NODE_H)
					}
				: edge;
		});
	}

	function scheduleDomGraphSync() {
		fullDomSyncPending = true;
		if (domGraphRaf === null) domGraphRaf = requestAnimationFrame(flushDomGraphSync);
	}

	function scheduleDomPositionSync(node: BoardNode) {
		dirtyDomNodeIds.add(node.data('questId') as number);
		if (domGraphRaf === null) domGraphRaf = requestAnimationFrame(flushDomGraphSync);
	}
	// BUG: sorted 가 일반 let — svelte 5 reactive 안 됨 (npm check warning). $state 로.
	let sorted: QuestStatus[] = $state([]);
	let laneOf = new Map<number, number>();

	// DEV-048: status_id (number) → status_slug (string). API 는 slug 전용.
	function slugOf(statusId: number): string {
		return sorted.find((s) => s.id === statusId)?.slug ?? '';
	}

	// DEV-015: 언어 토글 시 레인 라벨 갱신 — 레인 헤더는 buildLaneDivs 가
	// imperative DOM 으로 만들어 Svelte 반응이 안 닿음. locale 변경에만 반응해
	// 라벨 텍스트만 다시 쓴다(헤더 DOM 순서 = sorted 순서, buildLaneDivs 참고).
	$effect(() => {
		const loc = $locale;
		if (!headersEl) return;
		headersEl.querySelectorAll<HTMLElement>('.lane-label').forEach((el, i) => {
			const s = sorted[i];
			if (s) el.textContent = statusLabel(s, loc);
			el.classList.toggle('lane-label-en', loc === 'en');
		});
	});

	// ── 반응형 상태 ──────────────────────────────────────────────

	type HighlightType = 'pre' | 'sub' | 'next' | 'parent';

	let loading = $state(true);
	let error = $state<string | null>(null);
	let undoStack = $state<HistoryRecord[]>([]);
	let redoStack = $state<HistoryRecord[]>([]);
	// ── BUG-019: localStorage 길드별 namespace prefix ─────────────
	// hideSettings / lane cols / viewport / gridSnap 은 길드 데이터에 종속
	// (status slug 키, board 좌표 등) — 길드 A 의 설정이 B 로 누수되면 안 됨.
	// 활성 길드 경로의 FNV-1a 32-bit 해시 (8 hex) 를 prefix 로 사용.
	// 길드 경로 로드 전에는 ''. 모든 load/save 는 prefix 설정 후 호출 (init).
	let guildKeyPrefix = '';
	function fnv1a32(s: string): string {
		let h = 0x811c9dc5;
		for (let i = 0; i < s.length; i++) {
			h ^= s.charCodeAt(i);
			h = (h + ((h << 1) + (h << 4) + (h << 7) + (h << 8) + (h << 24))) >>> 0;
		}
		return h.toString(16).padStart(8, '0');
	}
	function gk(suffix: string): string {
		return guildKeyPrefix ? `openguild.${guildKeyPrefix}.${suffix}` : `openguild.${suffix}`;
	}
	function loadBoardOrientation(): BoardOrientation {
		try {
			return localStorage.getItem(gk('boardOrientation')) === 'rows' ? 'rows' : 'columns';
		} catch {
			return 'columns';
		}
	}
	function saveBoardOrientation() {
		try {
			localStorage.setItem(gk('boardOrientation'), boardOrientation);
		} catch {
			/* 무시 */
		}
	}

	// ── lane cols 영속화 헬퍼 (BUG-009) ─────────────────────────
	// status slug 를 키로 — sort_order / id 가 reindex / 시드 변경 시 흔들리므로.

	function statusSlug(nameEn: string): string {
		return nameEn.toLowerCase().replace(/ /g, '_').replace(/-/g, '_');
	}

	function loadLaneColsMap(): Record<string, number> {
		try {
			const raw = localStorage.getItem(gk('laneCols'));
			if (!raw) return {};
			const parsed = JSON.parse(raw);
			if (parsed && typeof parsed === 'object') return parsed as Record<string, number>;
		} catch {
			/* 무시 */
		}
		return {};
	}

	function saveLaneColsMap(map: Record<string, number>) {
		try {
			localStorage.setItem(gk('laneCols'), JSON.stringify(map));
		} catch {
			/* 무시 */
		}
	}

	function loadGlobalCols(): number {
		try {
			const raw = localStorage.getItem(gk('globalCols'));
			const n = raw ? parseInt(raw, 10) : NaN;
			if (Number.isFinite(n) && [1, 2, 3].includes(n)) return n;
		} catch {
			/* 무시 */
		}
		return 2;
	}

	function saveGlobalCols(n: number) {
		try {
			localStorage.setItem(gk('globalCols'), String(n));
		} catch {
			/* 무시 */
		}
	}

	// ── DEV-056: 레인 / 노드 숨김 설정 (status slug 키로 영속) ─────
	//
	// 각 lane (status) 마다:
	//   - laneHidden: true 면 lane 전체 숨김 (그 status 의 모든 노드 + lane DIV).
	//   - hideGroup:  그룹의 모든 멤버가 이 lane 에 있을 때만 그 그룹 노드들 숨김.
	//                 (한 그룹이 여러 lane 에 걸치면 어느 lane 에서도 안 숨김.)
	//   - hideSolo:   그룹에 속하지 않은 단독 노드 (연관관계 없음) 숨김.
	//
	// 그룹: parent/child + prerequisite/dependent 관계를 따라간 연결 컴포넌트.
	interface HideSetting {
		laneHidden: boolean;
		hideGroup: boolean;
		hideSolo: boolean;
	}
	function loadHideSettings(): Record<string, HideSetting> {
		try {
			const raw = localStorage.getItem(gk('hideSettings'));
			if (!raw) return {};
			const parsed = JSON.parse(raw);
			if (parsed && typeof parsed === 'object') return parsed as Record<string, HideSetting>;
		} catch {
			/* 무시 */
		}
		return {};
	}

	function saveHideSettings(map: Record<string, HideSetting>) {
		try {
			localStorage.setItem(gk('hideSettings'), JSON.stringify(map));
		} catch {
			/* 무시 */
		}
	}

	// ── DEV-058: Board viewport (pan + zoom) 영속화 ─────────────
	// 길드 → board 진입 시마다 fit() 이 화면 전체 보기로 reset 하던 동작 →
	// 사용자가 보고 있던 위치/확대율을 localStorage 에 저장 후 복원.
	interface BoardViewport {
		pan: { x: number; y: number };
		zoom: number;
	}
	function viewportStorageKey(orientation = boardOrientation): string {
		// 기존 key는 columns용으로 유지해 이전 버전의 viewport를 그대로 복원한다.
		return gk(orientation === 'columns' ? 'boardViewport' : 'boardViewport.rows');
	}
	function loadViewport(): BoardViewport | null {
		try {
			const raw = localStorage.getItem(viewportStorageKey());
			if (!raw) return null;
			const v = JSON.parse(raw) as BoardViewport;
			if (
				v &&
				typeof v.zoom === 'number' &&
				v.pan &&
				typeof v.pan.x === 'number' &&
				typeof v.pan.y === 'number'
			)
				return v;
		} catch {
			/* 무시 */
		}
		return null;
	}
	let viewportSaveTimer: ReturnType<typeof setTimeout> | null = null;
	function saveViewportNow(orientation = boardOrientation) {
		if (!cy) return;
		try {
			const v: BoardViewport = { pan: cy.pan(), zoom: cy.zoom() };
			localStorage.setItem(viewportStorageKey(orientation), JSON.stringify(v));
		} catch {
			/* 무시 */
		}
	}
	function scheduleViewportSave() {
		if (viewportSaveTimer) clearTimeout(viewportSaveTimer);
		viewportSaveTimer = setTimeout(() => {
			saveViewportNow();
		}, 250); // debounce 250ms — 연속 pan/zoom 시 저장 폭주 방지.
	}

	function defaultHideSetting(): HideSetting {
		return { laneHidden: false, hideGroup: false, hideSolo: false };
	}

	function getHideSetting(slug: string): HideSetting {
		return hideSettings[slug] ?? defaultHideSetting();
	}

	// BUG-019: guildKeyPrefix 가 아직 비어있어 load 가 무의미 — onMount 에서
	// guild path 확정 후 loadHideSettings() 결과로 대체.
	let hideSettings = $state<Record<string, HideSetting>>({});
	let showHideModal = $state(false);

	/** questId → 그룹 멤버 set (자기 자신 포함). cluster 와 동일 의미. */
	let groupOf: Map<number, Set<number>> = new Map();

	/** quests + dependencies 로부터 connected component (그룹) 계산. */
	function computeGroups(quests: Quest[], deps: QuestDependency[]): Map<number, Set<number>> {
		const adj = new Map<number, Set<number>>();
		const ensure = (id: number) => {
			if (!adj.has(id)) adj.set(id, new Set());
			return adj.get(id)!;
		};
		quests.forEach((q) => {
			ensure(q.id);
			if (q.parent_quest_id != null) {
				ensure(q.parent_quest_id).add(q.id);
				ensure(q.id).add(q.parent_quest_id);
			}
		});
		deps.forEach((d) => {
			ensure(d.prerequisite_id).add(d.quest_id);
			ensure(d.quest_id).add(d.prerequisite_id);
		});
		const result = new Map<number, Set<number>>();
		const visited = new Set<number>();
		quests.forEach((q) => {
			if (visited.has(q.id)) return;
			// BFS
			const group = new Set<number>();
			const queue = [q.id];
			while (queue.length > 0) {
				const cur = queue.shift()!;
				if (group.has(cur)) continue;
				group.add(cur);
				visited.add(cur);
				adj.get(cur)?.forEach((n) => {
					if (!group.has(n)) queue.push(n);
				});
			}
			group.forEach((id) => result.set(id, group));
		});
		return result;
	}

	/**
	 * hideSettings 와 groupOf 를 보고 각 quest 의 hidden 여부 결정.
	 * - laneHidden true 인 lane 의 모든 노드 → hidden.
	 * - hideGroup true 인 lane: 같은 그룹의 모든 멤버가 이 lane 에 있는 경우만 hidden.
	 * - hideSolo true 인 lane: 그룹 크기 1 인 노드 hidden.
	 */
	function computeHiddenIds(): Set<number> {
		const hidden = new Set<number>();
		const statusById = new Map<number, string>(); // quest_id → status_slug
		allQuests.forEach((q) => statusById.set(q.id, q.status_slug));
		allQuests.forEach((q) => {
			const setting = getHideSetting(q.status_slug);
			if (setting.laneHidden) {
				hidden.add(q.id);
				return;
			}
			const group = groupOf.get(q.id);
			if (!group) return;
			if (group.size === 1) {
				if (setting.hideSolo) hidden.add(q.id);
				return;
			}
			if (setting.hideGroup) {
				// 그룹의 모든 멤버가 이 lane (같은 status) 에 있는가?
				let allSame = true;
				group.forEach((mid) => {
					if (statusById.get(mid) !== q.status_slug) allSame = false;
				});
				if (allSame) hidden.add(q.id);
			}
		});
		return hidden;
	}

	/**
	 * 결정된 hidden set 을 BoardGraph + lane DOM 에 적용.
	 * 숨긴 노드에 연결된 SVG edge도 DOM snapshot에서 함께 제외된다.
	 *
	 * DEV-105 fix9: collapsed lane 의 노드도 자동 hide. 이전엔 status 변경 후
	 * applyHideSettings 가 호출되면 그 노드가 다시 'element' 로 표시되어 접힌
	 * lane 안 노드가 갑자기 보이는 버그.
	 */
	function applyHideSettings() {
		const c = cy;
		if (!c) return;
		const hidden = computeHiddenIds();
		c.batch(() => {
			c.nodes('[questId]').forEach((n) => {
				const qid = n.data('questId') as number;
				let shouldHide = hidden.has(qid);
				if (!shouldHide && collapsedLanes.size > 0) {
					const sid = n.data('statusId') as number;
					const s = sorted.find((x) => x.id === sid);
					if (s && collapsedLanes.has(s.slug)) shouldHide = true;
				}
				n.style('display', shouldHide ? 'none' : 'element');
			});
		});
		// lane DIV: laneHidden true 인 lane 의 col + header 시각 처리.
		applyLaneVisibility();
	}

	function applyLaneVisibility() {
		if (!lanesEl || !gridLanesEl || !headersEl) return;
		sorted.forEach((s, li) => {
			const setting = getHideSetting(s.slug);
			const col = lanesEl.children[li] as HTMLDivElement | undefined;
			const gridCol = gridLanesEl.children[li] as HTMLDivElement | undefined;
			const hdr = headersEl.children[li] as HTMLDivElement | undefined;
			if (col) col.style.display = setting.laneHidden ? 'none' : '';
			if (gridCol) gridCol.style.display = setting.laneHidden ? 'none' : '';
			if (hdr) hdr.style.display = setting.laneHidden ? 'none' : '';
		});
	}

	function toggleHideSetting(slug: string, key: keyof HideSetting) {
		const cur = getHideSetting(slug);
		const next = { ...cur, [key]: !cur[key] };
		hideSettings = { ...hideSettings, [slug]: next };
		saveHideSettings(hideSettings);
		applyHideSettings();
		// DEV-067: laneHidden 변경 시 시각 lane 압축 (모든 노드 visualX 재계산 + lane DOM left).
		if (key === 'laneHidden') {
			applyLaneVisualCompression();
			syncLanes();
		}
	}

	// ── lane 좌표계 ─────────────────────────────────────────────
	// DB의 x/y는 기존 columns 좌표를 정본으로 유지한다. rows는 저장 좌표를
	// 복제하지 않고 canonicalToBoardPoint로 화면에서만 축을 교환한다.

	function absoluteLaneLeftOfStatus(statusId: number): number {
		return (laneOf.get(statusId) ?? 0) * LANE_STRIDE;
	}

	// DEV-105: lane 별 width / stride — collapsed lane 는 좁게.
	function laneWidth(slug: string): number {
		return collapsedLanes.has(slug) ? LANE_COLLAPSED_W : LANE_W;
	}
	function laneStride(slug: string): number {
		return laneWidth(slug) + LANE_GAP;
	}
	function rowHeight(slug: string): number {
		return collapsedLanes.has(slug) ? LANE_COLLAPSED_W : ROW_LANE_H;
	}
	function rowStride(slug: string): number {
		return rowHeight(slug) + LANE_GAP;
	}

	function visibleLaneLeftOfStatus(statusId: number): number {
		// DEV-105: collapsed lane 는 좁은 폭으로 누적. laneHidden 은 0 폭.
		let left = 0;
		let lastLeft = 0;
		for (const s of sorted) {
			const setting = getHideSetting(s.slug);
			if (s.id === statusId) {
				return setting.laneHidden ? lastLeft : left;
			}
			if (!setting.laneHidden) {
				lastLeft = left;
				left += laneStride(s.slug);
			}
		}
		return 0;
	}

	function visibleLaneTopOfStatus(statusId: number): number {
		let top = 0;
		let lastTop = 0;
		for (const s of sorted) {
			const setting = getHideSetting(s.slug);
			if (s.id === statusId) return setting.laneHidden ? lastTop : top;
			if (!setting.laneHidden) {
				lastTop = top;
				top += rowStride(s.slug);
			}
		}
		return 0;
	}

	function visibleLaneStartOfStatus(statusId: number): number {
		return boardOrientation === 'columns'
			? visibleLaneLeftOfStatus(statusId)
			: visibleLaneTopOfStatus(statusId);
	}

	function canonicalToVisual(absX: number, absY: number, statusId: number): BoardPoint {
		return canonicalToBoardPoint(
			{ x: absX, y: absY },
			absoluteLaneLeftOfStatus(statusId),
			visibleLaneStartOfStatus(statusId),
			boardOrientation,
			ORIENTATION_METRICS
		);
	}

	function visualToCanonical(visualX: number, visualY: number, statusId: number): BoardPoint {
		return boardPointToCanonical(
			{ x: visualX, y: visualY },
			absoluteLaneLeftOfStatus(statusId),
			visibleLaneStartOfStatus(statusId),
			boardOrientation,
			ORIENTATION_METRICS
		);
	}

	/** visible lane index (drag drop 시 사용자 시각 위치) → status_id. */
	function statusIdAtVisibleIdx(visIdx: number): number | null {
		let i = 0;
		for (const s of sorted) {
			if (getHideSetting(s.slug).laneHidden) continue;
			if (i === visIdx) return s.id;
			i++;
		}
		return null;
	}

	function visibleLaneCount(): number {
		return sorted.filter((s) => !getHideSetting(s.slug).laneHidden).length;
	}

	/**
	 * DEV-105 fix10: 가변 폭 (collapsed lane) 인식하는 visual X → visible lane idx.
	 * 이전엔 `Math.floor(x / LANE_STRIDE)` 균등 stride 가정 → collapsed lane 의
	 * 시각 영역 (40px) 뒤로 클릭/드롭해도 같은 lane 으로 잡혀 "실제 영역은
	 * 그대로 차지" 현상.
	 */
	function visibleLaneIdxAtVisualPoint(point: BoardPoint): number {
		const cross = boardOrientation === 'columns' ? point.x : point.y;
		const strides = sorted
			.filter((s) => !getHideSetting(s.slug).laneHidden)
			.map((s) => (boardOrientation === 'columns' ? laneStride(s.slug) : rowStride(s.slug)));
		return laneIndexAtCrossCoordinate(cross, strides);
	}

	/** 모든 노드의 visual position을 현재 orientation/hide settings로 재계산. */
	function applyLaneVisualCompression() {
		const c = cy;
		if (!c) return;
		c.batch(() => {
			c.nodes('[questId]').forEach((n) => {
				const sid = n.data('statusId') as number;
				const absX = (n.data('absX') as number | undefined) ?? n.position().x;
				const absY = (n.data('absY') as number | undefined) ?? n.position().y;
				const next = canonicalToVisual(absX, absY, sid);
				const current = n.position();
				if (next.x !== current.x || next.y !== current.y) n.position(next);
			});
		});
	}

	let expandedQuest = $state<Quest | null>(null);
	let expandedPos = $state({ x: 0, y: 0 });
	let cardPinned = false; // 사용자가 카드를 드래그하면 true, 새 노드 클릭 시 false
	let activeHighlights = $state(new Set<HighlightType>());
	// globalCols — toolbar 의 Arrange cols. localStorage 영속 (BUG-009 / BUG-019).
	// 초기값은 default — onMount 의 guild prefix 확정 후 loadGlobalCols() 결과로 덮어씀.
	let globalCols = $state(2);

	// 전역 정렬 모드 (toolbar Arrange 버튼).
	//   'all'   = 단순 wrap (왼쪽 위부터 채움)
	//   'group' = 관계 분석 (isolated 위쪽 + cluster 직사각형)
	let arrangeMode = $state<'all' | 'group'>('group');

	// 각 lane 의 정렬 기준 cols (lane header 의 select 값과 동기화).
	// snapToGrid / arrange 둘 다 이 값을 보고 같은 격자에 배치.
	let laneCols = $state<number[]>([]);

	// 각 lane 의 정렬 모드 (lane header 의 mode select 값과 동기화). 초기값은 전역 모드.
	let laneArrangeModes = $state<('all' | 'group')[]>([]);

	// 그리드 스냅 — 노드 드래그 종료 시 그리드 셀에 정렬. localStorage 영구화.
	let gridSnap = $state(false);
	function toggleGridSnap() {
		gridSnap = !gridSnap;
		try {
			localStorage.setItem(gk('gridSnap'), String(gridSnap));
		} catch {
			/* 무시 */
		}
	}

	function toggleBoardOrientation() {
		if (!cy) return;
		if (viewportSaveTimer) {
			clearTimeout(viewportSaveTimer);
			viewportSaveTimer = null;
		}
		const previous = boardOrientation;
		saveViewportNow(previous);
		boardOrientation = previous === 'columns' ? 'rows' : 'columns';
		saveBoardOrientation();
		// 이력의 좌표는 당시 화면 orientation 기준이므로 모드 경계를 넘겨 재생하지 않는다.
		undoStack = [];
		redoStack = [];
		buildLaneDivs(sorted);
		applyHideSettings();
		applyLaneVisualCompression();
		syncLanes();
		const saved = loadViewport();
		if (saved) cy.viewport(saved);
		else cy.fit(undefined, 60);
		syncLanes();
	}

	// DEV-073: toolbar 접기 — 우상단 toolbar 가 첫 lane 의 header 라벨을 가리는
	// 문제. 접으면 ⊟ 한 버튼만 남고 나머지 숨김. localStorage 영구화.
	let toolbarCollapsed = $state(false);
	function toggleToolbarCollapsed() {
		toolbarCollapsed = !toolbarCollapsed;
		try {
			localStorage.setItem(gk('toolbarCollapsed'), String(toolbarCollapsed));
		} catch {
			/* 무시 */
		}
	}

	// DEV-074 fix: light theme 시 노드 bg 색 light tone 으로. theme store
	// subscribe — 변경 시 모든 노드 의 urgencyBg data 갱신 + cy.style 적용.
	// BUG-121: `theme`(ThemeChoice) 대신 `effectiveTheme` 을 구독 — system
	// 모드에서 OS 가 테마를 바꿔도 `theme` 값 자체는 'system' 그대로라
	// writable 이 재통지하지 않아, 노드/SVG 재생성이 전혀 발화하지 않았다.
	import { theme, effectiveTheme, resolveTheme, themePalette } from '$lib/stores/theme';
	function currentEffectiveTheme(): 'dark' | 'light' {
		return resolveTheme(getStore(theme));
	}
	// svelte/store get 임포트 alias.
	import { get as getStore } from 'svelte/store';

	// DEV-105: lane 접기 — columns에서는 폭, rows에서는 높이를 40px로 축소한다.
	let collapsedLanes = $state(new Set<string>());
	function loadCollapsedLanes(): Set<string> {
		try {
			const raw = localStorage.getItem(gk('collapsedLanes'));
			if (!raw) return new Set();
			const arr = JSON.parse(raw);
			return new Set(Array.isArray(arr) ? arr.filter((s) => typeof s === 'string') : []);
		} catch {
			return new Set();
		}
	}
	function saveCollapsedLanes() {
		try {
			localStorage.setItem(gk('collapsedLanes'), JSON.stringify([...collapsedLanes]));
		} catch {
			/* 무시 */
		}
	}

	// DEV-105 fix5: lane 별 설정 (cols-sel, arrange 그룹) 접기. 기본 접힘 — 자주
	// 안 쓰면서 영역만 차지하므로. 사용자가 ⚙ 버튼으로 펼침. slug 별 영속.
	let lanesSettingsOpen = $state(new Set<string>());
	function loadLanesSettingsOpen(): Set<string> {
		try {
			const raw = localStorage.getItem(gk('lanesSettingsOpen'));
			if (!raw) return new Set();
			const arr = JSON.parse(raw);
			return new Set(Array.isArray(arr) ? arr.filter((s) => typeof s === 'string') : []);
		} catch {
			return new Set();
		}
	}
	function saveLanesSettingsOpen() {
		try {
			localStorage.setItem(gk('lanesSettingsOpen'), JSON.stringify([...lanesSettingsOpen]));
		} catch {
			/* 무시 */
		}
	}
	function toggleLaneSettings(slug: string) {
		const next = new Set(lanesSettingsOpen);
		if (next.has(slug)) next.delete(slug);
		else next.add(slug);
		lanesSettingsOpen = next;
		saveLanesSettingsOpen();
	}
	/**
	 * 열린 레인 설정을 전부 닫는다 (바깥 클릭용, admin 요청).
	 *
	 * 열림 표시는 세 곳에 흩어져 있다 — 상태 Set, 헤더의 `settings-open`
	 * 클래스(⚙ 버튼 강조), 팝오버의 `open` 클래스(표시). 팝오버가 헤더 밖
	 * 별도 레이어에 살기 때문에 CSS 만으로는 한 번에 못 끄므로 여기서 직접
	 * 맞춰 준다. 버튼의 `aria-expanded` 도 함께.
	 */
	function closeAllLaneSettings() {
		if (lanesSettingsOpen.size === 0) return;
		lanesSettingsOpen = new Set();
		saveLanesSettingsOpen();
		headersEl?.querySelectorAll<HTMLElement>('.lane-hdr').forEach((hdr) => {
			hdr.classList.remove('settings-open');
			const btn = hdr.querySelector<HTMLElement>('.lane-settings-btn');
			btn?.setAttribute('aria-expanded', 'false');
			if (btn) btn.title = t('board.laneSettingsExpand', get(locale));
		});
		lanePopLayerEl?.querySelectorAll('.lane-settings-pop').forEach((pop) => {
			pop.classList.remove('open');
		});
	}

	/**
	 * 팝오버·⚙ 바깥을 누르면 닫는다.
	 *
	 * `pointerdown` 을 쓴다 — `click` 은 드래그로 보드를 pan 한 뒤에도 뜨는
	 * 경우가 있어 의도치 않게 닫히거나 안 닫히는 차이가 생긴다. ⚙ 자신은
	 * 제외해야 한다(자기 토글이 이미 처리 — 여기서 먼저 닫으면 다시 열린다).
	 */
	function onDocPointerDown(e: PointerEvent) {
		if (lanesSettingsOpen.size === 0) return;
		const el = e.target as HTMLElement | null;
		if (el?.closest('.lane-settings-pop, .lane-settings-btn')) return;
		closeAllLaneSettings();
	}

	function toggleLaneCollapsed(slug: string) {
		const next = new Set(collapsedLanes);
		if (next.has(slug)) next.delete(slug);
		else next.add(slug);
		collapsedLanes = next;
		saveCollapsedLanes();
		// DEV-105: lane 폭 축소 + 노드 hide/show + 다른 lane 의 visualX 도 재계산
		// (collapsed lane 가 좁아져 뒷 lane 이 왼쪽으로 당겨짐).
		if (cy) {
			const isCollapsed = next.has(slug);
			// DEV-105 fix7: 펼침 시 그 lane 의 모든 노드를 'element' 로 일괄
			// 복구하면 hideGroup / hideSolo 같은 hide 설정이 무시되는 버그.
			// 펼침일 때는 computeHiddenIds 기반으로 결정해야 함. 접힘 일 때만
			// 그 lane 의 노드 전부 hide.
			const hidden = isCollapsed ? null : computeHiddenIds();
			cy.nodes('[questId]').forEach((n) => {
				const sid = n.data('statusId') as number;
				const s = sorted.find((x) => x.id === sid);
				if (s?.slug === slug) {
					if (isCollapsed) {
						n.style('display', 'none');
					} else {
						const qid = n.data('questId') as number;
						n.style('display', hidden!.has(qid) ? 'none' : 'element');
					}
				}
				// 모든 노드의 화면 좌표 재계산 — collapsed가 lane 누적 크기에 영향.
				const absX = (n.data('absX') as number | undefined) ?? n.position().x;
				const absY = (n.data('absY') as number | undefined) ?? n.position().y;
				n.animate({ position: canonicalToVisual(absX, absY, sid), duration: 150 });
			});
			syncLanes();
		}
	}

	// DEV-059: 사용자 정의 lane 순서 — '보여지는 순서' 만. 파일 / DB / 다른 quest
	// 영향 X. status 추가/삭제는 sort_order 따라 자동 끝에 append (loadFromData
	// 의 ordered + remaining 패턴).
	//
	// 상태순서 통일: load/save 로직을 lib/utils/lane-order.ts 공유 헬퍼로 추출.
	// 상세페이지 status 드롭다운이 같은 laneOrder 를 따르도록(이전엔 보드 전용이라
	// 보드에서 바꾼 순서가 상세에 반영 안 됐음). guildKeyPrefix 는 gk() 와 동일.
	function loadLaneOrder(): string[] {
		return loadLaneOrderShared(guildKeyPrefix);
	}
	function saveLaneOrder(slugs: string[]) {
		saveLaneOrderShared(guildKeyPrefix, slugs);
	}
	// li (lane index) 의 lane 을 한 칸 좌/우 swap. 모든 노드를 새 lane 좌표로
	// 다시 그려야 하므로 cy reload.
	function swapLane(li: number, dir: -1 | 1) {
		const target = li + dir;
		if (target < 0 || target >= sorted.length) return;
		const oldLaneOf = new Map(laneOf);
		const next = [...sorted];
		[next[li], next[target]] = [next[target], next[li]];
		sorted = next;
		laneOf = new Map(sorted.map((s, i) => [s.id, i]));
		saveLaneOrder(sorted.map((s) => s.slug));
		// DEV-059 fix: lane 배경 / header div + 노드 모두 재배치.
		// 이전: 노드만 position() — 단 lane bg / header 의 left 는 옛 자리 → 시각상 안 바뀐 듯.
		// 지금: buildLaneDivs 가 새 sorted 순서대로 lane-col / lane-hdr 재구성. syncLanes 로 좌표.
		if (cy) {
			buildLaneDivs(sorted);
			// 정본 좌표의 lane-local offset을 보존하고 새 lane 순서에 맞춰 평행이동.
			cy.nodes('[questId]').forEach((n) => {
				const sid = n.data('statusId') as number;
				const newLi = laneOf.get(sid) ?? 0;
				const oldLi = oldLaneOf.get(sid) ?? 0;
				const absX = (n.data('absX') as number | undefined) ?? oldLi * LANE_STRIDE + LANE_W / 2;
				const absY =
					(n.data('absY') as number | undefined) ?? canonicalGridBaseY(ORIENTATION_METRICS);
				const nextAbsX = newLi * LANE_STRIDE + (absX - oldLi * LANE_STRIDE);
				n.data('absX', nextAbsX);
				n.animate({ position: canonicalToVisual(nextAbsX, absY, sid), duration: 200 });
			});
			syncLanes();
		}
	}
	/**
	 * 주어진 lane 의 그리드 첫 셀 X(보드 좌표). lane cols 에 맞춰 lane 중앙 기준 좌우 균등 배치.
	 *  - 1열: lane 중앙
	 *  - 2열: 중앙 좌우 (NODE_W+GAP)/2 씩
	 *  - 3열: 중앙 + ±(NODE_W+GAP)
	 */
	function laneFirstCellX(li: number, cols: number): number {
		const totalW = cols * NODE_W + Math.max(0, cols - 1) * NODE_GAP;
		const laneCenterX = li * LANE_STRIDE + LANE_W / 2;
		return laneCenterX - totalW / 2 + NODE_W / 2;
	}

	/** 보드 좌표 (visual) 를 NODE_W+GAP / NODE_H+GAP 단위 그리드의 가장 가까운 셀 중앙으로 스냅. */
	function snapToGrid(x: number, y: number): { x: number; y: number } {
		const cellW = NODE_W + NODE_GAP;
		const cellH = NODE_H + NODE_GAP;
		const visCount = Math.max(1, visibleLaneCount());
		const visIdx = Math.max(0, Math.min(visCount - 1, visibleLaneIdxAtVisualPoint({ x, y })));
		const statusId = statusIdAtVisibleIdx(visIdx);
		const li = statusId !== null ? (laneOf.get(statusId) ?? 0) : 0;
		const cols = laneCols[li] ?? 2;
		const sid = statusId ?? sorted[li]?.id ?? 0;
		const canonical = visualToCanonical(x, y, sid);
		const firstX = laneFirstCellX(li, cols);
		const localX = canonical.x - firstX;
		// BUG-113: colIdx 가 [0, cols-1] 로 안 잘려 있어, 레인 폭 자체는 항상
		// (더 많은 열도 들어갈 만큼) 넓기 때문에 1/2 열 snap 인 레인에서도 옆쪽에
		// 드롭하면 반올림된 colIdx 가 그 레인의 col 수를 넘어서 버렸다 —
		// 그 값이 우연히 3열 grid 의 바깥쪽 셀 위치와 겹쳐 "3열 snap 의 양 끝
		// 으로 스냅되는" 것처럼 보였음. 레인에 설정된 col 수 범위로 clamp.
		const colIdx = Math.max(0, Math.min(cols - 1, Math.round(localX / cellW)));
		const absX = firstX + colIdx * cellW;
		const baseY = canonicalGridBaseY(ORIENTATION_METRICS);
		const rowIdx = Math.round((canonical.y - baseY) / cellH);
		const absY = baseY + rowIdx * cellH;
		return canonicalToVisual(absX, absY, sid);
	}

	// ── 인앱 확인 다이얼로그 ────────────────────────────────────
	let confirmDialog = $state<{ msg: string; resolve: (ok: boolean) => void } | null>(null);
	function showConfirm(msg: string): Promise<boolean> {
		return new Promise((resolve) => {
			confirmDialog = { msg, resolve };
		});
	}
	function confirmDialogResolve(ok: boolean) {
		confirmDialog?.resolve(ok);
		confirmDialog = null;
	}

	// ── 일반 상태 ────────────────────────────────────────────────

	let allQuests: Quest[] = [];
	let allDependencies: QuestDependency[] = [];
	let busy = false;
	// 자동 정렬 진행 중 — 정렬 버튼의 disabled 반응성 위해 $state 필요 (BUG-006).
	let arranging = $state(false);
	let ctrlHeld = false;
	let boxDrag = $state<{
		left: number;
		top: number;
		width: number;
		height: number;
		color: string;
	} | null>(null);
	let boxDragStart: { x: number; y: number } | null = null;

	// 드래그 시작 상태 (노드별 Map)
	const dragStartMap = new Map<number, { x: number; y: number; statusId: number }>();
	// DEV-105 fix11: 드래그 중인 노드가 놓일 예정인 lane 의 slug — UI 하이라이트용.
	let dragHighlightSlug = $state<string | null>(null);
	// DEV-115: 최근 움직인 노드를 위로 — 단조 증가 카운터. 기본 z-index 는 10.
	let recentMoveZ = 10;
	// slug 변경 시 lane-col DOM 에 `.drag-target` 토글. lanesEl 의 children 순서
	// 가 sorted 와 동일 (buildLaneDivs).
	$effect(() => {
		const slug = dragHighlightSlug;
		if (!lanesEl) return;
		sorted.forEach((s, i) => {
			const col = lanesEl.children[i] as HTMLDivElement | undefined;
			if (!col) return;
			col.classList.toggle('drag-target', slug !== null && s.slug === slug);
		});
	});
	// 배치 dragfree 수집
	type PendingDragItem = {
		node: BoardNode;
		questId: number;
		fromPos: { x: number; y: number };
		fromStatus: number;
		toPos: { x: number; y: number };
		toLaneIdx: number;
	};
	let pendingDragBatch: PendingDragItem[] = [];
	let pendingDragTimer: ReturnType<typeof setTimeout> | null = null;
	type BoardInteraction =
		| {
				kind: 'pan';
				sx: number;
				sy: number;
				startPan: BoardPoint;
				moved: boolean;
		  }
		| {
				kind: 'node';
				sx: number;
				sy: number;
				primaryId: number;
				additive: boolean;
				moved: boolean;
		  };
	let boardInteraction: BoardInteraction | null = null;

	// 카드 드래그
	let cardDrag = $state<{ sx: number; sy: number; px: number; py: number } | null>(null);

	function startCardDrag(e: MouseEvent) {
		const t = e.target as HTMLElement;
		if (t.closest('button,select,input,a,code')) return;
		cardDrag = { sx: e.clientX, sy: e.clientY, px: expandedPos.x, py: expandedPos.y };
		cardPinned = true;
		e.preventDefault();
		e.stopPropagation();
	}

	// ── undo / redo ────────────────────────────────────────────

	// allQuests 캐시와 확장 카드를 동기화하는 헬퍼
	function applyStatusChange(questId: number, statusId: number) {
		const s = sorted.find((st) => st.id === statusId);
		if (!s) return;
		// 확장 카드 동기화 — DEV-046 후속: status_slug 도 함께 갱신 (일관성).
		if (expandedQuest?.id === questId) {
			expandedQuest = {
				...expandedQuest,
				status_id: s.id,
				status_slug: s.slug,
				status_name_en: s.name_en,
				status_name_ko: s.name_ko,
				status_color: s.color
			};
		}
		// allQuests 캐시 동기화 (tap으로 확장 시 최신 상태 반영)
		const idx = allQuests.findIndex((q) => q.id === questId);
		if (idx !== -1) {
			allQuests[idx] = {
				...allQuests[idx],
				status_id: s.id,
				status_slug: s.slug,
				status_name_en: s.name_en,
				status_name_ko: s.name_ko,
				status_color: s.color
			};
		}
		// DEV-056: status 가 바뀌면 그룹 분포 / hideGroup 평가 결과가 달라질 수 있음.
		// DEV-105 fix9: 새 lane 이 collapsed 면 visualX 도 재계산 필요.
		applyHideSettings();
		applyLaneVisualCompression();
		syncLanes();
	}

	async function applyRecord(record: HistoryRecord, direction: 'undo' | 'redo') {
		if (!cy || busy) return;
		busy = true;
		if (record.type === 'single') {
			const target = direction === 'undo' ? record.from : record.to;
			const node = cy.getElementById(`q-${record.questId}`) as BoardNode;
			if (node.length === 0) {
				busy = false;
				return;
			}
			if (record.from.statusId !== record.to.statusId) {
				try {
					await questsApi.changeStatus(record.questId, { status_slug: slugOf(target.statusId) });
					node.data('statusId', target.statusId);
					applyStatusChange(record.questId, target.statusId);
				} catch (e) {
					showToast(
						e instanceof Error ? e.message : t('common.statusChangeFailed', get(locale)),
						'error'
					);
					busy = false;
					return;
				}
			}
			node.animate({ position: { x: target.x, y: target.y }, duration: 120 });
			const canonical = visualToCanonical(target.x, target.y, target.statusId);
			node.data('absX', canonical.x);
			node.data('absY', canonical.y);
			questsApi.updatePosition(record.questId, canonical).catch(() => {});
			// DEV-115: undo/redo 로 움직인 노드도 위로.
			recentMoveZ += 1;
			node.style('z-index', recentMoveZ);
		} else {
			const promises: Promise<unknown>[] = [];
			for (const item of record.items) {
				const node = cy!.getElementById(`q-${item.questId}`) as BoardNode;
				if (node.length === 0) continue;
				const target = direction === 'undo' ? item.from : item.to;
				if (item.from.statusId !== item.to.statusId) {
					try {
						await questsApi.changeStatus(item.questId, { status_slug: slugOf(target.statusId) });
						node.data('statusId', target.statusId);
						applyStatusChange(item.questId, target.statusId);
					} catch (e) {
						showToast(
							e instanceof Error ? e.message : t('common.statusChangeFailed', get(locale)),
							'error'
						);
						continue;
					}
				}
				node.animate({ position: { x: target.x, y: target.y }, duration: 200 });
				const canonical = visualToCanonical(target.x, target.y, target.statusId);
				node.data('absX', canonical.x);
				node.data('absY', canonical.y);
				promises.push(questsApi.updatePosition(item.questId, canonical).catch(() => {}));
				// DEV-115: 배치 undo/redo 도 위로.
				recentMoveZ += 1;
				node.style('z-index', recentMoveZ);
			}
			await Promise.all(promises);
		}
		busy = false;
	}

	async function undo() {
		if (undoStack.length === 0 || busy) return;
		const record = undoStack[undoStack.length - 1];
		undoStack.pop();
		await applyRecord(record, 'undo');
		redoStack.push(record);
	}

	async function redo() {
		if (redoStack.length === 0 || busy) return;
		const record = redoStack[redoStack.length - 1];
		redoStack.pop();
		await applyRecord(record, 'redo');
		undoStack.push(record);
	}

	// ── Ctrl 모드 ──────────────────────────────────────────────

	function onCtrlDown() {
		if (ctrlHeld) return;
		ctrlHeld = true;
		container.style.cursor = 'crosshair';
	}
	function onCtrlUp() {
		ctrlHeld = false;
		container.style.cursor = '';
		cancelBoxSelection();
	}

	// ── 박스 선택 + Ctrl+클릭 ──────────────────────────────────

	function selectNodesInBox(sx1: number, sy1: number, sx2: number, sy2: number) {
		if (!cy) return;
		const zoom = cy.zoom(),
			pan = cy.pan();
		const mx1 = (sx1 - pan.x) / zoom,
			my1 = (sy1 - pan.y) / zoom;
		const mx2 = (sx2 - pan.x) / zoom,
			my2 = (sy2 - pan.y) / zoom;
		cy.nodes('[questId]').forEach((node) => {
			const pos = node.position();
			if (pos.x >= mx1 && pos.x <= mx2 && pos.y >= my1 && pos.y <= my2) node.select();
		});
	}

	function cancelBoxSelection() {
		boxDrag = null;
		boxDragStart = null;
	}

	function openNode(node: BoardNode) {
		if (!cy || node.length === 0) return;
		const quest = allQuests.find((q) => q.id === node.data('questId'));
		if (!quest) return;
		cy.nodes('[questId]').data('active', false).data('highlightType', '');
		cy.edges().data('dimmed', false);
		node.data('active', true);
		expandedQuest = quest;
		activeHighlights = new Set();
		cardPinned = false;
		cardDrag = null;
		syncExpandedPos();
	}

	function beginNodeInteraction(
		node: BoardNode,
		clientX: number,
		clientY: number,
		additive: boolean
	) {
		if (!cy || node.length === 0) return;
		const questId = node.data<number>('questId');
		dragStartMap.clear();
		dragStartMap.set(questId, {
			...node.position(),
			statusId: node.data<number>('statusId')
		});
		cy.nodes('[questId]:selected').forEach((selected) => {
			const id = selected.data<number>('questId');
			if (!dragStartMap.has(id)) {
				dragStartMap.set(id, {
					...selected.position(),
					statusId: selected.data<number>('statusId')
				});
			}
		});
		boardInteraction = {
			kind: 'node',
			sx: clientX,
			sy: clientY,
			primaryId: questId,
			additive,
			moved: false
		};
	}

	function beginPanInteraction(clientX: number, clientY: number) {
		if (!cy) return;
		boardInteraction = {
			kind: 'pan',
			sx: clientX,
			sy: clientY,
			startPan: cy.pan(),
			moved: false
		};
	}

	function moveBoardInteraction(clientX: number, clientY: number) {
		if (!cy || !boardInteraction) return;
		const dx = clientX - boardInteraction.sx;
		const dy = clientY - boardInteraction.sy;
		if (Math.hypot(dx, dy) > 3) boardInteraction.moved = true;
		if (boardInteraction.kind === 'pan') {
			cy.viewport({
				zoom: cy.zoom(),
				pan: {
					x: boardInteraction.startPan.x + dx,
					y: boardInteraction.startPan.y + dy
				}
			});
			return;
		}
		const zoom = cy.zoom();
		const worldDx = dx / zoom;
		const worldDy = dy / zoom;
		for (const [id, start] of dragStartMap) {
			const moving = cy.getElementById(`q-${id}`);
			if (moving.length > 0) {
				moving.position({ x: start.x + worldDx, y: start.y + worldDy });
			}
		}
		const primary = cy.getElementById(`q-${boardInteraction.primaryId}`);
		const visIdx = visibleLaneIdxAtVisualPoint(primary.position());
		const sid = statusIdAtVisibleIdx(visIdx);
		const slug = sid === null ? null : (sorted.find((status) => status.id === sid)?.slug ?? null);
		if (slug !== dragHighlightSlug) dragHighlightSlug = slug;
	}

	function queueFinishedNodeDrag() {
		if (!cy || dragStartMap.size === 0) return;
		dragHighlightSlug = null;
		for (const [qid, fromState] of dragStartMap) {
			const node = cy.getElementById(`q-${qid}`);
			if (node.length === 0) continue;
			const pos = node.position();
			const visMax = Math.max(0, visibleLaneCount() - 1);
			const visIdx = Math.max(0, Math.min(visibleLaneIdxAtVisualPoint(pos), visMax));
			const targetStatusId = statusIdAtVisibleIdx(visIdx) ?? fromState.statusId;
			pendingDragBatch.push({
				node,
				questId: qid,
				fromPos: { x: fromState.x, y: fromState.y },
				fromStatus: fromState.statusId,
				toPos: { ...pos },
				toLaneIdx: laneOf.get(targetStatusId) ?? 0
			});
			recentMoveZ += 1;
			node.style('z-index', recentMoveZ);
		}
		dragStartMap.clear();
		if (pendingDragTimer !== null) clearTimeout(pendingDragTimer);
		pendingDragTimer = setTimeout(() => {
			pendingDragTimer = null;
			void processPendingDrags();
		}, 0);
	}

	function endBoardInteraction() {
		if (!cy || !boardInteraction) return;
		const interaction = boardInteraction;
		boardInteraction = null;
		if (interaction.kind === 'node') {
			const node = cy.getElementById(`q-${interaction.primaryId}`);
			if (interaction.moved) {
				queueFinishedNodeDrag();
			} else {
				dragStartMap.clear();
				dragHighlightSlug = null;
				if (interaction.additive) {
					if (node.selected()) node.unselect();
					else node.select();
				} else {
					openNode(node);
				}
			}
			return;
		}
		if (!interaction.moved) {
			cy.elements().unselect();
			closeExpanded();
		}
	}

	function onNodeMouseDown(e: MouseEvent, nodeId: number) {
		if (!cy || e.button !== 0) return;
		e.preventDefault();
		e.stopPropagation();
		beginNodeInteraction(
			cy.getElementById(`q-${nodeId}`),
			e.clientX,
			e.clientY,
			e.ctrlKey || e.metaKey
		);
	}

	function onBoardMouseDown(e: MouseEvent) {
		if (!cy || e.button !== 0) return;
		if (e.ctrlKey || e.metaKey || ctrlHeld) {
			const rect = container.getBoundingClientRect();
			const sx = e.clientX - rect.left;
			const sy = e.clientY - rect.top;
			const palette = themePalette(currentEffectiveTheme());
			boxDrag = { left: sx, top: sy, width: 0, height: 0, color: palette.edgePre };
			boxDragStart = { x: sx, y: sy };
		} else {
			beginPanInteraction(e.clientX, e.clientY);
		}
		e.preventDefault();
	}

	function onBoxMouseMove(e: MouseEvent) {
		if (cardDrag) {
			const cw = container.clientWidth;
			const ch = container.clientHeight;
			expandedPos = {
				x: Math.max(0, Math.min(cw - CARD_W - 8, cardDrag.px + e.clientX - cardDrag.sx)),
				y: Math.max(0, Math.min(ch - 120, cardDrag.py + e.clientY - cardDrag.sy))
			};
			return;
		}
		if (boardInteraction) {
			moveBoardInteraction(e.clientX, e.clientY);
			return;
		}
		if (!boxDrag || !boxDragStart) return;
		const rect = container.getBoundingClientRect();
		const sx = e.clientX - rect.left;
		const sy = e.clientY - rect.top;
		const x1 = Math.min(boxDragStart.x, sx);
		const y1 = Math.min(boxDragStart.y, sy);
		boxDrag = {
			...boxDrag,
			left: x1,
			top: y1,
			width: Math.abs(sx - boxDragStart.x),
			height: Math.abs(sy - boxDragStart.y)
		};
	}

	function onBoxMouseUp(e: MouseEvent) {
		if (cardDrag) {
			cardDrag = null;
			return;
		}
		if (boardInteraction) {
			endBoardInteraction();
			return;
		}
		if (!boxDrag || !boxDragStart) return;
		const rect = container.getBoundingClientRect();
		const sx = e.clientX - rect.left;
		const sy = e.clientY - rect.top;
		const x1 = Math.min(boxDragStart.x, sx);
		const y1 = Math.min(boxDragStart.y, sy);
		const x2 = Math.max(boxDragStart.x, sx);
		const y2 = Math.max(boxDragStart.y, sy);
		if (x2 - x1 > 4 || y2 - y1 > 4) selectNodesInBox(x1, y1, x2, y2);
		cancelBoxSelection();
	}

	async function processPendingDrags() {
		const batch = pendingDragBatch.splice(0);
		if (batch.length === 0) return;
		const laneChanges = new Map<number, PendingDragItem[]>();
		for (const item of batch) {
			if (sorted[item.toLaneIdx].id !== item.fromStatus) {
				const existing = laneChanges.get(item.toLaneIdx) ?? [];
				existing.push(item);
				laneChanges.set(item.toLaneIdx, existing);
			}
		}

		const confirmedLanes = new Set<number>();
		const rejectedLanes = new Set<number>();
		for (const [laneIdx, items] of laneChanges) {
			const newStatus = sorted[laneIdx];
			const names = items.map((item) => item.node.data<string>('questSlug')).join(', ');
			const msg =
				items.length === 1
					? `${names} → "${statusLabel(newStatus, $locale)}"${t('board.confirmChangeSuffix', $locale)}`
					: `${items.length}${t('board.confirmChangeCountMid', $locale)}"${statusLabel(newStatus, $locale)}"${t('board.confirmChangeSuffix', $locale)}\n(${names})`;
			if (await showConfirm(msg)) confirmedLanes.add(laneIdx);
			else rejectedLanes.add(laneIdx);
		}

		const historyItems: BatchMove['items'] = [];
		const posUpdates: Promise<unknown>[] = [];
		for (const item of batch) {
			const { node, questId, fromPos, fromStatus, toPos, toLaneIdx } = item;
			const newStatus = sorted[toLaneIdx];
			const laneChanged = newStatus.id !== fromStatus;
			if (laneChanged && rejectedLanes.has(toLaneIdx)) {
				node.animate({ position: fromPos, duration: 150 });
				continue;
			}
			if (laneChanged && confirmedLanes.has(toLaneIdx)) {
				try {
					await questsApi.changeStatus(questId, { status_slug: newStatus.slug });
					node.data('statusId', newStatus.id);
					applyStatusChange(questId, newStatus.id);
				} catch (error) {
					node.animate({ position: fromPos, duration: 150 });
					showToast(
						error instanceof Error ? error.message : t('common.statusChangeFailed', get(locale)),
						'error'
					);
					continue;
				}
			}

			const snapped = gridSnap ? snapToGrid(toPos.x, toPos.y) : toPos;
			const finalStatusId =
				laneChanged && confirmedLanes.has(toLaneIdx) ? newStatus.id : fromStatus;
			const canonical = visualToCanonical(snapped.x, snapped.y, finalStatusId);
			const laneAbsLeft = absoluteLaneLeftOfStatus(finalStatusId);
			canonical.x = Math.max(
				laneAbsLeft + LANE_PAD_X + NODE_W / 2,
				Math.min(laneAbsLeft + LANE_W - LANE_PAD_X - NODE_W / 2, canonical.x)
			);
			const finalPoint = canonicalToVisual(canonical.x, canonical.y, finalStatusId);
			if (laneChanged || finalPoint.x !== toPos.x || finalPoint.y !== toPos.y) {
				node.position(finalPoint);
			}
			const moved =
				fromPos.x !== finalPoint.x || fromPos.y !== finalPoint.y || fromStatus !== finalStatusId;
			if (!moved) continue;
			node.data('absX', canonical.x);
			node.data('absY', canonical.y);
			historyItems.push({
				questId,
				from: { x: fromPos.x, y: fromPos.y, statusId: fromStatus },
				to: { x: finalPoint.x, y: finalPoint.y, statusId: finalStatusId }
			});
			posUpdates.push(questsApi.updatePosition(questId, canonical).catch(() => {}));
		}

		if (historyItems.length > 0) {
			const record: HistoryRecord =
				historyItems.length === 1
					? {
							type: 'single',
							questId: historyItems[0].questId,
							from: historyItems[0].from,
							to: historyItems[0].to
						}
					: { type: 'batch', items: historyItems };
			undoStack.push(record);
			if (undoStack.length > MAX_HISTORY) undoStack.shift();
			redoStack.length = 0;
		}
		await Promise.all(posUpdates);
		syncExpandedPos();
	}

	// ── 노드 확장 카드 ──────────────────────────────────────────

	function syncExpandedPos() {
		if (!cy || !expandedQuest || cardPinned) return;
		const node = cy.getElementById(`q-${expandedQuest.id}`) as BoardNode;
		if (node.length === 0) return;
		const rpos = node.renderedPosition();
		const cw = container.clientWidth;
		const ch = container.clientHeight;
		let x = rpos.x - CARD_W / 2;
		let y = rpos.y - 24;
		x = Math.max(8, Math.min(cw - CARD_W - 8, x));
		y = Math.max(8, Math.min(ch - 340, y));
		expandedPos = { x, y };
	}

	function closeExpanded() {
		if (cy) {
			cy.nodes('[questId]').data('active', false).data('highlightType', '');
			cy.edges().data('dimmed', false);
		}
		expandedQuest = null;
		activeHighlights = new Set();
		cardPinned = false;
		cardDrag = null;
	}

	// ── 하이라이트 (다중 선택) ─────────────────────────────────
	//   pre    = 이 퀘스트가 의존하는 선행 퀘스트     → 보라 #a371f7
	//   sub    = 이 퀘스트의 서브 퀘스트              → 청록 #3dc9b0
	//   next   = 이 퀘스트를 선행으로 갖는 후속 퀘스트 → 주황 #f0883e
	//   parent = 이 퀘스트의 부모 퀘스트              → 초록 var(--success)

	function applyHighlights(modes: Set<HighlightType>) {
		if (!cy || !expandedQuest) return;
		const qId = expandedQuest.id;
		const typeMap = new Map<number, HighlightType>();

		if (modes.has('pre'))
			allDependencies
				.filter((d) => d.quest_id === qId)
				.forEach((d) => typeMap.set(d.prerequisite_id, 'pre'));
		if (modes.has('sub'))
			allQuests.filter((q) => q.parent_quest_id === qId).forEach((q) => typeMap.set(q.id, 'sub'));
		if (modes.has('next'))
			allDependencies
				.filter((d) => d.prerequisite_id === qId)
				.forEach((d) => typeMap.set(d.quest_id, 'next'));
		if (modes.has('parent') && expandedQuest.parent_quest_id !== null)
			typeMap.set(expandedQuest.parent_quest_id, 'parent');

		cy.nodes('[questId]').forEach((n) => {
			const node = n as BoardNode;
			const nId = node.data('questId') as number;
			node.data('highlightType', nId === qId ? '' : (typeMap.get(nId) ?? 'dim'));
		});
		const litIds = new Set([qId, ...typeMap.keys()]);
		cy.edges().forEach((e) => {
			const s = e.source().data('questId') as number;
			const t = e.target().data('questId') as number;
			e.data('dimmed', !litIds.has(s) || !litIds.has(t));
		});
	}

	function toggleHighlight(mode: HighlightType) {
		const next = new Set(activeHighlights);
		if (next.has(mode)) next.delete(mode);
		else next.add(mode);
		activeHighlights = next;
		if (next.size > 0) applyHighlights(next);
		else clearHighlight();
	}

	function toggleAllHighlights() {
		const all: HighlightType[] = ['pre', 'sub', 'next', 'parent'];
		if (activeHighlights.size === 4) {
			activeHighlights = new Set();
			clearHighlight();
		} else {
			activeHighlights = new Set(all);
			applyHighlights(activeHighlights);
		}
	}

	/** 현재 활성 highlight 카테고리들에 해당하는 노드 ID + 확장된 본 퀘스트 ID 집합. */
	function getHighlightedNodeIds(): Set<number> {
		if (!expandedQuest) return new Set();
		const qId = expandedQuest.id;
		const ids = new Set<number>([qId]);
		if (activeHighlights.has('pre'))
			allDependencies.filter((d) => d.quest_id === qId).forEach((d) => ids.add(d.prerequisite_id));
		if (activeHighlights.has('sub'))
			allQuests.filter((q) => q.parent_quest_id === qId).forEach((q) => ids.add(q.id));
		if (activeHighlights.has('next'))
			allDependencies.filter((d) => d.prerequisite_id === qId).forEach((d) => ids.add(d.quest_id));
		if (activeHighlights.has('parent') && expandedQuest.parent_quest_id !== null)
			ids.add(expandedQuest.parent_quest_id);
		return ids;
	}

	/** 현재 highlight 된 노드들을 모두 선택 상태로 만든다. */
	function selectHighlighted() {
		if (!cy) return;
		const ids = getHighlightedNodeIds();
		if (ids.size === 0) return;
		cy.nodes('[questId]:selected').unselect();
		ids.forEach((id) => {
			const node = cy!.getElementById(`q-${id}`);
			if (node.length > 0) node.select();
		});
	}

	/**
	 * 그룹 정렬 (보드 전체 재배치, lane 무관 그룹 분석).
	 *
	 * 처리 순서:
	 *   1) 보드 전체 노드를 sub/prereq edge 로 connected components 로 분리
	 *      (cross-lane edge 포함 — 다른 lane 의 노드들이 한 그룹이 될 수 있음)
	 *   2) component 를 isolated (1개) / cluster (2개 이상) 로 분리
	 *   3) isolated 들을 lane 별로 row 0 부터 채움 (lane 끼리는 X 분리되어 있어서 같은 row OK)
	 *   4) 모든 lane 의 isolated 끝 row 의 max 를 global startRow 로
	 *   5) cluster 들을 startRow 부터 차례로:
	 *      - cluster 의 노드들은 자기 lane 안에서 lane cols 폭으로 채움
	 *      - cluster 가 차지하는 row 수 = max over lanes (ceil(이 lane 안 cluster 노드 수 / lane cols))
	 *      - 다음 cluster 는 그 row 끝 다음부터 (+ 1 row gap) → AABB Y 범위 분리
	 *   6) 그리드는 각 lane 의 laneCols 기준 (snap dot 위에 정렬)
	 *
	 * @param nodesToArrange 정렬 대상 노드. 빈 배열이면 보드 전체.
	 *                       부분 호출(예: 한 lane만)이면 그 노드들 사이 edge 로만 component 분석
	 *                       (cross-lane edge 가 외부 노드와 연결돼있어도 외부는 안 건드림)
	 * @param _cols          사용 안 함 — lane 의 laneCols 가 기준
	 */
	async function arrangeNodesGrouped(nodesToArrange: BoardNode[], _cols: number) {
		void _cols;
		if (!cy || arranging) return;
		// DEV-056 fix1: hidden 노드 제외 — 정렬 시 자리 안 차지하도록.
		nodesToArrange = nodesToArrange.filter((n) => n.style('display') !== 'none');
		// 빈 배열 시 no-op — 빈 lane 의 정렬 버튼이 전체 정렬을 trigger 하지 않도록.
		// 전체 정렬을 원하면 호출자가 명시적으로 모든 노드 전달 (toolbar 의 전체 정렬 버튼처럼).
		if (nodesToArrange.length === 0) return;
		arranging = true;
		try {
			const cellW = NODE_W + NODE_GAP;
			const cellH = NODE_H + NODE_GAP;
			const baseY = canonicalGridBaseY(ORIENTATION_METRICS);

			const allNodes = nodesToArrange;
			const allIds = new Set(allNodes.map((n) => n.data('questId') as number));

			// BUG-020 fix2: 사용자 피드백 — lane 한정 BFS 는 cross-lane 만으로 연결된
			// 같은 그룹의 노드 (lane 내엔 직접 edge 없음) 를 별도 cluster 로 쪼개버려
			// 같은 y 에 정렬할 수 있는데도 다른 y 에 배치됨.
			// → cluster 식별을 lane-local BFS 가 아닌 GLOBAL `groupOf` (cross-lane 포함
			//   전체 의존 그래프의 connected component) 기반으로 변경.
			//
			// 알고리즘:
			//   1) 각 lane-내 노드의 groupOf set 의 canonical 키 (min id) 로 묶음.
			//   2) 같은 키 = 같은 cluster.
			//   3) lane 안에 1 개 뿐인 group 이지만 외부 그룹과 연결 — cluster 처리
			//      (size 1, hasExternalEdge 와 같은 의미 — 자동 포함됨).
			const clusterMap = new Map<number, number[]>();
			const isolated: number[] = [];
			for (const id of allIds) {
				const fullGroup = groupOf.get(id);
				if (!fullGroup || fullGroup.size === 1) {
					// 진짜 단독 — 외부 연결도 없음 → isolated 위쪽.
					isolated.push(id);
					continue;
				}
				// canonical 키 = group 전체 (cross-lane 포함) 의 최소 id. 같은 그룹의
				// 모든 노드 (어느 lane 에 있든) 가 같은 키 → 같은 cluster.
				let key = Number.POSITIVE_INFINITY;
				for (const m of fullGroup) if (m < key) key = m;
				const arr = clusterMap.get(key);
				if (arr) arr.push(id);
				else clusterMap.set(key, [id]);
			}
			const clusters: number[][] = Array.from(clusterMap.values());

			const slugOf = (qid: number) =>
				(cy!.getElementById(`q-${qid}`) as BoardNode).data('questSlug') as string;
			const statusOf = (qid: number) =>
				(cy!.getElementById(`q-${qid}`) as BoardNode).data('statusId') as number;

			const batchItems: BatchMove['items'] = [];
			const savePromises: Promise<unknown>[] = [];
			const place = (qid: number, col: number, row: number) => {
				const node = cy!.getElementById(`q-${qid}`) as BoardNode;
				if (node.length === 0) return;
				const sid = node.data('statusId') as number;
				const li = laneOf.get(sid) ?? 0;
				const lcols = laneCols[li] ?? 2;
				const firstX = laneFirstCellX(li, lcols);
				const absX = firstX + col * cellW;
				const absY = baseY + row * cellH;
				const visual = canonicalToVisual(absX, absY, sid);
				const fromPos = { ...node.position() };
				if (Math.abs(fromPos.x - visual.x) < 0.5 && Math.abs(fromPos.y - visual.y) < 0.5) return;
				node.data('absX', absX);
				node.data('absY', absY);
				batchItems.push({
					questId: qid,
					from: { ...fromPos, statusId: sid },
					to: { ...visual, statusId: sid }
				});
				node.animate({ position: visual, duration: 200 });
				savePromises.push(questsApi.updatePosition(qid, { x: absX, y: absY }).catch(() => {}));
			};

			// 3) isolated 를 lane 별로 묶어서 row 0 부터 채움
			const isolatedByLane = new Map<number, number[]>();
			for (const qid of isolated) {
				const sid = statusOf(qid);
				const li = laneOf.get(sid) ?? 0;
				const arr = isolatedByLane.get(li) ?? [];
				arr.push(qid);
				isolatedByLane.set(li, arr);
			}

			let globalRow = 0;
			for (const [li, ids] of isolatedByLane) {
				ids.sort((a, b) => slugOf(a).localeCompare(slugOf(b)));
				const lcols = laneCols[li] ?? 2;
				ids.forEach((qid, i) => {
					const col = i % lcols;
					const r = Math.floor(i / lcols);
					place(qid, col, r);
				});
				globalRow = Math.max(globalRow, Math.ceil(ids.length / lcols));
			}
			// isolated 와 첫 cluster 사이 1 row gap (시각적 분리)
			if (isolated.length > 0 && clusters.length > 0) globalRow += 1;

			// 4) cluster 자체 순서: 첫 노드 슬러그 기준
			clusters.forEach((c) => c.sort((a, b) => slugOf(a).localeCompare(slugOf(b))));
			clusters.sort((a, b) => slugOf(a[0]).localeCompare(slugOf(b[0])));

			// 5) cluster 별 배치 — DEV-077: lane 겹침 없으면 같은 y 공유 가능.
			//
			// 이전: globalRow 가 순차 증가 — cluster A 가 lane 1,2 만 써도 cluster B
			//       는 무조건 그 아래 row 부터. 빈 공간 낭비.
			// 이후: laneNextRow[li] 로 각 lane 의 "다음 빈 row" 추적. cluster 시작 row
			//       = max(laneNextRow[참여 lane 들]). 다른 lane 만 사용하면 더 위에 들어감.
			//
			// 모든 lane 의 초기 row = globalRow (isolated 가 차지한 영역 다음).
			const laneNextRow = new Map<number, number>();
			clusters.forEach((cluster, ci) => {
				const byLane = new Map<number, number[]>();
				for (const qid of cluster) {
					const sid = statusOf(qid);
					const li = laneOf.get(sid) ?? 0;
					const arr = byLane.get(li) ?? [];
					arr.push(qid);
					byLane.set(li, arr);
				}
				// cluster 시작 row = 참여 lane 들의 next-row 의 max.
				let startRow = globalRow;
				for (const li of byLane.keys()) {
					startRow = Math.max(startRow, laneNextRow.get(li) ?? globalRow);
				}
				// 각 lane 안 cluster height.
				let clusterHeight = 1;
				for (const [li, ids] of byLane) {
					const lcols = laneCols[li] ?? 2;
					clusterHeight = Math.max(clusterHeight, Math.ceil(ids.length / lcols));
				}
				// 배치 + 참여 lane 들의 next-row 갱신 (+1 row gap 으로 시각 분리).
				for (const [li, ids] of byLane) {
					ids.sort((a, b) => slugOf(a).localeCompare(slugOf(b)));
					const lcols = laneCols[li] ?? 2;
					ids.forEach((qid, i) => {
						const col = i % lcols;
						const r = startRow + Math.floor(i / lcols);
						place(qid, col, r);
					});
					laneNextRow.set(li, startRow + clusterHeight + 1);
				}
				void ci; // ci 는 더 이상 순차 globalRow 증가에 사용 안 함.
			});

			if (batchItems.length > 0) {
				undoStack.push({ type: 'batch', items: batchItems });
				if (undoStack.length > MAX_HISTORY) undoStack.shift();
				redoStack.length = 0;
			}
			// animate 완료 + SQL 저장 둘 다 끝날 때까지 보호.
			await Promise.all([
				Promise.all(savePromises),
				new Promise<void>((r) => setTimeout(r, ARRANGE_ANIM_MS))
			]);
			syncExpandedPos();
		} finally {
			arranging = false;
		}
	}

	/** 확장 카드의 "그룹 정렬" 버튼 — 현재 highlight 된 노드들을 그룹 정렬. */
	async function arrangeHighlightedGroup() {
		if (!cy) return;
		const ids = getHighlightedNodeIds();
		if (ids.size === 0) return;
		const nodes: BoardNode[] = [];
		for (const id of ids) {
			const n = cy.getElementById(`q-${id}`) as BoardNode;
			if (n.length > 0) nodes.push(n);
		}
		await arrangeNodesGrouped(nodes, globalCols);
	}

	function clearHighlight() {
		if (!cy) return;
		cy.nodes('[questId]').data('highlightType', '');
		cy.edges().data('dimmed', false);
		activeHighlights = new Set();
	}

	// ── 노드 정렬 ───────────────────────────────────────────────

	async function arrangeNodes(targetStatusIds: number[] | null, cols: number) {
		if (!cy || arranging) return;
		arranging = true;
		const ids = targetStatusIds ?? sorted.map((s) => s.id);
		const startY = canonicalGridBaseY(ORIENTATION_METRICS);
		const cellW = NODE_W + NODE_GAP;
		const cellH = NODE_H + NODE_GAP;
		const batchItems: BatchMove['items'] = [];
		const savePromises: Promise<unknown>[] = [];

		for (const statusId of ids) {
			const li = laneOf.get(statusId) ?? 0;
			// 이 lane 의 cols 도 인자로 받은 cols 로 동기화 (snap grid 와 일치)
			laneCols[li] = cols;
			const firstX = laneFirstCellX(li, cols);
			// DEV-056 fix1: hidden 노드 제외 — 정렬 시 자리 안 차지하도록.
			const nodes = cy
				.nodes(`[statusId = ${statusId}]`)
				.filter((n) => (n as BoardNode).style('display') !== 'none');
			if (nodes.length === 0) continue;
			const sortedNodes = nodes.toArray().sort((a, b) => {
				const sa = (a as BoardNode).data('questSlug') as string;
				const sb = (b as BoardNode).data('questSlug') as string;
				return sa.localeCompare(sb);
			});
			sortedNodes.forEach((n, idx) => {
				const node = n as BoardNode;
				const fromPos = { ...node.position() };
				const col = idx % cols,
					row = Math.floor(idx / cols);
				const absX = firstX + col * cellW;
				const sid = node.data('statusId') as number;
				const absY = startY + row * cellH;
				const visual = canonicalToVisual(absX, absY, sid);
				node.data('absX', absX);
				node.data('absY', absY);
				batchItems.push({
					questId: node.data('questId') as number,
					from: { ...fromPos, statusId: sid },
					to: { ...visual, statusId: sid }
				});
				node.animate({ position: visual, duration: 200 });
				savePromises.push(
					questsApi
						.updatePosition(node.data('questId') as number, { x: absX, y: absY })
						.catch(() => {})
				);
			});
		}
		// laneCols 가 바뀌었으므로 trigger
		laneCols = [...laneCols];
		if (batchItems.length > 0) {
			undoStack.push({ type: 'batch', items: batchItems });
			if (undoStack.length > MAX_HISTORY) undoStack.shift();
			redoStack.length = 0;
		}
		// animate 완료 + SQL 저장 둘 다 끝날 때까지 보호 (BUG-008).
		await Promise.all([
			Promise.all(savePromises),
			new Promise<void>((r) => setTimeout(r, ARRANGE_ANIM_MS))
		]);
		syncExpandedPos();
		arranging = false;
	}

	// ── fit view ────────────────────────────────────────────────

	function fitView() {
		if (!cy) return;
		cy.fit(undefined, 60);
		syncLanes();
		syncExpandedPos();
	}

	// DEV-208/317: 터치스크린의 한 손가락 pan/노드 drag와 두 손가락 pinch를
	// BoardGraph viewport에 직접 반영한다.
	let pinch: { lastDist: number; lastMidX: number; lastMidY: number } | null = null;
	function touchDistMid(e: TouchEvent): { dist: number; midX: number; midY: number } {
		const a = e.touches[0];
		const b = e.touches[1];
		return {
			dist: Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY),
			midX: (a.clientX + b.clientX) / 2,
			midY: (a.clientY + b.clientY) / 2
		};
	}
	function onBoardTouchStart(e: TouchEvent) {
		if (!cy) return;
		const target = e.target as HTMLElement;
		const nodeEl = target.closest<HTMLElement>('.board-node');
		// pan/pinch는 노드 또는 명시적인 빈 보드 입력면에서 시작할 때만 받는다.
		// 나머지 UI에서는 preventDefault하지 않아 브라우저가 tap 뒤 click을 만든다.
		if (!nodeEl && !isBoardPanSurfaceTarget(target)) return;
		if (e.touches.length === 2) {
			e.preventDefault();
			// 한 손가락 node drag 도중 두 번째 손가락이 들어오면, pinch 로 전환하기
			// 전 아직 확정하지 않은 이동을 시작 좌표로 되돌린다.
			if (boardInteraction?.kind === 'node') {
				for (const [id, start] of dragStartMap) {
					const node = cy.getElementById(`q-${id}`);
					if (node.length > 0) node.position({ x: start.x, y: start.y });
				}
			}
			boardInteraction = null;
			dragStartMap.clear();
			dragHighlightSlug = null;
			const { dist, midX, midY } = touchDistMid(e);
			beginGridZoomTransform();
			pinch = { lastDist: dist, lastMidX: midX, lastMidY: midY };
			return;
		}
		if (e.touches.length !== 1) return;
		const touch = e.touches[0];
		if (nodeEl?.dataset.nodeId) {
			beginNodeInteraction(
				cy.getElementById(`q-${nodeEl.dataset.nodeId}`),
				touch.clientX,
				touch.clientY,
				false
			);
		} else {
			beginPanInteraction(touch.clientX, touch.clientY);
		}
		e.preventDefault();
	}
	function onBoardTouchMove(e: TouchEvent) {
		if (!cy) return;
		if (pinch && e.touches.length === 2) {
			e.preventDefault();
			const { dist, midX, midY } = touchDistMid(e);
			if (pinch.lastDist > 0 && dist > 0) {
				const rect = container.getBoundingClientRect();
				cy.zoom({
					level: cy.zoom() * (dist / pinch.lastDist),
					renderedPosition: { x: midX - rect.left, y: midY - rect.top }
				});
				cy.panBy({ x: midX - pinch.lastMidX, y: midY - pinch.lastMidY });
			}
			pinch = { lastDist: dist, lastMidX: midX, lastMidY: midY };
			return;
		}
		if (e.touches.length === 1 && boardInteraction) {
			e.preventDefault();
			moveBoardInteraction(e.touches[0].clientX, e.touches[0].clientY);
		}
	}
	function onBoardTouchEnd(e: TouchEvent) {
		if (pinch && e.touches.length < 2) {
			pinch = null;
			finishGridZoomTransform();
			if (e.touches.length === 1) {
				beginPanInteraction(e.touches[0].clientX, e.touches[0].clientY);
			}
			return;
		}
		if (e.touches.length === 0 && boardInteraction) endBoardInteraction();
	}

	// BUG-090(admin 후속): "마우스로 컨트롤시에는 이전과 같이 동작해야함" —
	// 트랙패드 two-finger 스크롤은 pan, 그러나 일반 마우스의 plain wheel 은
	// 예전처럼 줌이어야 한다는 피드백. 둘 다 ctrlKey=false 로 들어와 modifier 로는
	// 구분 못 함 — 별도 휴리스틱 필요.
	//
	// BUG-090: 마우스 휠 노치 판별. admin 노트북 트랙패드 *실제 보드* 실측(WebView2):
	//   - 마우스 휠      : wheelDeltaY = ±120 의 배수(WHEEL_DELTA 하드웨어 고정값).
	//   - 트랙패드 스크롤: wheelDeltaY ≈ -deltaY (예: dY=44 → wDY=-44) — 120 배수 아님.
	//                      *정수* deltaY 도 나오므로(44.00) 정수성/매그니튜드로는 구분 불가.
	// 따라서 wheelDeltaY 가 120 의 배수인지가 마우스 vs 트랙패드의 견고한 단일 신호.
	// (빠른 트랙패드 스크롤이 deltaY 50+ 여도 wheelDeltaY 는 그 값을 따라가 120 배수가
	// 아니므로 pan 유지 — 옛 `|deltaY|<50` 임계값이 빠른 스크롤을 줌으로 오판한 버그 해결.)
	function isMouseWheelNotch(e: WheelEvent): boolean {
		if (e.deltaMode !== 0) return true; // LINE/PAGE 단위 → 거의 항상 마우스
		const w = Math.abs((e as WheelEvent & { wheelDeltaY?: number }).wheelDeltaY ?? 0);
		return w !== 0 && w % 120 === 0; // ±120 배수 = 마우스 노치
	}

	// BUG-090: 트랙패드/마우스 줌 표준화(Figma/Miro 관례):
	//   - Ctrl + wheel = 커서 기준 줌. 트랙패드 줌은 Ctrl+two-finger 스크롤로 함
	//     (WebView2 는 트랙패드 pinch 를 Page Scale 줌으로 자체 소비해 DOM wheel
	//     이벤트로 안 내려보냄 — JS 에서 가로챌 수 없는 플랫폼 한계라 Ctrl+스크롤로 대체).
	//   - 일반 마우스의 plain wheel = 줌(admin 피드백 — 이전 동작 유지)
	//   - 트랙패드의 plain two-finger 스크롤만 pan
	// BUG-144: 트랙패드 연속 스크롤/줌은 프레임당 여러 wheel 이벤트가 들어오는데,
	// 같은 프레임 안 이벤트를 누적하고 BoardGraph viewport 반영을 1회로 제한.
	let wheelRaf: number | null = null;
	let pendingZoom: { level: number; x: number; y: number } | null = null;
	let pendingPan: { x: number; y: number } | null = null;

	// DEV-317: Canvas 경로가 완전히 사라졌으므로 아래 rAF는 순수하게 같은
	// 프레임의 wheel 입력을 한 viewport transform으로 합치는 역할만 한다.

	function flushBoardWheel() {
		wheelRaf = null;
		if (!cy) {
			pendingZoom = null;
			pendingPan = null;
			return;
		}
		if (pendingZoom) {
			cy.zoom({
				level: pendingZoom.level,
				renderedPosition: { x: pendingZoom.x, y: pendingZoom.y }
			});
			pendingZoom = null;
		}
		if (pendingPan) {
			cy.panBy(pendingPan);
			pendingPan = null;
		}
	}

	function onBoardWheel(e: WheelEvent) {
		if (!cy) return;
		e.preventDefault();
		const mouse = isMouseWheelNotch(e);
		// Ctrl+wheel(트랙패드 Ctrl+스크롤 또는 마우스 Ctrl+휠) 또는 plain 마우스 휠 = 줌.
		// 그 외(트랙패드 plain two-finger 스크롤) = pan.
		if (e.ctrlKey || mouse) {
			beginGridZoomTransform();
			scheduleGridZoomFinish();
			const rect = container.getBoundingClientRect();
			// 감도 분리: 마우스 노치는 deltaY≈100(캡 60) 으로 변화폭이 커서 둔감하게
			// (0.0012 — 캡 60 기준 약 7%, 기존 보드 체감 복원).
			// 트랙패드 Ctrl+스크롤은 deltaY 가 연속적이라(노치 아님) 민감하게(0.005) —
			// mouse(=wheelDeltaY 120배수)가 아닌 ctrlKey 줌이 여기 해당.
			const sensitivity = mouse ? 0.0012 : 0.005;
			const dy = Math.max(-60, Math.min(60, e.deltaY));
			// 같은 프레임에 이미 대기 중인 줌이 있으면 그 위에 누적(합성) —
			// 아니면 현재 cy 줌에서 시작.
			const baseLevel = pendingZoom ? pendingZoom.level : cy.zoom();
			pendingZoom = {
				level: baseLevel * Math.exp(-dy * sensitivity),
				x: e.clientX - rect.left,
				y: e.clientY - rect.top
			};
		} else {
			// 트랙패드 자연 스크롤: 아래로 스크롤(deltaY>0) → 콘텐츠가 위로.
			const dx = -e.deltaX;
			const dy = -e.deltaY;
			pendingPan = pendingPan ? { x: pendingPan.x + dx, y: pendingPan.y + dy } : { x: dx, y: dy };
		}
		if (wheelRaf === null) wheelRaf = requestAnimationFrame(flushBoardWheel);
	}

	// ── 키보드 ─────────────────────────────────────────────────

	function handleKeydown(e: KeyboardEvent) {
		if (['ControlLeft', 'ControlRight', 'MetaLeft', 'MetaRight'].includes(e.code)) onCtrlDown();
		const tag = (e.target as HTMLElement).tagName;
		if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
		const ctrl = e.ctrlKey || e.metaKey;
		if (
			isPerformanceMonitorShortcut(e.code, e.ctrlKey, e.metaKey, e.shiftKey, performanceEnabled)
		) {
			e.preventDefault();
			togglePerformanceMonitor();
		} else if (ctrl && e.code === 'KeyZ' && !e.shiftKey) {
			e.preventDefault();
			undo();
		} else if (ctrl && e.code === 'KeyZ' && e.shiftKey) {
			e.preventDefault();
			redo();
		} else if (!ctrl && e.code === 'KeyF') {
			e.preventDefault();
			fitView();
		} else if (!ctrl && e.code === 'KeyG') {
			e.preventDefault();
			toggleGridSnap();
		} else if (e.code === 'Escape') closeExpanded();
	}
	function handleKeyup(e: KeyboardEvent) {
		if (['ControlLeft', 'ControlRight', 'MetaLeft', 'MetaRight'].includes(e.code)) onCtrlUp();
	}

	// ── 초기화 ──────────────────────────────────────────────────
	// DEV-074 fix: theme 변경 시 모든 노드의 urgencyBg 를 갱신.
	// DEV-074 fix3: cy.style() 전체 교체 — Cytoscape 자체 색 값도 theme 반영.
	function refreshNodeBgForTheme() {
		if (!cy) return;
		const eff = currentEffectiveTheme();
		cy.nodes('[questId]').forEach((n) => {
			// BUG-122(admin 보고): 원인 — node data 엔 파생값(urgencyColor/urgencyBg)만
			// 저장하고 원본 `urgency` 는 애초에 저장한 적이 없어(DEV-074 최초 커밋부터),
			// `n.data('urgency')` 가 항상 undefined → urgencyBg 가 테마 전환 시 절대
			// 갱신되지 않았다. 노드가 active(클릭) 상태일 때만 이 값이 배경색으로
			// 노출돼([?active] 셀렉터) 평소엔 안 보이다가, 클릭해야 새로고침 시점
			// 테마의 색(어두운/밝은 urgency 톤)이 그대로 드러난 것 — 이게 "클릭하면
			// 까맣게/하얗게 변함" 의 정체. allQuests 에서
			// 원본 quest 를 찾아 quest.urgency 를 써야 한다.
			const qid = n.data('questId') as number | undefined;
			const q = qid != null ? allQuests.find((x) => x.id === qid) : undefined;
			if (q) {
				n.data('urgencyBg', urgencyBgFor(q.urgency, eff));
			}
		});
		// BUG-225: grid 점은 CSS palette를 직접 쓰므로 theme 전환 시 재생성 불필요.
	}

	onMount(() => {
		const unsubTheme = effectiveTheme.subscribe(() => refreshNodeBgForTheme());
		// BUG-225: 릴리스 빌드에서는 HUD 진입 경로 자체가 없다. Vite dev가 아닌
		// 패키징 debug 앱은 Rust 프로파일을 조회해야 정확히 판별할 수 있다.
		void (async () => {
			if (performanceEnabled || detectEnvironment() !== 'tauri') return;
			try {
				const { invoke } = await import('@tauri-apps/api/core');
				performanceEnabled = await invoke<boolean>('is_debug_build');
			} catch {
				/* 브라우저 production / 구 backend — HUD 비활성 유지 */
			}
		})();
		// DEV-317: locale 문자열은 실제 DOM text 라 $locale 변경에 Svelte 가
		// 직접 반응한다. 예전 SVG data URL 재생성 구독은 더 이상 필요 없다.

		// gridSnap 은 guildKeyPrefix 가 두 번째 onMount 에서 set 된 직후 다시
		// loadGridSnap 호출. 여기서는 listener 만.
		window.addEventListener('keydown', handleKeydown);
		window.addEventListener('keyup', handleKeyup);
		window.addEventListener('blur', onCtrlUp);
		window.addEventListener('mousemove', onBoxMouseMove);
		window.addEventListener('mouseup', onBoxMouseUp);
		container.addEventListener('mousedown', onBoardMouseDown);
		boardWrapEl.addEventListener('wheel', onBoardWheel, { passive: false });
		boardWrapEl.addEventListener('touchstart', onBoardTouchStart, { passive: false });
		boardWrapEl.addEventListener('touchmove', onBoardTouchMove, { passive: false });
		boardWrapEl.addEventListener('touchend', onBoardTouchEnd);
		boardWrapEl.addEventListener('touchcancel', onBoardTouchEnd);
		// 레인 설정 바깥 클릭으로 닫기. capture 로 받는다 — 안쪽 요소가
		// stopPropagation 을 해도(예: 보드 캔버스 드래그 시작) 놓치지 않는다.
		document.addEventListener('pointerdown', onDocPointerDown, true);
		return () => {
			unsubTheme();
			window.removeEventListener('keydown', handleKeydown);
			window.removeEventListener('keyup', handleKeyup);
			window.removeEventListener('blur', onCtrlUp);
			window.removeEventListener('mousemove', onBoxMouseMove);
			window.removeEventListener('mouseup', onBoxMouseUp);
			container.removeEventListener('mousedown', onBoardMouseDown);
			boardWrapEl.removeEventListener('wheel', onBoardWheel);
			boardWrapEl.removeEventListener('touchstart', onBoardTouchStart);
			boardWrapEl.removeEventListener('touchmove', onBoardTouchMove);
			boardWrapEl.removeEventListener('touchend', onBoardTouchEnd);
			boardWrapEl.removeEventListener('touchcancel', onBoardTouchEnd);
			document.removeEventListener('pointerdown', onDocPointerDown, true);
		};
	});

	onMount(async () => {
		try {
			// BUG-019: 길드별 localStorage namespace — guildKeyPrefix 를 fetch 후
			// 모든 localStorage 의존 상태 (hideSettings / globalCols / gridSnap /
			// viewport / laneCols) 가 그 prefix 로 접근. detect Tauri 안 되면
			// (web GUI) prefix 빈 채로 두어 기존 단일 namespace 유지.
			try {
				if (detectEnvironment() === 'tauri') {
					const { invoke } = await import('@tauri-apps/api/core');
					const path = await invoke<string>('current_guild_path');
					if (path) guildKeyPrefix = fnv1a32(path);
				}
			} catch {
				/* 무시 — prefix 빈 채로 fallback */
			}
			// prefix 확정 후 가벼운 영속 상태 즉시 로드 (init 이전 — UI 깜빡임 최소화).
			// DEV-135 fix: 보드로 바로 새로고침/재시작 시 questFilters store 는
			// 비어 있어 dim 이 안 걸렸다 (List 를 거쳐야만 store 채워짐). store 가
			// 비었으면 localStorage(List 와 동일 키)에서 필터를 hydrate.
			//
			// BUG-112 fix: questFilters 는 모듈 전역 store 라 다른 길드에서 필터를
			// 걸어둔 채 그 길드를 나가고 이 길드로 들어와도 메모리에 그대로 남아
			// 있음 — `!isFilterActive(...)` 가드가 "이미 뭔가 걸려있으면(다른
			// 길드 값이라도) 건드리지 않는다"로 오작동해, 다른 길드의 필터가
			// 새 길드에 그대로 유지되는 버그였음. localStorage 는 이미 gk() 로
			// 길드별로 스코프돼 있으므로, 항상 **이 길드의 저장값**으로 덮어써야
			// 맞다 — 없으면 EMPTY_FILTER 로 리셋.
			{
				const savedFilter = deserializeFilter(localStorage.getItem(gk(FILTER_STORAGE_SUFFIX)));
				questFilters.set(savedFilter ?? EMPTY_FILTER);
			}
			hideSettings = loadHideSettings();
			globalCols = loadGlobalCols();
			boardOrientation = loadBoardOrientation();
			// DEV-105 fix4: collapsed lane 상태 복원이 누락되어 새로고침 시
			// 모든 lane 이 펼쳐진 상태로 초기화되던 버그.
			collapsedLanes = loadCollapsedLanes();
			// DEV-105 fix5: 레인별 설정 영역 열림 상태 복원.
			lanesSettingsOpen = loadLanesSettingsOpen();
			try {
				gridSnap = localStorage.getItem(gk('gridSnap')) === 'true';
				// DEV-073: 같이 복원.
				toolbarCollapsed = localStorage.getItem(gk('toolbarCollapsed')) === 'true';
			} catch {
				/* 무시 */
			}

			await loadBoardData();
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
		}
	});

	async function loadBoardData() {
		const [quests, statuses, positions, dependencies, types] = await Promise.all([
			questsApi.list(true),
			metaApi.getQuestStatuses(),
			questsApi.listPositions(),
			questsApi.listDependencies(),
			metaApi.getQuestTypes() // DEV-135: 보드 필터 UI 의 타입 칩.
		]);
		boardTypes = types;
		await init(quests, statuses, positions, dependencies);
		// init 직후 store 에 flash id 가 이미 있으면 즉시 처리
		//  (Nav 의 New Quest 모달 → goto → 보드 페이지 도착 흐름)
		const pending = get(flashQuestId);
		if (pending) handleFlash(pending);
	}

	// DEV-033: List 필터 → Board 반영. 매치 안 되는 노드 dim (fdim data).
	// UX 는 'dim' (hide 가 아니라) — 위치 관계 (edge / lane 맥락) 보존.
	import {
		questFilters,
		isFilterActive,
		EMPTY_FILTER,
		serializeFilter,
		deserializeFilter,
		FILTER_STORAGE_SUFFIX,
		type QuestFilterState
	} from '$lib/stores/quest-filter';
	import { filterQuests as filterQuestsForBoard, type TriState } from '$lib/utils/quest-list';
	// DEV-135: 필터 활성 chip — 매치 수 표시 + 해제. allQuests 는 의도적
	// 비-reactive (BUG-054) 라 template 에서 직접 참조하지 않고 effect 가 복사.
	let filterMatchCount = $state(0);
	let filterTotalCount = $state(0);
	let filterActive = $state(false);
	// DEV-135 fix: dim 적용 로직을 함수로 분리. effect(필터 변경) 뿐 아니라 cy /
	// 노드가 만들어진 직후(render 완료)에도 명시 호출 — cy/allQuests 가 비반응
	// (BUG-054) 라, List→Board 전환처럼 cy 준비 전에 effect 가 한 번 돌고 끝나면
	// dim 이 영영 안 걸리던 버그(필터 걸어도 보드 변화 없음) 수정.
	function applyFilterDim() {
		if (!cy) return;
		const f = get(questFilters);
		if (!isFilterActive(f)) {
			filterActive = false;
			cy.nodes('[questId]').forEach((n) => {
				n.data('fdim', false);
			});
			// BUG-078: 필터 해제 시 edge 디밍도 해제.
			cy.edges().forEach((e) => {
				e.data('fdim', false);
			});
			return;
		}
		const prereqQuestIds = new Set(allDependencies.map((d) => d.quest_id));
		const parentIds = new Set(
			allQuests.map((q) => q.parent_quest_id).filter((p): p is number => p != null)
		);
		const matched = new Set(
			filterQuestsForBoard(allQuests, f.typeIds, f.statusIds, f.search, f.titleOnly, f.tags, {
				urgencies: f.urgencies,
				prereq: f.prereq,
				sub: f.sub,
				createdAfter: f.createdAfter,
				createdBefore: f.createdBefore,
				updatedAfter: f.updatedAfter,
				updatedBefore: f.updatedBefore,
				prereqQuestIds,
				parentIds
			}).map((q) => q.id)
		);
		filterActive = true;
		filterMatchCount = matched.size;
		filterTotalCount = allQuests.length;
		cy.nodes('[questId]').forEach((n) => {
			const qid = n.data('questId') as number;
			n.data('fdim', !matched.has(qid));
		});
		// BUG-078: edge 도 디밍 — 단 양 끝 노드가 모두 매치(둘 다 비-디밍)면 선명 유지.
		cy.edges().forEach((e) => {
			const s = e.source().data('questId') as number;
			const t = e.target().data('questId') as number;
			e.data('fdim', !matched.has(s) || !matched.has(t));
		});
	}
	$effect(() => {
		void $questFilters; // store 변경 추적 → dim 재적용.
		applyFilterDim();
	});

	// DEV-135: 필터 해제 — store 리셋 + URL 의 필터 param 제거.
	// URL 을 안 지우면 List 재진입 시 URL 파싱이 필터를 되살림. 정렬 (sort/desc)
	// 은 필터가 아니므로 유지.
	function clearBoardFilter() {
		questFilters.set(EMPTY_FILTER);
		saveFilterToStorage(EMPTY_FILTER);
		if (boardFilterReady) initBoardFilterFromStore();
		const url = new URL(window.location.href);
		for (const k of ['search', 'title_only', 'tags']) {
			url.searchParams.delete(k);
		}
		goto(`${url.pathname}${url.search}`, { replaceState: true, keepFocus: true, noScroll: true });
	}

	// ── DEV-135: 보드에서 필터 설정 ('보드 설정' 모달의 필터 섹션) ──
	// List 와 동일한 QuestListFilter 를 모달에 띄우고, 편집 시 공유 store +
	// localStorage(List 와 동일 키) 에 반영 → dim 즉시 갱신 + List 와 일관.
	let boardTypes = $state<QuestType[]>([]);
	let boardFilterReady = $state(false);
	let bfTypeIds = $state(new Set<number>());
	let bfStatusIds = $state(new Set<number>());
	let bfSearch = $state('');
	let bfTitleOnly = $state(false);
	let bfUrgencies = $state(new Set<number>());
	let bfPrereq = $state<TriState>('any');
	let bfSub = $state<TriState>('any');
	let bfCreatedAfter = $state('');
	let bfCreatedBefore = $state('');
	let bfUpdatedAfter = $state('');
	let bfUpdatedBefore = $state('');

	function saveFilterToStorage(f: QuestFilterState) {
		try {
			localStorage.setItem(gk(FILTER_STORAGE_SUFFIX), serializeFilter(f));
		} catch {
			/* 무시 */
		}
	}
	function initBoardFilterFromStore() {
		const f = get(questFilters);
		bfTypeIds = new Set(f.typeIds);
		bfStatusIds = new Set(f.statusIds);
		bfSearch = f.search;
		bfTitleOnly = f.titleOnly;
		bfUrgencies = new Set(f.urgencies);
		bfPrereq = f.prereq;
		bfSub = f.sub;
		bfCreatedAfter = f.createdAfter;
		bfCreatedBefore = f.createdBefore;
		bfUpdatedAfter = f.updatedAfter;
		bfUpdatedBefore = f.updatedBefore;
	}
	function boardFilterSnapshot(): QuestFilterState {
		return {
			typeIds: bfTypeIds,
			statusIds: bfStatusIds,
			search: bfSearch,
			titleOnly: bfTitleOnly,
			// tags 와 검색 범위는 board UI 에서 편집 안 함 — store 의 기존 값 유지.
			// BUG-243: 여기서 false 로 덮으면 보드에 들렀다 오는 것만으로 목록의
			// '댓글/첨부 이름 포함' 이 풀린다.
			tags: get(questFilters).tags,
			searchComments: get(questFilters).searchComments,
			searchAttachments: get(questFilters).searchAttachments,
			urgencies: bfUrgencies,
			prereq: bfPrereq,
			sub: bfSub,
			createdAfter: bfCreatedAfter,
			createdBefore: bfCreatedBefore,
			updatedAfter: bfUpdatedAfter,
			updatedBefore: bfUpdatedBefore
		};
	}
	// 보드 필터 편집 → store + localStorage. init 후(boardFilterReady) 에만.
	$effect(() => {
		const snap = boardFilterSnapshot(); // bf* 추적.
		if (!boardFilterReady) return;
		questFilters.set(snap);
		saveFilterToStorage(snap);
	});
	// '보드 설정' 모달 열림/닫힘에 맞춰 편집 상태 init / 비활성화.
	$effect(() => {
		if (showHideModal && !boardFilterReady) {
			initBoardFilterFromStore();
			boardFilterReady = true;
		} else if (!showHideModal && boardFilterReady) {
			boardFilterReady = false;
		}
	});

	// DEV-095: Nav reindex → board 데이터 reload.
	import { reindexBump } from '$lib/stores/reindex';
	let lastReindexBump = $state(0);
	$effect(() => {
		const bump = $reindexBump;
		if (bump !== lastReindexBump && bump > 0) {
			lastReindexBump = bump;
			loading = true;
			loadBoardData()
				.catch((e) => {
					error = e instanceof Error ? e.message : 'failed to reload';
				})
				.finally(() => {
					loading = false;
				});
		}
	});

	// 보드가 이미 마운트된 상태에서 새 quest 가 만들어지면 store 만 변하므로 effect 로 처리
	$effect(() => {
		const fid = $flashQuestId;
		if (!fid || !cy) return;
		handleFlash(fid);
	});

	// gridSnap 토글 시 lane-col background 즉시 업데이트
	$effect(() => {
		void gridSnap;
		if (cy) syncLanes();
	});

	/** Toolbar 의 1/2/3열 select 변경 핸들러 — 모든 레인의 그리드만 즉시 갱신. */
	function setGlobalCols(cols: number) {
		globalCols = cols;
		saveGlobalCols(cols);
		if (!cy) return;
		laneCols = laneCols.map(() => cols);
		// 모든 lane 의 status slug 에 같은 값으로 영속.
		const map: Record<string, number> = {};
		sorted.forEach((s) => {
			map[statusSlug(s.name_en)] = cols;
		});
		saveLaneColsMap(map);
		headersEl?.querySelectorAll<HTMLSelectElement>('.lane-cols-sel').forEach((sel) => {
			sel.value = String(cols);
		});
		syncLanes();
	}

	/**
	 * 새로 만들어진 (또는 다시 강조해야 하는) 퀘스트 노드를 보드에 보이게 하고 펄스 효과.
	 * 노드가 보드에 없으면 새로 추가, 있으면 그대로. 그 후 panTo + 시각 강조 후 store clear.
	 */
	async function handleFlash(qid: number) {
		if (!cy) return;
		try {
			let quest = allQuests.find((q) => q.id === qid);
			if (!quest) {
				// 보드 init 후 만들어진 새 quest — 목록 다시 가져오기
				const fresh = await questsApi.list(true);
				allQuests = fresh;
				quest = fresh.find((q) => q.id === qid);
			}
			if (!quest || !cy) {
				flashQuestId.set(null);
				return;
			}

			let node = cy.getElementById(`q-${qid}`) as BoardNode;
			if (node.length === 0) {
				// 보드에 없는 노드 — 적당한 위치에 추가하고 위치 저장
				const li = laneOf.get(quest.status_id) ?? 0;
				// 같은 레인의 기존 노드들 아래에 자연스럽게 배치
				const existing = cy.nodes(`[statusId = ${quest.status_id}]`).toArray();
				const maxAbsY = existing.reduce(
					(m, n) =>
						Math.max(
							m,
							((n as BoardNode).data('absY') as number | undefined) ??
								canonicalGridBaseY(ORIENTATION_METRICS)
						),
					canonicalGridBaseY(ORIENTATION_METRICS) - NODE_H - NODE_GAP
				);
				const absX = li * LANE_STRIDE + LANE_W / 2;
				const absY = maxAbsY + NODE_H + NODE_GAP;
				const visual = canonicalToVisual(absX, absY, quest.status_id);

				cy.add({
					group: 'nodes',
					data: {
						id: `q-${qid}`,
						label: '',
						questId: qid,
						questSlug: quest.quest_id,
						statusId: quest.status_id,
						urgencyColor: urgencyColor(quest.urgency),
						urgencyBg: urgencyBgFor(quest.urgency, currentEffectiveTheme()),
						typeColor: quest.type_color,
						highlightType: '',
						active: false,
						absX,
						absY
					},
					position: visual
				});
				if (quest.parent_quest_id) {
					cy.add({
						data: {
							id: `sub-${quest.parent_quest_id}-${qid}`,
							source: `q-${quest.parent_quest_id}`,
							target: `q-${qid}`,
							etype: 'sub',
							dimmed: false
						}
					});
				}
				// DEV-067: DB 는 absolute X.
				questsApi.updatePosition(qid, { x: absX, y: absY }).catch(() => {});
				node = cy.getElementById(`q-${qid}`) as BoardNode;
			}

			// panTo (해당 노드를 화면 중앙으로)
			cy.center(node);

			// 시각 강조 — 1.5초 동안 flash data on, 그 후 off
			node.data('flash', true);
			setTimeout(() => {
				const n = cy?.getElementById(`q-${qid}`);
				if (n && n.length > 0) n.data('flash', false);
			}, 1500);
		} finally {
			// store 는 즉시 clear (효과는 setTimeout 으로 진행 중)
			flashQuestId.set(null);
		}
	}

	onDestroy(() => {
		if (wheelRaf !== null) cancelAnimationFrame(wheelRaf); // BUG-144
		if (domGraphRaf !== null) cancelAnimationFrame(domGraphRaf); // DEV-317
		if (performanceRaf !== null) cancelAnimationFrame(performanceRaf); // BUG-225
		if (gridZoomEndTimer !== null) clearTimeout(gridZoomEndTimer); // BUG-225
		boardWrapEl?.removeEventListener('wheel', onBoardWheel); // BUG-090
		// DEV-208: 터치 핀치.
		boardWrapEl?.removeEventListener('touchstart', onBoardTouchStart);
		boardWrapEl?.removeEventListener('touchmove', onBoardTouchMove);
		boardWrapEl?.removeEventListener('touchend', onBoardTouchEnd);
		boardWrapEl?.removeEventListener('touchcancel', onBoardTouchEnd);
		cy?.destroy();
	});

	// ── 레인 HTML ───────────────────────────────────────────────

	/**
	 * 레인 설정 팝오버를 ⚙ 버튼에 맞춰 배치.
	 *
	 * 팝오버가 헤더 밖(별도 레이어)에 있으므로 CSS 로는 버튼을 기준 삼을 수
	 * 없다 — 좌표를 직접 계산한다. 기준은 `.board-wrap`(팝오버 레이어의
	 * offsetParent).
	 *
	 * 세로 배치: 버튼 **아래**, 오른쪽 끝을 버튼에 맞춰 **왼쪽으로** 펼친다.
	 * ⚙ 가 헤더 오른쪽 끝에 있어 오른쪽으로 펼치면 옆 레인을 덮기 때문
	 * (admin 보고). 그러다 화면 왼쪽으로 나가면 반대로 뒤집는다.
	 * 가로 배치: 헤더가 세로라 버튼 **오른쪽**에 붙인다.
	 */
	function positionLanePop(btn: HTMLElement, pop: HTMLElement) {
		if (!boardWrapEl) return;
		const wrap = boardWrapEl.getBoundingClientRect();
		const b = btn.getBoundingClientRect();
		const w = pop.offsetWidth;
		if (boardOrientation === 'rows') {
			pop.style.top = `${b.top - wrap.top}px`;
			pop.style.left = `${b.right - wrap.left + 2}px`;
			return;
		}
		pop.style.top = `${b.bottom - wrap.top + 2}px`;
		// 왼쪽으로 펼쳤을 때 화면 밖으로 나가면 버튼 왼쪽 기준으로 뒤집는다.
		const leftAligned = b.right - w;
		pop.style.left = `${(leftAligned < wrap.left + 4 ? b.left : leftAligned) - wrap.left}px`;
	}

	/** 열려 있는 팝오버들을 다시 배치 — pan / zoom / 레인 크기 변경 후. */
	function repositionOpenLanePops() {
		if (!lanePopLayerEl || !headersEl) return;
		headersEl.querySelectorAll<HTMLElement>('.lane-hdr.settings-open').forEach((hdr) => {
			const btn = hdr.querySelector<HTMLElement>('.lane-settings-btn');
			const slug = btn?.dataset.laneSlug;
			if (!btn || !slug) return;
			const pop = lanePopLayerEl.querySelector<HTMLElement>(
				`.lane-settings-pop[data-lane-slug="${CSS.escape(slug)}"]`
			);
			if (pop) positionLanePop(btn, pop);
		});
	}

	function buildLaneDivs(sorted: QuestStatus[]) {
		lanesEl.innerHTML = '';
		if (lanePopLayerEl) lanePopLayerEl.innerHTML = '';
		gridLanesEl.innerHTML = '';
		sorted.forEach(() => {
			const col = document.createElement('div');
			col.className = 'lane-col';
			lanesEl.appendChild(col);
			const gridCol = document.createElement('div');
			gridCol.className = 'lane-grid-col';
			gridCol.classList.toggle('orientation-rows', boardOrientation === 'rows');
			// BUG-225: viewport 크기의 CSS grid만 유지한다. 거대한 world bitmap을
			// 만들지 않는다. 자식 DOM 없이 이 레인 요소의 CSS 다중 background만
			// laneCols만큼 겹쳐 세로 반복한다.
			gridLanesEl.appendChild(gridCol);
		});
		headersEl.innerHTML = '';
		// laneCols / laneArrangeModes 초기값.
		// laneCols: localStorage 의 status slug 별 저장값 (BUG-009). 없으면 globalCols.
		const savedLaneCols = loadLaneColsMap();
		laneCols = sorted.map((s) => savedLaneCols[statusSlug(s.name_en)] ?? globalCols);
		laneArrangeModes = sorted.map(() => arrangeMode);
		sorted.forEach((s, li) => {
			const hdr = document.createElement('div');
			hdr.className = 'lane-hdr';
			// DEV-105 fix2: collapsed 상태 시 lane-hdr 도 같이 마킹 → CSS 가
			// label 외 자식 (cols-sel, arrange-group) 숨김.
			if (collapsedLanes.has(s.slug)) hdr.classList.add('collapsed');
			const label = document.createElement('button');
			label.className = 'lane-label';
			label.classList.toggle('lane-label-en', get(locale) === 'en');
			// DEV-015: 언어 반응 표시 이름(레인 저장 키는 여전히 name_en 기반 —
			// statusSlug(s.name_en) — 표시만 바뀜).
			label.textContent = statusLabel(s, get(locale));
			label.style.color = s.color;
			// DEV-105: 클릭으로 collapse 토글. label 이 button — keyboard / 접근성 OK.
			label.type = 'button';
			label.title = t('board.laneToggle', get(locale));
			if (collapsedLanes.has(s.slug)) label.classList.add('collapsed');
			label.onclick = () => {
				toggleLaneCollapsed(s.slug);
				const on = collapsedLanes.has(s.slug);
				label.classList.toggle('collapsed', on);
				hdr.classList.toggle('collapsed', on);
			};
			const sel = document.createElement('select');
			sel.className = 'lane-cols-sel';
			sel.title = t(
				boardOrientation === 'columns' ? 'board.laneSortCols' : 'board.laneSortRows',
				get(locale)
			);
			const initialCols = laneCols[li];
			[1, 2, 3].forEach((n) => {
				const opt = document.createElement('option');
				opt.value = String(n);
				opt.textContent = `${n}${t(
					boardOrientation === 'columns' ? 'board.colSuffix' : 'board.rowSuffix',
					get(locale)
				)}`;
				if (n === initialCols) opt.selected = true;
				sel.appendChild(opt);
			});
			// select 변경 시 laneCols 즉시 업데이트 + localStorage 영속 (BUG-009).
			sel.onchange = () => {
				const cols = parseInt(sel.value);
				laneCols[li] = cols;
				laneCols = [...laneCols]; // reactive trigger
				const map = loadLaneColsMap();
				map[statusSlug(s.name_en)] = cols;
				saveLaneColsMap(map);
				syncLanes(); // grid 시각화 재계산
			};
			const btn = document.createElement('button');
			btn.className = 'lane-arrange-btn';
			btn.title = 'Arrange this lane';
			btn.textContent = '⊟';

			// lane 별 정렬 모드 select (Group/All) — 전역 toolbar 의 mode select 와 같은 역할
			const modeSel = document.createElement('select');
			modeSel.className = 'lane-mode-sel';
			modeSel.title = t('board.laneSortMode', get(locale));
			(['group', 'all'] as const).forEach((v) => {
				const opt = document.createElement('option');
				opt.value = v;
				opt.textContent = v === 'group' ? 'Group' : 'All';
				if (v === arrangeMode) opt.selected = true;
				modeSel.appendChild(opt);
			});
			modeSel.onchange = () => {
				laneArrangeModes[li] = modeSel.value as 'all' | 'group';
				laneArrangeModes = [...laneArrangeModes];
			};

			btn.onclick = () => {
				const cols = parseInt(sel.value);
				const mode = laneArrangeModes[li] ?? arrangeMode;
				if (mode === 'group' && cy) {
					// 이 lane 의 노드만으로 group 정렬 (cross-lane edge 무시)
					const laneNodes = cy.nodes(`[statusId = ${s.id}]`).toArray() as BoardNode[];
					arrangeNodesGrouped(laneNodes, cols);
				} else {
					arrangeNodes([s.id], cols);
				}
			};

			// ⊟ 버튼 + mode select 를 segmented 묶음으로 (toolbar 의 [Arrange][Mode] 와 동일한 패턴)
			const arrangeWrap = document.createElement('div');
			arrangeWrap.className = 'lane-arrange-group';
			arrangeWrap.appendChild(btn);
			arrangeWrap.appendChild(modeSel);

			// BUG: cols-sel + arrange-group 을 헤더와 같은 줄에 인라인으로 두면
			// lane 폭(zoom 에 비례, 모바일에서 매우 좁아짐)이 실제 필요폭보다 작을 때
			// 옆 lane 헤더 밑으로 잘려서 안 보임 — 헤더 아래로 뜨는 팝오버로 분리.
			const pop = document.createElement('div');
			pop.className = 'lane-settings-pop';
			// 헤더 밖 레이어에 살기 때문에 어느 레인 것인지 표시가 필요하다.
			pop.dataset.laneSlug = s.slug;
			pop.appendChild(sel);
			pop.appendChild(arrangeWrap);

			// DEV-105 fix5: 레인별 설정 (cols-sel + arrange-group) 토글 ⚙.
			// 자주 안 쓰는데 영역만 차지하므로 기본 접힘 — 사용자가 펼침.
			const settingsBtn = document.createElement('button');
			settingsBtn.className = 'lane-settings-btn';
			settingsBtn.type = 'button';
			settingsBtn.dataset.laneSlug = s.slug;
			settingsBtn.textContent = '⚙';
			const setOpenAttrs = () => {
				const open = lanesSettingsOpen.has(s.slug);
				settingsBtn.title = open
					? t('board.laneSettingsCollapse', get(locale))
					: t('board.laneSettingsExpand', get(locale));
				settingsBtn.setAttribute('aria-expanded', String(open));
				hdr.classList.toggle('settings-open', open);
				// 팝오버가 헤더 밖에 있으므로 표시 여부는 자기 클래스로 정한다.
				pop.classList.toggle('open', open);
				if (open) positionLanePop(settingsBtn, pop);
			};
			setOpenAttrs();
			settingsBtn.onclick = () => {
				toggleLaneSettings(s.slug);
				setOpenAttrs();
			};

			// DEV-059 fix2: lane 순서 변경은 '보드 설정' 모달로 이전 — 헤더에 ◀ ▶ 안 둠.
			// 헤더 폭이 좁아질 때 라벨이 가려지는 문제 회피.
			// BUG(admin 보고): 팝오버가 `.lane-hdr` 기준으로 `left: 0` 이라 헤더
			// 왼쪽 끝, 즉 **레인 제목 아래**에 떴다. ⚙ 는 `.lane-label { flex: 1 }`
			// 때문에 헤더 오른쪽 끝에 있으므로 버튼과 한참 떨어진다.
			// 버튼과 팝오버를 `position: relative` 래퍼로 묶어 **버튼 기준**으로
			// 위치를 잡고, 버튼 오른쪽에 맞춰 왼쪽으로 펼친다 — 그래야 자기
			// 레인 안에 머문다(오른쪽으로 펼치면 옆 레인을 덮는다).
			hdr.appendChild(label);
			hdr.appendChild(settingsBtn);
			headersEl.appendChild(hdr);
			lanePopLayerEl?.appendChild(pop);
		});
	}

	function syncScreenGrid(pan: BoardPoint, zoom: number) {
		if (!gridLanesEl) return;
		const cellW = NODE_W + NODE_GAP;
		const cellH = NODE_H + NODE_GAP;
		const baseY = canonicalGridBaseY(ORIENTATION_METRICS);
		const safeZoom = Math.max(zoom, 0.0001);
		const metrics = screenGridMetrics(safeZoom, boardOrientation === 'columns' ? cellH : cellW);
		const dotRadiusWorld = metrics.dotRadius / safeZoom;
		const dotFeatherWorld = 0.55 / safeZoom;
		const dotBand = dotRadiusWorld * 2 + dotFeatherWorld * 2;
		const viewportWorldHeight = Math.max(container.clientHeight / safeZoom, 1);
		const viewportWorldWidth = Math.max(container.clientWidth / safeZoom, 1);
		const gridTop = -pan.y / safeZoom - viewportWorldHeight;
		const gridHeight = viewportWorldHeight * 3;
		const gridLeft = -pan.x / safeZoom - viewportWorldWidth;
		const gridWidth = viewportWorldWidth * 3;
		let laneStart = 0;
		gridLanesEl.querySelectorAll<HTMLElement>('.lane-grid-col').forEach((gridCol, i) => {
			const s = sorted[i];
			const laneHidden = s ? getHideSetting(s.slug).laneHidden : false;
			if (laneHidden) {
				gridCol.style.display = 'none';
				return;
			}
			const laneSize = s
				? boardOrientation === 'columns'
					? laneWidth(s.slug)
					: rowHeight(s.slug)
				: boardOrientation === 'columns'
					? LANE_W
					: ROW_LANE_H;
			const collapsed = s ? collapsedLanes.has(s.slug) : false;
			if (!gridSnap || collapsed) {
				gridCol.style.display = 'none';
				laneStart += laneSize + LANE_GAP;
				return;
			}

			const cols = Math.max(1, Math.min(3, laneCols[i] ?? 2));
			gridCol.style.display = '';
			gridCol.style.bottom = '';
			gridCol.style.right = '';
			gridCol.classList.remove('grid-cols-1', 'grid-cols-2', 'grid-cols-3');
			gridCol.classList.add(`grid-cols-${cols}`);
			if (boardOrientation === 'columns') {
				const firstCxLocal = laneFirstCellX(i, cols) - i * LANE_STRIDE;
				const columnCenters = screenGridColumnCenters(firstCxLocal, cellW, 1, cols);
				gridCol.style.left = `${laneStart}px`;
				gridCol.style.width = `${laneSize}px`;
				gridCol.style.top = `${gridTop}px`;
				gridCol.style.height = `${gridHeight}px`;
				gridCol.style.backgroundSize = `${dotBand}px ${cellH}px`;
				gridCol.style.backgroundPosition = columnCenters
					.map((centerX) => `${centerX - dotBand / 2}px ${baseY - gridTop - cellH / 2}px`)
					.join(', ');
			} else if (s) {
				const absoluteStart = absoluteLaneLeftOfStatus(s.id);
				const firstX = laneFirstCellX(i, cols);
				const rowCenters = Array.from(
					{ length: cols },
					(_, row) =>
						canonicalToBoardPoint(
							{ x: firstX + row * cellW, y: baseY },
							absoluteStart,
							laneStart,
							'rows',
							ORIENTATION_METRICS
						).y - laneStart
				);
				gridCol.style.left = `${gridLeft}px`;
				gridCol.style.width = `${gridWidth}px`;
				gridCol.style.top = `${laneStart}px`;
				gridCol.style.height = `${laneSize}px`;
				gridCol.style.backgroundSize = `${cellW}px ${dotBand}px`;
				gridCol.style.backgroundPosition = rowCenters
					.map(
						(centerY) =>
							`${rowGridBaseX(ORIENTATION_METRICS) - gridLeft - cellW / 2}px ${centerY - dotBand / 2}px`
					)
					.join(', ');
			}
			gridCol.style.setProperty('--grid-dot-radius', `${dotRadiusWorld}px`);
			gridCol.style.setProperty('--grid-dot-feather', `${dotFeatherWorld}px`);
			laneStart += laneSize + LANE_GAP;
		});
	}

	/**
	 * zoom gesture 시작 순간의 world-space grid를 texture로 고정한다.
	 * 제스처 중에는 radial-gradient를 매 프레임 재 paint하지 않고
	 * 부모에 노드 월드와 동일한 pan/zoom transform만 적용한다.
	 */
	function beginGridZoomTransform() {
		if (!cy || !gridLanesEl || gridZoomActive) return;
		const pan = cy.pan();
		const zoom = cy.zoom();
		syncScreenGrid(pan, zoom);
		gridZoomActive = true;
	}

	function finishGridZoomTransform() {
		if (gridZoomEndTimer !== null) {
			clearTimeout(gridZoomEndTimer);
			gridZoomEndTimer = null;
		}
		gridZoomActive = false;
		if (cy) syncViewportVisuals();
	}

	function scheduleGridZoomFinish() {
		if (gridZoomEndTimer !== null) clearTimeout(gridZoomEndTimer);
		gridZoomEndTimer = setTimeout(finishGridZoomTransform, 120);
	}

	/** 제스처 hot path: 모든 world 요소가 공유하는 transform 하나만 갱신. */
	function syncViewportVisuals() {
		if (!cy) return;
		const pan = cy.pan();
		const zoom = cy.zoom();
		viewportVisualUpdateCount += 1;
		const nextLod = boardLodForZoom(zoom);
		if (nextLod !== boardLod) boardLod = nextLod;
		if (worldViewportEl) {
			// 제스처 중에만 3D compositor layer를 사용한다. 종료 후
			// 동일한 pan/zoom의 2D transform으로 바꿔 WebKit backing layer를
			// 다시 구성하되 폰트/노드 layout 크기는 절대 변경하지 않는다.
			worldViewportEl.style.transform = gridZoomActive
				? `translate3d(${pan.x}px, ${pan.y}px, 0) scale(${zoom})`
				: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`;
		}
		if (!gridZoomActive) {
			syncScreenGrid(pan, zoom);
		}
		// 단색 lane은 교차축만 camera를 따르고 반대축은 viewport를 채운다.
		let laneStart = 0;
		lanesEl?.querySelectorAll<HTMLElement>('.lane-col').forEach((col, i) => {
			const s = sorted[i];
			const laneHidden = s ? getHideSetting(s.slug).laneHidden : false;
			if (laneHidden) {
				col.style.display = 'none';
				return;
			}
			const size = s
				? boardOrientation === 'columns'
					? laneWidth(s.slug)
					: rowHeight(s.slug)
				: boardOrientation === 'columns'
					? LANE_W
					: ROW_LANE_H;
			col.style.display = '';
			if (boardOrientation === 'columns') {
				col.style.left = `${laneStart * zoom + pan.x}px`;
				col.style.width = `${size * zoom}px`;
				col.style.top = '0';
				col.style.bottom = '0';
				col.style.right = '';
				col.style.height = '';
			} else {
				col.style.left = '0';
				col.style.right = '0';
				col.style.width = '';
				col.style.top = `${laneStart * zoom + pan.y}px`;
				col.style.height = `${size * zoom}px`;
				col.style.bottom = '';
			}
			laneStart += size + LANE_GAP;
		});
		// Header는 screen-space 크기를 유지하고 lane의 교차축 위치만 따른다.
		let headerStart = 0;
		headersEl?.querySelectorAll<HTMLElement>('.lane-hdr').forEach((hdr, i) => {
			const s = sorted[i];
			const laneHidden = s ? getHideSetting(s.slug).laneHidden : false;
			if (laneHidden) {
				hdr.style.display = 'none';
				return;
			}
			const size = s
				? boardOrientation === 'columns'
					? laneWidth(s.slug)
					: rowHeight(s.slug)
				: boardOrientation === 'columns'
					? LANE_W
					: ROW_LANE_H;
			hdr.style.display = '';
			if (boardOrientation === 'columns') {
				hdr.style.left = `${headerStart * zoom + pan.x}px`;
				hdr.style.width = `${size * zoom}px`;
				hdr.style.top = '0';
				hdr.style.right = '';
				hdr.style.height = '38px';
			} else {
				hdr.style.left = '0';
				hdr.style.right = '';
				hdr.style.width = '38px';
				hdr.style.top = `${headerStart * zoom + pan.y}px`;
				hdr.style.height = `${Math.max(size * zoom, 1)}px`;
			}
			headerStart += size + LANE_GAP;
		});
		// 헤더가 움직였으면 열려 있는 팝오버도 따라가야 한다 — 별도 레이어에
		// 있어 CSS 로는 안 따라온다.
		repositionOpenLanePops();
		syncExpandedPos();
	}

	function syncLanes() {
		if (!cy) return;
		// DEV-334: 레인 수/화면 폭이 바뀌면 최소 zoom 도 따라가야 한다(상태 추가,
		// 창 크기 변경, 폰 회전). syncLanes 는 그 모든 경우에 호출된다.
		cy.minZoom(computeMinZoom());

		const bb = cy.elements().nonempty() ? cy.elements().boundingBox() : null;
		const minZoom = Math.max(computeMinZoom(), 0.02);
		let laneExtent = 0;
		sorted.forEach((s) => {
			const laneHidden = getHideSetting(s.slug).laneHidden;
			if (laneHidden) return;
			laneExtent +=
				(boardOrientation === 'columns' ? laneWidth(s.slug) : rowHeight(s.slug)) + LANE_GAP;
		});
		laneExtent = Math.max(laneExtent - LANE_GAP, 1);
		const worldWidth =
			boardOrientation === 'columns'
				? laneExtent
				: Math.max(container.clientWidth / minZoom + 200, (bb?.x2 ?? 0) + NODE_W + 600, 2000);
		const worldHeight =
			boardOrientation === 'columns'
				? Math.max(container.clientHeight / minZoom + 200, (bb?.y2 ?? 0) + NODE_H + 600, 2000)
				: laneExtent;
		if (worldEl) {
			worldEl.style.width = `${worldWidth}px`;
			worldEl.style.height = `${worldHeight}px`;
		}
		syncViewportVisuals();
	}

	// ── Board model 초기화 ──────────────────────────────────────

	async function init(
		quests: Quest[],
		statuses: QuestStatus[],
		positions: QuestPosition[],
		dependencies: QuestDependency[]
	) {
		// DEV-059: 사용자가 lane 순서 바꾼 결과 (localStorage) 가 있으면 그것 우선,
		// 없는 status (새로 추가됨) 는 sort_order 기준으로 뒤에 append.
		const userOrder = loadLaneOrder();
		const bySlug = new Map(statuses.map((s) => [s.slug, s]));
		const ordered: QuestStatus[] = [];
		for (const slug of userOrder) {
			const s = bySlug.get(slug);
			if (s) {
				ordered.push(s);
				bySlug.delete(slug);
			}
		}
		// 미저장 (새 status / 첫 진입) 는 sort_order 기본 순.
		const remaining = [...bySlug.values()].sort((a, b) => a.sort_order - b.sort_order);
		sorted = [...ordered, ...remaining];
		laneOf = new Map(sorted.map((s, i) => [s.id, i]));
		allQuests = quests;
		allDependencies = dependencies;

		const posMap = new Map<number, { x: number; y: number }>();
		positions.forEach((p) => {
			const quest = quests.find((q) => q.id === p.quest_id);
			if (!quest) return;
			const li = laneOf.get(quest.status_id) ?? 0;
			const laneLeft = li * LANE_STRIDE;
			// 저장된 x 가 현재 lane 범위 밖이면 (status 변경 또는 lane 순서 변경 후)
			// 가장 가까운 lane 내부 좌표로 평행이동 — 가로 위치 (lane 내부 col) 는 보존.
			// 기존 코드는 lane 중앙으로 강제 → 여러 노드가 한 열로 겹치는 BUG-002.
			let x = p.x;
			if (x < laneLeft || x >= laneLeft + LANE_W) {
				const oldLaneLeft = Math.floor(x / LANE_STRIDE) * LANE_STRIDE;
				const offsetInOldLane = x - oldLaneLeft;
				// 새 lane 의 동일 offset 으로. lane 폭을 넘어서면 lane 내부로 clamp.
				const clamped = Math.max(0, Math.min(LANE_W - 1, offsetInOldLane));
				x = laneLeft + clamped;
			}
			posMap.set(p.quest_id, { x, y: p.y });
		});

		buildLaneDivs(sorted);

		const initialY = canonicalGridBaseY(ORIENTATION_METRICS);
		const laneNextY = new Map<number, number>(sorted.map((s) => [s.id, initialY]));
		posMap.forEach(({ y }, questId) => {
			const quest = quests.find((q) => q.id === questId);
			if (!quest) return;
			const cur = laneNextY.get(quest.status_id) ?? initialY;
			laneNextY.set(quest.status_id, Math.max(cur, y + NODE_H + NODE_GAP));
		});
		const autoCount = new Map<number, number>();
		const elements: BoardElementDefinition[] = [];

		// lane 안의 3개 열 (col 0/1/2) 중심 x — 자동 배치 시 골고루 채워 1열 stacking 방지.
		const COL_OFFSETS = [
			LANE_PAD_X + NODE_W / 2,
			LANE_PAD_X + NODE_W + NODE_GAP + NODE_W / 2,
			LANE_PAD_X + 2 * (NODE_W + NODE_GAP) + NODE_W / 2
		];

		quests.forEach((q) => {
			const li = laneOf.get(q.status_id) ?? 0;
			let pos = posMap.get(q.id);
			if (!pos) {
				const n = autoCount.get(q.status_id) ?? 0;
				const startY = laneNextY.get(q.status_id) ?? initialY;
				const col = n % 3;
				const row = Math.floor(n / 3);
				const laneLeft = li * LANE_STRIDE;
				pos = {
					x: laneLeft + COL_OFFSETS[col],
					y: startY + row * (NODE_H + NODE_GAP)
				};
				autoCount.set(q.status_id, n + 1);
			}
			// DB x/y를 정본으로 data에 보관하고, init 끝에서 orientation 화면 좌표로 변환.
			elements.push({
				data: {
					id: `q-${q.id}`,
					label: '',
					questId: q.id,
					questSlug: q.quest_id,
					statusId: q.status_id,
					urgencyColor: urgencyColor(q.urgency),
					urgencyBg: urgencyBgFor(q.urgency, currentEffectiveTheme()),
					typeColor: q.type_color,
					highlightType: '',
					active: false,
					absX: pos.x,
					absY: pos.y
				},
				position: pos // 초기엔 absolute 그대로. applyLaneVisualCompression 이 visual 변환.
			});
		});

		dependencies.forEach((d) => {
			elements.push({
				data: {
					id: `pre-${d.prerequisite_id}-${d.quest_id}`,
					source: `q-${d.prerequisite_id}`,
					target: `q-${d.quest_id}`,
					etype: 'pre',
					dimmed: false
				}
			});
		});
		quests
			.filter((q) => q.parent_quest_id !== null)
			.forEach((q) => {
				elements.push({
					data: {
						id: `sub-${q.parent_quest_id}-${q.id}`,
						source: `q-${q.parent_quest_id}`,
						target: `q-${q.id}`,
						etype: 'sub',
						dimmed: false
					}
				});
			});

		cy?.destroy();
		cy = new BoardGraph(
			elements,
			() => ({ width: container.clientWidth, height: container.clientHeight }),
			scheduleDomPositionSync,
			scheduleDomGraphSync,
			() => {
				syncViewportVisuals();
				scheduleViewportSave();
			}
		);
		cy.minZoom(computeMinZoom());

		// DEV-056: hide settings 적용. computeGroups → applyHideSettings.
		// DEV-105 fix8/9: applyHideSettings 가 이제 collapsedLanes 도 인식 — 별도
		// 코드 불필요.
		groupOf = computeGroups(allQuests, allDependencies);
		applyHideSettings();
		// DEV-067: visible lane 압축 (laneHidden 자리 회수). 노드 visual 좌표
		// 일관 재계산. syncLanes 도 visible 압축 반영.
		applyLaneVisualCompression();
		syncLanes();
		// orientation 변환이 끝난 실제 node bounds 기준으로 viewport를 복원/계산한다.
		const savedViewport = loadViewport();
		if (savedViewport) cy.viewport(savedViewport);
		else cy.fit(undefined, 60);
		syncLanes();
		syncDomGraphNow();
		// DEV-135 fix: 렌더 완료(노드 생성) 직후 현재 필터로 dim 적용 —
		// effect 만으론 cy 준비 전에 돌고 끝나 List→Board 전환 시 dim 누락됨.
		applyFilterDim();
	}
</script>

<div
	class="board-wrap"
	class:lod-compact={boardLod === 'compact'}
	class:lod-overview={boardLod === 'overview'}
	class:orientation-rows={boardOrientation === 'rows'}
	bind:this={boardWrapEl}
>
	<!-- lane 단색 배경은 orientation 반대축의 viewport를 항상 채운다. -->
	<div class="lanes-bg" bind:this={lanesEl}></div>
	<!-- DEV-317/BUG-225: snap + SVG edge + DOM node가 이 transform 하나를 공유한다. -->
	<div class="board-world-viewport" bind:this={worldViewportEl}>
		<!-- 점 중심은 world 좌표, 굵기는 gesture 종료 시 screen px에 맞춘다. -->
		<div class="lane-grid-layer" bind:this={gridLanesEl}></div>
		<div class="board-world" bind:this={worldEl}>
			<svg class="edge-layer" width="100%" height="100%">
				<defs>
					<marker
						id="arrow-pre"
						viewBox="0 0 10 10"
						refX="9"
						refY="5"
						markerWidth="7"
						markerHeight="7"
						orient="auto-start-reverse"
					>
						<path d="M 0 0 L 10 5 L 0 10 z" fill="var(--edge-pre)" />
					</marker>
					<marker
						id="arrow-sub"
						viewBox="0 0 10 10"
						refX="9"
						refY="5"
						markerWidth="7"
						markerHeight="7"
						orient="auto-start-reverse"
					>
						<path d="M 1 1 L 9 5 L 1 9" fill="none" stroke="var(--text-faint)" stroke-width="2" />
					</marker>
				</defs>
				{#each domEdges as edge (edge.id)}
					{#if !edge.hidden}
						<path
							class="board-edge {edge.etype}"
							class:dimmed={edge.dimmed}
							class:filter-dim={edge.fdim}
							d={edge.path}
							marker-end={edge.etype === 'pre' ? 'url(#arrow-pre)' : 'url(#arrow-sub)'}
						/>
					{/if}
				{/each}
			</svg>
			<div class="node-layer">
				{#each domNodes as node (node.id)}
					{#if !node.hidden}
						{@const q = node.quest}
						{@const due = effectiveQuestDue(q)}
						{@const unresolved = q.discussion_unresolved ?? 0}
						{@const resolved = q.discussion_resolved ?? 0}
						<div
							class="board-node {node.highlightType ? `hl-${node.highlightType}` : ''}"
							class:active={node.active}
							class:selected={node.selected}
							class:filter-dim={node.fdim}
							class:flash={node.flash}
							style:left="{node.x - NODE_W / 2}px"
							style:top="{node.y - NODE_H / 2}px"
							style:z-index={node.zIndex}
							style:--node-border={node.urgencyColor}
							style:--node-active-bg={node.urgencyBg}
							role="button"
							tabindex="0"
							data-node-id={node.id}
							aria-label={`${q.quest_id}: ${q.title}`}
							onmousedown={(event) => onNodeMouseDown(event, node.id)}
							onkeydown={(event) => {
								if (event.key === 'Enter' || event.key === ' ') {
									event.preventDefault();
									if (cy) openNode(cy.getElementById(`q-${node.id}`));
								}
							}}
						>
							{#if boardLod === 'detail'}
								<div class="node-topline">
									<span class="pill mono xs" style:--c={q.type_color}>{q.quest_id}</span>
									<span class="pill xs" style:--c={urgencyColor(q.urgency)}
										>{urgencyLabel(q.urgency, $locale)}</span
									>
									{#if urgencyOutOfRange(q.urgency)}
										<span
											class="urgency-warning"
											title={`${t('board.urgencyClampPre', $locale)}${q.urgency}${t('board.urgencyClampPost', $locale)}`}
											>⚠</span
										>
									{/if}
									<span class="node-metrics">
										{#if unresolved > 0}
											<span class="pill xs discussion-count unresolved">✗ {unresolved}</span>
										{:else if resolved > 0}
											<span class="pill xs discussion-count resolved">✓ {resolved}</span>
										{/if}
										{#if (q.comment_count ?? 0) > 0}
											<span class="comment-count"
												><Icon name="comment" size={10} />{q.comment_count}</span
											>
										{/if}
									</span>
								</div>
								<div class="node-title" class:with-due={!!due.date}>{q.title}</div>
								{#if due.date}
									<div class="node-due {dueState(due.date)}">
										<Icon name={due.source === 'campaign' ? 'campaign' : 'clock'} size={10} />
										<span>{due.date}</span>
									</div>
								{/if}
							{:else if boardLod === 'compact'}
								<div class="node-compact-id mono">{q.quest_id}</div>
							{/if}
						</div>
					{/if}
				{/each}
			</div>
		</div>
	</div>
	<!-- 노드 사이의 빈 영역에서 pan / box selection 을 받는 입력면. -->
	<div class="board" bind:this={container}></div>
	{#if boxDrag}
		<div
			class="box-selection"
			style:left="{boxDrag.left}px"
			style:top="{boxDrag.top}px"
			style:width="{boxDrag.width}px"
			style:height="{boxDrag.height}px"
			style:--box-color={boxDrag.color}
		></div>
	{/if}
	<div class="lane-hdrs" bind:this={headersEl}></div>
	<!-- 레인 설정 팝오버 층 — toolbar(z:10) 위. 헤더 레이어와 분리한 이유는
	     위 `lanePopLayerEl` 주석 참고. -->
	<div class="lane-pop-layer" bind:this={lanePopLayerEl}></div>

	<!-- 노드 확장 카드 (z:6, 노드 위에 플로팅) -->
	{#if expandedQuest}
		<div
			class="node-card"
			class:card-dragging={!!cardDrag}
			style:left="{expandedPos.x}px"
			style:top="{expandedPos.y}px"
			role="dialog"
			tabindex="-1"
			onmousedown={startCardDrag}
		>
			<div class="card-head">
				<span class="drag-hint" title={t('board.dragToMove', $locale)}>⠿</span>
				<div class="card-badges">
					<span class="pill sm" style:--c={expandedQuest.type_color}>{expandedQuest.quest_id}</span>
					<span class="pill sm" style:--c={urgencyColor(expandedQuest.urgency)}
						>{urgencyLabel(expandedQuest.urgency, $locale)}</span
					>
					<span class="pill sm" style:--c={expandedQuest.status_color}
						>{questStatusLabel(expandedQuest, $locale)}</span
					>
				</div>
				<button class="card-close" onclick={closeExpanded} title={t('board.closeEsc', $locale)}
					>×</button
				>
			</div>

			<p class="card-title">{expandedQuest.title}</p>

			<div class="card-branch">
				<span class="blabel">Branch</span>
				<code class="bname"
					>{expandedQuest.type_prefix}-{String(expandedQuest.number).padStart(3, '0')}</code
				>
			</div>

			<button
				class="card-goto"
				onclick={() => goto(`/quests/${expandedQuest!.quest_id}?from=board`)}
			>
				{t('board.gotoDetail', $locale)}
			</button>

			<div class="card-divider"></div>
			<p class="card-sec-label">
				{t('board.highlightRelated', $locale)}
				<span class="hl-multi-hint">{t('board.multiSelect', $locale)}</span>
			</p>

			<div class="card-hl-grid">
				<button
					class="hl-btn all"
					class:on={activeHighlights.size === 4}
					onclick={toggleAllHighlights}>{t('board.allRelated', $locale)}</button
				>
				<button
					class="hl-btn pre"
					class:on={activeHighlights.has('pre')}
					onclick={() => toggleHighlight('pre')}>{t('board.hlPre', $locale)}</button
				>
				<button
					class="hl-btn sub"
					class:on={activeHighlights.has('sub')}
					onclick={() => toggleHighlight('sub')}>{t('board.hlSub', $locale)}</button
				>
				<button
					class="hl-btn next"
					class:on={activeHighlights.has('next')}
					onclick={() => toggleHighlight('next')}>{t('board.hlNext', $locale)}</button
				>
				<button
					class="hl-btn parent"
					class:on={activeHighlights.has('parent')}
					onclick={() => toggleHighlight('parent')}>{t('board.hlParent', $locale)}</button
				>
			</div>

			{#if activeHighlights.size > 0}
				<div class="hl-actions">
					<button
						class="hl-act sel"
						onclick={selectHighlighted}
						title={t('board.selectHighlighted', $locale)}
					>
						<!-- DEV-302: 라벨의 🔘 를 아이콘으로 분리. -->
						<Icon name="select" size={11} />
						{t('board.selectBtn', $locale)}
					</button>
					<button
						class="hl-act arr"
						onclick={arrangeHighlightedGroup}
						disabled={arranging}
						title={t('board.arrangeHighlighted', $locale)}
					>
						{t('board.arrangeBtn', $locale)}
					</button>
					<button
						class="hl-act clear"
						onclick={clearHighlight}
						title={t('board.clearHighlightTitle', $locale)}
					>
						{t('common.clearBtn', $locale)}
					</button>
				</div>
			{/if}

			<p class="card-note">
				{t('board.cardNote', $locale)}
			</p>
		</div>
	{/if}

	<!-- DEV-135: List 필터 활성 표시 — Board 의 dim 이 '왜' 인지 + 한 클릭 해제. -->
	{#if filterActive}
		<div class="filter-chip" role="status">
			<span class="fc-label"
				>{t('board.filterActivePre', $locale)}{filterMatchCount}/{filterTotalCount}{t(
					'board.filterActivePost',
					$locale
				)}</span
			>
			<button class="fc-clear" onclick={clearBoardFilter} title={t('common.clearFilter', $locale)}
				>{t('common.clearBtn', $locale)}</button
			>
		</div>
	{/if}
	{#if performanceVisible}
		<div class="performance-hud" role="status" aria-live="polite">
			<strong>{performanceStats.rafHz.toFixed(0)} Hz</strong>
			<span>median {performanceStats.medianMs.toFixed(1)} ms</span>
			<span>p95 {performanceStats.p95Ms.toFixed(1)} ms</span>
			<span>&gt;12.5 ms {performanceStats.missed120Percent.toFixed(0)}%</span>
			<span>viewport {performanceViewportHz.toFixed(0)}/s</span>
			<span>zoom {performanceZoom.toFixed(3)} · {boardLod}</span>
			<span>page {performancePageState}</span>
		</div>
	{/if}
	<!-- DEV-073 fix3: New Quest 는 상단 우측 고정 (항상 노출), 나머지 도구바는
	     그 아래로 내림 (사용자 피드백). 접기 토글로 도구만 숨길 수 있음. -->
	{#if onNewQuest}
		<div class="tb-newquest-wrap">
			<button class="tb-btn tb-new" onclick={onNewQuest} title={t('board.newQuest', $locale)}>
				<span class="icon">+</span><span>{t('board.newQuest', $locale)}</span>
			</button>
		</div>
	{/if}
	<!-- 툴바 — DEV-073: collapsed 시 ⊟ 토글만 보이고 나머지 숨김 (lane header 안 가림).
		 fix4: 사용자 피드백 — 토글 버튼이 펴진 도구바의 왼쪽 끝에 있으니 다시 접을
		 때 마우스를 멀리 이동해야 함. 펴는 / 접는 위치를 동일하게 (우측 끝) 유지하기
		 위해 토글 버튼을 markup 의 마지막에 두고 나머지 버튼들이 그 왼쪽에 배치되게
		 함. flex 의 자연스러운 row 순서로 우측 anchor + 토글 항상 우측 끝. -->
	<div class="toolbar" class:collapsed={toolbarCollapsed} class:has-newquest={!!onNewQuest}>
		{#if !toolbarCollapsed}
			<button class="tb-btn" onclick={fitView} title={t('board.fitView', $locale)}
				><span class="icon">⊞</span></button
			>
			<button
				class="tb-btn"
				class:tb-on={boardOrientation === 'rows'}
				onclick={toggleBoardOrientation}
				title={t(
					boardOrientation === 'columns'
						? 'board.orientationSwitchRows'
						: 'board.orientationSwitchColumns',
					$locale
				)}
			>
				<span class="icon">{boardOrientation === 'columns' ? '↕' : '↔'}</span>
				<span>{t(boardOrientation === 'columns' ? 'board.columns' : 'board.rows', $locale)}</span>
			</button>
			<div class="tb-sep"></div>
			<button
				class="tb-btn"
				onclick={undo}
				disabled={undoStack.length === 0}
				title={t('board.undo', $locale)}
			>
				<span class="icon">↩</span>
				{#if undoStack.length > 0}<span class="count">{undoStack.length}</span>{/if}
			</button>
			<button
				class="tb-btn"
				onclick={redo}
				disabled={redoStack.length === 0}
				title={t('board.redo', $locale)}
			>
				<span class="icon">↪</span>
				{#if redoStack.length > 0}<span class="count">{redoStack.length}</span>{/if}
			</button>
			<div class="tb-sep"></div>
			<button
				class="tb-btn"
				class:tb-on={gridSnap}
				onclick={toggleGridSnap}
				title={t('board.gridSnap', $locale)}
			>
				<span class="icon">⊞</span><span>{t('board.snapBtn', $locale)}</span>
			</button>
			<div class="tb-sep"></div>
			<select
				class="tb-select"
				value={globalCols}
				onchange={(e) => setGlobalCols(parseInt((e.currentTarget as HTMLSelectElement).value))}
				title={t(boardOrientation === 'columns' ? 'board.gridCols' : 'board.gridRows', $locale)}
			>
				<option value={1}
					>1{t(
						boardOrientation === 'columns' ? 'board.colSuffix' : 'board.rowSuffix',
						$locale
					)}</option
				>
				<option value={2}
					>2{t(
						boardOrientation === 'columns' ? 'board.colSuffix' : 'board.rowSuffix',
						$locale
					)}</option
				>
				<option value={3}
					>3{t(
						boardOrientation === 'columns' ? 'board.colSuffix' : 'board.rowSuffix',
						$locale
					)}</option
				>
			</select>
			<div class="tb-sep"></div>
			<!-- DEV-056 → DEV-059 fix2: 숨김 + 순서 변경 통합 → '보드 설정'. -->
			<button
				class="tb-btn"
				class:tb-on={Object.values(hideSettings).some(
					(s) => s.laneHidden || s.hideGroup || s.hideSolo
				)}
				onclick={() => (showHideModal = true)}
				title={t('board.settingsTitle', $locale)}
			>
				<span class="icon">⚙</span><span>{t('board.settings', $locale)}</span>
			</button>
			<div class="tb-sep"></div>
			<!-- arrange 버튼 + mode select 는 하나의 컨트롤처럼 시각적으로 묶음 -->
			<div class="tb-arrange-group">
				<button
					class="tb-btn tb-arrange"
					onclick={() => {
						if (!cy) return;
						if (arrangeMode === 'group') {
							arrangeNodesGrouped(cy.nodes('[questId]').toArray() as BoardNode[], globalCols);
						} else {
							arrangeNodes(null, globalCols);
						}
					}}
					title={arrangeMode === 'group'
						? t('board.arrangeGroupTitle', $locale)
						: t('board.arrangeFlatTitle', $locale)}
				>
					<span class="icon">⊟</span><span>{t('board.arrangeToolbarBtn', $locale)}</span>
				</button>
				<select
					class="tb-select tb-mode"
					bind:value={arrangeMode}
					title={t('board.arrangeMode', $locale)}
				>
					<option value="group">{t('board.arrangeModeGroup', $locale)}</option>
					<option value="all">{t('board.arrangeModeAll', $locale)}</option>
				</select>
			</div>
		{/if}
		<!-- 토글 버튼 — 항상 우측 끝 (collapsed / expanded 동일 위치, 사용자 피드백). -->
		<button
			class="tb-btn tb-collapse"
			onclick={toggleToolbarCollapsed}
			title={toolbarCollapsed
				? t('board.toolbarExpand', $locale)
				: t('board.toolbarCollapse', $locale)}
			aria-label={toolbarCollapsed
				? t('board.toolbarExpand', $locale)
				: t('board.toolbarCollapseShort', $locale)}
		>
			<span class="icon">{toolbarCollapsed ? '☰' : '⇥'}</span>
		</button>
	</div>
</div>

{#if loading}
	<div class="overlay">Loading...</div>
{:else if error}
	<div class="overlay error">{error}</div>
{/if}

{#if confirmDialog}
	<div class="dialog-backdrop" role="presentation">
		<div class="dialog" role="alertdialog" tabindex="-1">
			<p class="dialog-msg">{confirmDialog.msg}</p>
			<div class="dialog-btns">
				<button class="dialog-ok" onclick={() => confirmDialogResolve(true)}
					>{t('common.change', $locale)}</button
				>
				<button class="dialog-cancel" onclick={() => confirmDialogResolve(false)}
					>{t('common.cancel', $locale)}</button
				>
			</div>
		</div>
	</div>
{/if}

<!-- DEV-056: 숨김 설정 모달 -->
{#if showHideModal}
	<div
		class="dialog-backdrop"
		role="presentation"
		onclick={(e) => {
			if (e.target === e.currentTarget) showHideModal = false;
		}}
	>
		<div class="hide-modal" role="dialog" aria-modal="true" tabindex="-1">
			<div class="hide-head">
				<h3 class="hide-title">{t('board.settings', $locale)}</h3>
				<button
					class="hide-close"
					onclick={() => (showHideModal = false)}
					aria-label={t('common.close', $locale)}>×</button
				>
			</div>
			<p class="hide-help">
				{t(boardOrientation === 'columns' ? 'board.hideHelp' : 'board.hideHelpRows', $locale)}
			</p>
			<div class="hide-table-wrap">
				<table class="hide-table">
					<thead>
						<tr>
							<th style="width: 6ch">{t('board.colOrder', $locale)}</th>
							<th style="width: 14ch">{t('board.colLane', $locale)}</th>
							<th>{t('board.colShow', $locale)}</th>
							<th>{t('board.colHideGroup', $locale)}</th>
							<th>{t('board.colHideSolo', $locale)}</th>
						</tr>
					</thead>
					<tbody>
						{#each sorted as s, li (s.id)}
							{@const setting = getHideSetting(s.slug)}
							{@const laneVisible = !setting.laneHidden}
							<tr class:lane-off={!laneVisible}>
								<td class="reorder-cell">
									<button
										class="reorder-btn"
										onclick={() => swapLane(li, -1)}
										disabled={li === 0}
										title={t(
											boardOrientation === 'columns' ? 'board.moveLeft' : 'board.moveUp',
											$locale
										)}
										aria-label={t(
											boardOrientation === 'columns' ? 'board.moveLeft' : 'board.moveUp',
											$locale
										)}>{boardOrientation === 'columns' ? '◀' : '▲'}</button
									>
									<button
										class="reorder-btn"
										onclick={() => swapLane(li, 1)}
										disabled={li === sorted.length - 1}
										title={t(
											boardOrientation === 'columns' ? 'board.moveRight' : 'board.moveDown',
											$locale
										)}
										aria-label={t(
											boardOrientation === 'columns' ? 'board.moveRight' : 'board.moveDown',
											$locale
										)}>{boardOrientation === 'columns' ? '▶' : '▼'}</button
									>
								</td>
								<td>
									<span class="hide-lane-name" style:color={s.color}>{statusLabel(s, $locale)}</span
									>
								</td>
								<td>
									<input
										type="checkbox"
										checked={laneVisible}
										onchange={() => toggleHideSetting(s.slug, 'laneHidden')}
										title={t('board.laneShowTitle', $locale)}
									/>
								</td>
								<td>
									<input
										type="checkbox"
										checked={setting.hideGroup}
										disabled={!laneVisible}
										onchange={() => toggleHideSetting(s.slug, 'hideGroup')}
									/>
								</td>
								<td>
									<input
										type="checkbox"
										checked={setting.hideSolo}
										disabled={!laneVisible}
										onchange={() => toggleHideSetting(s.slug, 'hideSolo')}
									/>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			<!-- DEV-135: 보드 필터 — List 와 동일한 필터 UI. 변경 시 공유 store +
			     localStorage 에 반영되어 dim 이 즉시 갱신되고 List 와도 일관. -->
			<div class="bf-section">
				<div class="bf-head">
					<h4 class="bf-title">{t('board.filter', $locale)}</h4>
					{#if filterActive}
						<span class="bf-count"
							>{filterMatchCount}/{filterTotalCount}{t('board.filterActivePost', $locale)}</span
						>
						<button
							class="bf-clear"
							onclick={clearBoardFilter}
							title={t('common.clearFilter', $locale)}>{t('common.clearBtn', $locale)}</button
						>
					{/if}
				</div>
				<p class="hide-help">
					{t('board.filterHelp', $locale)}
				</p>
				<div class="bf-filter">
					<QuestListFilter
						types={boardTypes}
						statuses={sorted}
						bind:typeIds={bfTypeIds}
						bind:statusIds={bfStatusIds}
						bind:search={bfSearch}
						bind:titleOnly={bfTitleOnly}
						bind:urgencies={bfUrgencies}
						bind:prereqState={bfPrereq}
						bind:subState={bfSub}
						bind:createdAfter={bfCreatedAfter}
						bind:createdBefore={bfCreatedBefore}
						bind:updatedAfter={bfUpdatedAfter}
						bind:updatedBefore={bfUpdatedBefore}
					/>
				</div>
			</div>
		</div>
	</div>
{/if}

<style>
	.board-wrap {
		position: relative;
		width: 100%;
		height: calc(100vh - var(--nav-h, 3.25rem) - var(--titlebar-h, 0px));
		background: var(--bg);
		overflow: hidden;
		touch-action: none;
	}
	.board-world-viewport {
		position: absolute;
		top: 0;
		left: 0;
		z-index: 2;
		transform-origin: 0 0;
		pointer-events: none;
	}
	.board-world {
		position: absolute;
		top: 0;
		left: 0;
		z-index: 1;
		transform-origin: 0 0;
		pointer-events: none;
	}

	.lanes-bg {
		position: absolute;
		inset: 0;
		z-index: 0;
		pointer-events: none;
		overflow: hidden;
	}
	.lane-grid-layer {
		position: absolute;
		top: 0;
		left: 0;
		z-index: 0;
		pointer-events: none;
		overflow: visible;
	}
	:global(.lane-grid-col) {
		--grid-dot-image: radial-gradient(
			circle at center,
			color-mix(in srgb, var(--warning) 68%, transparent) 0 var(--grid-dot-radius),
			transparent calc(var(--grid-dot-radius) + var(--grid-dot-feather))
		);
		position: absolute;
		top: 0;
		bottom: 0;
		overflow: hidden;
		contain: strict;
		background-repeat: repeat-y;
		pointer-events: none;
	}
	:global(.lane-grid-col.grid-cols-1) {
		background-image: var(--grid-dot-image);
	}
	:global(.lane-grid-col.grid-cols-2) {
		background-image: var(--grid-dot-image), var(--grid-dot-image);
	}
	:global(.lane-grid-col.grid-cols-3) {
		background-image: var(--grid-dot-image), var(--grid-dot-image), var(--grid-dot-image);
	}
	:global(.lane-grid-col.orientation-rows) {
		background-repeat: repeat-x;
	}
	.board {
		position: absolute;
		inset: 0;
		z-index: 1;
		background: transparent;
		/* DEV-208/317: 네이티브 Page Scale 대신 보드 viewport가 직접 처리. */
		touch-action: none;
	}
	.box-selection {
		position: absolute;
		z-index: 100;
		box-sizing: border-box;
		border: var(--bw) dashed var(--box-color);
		background: color-mix(in srgb, var(--box-color) 10%, transparent);
		pointer-events: none;
	}
	.lane-hdrs {
		position: absolute;
		inset: 0;
		z-index: 3;
		pointer-events: none;
		overflow: hidden;
	}
	/* 레인 설정 팝오버 층 — toolbar(z:10) 위.
	   예전엔 팝오버를 올리려고 `.lane-hdrs` 전체를 11 로 올렸는데, 그러면
	   **레인 제목까지** toolbar·새 퀘스트 버튼을 덮었다(admin 보고).
	   `.lane-hdrs` 는 z-index 가 있어 stacking context 라 안쪽 팝오버만 따로
	   올릴 수가 없다 — 그래서 팝오버를 이 별도 층으로 뺐다. 헤더는 z:3 그대로
	   toolbar 아래에 남는다. */
	.lane-pop-layer {
		position: absolute;
		inset: 0;
		z-index: 11;
		pointer-events: none;
		overflow: hidden;
	}

	.edge-layer,
	.node-layer {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}
	.edge-layer {
		z-index: 1;
		overflow: visible;
	}
	.node-layer {
		z-index: 2;
	}
	.board-edge {
		fill: none;
		transition: opacity 0.12s;
	}
	.board-edge.pre {
		stroke: var(--edge-pre);
		stroke-width: 2px;
	}
	.board-edge.sub {
		stroke: var(--text-faint);
		stroke-width: 1.5px;
		stroke-dasharray: 6 3;
	}
	.board-edge.dimmed,
	.board-edge.filter-dim {
		opacity: 0.07;
	}
	/* check-spacing:off — 노드 기하는 JS 상수와 짝이다.
	   `NODE_W`/`NODE_H`(= BOARD_NODE_WIDTH/HEIGHT)로 배치를 계산하므로 CSS 만
	   rem 으로 바꾸면 좌표와 상자가 어긋난다. admin: "보드에 표시되는 노드는
	   크기가 변하면 안된다". 노드 안쪽 여백·글자도 그 80px 안에 맞춘 값이라
	   함께 px 로 둔다. */
	.board-node {
		/* DEV-369 후속: 위 px 섬 설명 참고 — 노드 곡률도 px 이어야 상자와 짝이
		   맞는다. 토큰 자체를 이 서브트리에서만 덮으므로 리터럴 검사는 통과한다. */
		--r-xl: 10px;
		position: absolute;
		width: 284px;
		height: 80px;
		box-sizing: border-box;
		border: 2px solid var(--node-border);
		border-radius: var(--r-xl);
		background: color-mix(in srgb, var(--bg) 92%, transparent);
		overflow: hidden;
		pointer-events: auto;
		cursor: grab;
		user-select: none;
		transition:
			opacity 0.12s,
			border-color 0.12s,
			box-shadow 0.12s,
			background 0.12s;
	}
	.board-node:active {
		cursor: grabbing;
	}
	.node-topline {
		position: absolute;
		top: 7px;
		left: 8px;
		right: 8px;
		display: flex;
		align-items: center;
		gap: 5px;
		height: 18px;
		white-space: nowrap;
	}
	/* DEV-369 후속: 노드 안은 **px 섬**이다.
	   `.board-node` 가 `width: 284px / height: 80px` 고정이고 그 값이 JS 상수
	   (`quest-node-svg.ts` 의 `NODE_W` / `NODE_H`)와 짝이라 Cytoscape 레이아웃·
	   히트테스트가 같은 숫자를 쓴다. 상자는 안 커지는데 안쪽만 rem/em 으로
	   커지면 UI 배율에서 내용이 넘친다(admin 지적).

	   그래서 노드 안에서는 모양 공식(`.pill`)은 그대로 쓰되 **치수 토큰만 px 로
	   덮는다.** 값은 전부 DEV-364 이전과 같다 — 글자 10px / 높이 17px /
	   좌우 여백 7px / 테두리 1px.

	   `.pill.xs` 가 자기 요소에 `--pill-*` 를 직접 얹으므로, 상속으로는 못
	   덮고 **더 높은 specificity 로 같은 요소에** 얹어야 한다(0,3,0). */
	.node-topline :global(.pill.xs) {
		--pill-fs: 10px;
		--pill-h: 17px;
		--pill-px: 7px;
		--bw: 1px;
	}
	.urgency-warning {
		color: var(--danger);
		font-size: 12px;
		font-weight: 700;
	}
	.node-metrics {
		margin-left: auto;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		color: var(--text-muted);
		font-size: 10px;
	}
	.discussion-count {
		--c: var(--success);
		font-weight: 600;
	}
	.discussion-count.unresolved {
		--c: var(--danger);
	}
	.comment-count,
	.node-due {
		display: inline-flex;
		align-items: center;
		gap: 3px;
	}
	.node-title {
		position: absolute;
		top: 33px;
		left: 9px;
		right: 9px;
		display: -webkit-box;
		overflow: hidden;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		color: var(--text);
		font-size: 12px;
		line-height: 15px;
		overflow-wrap: anywhere;
	}
	.node-title.with-due {
		bottom: 17px;
	}
	.node-due {
		position: absolute;
		right: 8px;
		bottom: 6px;
		color: var(--text-muted);
		font-size: 10px;
		font-weight: 500;
	}
	.node-due.soon {
		color: var(--orange);
	}
	.node-due.overdue {
		color: var(--danger);
	}
	/* check-spacing:on */
	.board-node.active {
		border-width: 3px;
		background: var(--node-active-bg);
		box-shadow: 0 0 18px var(--node-border);
	}
	/* BUG-225: 중간 배율에서는 추적번호만 크게 표시한다. */
	.lod-compact .board-node {
		background: color-mix(in srgb, var(--bg) 97%, var(--node-border));
		transition: none;
	}
	.node-compact-id {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--node-border);
		font-size: 32px;
		font-weight: 800;
		white-space: nowrap;
	}
	/* 전체 보기에서는 카드 하나를 단순한 색 marker 하나로 축약한다. */
	.board-wrap.lod-overview .board-node {
		/* 원래 14px 이다. DEV-369 의 일괄 치환이 --r-xl(10px)로 바꿔 값까지
		   줄어 있었다 — 되돌린다. */
		--r-xl: 14px;
		border: 0;
		border-radius: var(--r-xl);
		background: var(--node-border);
		box-shadow: none;
		transition: none;
	}
	.board-wrap.lod-overview .board-node.active,
	.board-wrap.lod-overview .board-node.selected {
		background: var(--node-border);
		box-shadow: none;
		outline: 5px solid var(--accent);
		outline-offset: 4px;
	}
	.lod-overview .board-node.filter-dim {
		opacity: 0.2;
	}
	.board-node.hl-pre {
		border: 3px solid var(--hl-pre);
		background: var(--hl-pre-bg);
		box-shadow: 0 0 12px var(--hl-pre);
	}
	.board-node.hl-sub {
		border: 3px solid var(--hl-sub);
		background: var(--hl-sub-bg);
		box-shadow: 0 0 12px var(--hl-sub);
	}
	.board-node.hl-next {
		border: 3px solid var(--hl-next);
		background: var(--hl-next-bg);
		box-shadow: 0 0 12px var(--hl-next);
	}
	.board-node.hl-parent {
		border: 3px solid var(--success);
		background: var(--hl-parent-bg);
		box-shadow: 0 0 12px var(--success);
	}
	.board-node.hl-dim,
	.board-node.filter-dim {
		opacity: 0.12;
	}
	.board-node.selected {
		border: 3px solid var(--accent);
		background: var(--selected-bg);
		box-shadow: 0 0 14px var(--accent);
	}
	.board-node.flash {
		border: 5px solid var(--accent-secondary);
		box-shadow: 0 0 28px var(--accent-secondary);
	}

	:global(.lane-col) {
		position: absolute;
		top: 0;
		bottom: 0;
		background: var(--bg-elevated);
		border-right: var(--bw) solid var(--bg-subtle);
		box-sizing: border-box;
		pointer-events: none;
		transition:
			background 0.12s,
			box-shadow 0.12s;
		overflow: hidden;
	}
	.orientation-rows :global(.lane-col) {
		border-right: 0;
		border-bottom: var(--bw) solid var(--bg-subtle);
	}
	/* DEV-105 fix11: 드래그 중 노드가 놓일 lane 강조. */
	:global(.lane-col.drag-target) {
		background: color-mix(in srgb, var(--accent) 14%, var(--bg-elevated));
		box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--accent) 55%, transparent);
	}
	/* check-spacing:off — 레인 헤더도 기하다. 높이가 JS 의 `LANE_TOP=52`
	   (ORIENTATION_METRICS.laneHeaderSize)와 정렬돼 있어 스케일하면 노드와
	   겹친다. 헤더 안의 버튼·셀렉트는 그 38px 안에 맞춘 값이다. */
	/* DEV-101 fix8: 헤더 height 는 보드 `LANE_TOP=52` 와 정렬 위해 px 고정
	   (스케일하면 노드와 겹침). 내부 padding/gap/font 만 rem 으로 — UI 크기에
	   비례해 컨텐츠가 자연스럽게 자람 (max 2x 까지 38px 안에 fit). */
	:global(.lane-hdr) {
		position: absolute;
		top: 0;
		height: 38px;
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0 0.5rem 0 0.875rem;
		border-right: var(--bw) solid var(--bg-subtle);
		border-bottom: var(--bw) solid var(--bg-subtle);
		box-sizing: border-box;
		background: var(--bg-elevated);
		pointer-events: none;
	}
	.orientation-rows :global(.lane-hdr) {
		border-right: var(--bw) solid var(--bg-subtle);
		flex-direction: column;
		justify-content: center;
		gap: 0.25rem;
		padding: 0.375rem 0;
	}
	/* DEV-105 fix2: 접혔을 때 label 만 표시 — 다른 컨트롤 (cols-sel, arrange-group)
	   은 좁은 폭에서 시각적으로 깨지고 label 을 가려서 다시 펼치기가 어려워짐. */
	:global(.lane-hdr.collapsed > :not(.lane-label)) {
		display: none !important;
	}
	:global(.lane-hdr.collapsed) {
		/* 화면 맞춤으로 zoom 이 작아지면 collapsed 폭(40px * zoom)이 좌우
		   padding 합보다 작아져 label content box가 0px가 될 수 있다. 이때
		   다시 펼칠 hit-area 자체가 사라지므로 padding은 label 안으로 옮긴다. */
		padding: 0;
		justify-content: center;
	}
	:global(.lane-label) {
		flex: 1;
		font-size: 0.75rem;
		font-weight: bold;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		/* DEV-105: button 으로 변경 — 기본 button 스타일 reset. */
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		text-align: left;
		pointer-events: auto;
		transition: opacity 0.15s;
	}
	:global(.lane-label:hover) {
		opacity: 0.75;
	}
	.orientation-rows :global(.lane-label) {
		flex: 1 1 auto;
		width: 100%;
		min-height: 0;
		padding: 0.25rem 0;
		text-align: start;
		writing-mode: vertical-rl;
		text-orientation: mixed;
	}
	/* vertical-rl의 Latin 문자는 오른쪽으로 눕는다. 영어만 180도 뒤집어
	   왼쪽으로 90도 회전한 방향(아래에서 위로 읽는 방향)으로 표시한다. */
	.orientation-rows :global(.lane-label.lane-label-en) {
		/* 요소 전체를 뒤집으면 inline-start도 아래로 뒤집힌다. 회전 전에는
		   끝에 정렬해야 화면에서는 레인 위쪽에 붙는다. */
		text-align: end;
		transform: rotate(180deg);
		transform-origin: center;
	}
	/* DEV-105: collapsed 시 90도 회전 (세로) + 글자 한 줄 압축.
	   DEV-105 fix4: max-height 60px 가 lane-hdr (38px) 보다 커서 긴 이름이 위로
	   삐져나가 잘림. 헤더 안에 들어가도록 28px 로 축소 + ellipsis. */
	:global(.lane-label.collapsed) {
		position: absolute;
		top: 0;
		left: 0;
		width: 100%;
		min-height: 100%;
		box-sizing: border-box;
		writing-mode: vertical-rl;
		text-orientation: mixed;
		white-space: nowrap;
		/* DEV-105 fix6: 사용자 피드백 — 위로 짤리는것만 방지. flex 부모의
		   align-items:center 무시하고 위에 붙여, 긴 이름은 아래로 자연스럽게
		   넘어가게 (lane-hdrs 가 board 전체를 덮어 아래 overflow 는 보임). */
		align-self: flex-start;
		padding: 4px 0 0;
	}
	.orientation-rows :global(.lane-label.collapsed) {
		position: absolute;
		inset: 0;
		width: 100%;
		min-height: 100%;
		writing-mode: vertical-rl;
		text-orientation: mixed;
		padding: 4px 0 0;
		align-self: flex-start;
	}
	/* DEV-105 fix5: 레인별 설정 토글 ⚙ — 항상 보임, 작은 라벨 옆 버튼. */
	:global(.lane-settings-btn) {
		flex-shrink: 0;
		pointer-events: auto;
		background: none;
		border: var(--bw) solid transparent;
		border-radius: var(--r-sm);
		color: var(--text-faint);
		font-size: 0.85rem;
		padding: 0 4px;
		cursor: pointer;
		line-height: 1.2;
		transition:
			background 0.1s,
			color 0.1s,
			border-color 0.1s;
	}
	:global(.lane-settings-btn:hover) {
		background: var(--bg-subtle);
		color: var(--text-muted);
		border-color: var(--border);
	}
	:global(.lane-hdr.settings-open .lane-settings-btn) {
		color: var(--text);
		background: var(--bg-subtle);
		border-color: var(--border);
	}
	/* BUG: lane 폭이 좁으면(모바일 저배율) cols-sel + arrange-group 이 헤더 한 줄에
	   안 들어가 옆 헤더에 가려 안 보이던 문제 — 헤더 아래로 뜨는 팝오버로 분리.
	   settings-open 인 헤더만 다른 헤더 위로 올라오도록 z-index 도 올림. */
	:global(.lane-settings-pop) {
		display: none;
		position: absolute;
		/* 좌표는 `positionLanePop` 이 버튼 기준으로 계산한다 — 팝오버가 헤더
		   밖에 있어 CSS 로는 버튼을 기준 삼을 수 없다. */
		flex-direction: column;
		gap: 4px;
		align-items: stretch;
		padding: 6px;
		width: max-content;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
		pointer-events: auto;
	}
	:global(.lane-settings-pop.open) {
		display: flex;
	}
	:global(.lane-cols-sel) {
		flex-shrink: 0;
		pointer-events: auto;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text-muted);
		font-size: 0.72rem;
		padding: 1px 3px;
		cursor: pointer;
		outline: none;
	}
	:global(.lane-cols-sel:hover) {
		border-color: var(--text-faint);
		color: var(--text);
	}
	:global(.lane-arrange-btn) {
		flex-shrink: 0;
		pointer-events: auto;
		background: none;
		border: var(--bw) solid transparent;
		border-radius: var(--r-sm);
		color: var(--text-faint);
		font-size: 0.85rem;
		padding: 1px 5px;
		cursor: pointer;
		line-height: 1.4;
		transition:
			background 0.1s,
			color 0.1s,
			border-color 0.1s;
	}
	:global(.lane-arrange-btn:hover) {
		background: var(--bg-subtle);
		border-color: var(--border);
		color: var(--text-muted);
	}

	/* DEV-059: lane 순서 변경 — label 양 끝 ◀ ▶. */
	:global(.lane-move-btn) {
		flex-shrink: 0;
		pointer-events: auto;
		background: none;
		border: none;
		border-radius: var(--r-sm);
		color: var(--text-faint);
		font-size: 0.7rem;
		padding: 0 4px;
		cursor: pointer;
		line-height: 1;
		transition:
			background 0.1s,
			color 0.1s;
	}
	:global(.lane-move-btn:hover:not(:disabled)) {
		background: var(--bg-subtle);
		color: var(--text);
	}
	:global(.lane-move-btn:disabled) {
		opacity: 0.25;
		cursor: not-allowed;
	}

	/* lane header 의 mode select (Group / All) — lane-cols-sel 과 비슷한 비주얼 */
	:global(.lane-mode-sel) {
		flex-shrink: 0;
		pointer-events: auto;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text-muted);
		font-size: 0.72rem;
		padding: 1px 3px;
		cursor: pointer;
		outline: none;
	}
	:global(.lane-mode-sel:hover) {
		border-color: var(--text-faint);
		color: var(--text);
	}
	/* check-spacing:on */

	/* lane header 의 ⊟ 버튼 + mode select 를 segmented 컨트롤로 묶음 (toolbar 와 동일 패턴) */
	:global(.lane-arrange-group) {
		flex-shrink: 0;
		display: flex;
		align-items: stretch;
		gap: 0;
		pointer-events: auto;
	}
	:global(.lane-arrange-group .lane-arrange-btn) {
		border: var(--bw) solid var(--border);
		border-right: none;
		border-top-right-radius: 0;
		border-bottom-right-radius: 0;
		background: var(--bg);
	}
	:global(.lane-arrange-group .lane-mode-sel) {
		border-top-left-radius: 0;
		border-bottom-left-radius: 0;
	}

	/* ── 노드 확장 카드 (z:6) ── */
	.node-card {
		position: absolute;
		width: calc(18.75rem * var(--popup-scale, 1)); /* BUG-064 */
		z-index: 6;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-xl);
		box-shadow:
			0 8px 32px rgba(0, 0, 0, 0.55),
			0 0 0 1px rgba(255, 255, 255, 0.04);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		animation: card-expand 0.2s cubic-bezier(0.34, 1.4, 0.64, 1) forwards;
		transform-origin: top center;
		cursor: default;
		user-select: none;
	}
	.node-card:not(.card-dragging) .card-head {
		cursor: grab;
	}
	.node-card.card-dragging {
		cursor: grabbing;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.7);
	}
	@keyframes card-expand {
		from {
			opacity: 0;
			transform: scale(0.72) translateY(-8px);
		}
		to {
			opacity: 1;
			transform: scale(1) translateY(0);
		}
	}

	/* check-spacing:off — 노드 상세 카드는 px 캔버스 위에 노드 좌표를 기준으로
	   떠 있다. 카드만 배율을 따라 커지면 가리는 노드 수가 달라져 위치 계산과
	   어긋난다. 노드 섬의 연장으로 본다. */
	.card-head {
		display: flex;
		align-items: flex-start;
		gap: 6px;
		padding: 8px 10px 8px 8px;
		border-bottom: var(--bw) solid var(--bg-subtle);
	}
	.drag-hint {
		flex-shrink: 0;
		color: var(--border);
		font-size: 1rem;
		line-height: 1.4;
		padding: 1px 2px;
		transition: color 0.1s;
	}
	.card-head:hover .drag-hint {
		color: var(--text-faint);
	}
	.card-badges {
		flex: 1;
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
	}
	/* DEV-364: 모양은 global.css 의 `.pill` 이 정본. */
	.card-close {
		flex-shrink: 0;
		background: none;
		border: none;
		color: var(--text-faint);
		font-size: 1.1rem;
		line-height: 1;
		padding: 0 2px;
		cursor: pointer;
		transition: color 0.1s;
	}
	.card-close:hover {
		color: var(--text);
	}

	.card-title {
		margin: 0;
		padding: 10px 12px 6px;
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text-strong);
		line-height: 1.45;
		word-break: break-word;
	}
	.card-branch {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0 12px 8px;
		padding: 4px 8px;
		background: var(--bg);
		border: var(--bw) solid var(--bg-subtle);
		border-radius: var(--r-sm);
	}
	.blabel {
		font-size: 0.7rem;
		color: var(--text-faint);
	}
	.bname {
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 0.78rem;
		color: var(--accent-secondary);
	}

	.card-goto {
		margin: 0 12px 10px;
		padding: 6px 10px;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--accent);
		font-size: 0.78rem;
		cursor: pointer;
		text-align: left;
		transition:
			background 0.1s,
			border-color 0.1s;
	}
	.card-goto:hover {
		background: var(--border);
		border-color: var(--text-faint);
	}

	.card-divider {
		height: 1px;
		background: var(--bg-subtle);
	}

	.card-sec-label {
		margin: 10px 12px 4px;
		font-size: 0.67rem;
		font-weight: 600;
		color: var(--text-faint);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.hl-multi-hint {
		font-size: 0.62rem;
		color: var(--border);
		text-transform: none;
		letter-spacing: 0;
		font-weight: 400;
	}

	.card-hl-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 4px;
		padding: 0 12px;
	}
	/* "연관 전체"는 한 줄 전체 */
	.hl-btn.all {
		grid-column: 1 / -1;
	}

	.hl-btn {
		padding: 5px 8px;
		border-radius: var(--r-sm);
		font-size: 0.75rem;
		cursor: pointer;
		text-align: left;
		transition: all 0.12s;
		border: var(--bw) solid transparent;
		background: var(--bg);
	}
	/* DEV-074 fix20 (sweep): 토큰 + color-mix 로 통일.
	   이전엔 hex 와 rgba() 직접 — 라이트모드에서 색 안 변함. */
	.hl-btn.pre {
		color: var(--hl-pre);
		border-color: color-mix(in srgb, var(--hl-pre) 25%, transparent);
	}
	.hl-btn.sub {
		color: var(--hl-sub);
		border-color: color-mix(in srgb, var(--hl-sub) 25%, transparent);
	}
	.hl-btn.next {
		color: var(--hl-next);
		border-color: color-mix(in srgb, var(--hl-next) 25%, transparent);
	}
	.hl-btn.parent {
		color: var(--success);
		border-color: color-mix(in srgb, var(--success) 25%, transparent);
	}
	.hl-btn.all {
		color: var(--text);
		border-color: var(--border);
	}

	.hl-btn:hover {
		background: var(--bg-subtle);
	}
	.hl-btn.pre.on {
		background: color-mix(in srgb, var(--hl-pre) 15%, transparent);
		border-color: var(--hl-pre);
	}
	.hl-btn.sub.on {
		background: color-mix(in srgb, var(--hl-sub) 15%, transparent);
		border-color: var(--hl-sub);
	}
	.hl-btn.next.on {
		background: color-mix(in srgb, var(--hl-next) 15%, transparent);
		border-color: var(--hl-next);
	}
	.hl-btn.parent.on {
		background: color-mix(in srgb, var(--success) 15%, transparent);
		border-color: var(--success);
	}
	.hl-btn.all.on {
		background: color-mix(in srgb, var(--text-muted) 10%, transparent);
		border-color: var(--text-muted);
	}

	.hl-actions {
		margin: 6px 12px 0;
		display: flex;
		gap: 4px;
	}
	.hl-act {
		flex: 1;
		/* DEV-302: 아이콘 + 라벨 정렬. */
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.3em;
		padding: 4px 6px;
		background: var(--bg);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text-muted);
		font-size: 0.72rem;
		cursor: pointer;
		transition:
			background 0.1s,
			color 0.1s,
			border-color 0.1s;
	}
	.hl-act:hover:not(:disabled) {
		background: var(--bg-subtle);
		color: var(--text);
	}
	.hl-act:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.hl-act.sel {
		color: var(--accent);
		border-color: color-mix(in srgb, var(--accent) 30%, transparent);
	}
	.hl-act.sel:hover:not(:disabled) {
		background: color-mix(in srgb, var(--accent) 10%, transparent);
		color: var(--accent-secondary);
	}
	.hl-act.arr {
		color: var(--warning);
		border-color: color-mix(in srgb, var(--warning) 30%, transparent);
	}
	.hl-act.arr:hover:not(:disabled) {
		background: color-mix(in srgb, var(--warning) 10%, transparent);
		color: var(--orange);
	}
	.hl-act.clear {
		color: var(--text-faint);
	}

	.card-note {
		margin: 6px 12px 10px;
		font-size: 0.67rem;
		color: var(--border);
		line-height: 1.4;
	}
	/* check-spacing:on */

	/* ── 툴바 (z:10) ── */
	/* DEV-073 fix3: New Quest 는 상단 고정, 나머지 도구바는 그 아래로. */
	/* DEV-135: 필터 활성 chip — 좌상단 (toolbar 와 반대편). */
	.filter-chip {
		position: absolute;
		top: 0.625rem;
		left: 0.875rem;
		z-index: 10;
		pointer-events: auto;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.3rem 0.7rem;
		background: color-mix(in srgb, var(--accent) 12%, var(--bg-elevated));
		border: var(--bw) solid color-mix(in srgb, var(--accent) 45%, transparent);
		border-radius: var(--r-pill);
		font-size: 0.78rem;
		color: var(--text);
	}
	.fc-label {
		color: var(--accent);
		font-weight: 500;
	}
	.fc-clear {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-muted);
		font-size: 0.78rem;
		padding: 0;
	}
	.fc-clear:hover {
		color: var(--danger);
	}
	.performance-hud {
		position: absolute;
		left: 0.875rem;
		bottom: 0.875rem;
		z-index: 10;
		display: grid;
		grid-template-columns: auto auto;
		gap: 0.1875rem 0.75rem;
		padding: 0.5rem 0.625rem;
		border: var(--bw) solid color-mix(in srgb, var(--accent) 45%, transparent);
		border-radius: var(--r-md);
		background: color-mix(in srgb, var(--bg-elevated) 92%, transparent);
		box-shadow: 0 4px 18px rgba(0, 0, 0, 0.28);
		color: var(--text-muted);
		font-family: 'SFMono-Regular', Consolas, monospace;
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		line-height: 1.35;
		pointer-events: none;
		backdrop-filter: blur(8px);
	}
	.performance-hud strong {
		color: var(--accent);
		font-size: 13px;
	}

	.tb-newquest-wrap {
		position: absolute;
		top: 0.625rem;
		right: 0.875rem;
		z-index: 10;
		pointer-events: auto;
	}
	.toolbar {
		position: absolute;
		top: 0.625rem;
		right: 0.875rem;
		z-index: 10;
		display: flex;
		align-items: center;
		gap: 0.25rem;
		/* DEV-352: 레인 위에 버튼이 낱개로 떠 보이지 않도록 실제 컨트롤 크기의
		   불투명 surface로 묶는다. width:max-content를 쓰되 max-width로 좁은
		   화면에서는 기존처럼 줄바꿈한다. 화면 전체 폭의 클릭 띠를 만들지 않으므로
		   BUG-226의 빈 보드 클릭 회귀도 피한다. */
		left: auto;
		width: max-content;
		max-width: calc(100% - 28px);
		padding: 0.25rem;
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: 0 4px 14px color-mix(in srgb, var(--text) 16%, transparent);
		pointer-events: auto;
		flex-wrap: wrap;
		justify-content: flex-end;
		row-gap: 0.25rem;
	}
	/* BUG-226: 버튼·셀렉트 등 실제 컨트롤만 클릭을 받는다. */
	.toolbar > * {
		pointer-events: auto;
	}
	/* New Quest 가 있으면 도구바를 그 아래로 내림 — 새 퀘스트 버튼 높이 (~32px) + 여백. */
	.toolbar.has-newquest {
		top: 3.125rem;
	}
	/* DEV-073, DEV-352: collapsed 시 토글 버튼만 남기고 바깥 패널은 완전히 숨긴다. */
	.toolbar.collapsed {
		gap: 0;
		padding: 0;
		background: transparent;
		border: 0;
		border-radius: 0;
		box-shadow: none;
	}
	/* DEV-073: 접기 토글 — 항상 표시. */
	.tb-btn.tb-collapse {
		padding: 0.25rem 0.5rem;
		opacity: 1;
	}
	.tb-btn {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.25rem 0.625rem;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text-muted);
		font-size: 0.8rem;
		cursor: pointer;
		transition:
			background 0.1s,
			color 0.1s,
			border-color 0.1s;
	}
	.tb-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--accent) 8%, var(--bg-subtle));
		border-color: var(--text-faint);
		color: var(--text);
	}
	.tb-btn:disabled {
		opacity: 1;
		background: color-mix(in srgb, var(--bg-subtle) 70%, var(--bg-elevated));
		border-color: var(--border);
		color: var(--text-faint);
		cursor: default;
	}
	.tb-btn.tb-on {
		background: color-mix(in srgb, var(--warning) 14%, var(--bg-subtle));
		border-color: color-mix(in srgb, var(--warning) 55%, var(--border));
		color: var(--warning);
	}
	.tb-btn.tb-on:hover:not(:disabled) {
		background: color-mix(in srgb, var(--warning) 22%, var(--bg-subtle));
		border-color: var(--warning);
		color: var(--orange);
	}
	.tb-btn .icon {
		font-size: 0.95rem;
		line-height: 1;
	}
	.tb-btn .count {
		font-size: 0.7rem;
		color: var(--text-faint);
		min-width: 0.625rem;
		text-align: right;
	}
	.tb-btn:hover:not(:disabled) .count {
		color: var(--text-muted);
	}
	/* DEV-084: New Quest — toolbar 안 primary 강조 (초록).
	   DEV-074 fix6: --btn-primary-* 토큰으로 통일 (dark/light 자동). */
	.tb-btn.tb-new {
		background: var(--btn-primary-bg);
		border-color: var(--btn-primary-border);
		color: var(--btn-primary-text);
		font-weight: 600;
	}
	.tb-btn.tb-new:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
	.tb-sep {
		/* BUG-256: 세로 헤어라인이라 테두리와 같은 두께여야 한다 — 토큰. */
		width: var(--bw);
		background: var(--border);
		align-self: stretch;
		margin: 0.125rem 0;
	}
	.tb-select {
		padding: 0.1875rem 0.375rem;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text-muted);
		font-size: 0.8rem;
		cursor: pointer;
		outline: none;
	}
	.tb-select:hover {
		border-color: var(--text-faint);
		color: var(--text);
	}

	/* Arrange 버튼 + mode select 를 하나의 컨트롤처럼 묶음 */
	.tb-arrange-group {
		display: flex;
		align-items: stretch;
		gap: 0;
	}
	.tb-arrange-group .tb-arrange {
		border-top-right-radius: 0;
		border-bottom-right-radius: 0;
		border-right: none;
	}
	.tb-arrange-group .tb-mode {
		border-top-left-radius: 0;
		border-bottom-left-radius: 0;
		padding-left: 0.25rem;
	}

	.overlay {
		position: fixed;
		/* Nav(3.25rem) + 커스텀 타이틀바(Windows Tauri, 없으면 0px) 아래부터. */
		inset: calc(var(--nav-h, 3.25rem) + var(--titlebar-h, 0px)) 0 0 0;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-faint);
		font-size: 0.9rem;
		pointer-events: none;
		z-index: 2;
	}
	.overlay.error {
		color: var(--danger);
	}

	/* ── 인앱 확인 다이얼로그 ── */
	.dialog-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		z-index: 500;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.dialog {
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-xl);
		padding: 1.25rem 1.5rem 1rem;
		min-width: calc(17.5rem * var(--popup-scale, 1));
		max-width: calc(26.25rem * var(--popup-scale, 1)); /* BUG-064 */
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	.dialog-msg {
		margin: 0;
		font-size: 0.9rem;
		color: var(--text);
		line-height: 1.5;
		white-space: pre-wrap;
	}
	.dialog-btns {
		display: flex;
		gap: 0.5rem;
		justify-content: flex-end;
	}
	.dialog-ok {
		padding: 0.4rem 1.1rem;
		background: var(--btn-primary-bg);
		border: var(--bw) solid var(--btn-primary-border);
		border-radius: var(--r-md);
		color: var(--btn-primary-text);
		font-size: 0.875rem;
		cursor: pointer;
		transition:
			background 0.1s,
			border-color 0.1s;
	}
	.dialog-ok:hover {
		background: var(--btn-primary-bg-hover);
		border-color: var(--btn-primary-border-hover);
	}
	.dialog-cancel {
		padding: 0.4rem 1rem;
		background: transparent;
		border: var(--bw) solid var(--border);
		border-radius: var(--r-md);
		color: var(--text-muted);
		font-size: 0.875rem;
		cursor: pointer;
	}
	.dialog-cancel:hover {
		background: var(--bg-subtle);
	}

	/* DEV-056: 숨김 설정 모달 */
	.hide-modal {
		background: var(--bg-elevated);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-xl);
		padding: 1.25rem 1.5rem 1.25rem;
		/* 모바일: 30~40rem 고정폭이 좁은 화면에서 밖으로 삐져나감 — 뷰포트 폭 안으로 clamp. */
		width: min(calc(40rem * var(--popup-scale, 1)), calc(100vw - 2rem));
		min-width: min(calc(30rem * var(--popup-scale, 1)), calc(100vw - 2rem));
		max-width: calc(100vw - 2rem);
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		color: var(--text);
		box-sizing: border-box;
		/* DEV-135: 필터 섹션 추가로 길어질 수 있어 모달 자체 스크롤. */
		max-height: calc(100vh - 4rem);
		overflow-y: auto;
	}
	/* DEV-135: 보드 설정 모달 안 필터 섹션. */
	.bf-section {
		border-top: var(--bw) solid var(--bg-subtle);
		padding-top: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.bf-head {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}
	.bf-title {
		margin: 0;
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.bf-count {
		font-size: 0.8rem;
		color: var(--accent);
		font-weight: 600;
	}
	.bf-clear {
		margin-left: auto;
		background: transparent;
		border: var(--bw) solid color-mix(in srgb, var(--danger) 35%, transparent);
		color: var(--danger);
		border-radius: var(--r-md);
		padding: 0.15rem 0.5rem;
		font-size: 0.78rem;
		cursor: pointer;
	}
	.bf-clear:hover {
		background: color-mix(in srgb, var(--danger) 12%, transparent);
	}
	/* QuestListFilter 는 본래 가로 바 — 모달 안에선 우측 130px 예약 padding 불필요. */
	.bf-filter :global(.filter-bar),
	.bf-filter :global(.xfilter-panel) {
		padding-right: 1rem;
		background: transparent;
		border-bottom: none;
	}
	.hide-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.hide-title {
		margin: 0;
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.hide-close {
		background: transparent;
		border: none;
		color: var(--text-muted);
		font-size: 1.4rem;
		line-height: 1;
		cursor: pointer;
		padding: 0 0.3rem;
	}
	.hide-close:hover {
		color: var(--text);
	}
	.hide-help {
		margin: 0;
		font-size: 0.825rem;
		color: var(--text-muted);
		line-height: 1.45;
	}
	.hide-table-wrap {
		/* 모바일: 컬럼 최소폭 합이 모달폭을 넘으면 테이블만 가로 스크롤 (모달 자체는 안 넘침). */
		max-width: 100%;
		overflow-x: auto;
	}
	.hide-table {
		width: 100%;
		min-width: 26rem;
		border-collapse: collapse;
		font-size: 0.875rem;
	}
	.hide-table th,
	.hide-table td {
		text-align: left;
		padding: 0.5rem 0.6rem;
		border-bottom: var(--bw) solid var(--bg-subtle);
	}
	.hide-table th {
		color: var(--text-muted);
		font-weight: 500;
		font-size: 0.8rem;
	}
	.hide-table tr.lane-off .hide-lane-name {
		opacity: 0.45;
		text-decoration: line-through;
	}
	.hide-lane-name {
		font-weight: 500;
	}
	/* DEV-074 fix11: 체크박스 기본 스타일은 global.css 로 통일.
	   hide-table 만의 override 는 없음. */
	/* DEV-059 fix2: 보드 설정 모달의 lane 순서 변경 버튼. */
	/* BUG-143: td 에 display:flex 금지(table-cell 렌더 깨짐) — 일반 셀 +
	   버튼 간격은 margin. */
	.hide-table .reorder-cell {
		white-space: nowrap;
	}
	.hide-table .reorder-cell button + button {
		margin-left: 0.2rem;
	}
	.hide-table .reorder-btn {
		padding: 0.1rem 0.4rem;
		font-size: 0.75rem;
		background: var(--bg-subtle);
		border: var(--bw) solid var(--border);
		border-radius: var(--r-sm);
		color: var(--text-muted);
		cursor: pointer;
		transition:
			background 0.1s,
			color 0.1s;
	}
	.hide-table .reorder-btn:hover:not(:disabled) {
		background: var(--border);
		color: var(--text);
	}
	.hide-table .reorder-btn:disabled {
		opacity: 0.3;
		cursor: not-allowed;
	}
</style>
