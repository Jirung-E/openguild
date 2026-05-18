<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { get } from 'svelte/store';
	import cytoscape from 'cytoscape';
	import type { Core, NodeSingular } from 'cytoscape';
	import { goto } from '$app/navigation';
	import { questsApi } from '$lib/api/quests';
	import { metaApi } from '$lib/api/meta';
	import { flashQuestId } from '$lib/stores';
	import {
		URGENCY_COLOR,
		URGENCY_BG,
		URGENCY_LABEL,
		type Quest,
		type QuestDependency,
		type QuestPosition,
		type QuestStatus
	} from '$lib/types';

	const NODE_W = 284;
	const NODE_H = 80;

	function makeSvgUrl(quest: Quest): string {
		const W = NODE_W, H = NODE_H;
		const uc = URGENCY_COLOR[quest.urgency];
		const tc = quest.type_color;
		const ul = URGENCY_LABEL[quest.urgency];
		const qid = quest.quest_id;

		const x = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
		const qidW = Math.ceil(qid.length * 6.4) + 16;
		const ulW  = Math.ceil(ul.length  * 5.6) + 14;
		const ulX  = 10 + qidW + 6;

		// 제목 2줄 처리 (한 줄 ~36자)
		const full = quest.title;
		const CPL = 36;
		const line1 = full.slice(0, CPL);
		const rawL2 = full.length > CPL ? full.slice(CPL, CPL * 2) : '';
		const line2 = full.length > CPL * 2 ? rawL2.slice(0, CPL - 1) + '…' : rawL2;

		const titleY = line2 ? 44 : 52;

		const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}">
  <rect x="0" y="0" width="3" height="${H}" rx="1.5" fill="${uc}" opacity="0.9"/>
  <rect x="10" y="9" width="${qidW}" height="17" rx="8.5"
    fill="${tc}" fill-opacity="0.16" stroke="${tc}" stroke-opacity="0.55" stroke-width="1"/>
  <text x="${10 + qidW / 2}" y="21.5" text-anchor="middle"
    fill="${tc}" font-size="10" font-weight="600"
    font-family="'SFMono-Regular',Consolas,monospace">${x(qid)}</text>
  <rect x="${ulX}" y="9" width="${ulW}" height="17" rx="8.5"
    fill="${uc}" fill-opacity="0.16" stroke="${uc}" stroke-opacity="0.55" stroke-width="1"/>
  <text x="${ulX + ulW / 2}" y="21.5" text-anchor="middle"
    fill="${uc}" font-size="10" font-weight="500"
    font-family="system-ui,sans-serif">${x(ul)}</text>
  <text x="10" y="${titleY}" fill="#c9d1d9" font-size="12"
    font-family="system-ui,-apple-system,sans-serif">${x(line1)}</text>
  ${line2 ? `<text x="10" y="${titleY + 16}" fill="#c9d1d9" font-size="12"
    font-family="system-ui,-apple-system,sans-serif">${x(line2)}</text>` : ''}
</svg>`;
		return 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
	}
	const NODE_GAP = 28;
	const LANE_PAD_X = 20; // lane 양쪽 가장자리 여백 (한쪽)
	const LANE_W = NODE_W * 3 + NODE_GAP * 2 + LANE_PAD_X * 2; // 948px
	const LANE_GAP = 36;   // lane 사이 시각적 간격
	const LANE_STRIDE = LANE_W + LANE_GAP; // 한 lane 의 X 단위 (다음 lane 시작점까지)
	const LANE_TOP = 52;
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
		items: { questId: number; from: { x: number; y: number; statusId: number }; to: { x: number; y: number; statusId: number } }[];
	}
	type HistoryRecord = SingleMove | BatchMove;

	// ── DOM refs ─────────────────────────────────────────────────

	let container: HTMLDivElement;
	let lanesEl: HTMLDivElement;
	let headersEl: HTMLDivElement;
	let cy: Core | null = null;
	let sorted: QuestStatus[] = [];
	let laneOf = new Map<number, number>();

	// ── 반응형 상태 ──────────────────────────────────────────────

	type HighlightType = 'pre' | 'sub' | 'next' | 'parent';

	let loading = $state(true);
	let error = $state<string | null>(null);
	let undoStack = $state<HistoryRecord[]>([]);
	let redoStack = $state<HistoryRecord[]>([]);
	let expandedQuest = $state<Quest | null>(null);
	let expandedPos = $state({ x: 0, y: 0 });
	let cardPinned = false; // 사용자가 카드를 드래그하면 true, 새 노드 클릭 시 false
	let activeHighlights = $state(new Set<HighlightType>());
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
			localStorage.setItem('openguild.gridSnap', String(gridSnap));
		} catch {
			/* 무시 */
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

	/** 보드 좌표를 NODE_W+GAP / NODE_H+GAP 단위 그리드의 가장 가까운 셀 중앙으로 스냅. */
	function snapToGrid(x: number, y: number): { x: number; y: number } {
		const cellW = NODE_W + NODE_GAP;
		const cellH = NODE_H + NODE_GAP;
		// X 그리드 기준: 해당 lane 의 cols 에 따라 firstX 가 결정 (lane 중앙 기준 균등)
		const li = Math.max(0, Math.min((sorted?.length ?? 1) - 1, Math.floor(x / LANE_STRIDE)));
		const cols = laneCols[li] ?? 2;
		const firstX = laneFirstCellX(li, cols);
		const localX = x - firstX;
		const colIdx = Math.round(localX / cellW);
		const sx = firstX + colIdx * cellW;
		// Y 그리드 기준: 보드 상단 + NODE_H/2 (첫 셀 중앙)
		const baseY = LANE_TOP + 16 + NODE_H / 2;
		const rowIdx = Math.round((y - baseY) / cellH);
		const sy = baseY + rowIdx * cellH;
		return { x: sx, y: sy };
	}

	// ── 인앱 확인 다이얼로그 ────────────────────────────────────
	let confirmDialog = $state<{ msg: string; resolve: (ok: boolean) => void } | null>(null);
	function showConfirm(msg: string): Promise<boolean> {
		return new Promise((resolve) => { confirmDialog = { msg, resolve }; });
	}
	function confirmDialogResolve(ok: boolean) {
		confirmDialog?.resolve(ok);
		confirmDialog = null;
	}

	// ── 일반 상태 ────────────────────────────────────────────────

	let allQuests: Quest[] = [];
	let allDependencies: QuestDependency[] = [];
	let busy = false;
	let arranging = false;
	let ctrlHeld = false;
	let ctrlClickNode: NodeSingular | null = null;
	let boxDragEl: HTMLDivElement | null = null;
	let boxDragStart: { x: number; y: number } | null = null;

	// 드래그 시작 상태 (노드별 Map)
	const dragStartMap = new Map<number, { x: number; y: number; statusId: number }>();
	// 배치 dragfree 수집
	type PendingDragItem = {
		node: NodeSingular; questId: number;
		fromPos: { x: number; y: number }; fromStatus: number;
		toPos: { x: number; y: number }; toLaneIdx: number;
	};
	let pendingDragBatch: PendingDragItem[] = [];
	let pendingDragTimer: ReturnType<typeof setTimeout> | null = null;

	// 카드 드래그
	let cardDrag = $state<{ sx: number; sy: number; px: number; py: number } | null>(null);

	// 프로그램적으로 select() 를 호출할 때, 'select' 핸들러가 자동 해제하지 않도록 잠시 끔.
	let suppressUnselect = false;

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
		// 확장 카드 동기화
		if (expandedQuest?.id === questId) {
			expandedQuest = { ...expandedQuest, status_id: s.id, status_name_en: s.name_en, status_name_ko: s.name_ko, status_color: s.color };
		}
		// allQuests 캐시 동기화 (tap으로 확장 시 최신 상태 반영)
		const idx = allQuests.findIndex((q) => q.id === questId);
		if (idx !== -1) {
			allQuests[idx] = { ...allQuests[idx], status_id: s.id, status_name_en: s.name_en, status_name_ko: s.name_ko, status_color: s.color };
		}
	}

	async function applyRecord(record: HistoryRecord, direction: 'undo' | 'redo') {
		if (!cy || busy) return;
		busy = true;
		if (record.type === 'single') {
			const target = direction === 'undo' ? record.from : record.to;
			const node = cy.getElementById(`q-${record.questId}`) as NodeSingular;
			if (node.length === 0) { busy = false; return; }
			if (record.from.statusId !== record.to.statusId) {
				try {
					await questsApi.changeStatus(record.questId, { status_id: target.statusId });
					node.data('statusId', target.statusId);
					applyStatusChange(record.questId, target.statusId);
				} catch { busy = false; return; }
			}
			node.animate({ position: { x: target.x, y: target.y }, duration: 120 });
			questsApi.updatePosition(record.questId, { x: target.x, y: target.y }).catch(() => {});
		} else {
			const promises: Promise<unknown>[] = [];
			for (const item of record.items) {
				const node = cy!.getElementById(`q-${item.questId}`) as NodeSingular;
				if (node.length === 0) continue;
				const target = direction === 'undo' ? item.from : item.to;
				if (item.from.statusId !== item.to.statusId) {
					try {
						await questsApi.changeStatus(item.questId, { status_id: target.statusId });
						node.data('statusId', target.statusId);
						applyStatusChange(item.questId, target.statusId);
					} catch { continue; }
				}
				node.animate({ position: { x: target.x, y: target.y }, duration: 200 });
				promises.push(questsApi.updatePosition(item.questId, { x: target.x, y: target.y }).catch(() => {}));
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

	function getNodeAtScreenPos(sx: number, sy: number): NodeSingular | null {
		if (!cy) return null;
		const zoom = cy.zoom(), pan = cy.pan();
		const mx = (sx - pan.x) / zoom, my = (sy - pan.y) / zoom;
		let found: NodeSingular | null = null;
		cy.nodes('[questId]').forEach((n) => {
			const bb = n.boundingBox();
			if (mx >= bb.x1 && mx <= bb.x2 && my >= bb.y1 && my <= bb.y2) found = n as NodeSingular;
		});
		return found;
	}

	function selectNodesInBox(sx1: number, sy1: number, sx2: number, sy2: number) {
		if (!cy) return;
		const zoom = cy.zoom(), pan = cy.pan();
		const mx1 = (sx1 - pan.x) / zoom, my1 = (sy1 - pan.y) / zoom;
		const mx2 = (sx2 - pan.x) / zoom, my2 = (sy2 - pan.y) / zoom;
		cy.nodes('[questId]').forEach((node) => {
			const pos = node.position();
			if (pos.x >= mx1 && pos.x <= mx2 && pos.y >= my1 && pos.y <= my2) node.select();
		});
	}

	function cancelBoxSelection() {
		boxDragEl?.remove();
		boxDragEl = null;
		boxDragStart = null;
		cy?.panningEnabled(true);
	}

	function onBoxMouseDown(e: MouseEvent) {
		if (!ctrlHeld || e.button !== 0) return;
		const rect = container.getBoundingClientRect();
		const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
		const node = getNodeAtScreenPos(sx, sy);
		if (node) {
			ctrlClickNode = node;
			e.stopPropagation();
			return;
		}
		e.preventDefault();
		cy?.panningEnabled(false);
		boxDragEl = document.createElement('div');
		boxDragEl.style.cssText =
			`position:absolute;left:${sx}px;top:${sy}px;width:0;height:0;` +
			`border:1.5px dashed #4a90d9;background:rgba(74,144,217,0.1);` +
			`pointer-events:none;z-index:100;box-sizing:border-box;`;
		container.appendChild(boxDragEl);
		boxDragStart = { x: sx, y: sy };
	}

	function onBoxMouseMove(e: MouseEvent) {
		if (cardDrag) {
			const cw = container.clientWidth, ch = container.clientHeight;
			expandedPos = {
				x: Math.max(0, Math.min(cw - CARD_W - 8, cardDrag.px + e.clientX - cardDrag.sx)),
				y: Math.max(0, Math.min(ch - 120, cardDrag.py + e.clientY - cardDrag.sy))
			};
			return;
		}
		if (!boxDragEl || !boxDragStart) return;
		const rect = container.getBoundingClientRect();
		const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
		const x1 = Math.min(boxDragStart.x, sx), y1 = Math.min(boxDragStart.y, sy);
		boxDragEl.style.left = `${x1}px`;
		boxDragEl.style.top = `${y1}px`;
		boxDragEl.style.width = `${Math.abs(sx - boxDragStart.x)}px`;
		boxDragEl.style.height = `${Math.abs(sy - boxDragStart.y)}px`;
	}

	function onBoxMouseUp(e: MouseEvent) {
		if (cardDrag) { cardDrag = null; return; }
		if (ctrlClickNode) {
			ctrlClickNode.selected() ? ctrlClickNode.unselect() : ctrlClickNode.select();
			ctrlClickNode = null;
			return;
		}
		if (!boxDragEl || !boxDragStart) return;
		const rect = container.getBoundingClientRect();
		const sx = e.clientX - rect.left, sy = e.clientY - rect.top;
		const x1 = Math.min(boxDragStart.x, sx), y1 = Math.min(boxDragStart.y, sy);
		const x2 = Math.max(boxDragStart.x, sx), y2 = Math.max(boxDragStart.y, sy);
		if (x2 - x1 > 4 || y2 - y1 > 4) selectNodesInBox(x1, y1, x2, y2);
		cancelBoxSelection();
	}

	// ── 노드 확장 카드 ──────────────────────────────────────────

	function syncExpandedPos() {
		if (!cy || !expandedQuest || cardPinned) return;
		const node = cy.getElementById(`q-${expandedQuest.id}`) as NodeSingular;
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
	//   parent = 이 퀘스트의 부모 퀘스트              → 초록 #7ee787

	function applyHighlights(modes: Set<HighlightType>) {
		if (!cy || !expandedQuest) return;
		const qId = expandedQuest.id;
		const typeMap = new Map<number, HighlightType>();

		if (modes.has('pre'))
			allDependencies.filter((d) => d.quest_id === qId).forEach((d) => typeMap.set(d.prerequisite_id, 'pre'));
		if (modes.has('sub'))
			allQuests.filter((q) => q.parent_quest_id === qId).forEach((q) => typeMap.set(q.id, 'sub'));
		if (modes.has('next'))
			allDependencies.filter((d) => d.prerequisite_id === qId).forEach((d) => typeMap.set(d.quest_id, 'next'));
		if (modes.has('parent') && expandedQuest.parent_quest_id !== null)
			typeMap.set(expandedQuest.parent_quest_id, 'parent');

		cy.nodes('[questId]').forEach((n) => {
			const node = n as NodeSingular;
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
		if (next.has(mode)) next.delete(mode); else next.add(mode);
		activeHighlights = next;
		if (next.size > 0) applyHighlights(next); else clearHighlight();
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

	/** 현재 highlight 된 노드들을 모두 Cytoscape selected 상태로 만든다. */
	function selectHighlighted() {
		if (!cy) return;
		const ids = getHighlightedNodeIds();
		if (ids.size === 0) return;
		// suppressUnselect 동안은 select 핸들러의 자동 해제 로직을 건너뛴다.
		suppressUnselect = true;
		try {
			cy.nodes('[questId]:selected').unselect();
			ids.forEach((id) => {
				const n = cy!.getElementById(`q-${id}`);
				if (n.length > 0) n.select();
			});
		} finally {
			suppressUnselect = false;
		}
	}

	/**
	 * cluster size N 에 대해 lane cols 이하 범위에서 직사각형 (c × r) 결정.
	 *  - c * r >= N
	 *  - 영역(c * r) 최소
	 *  - tiebreaker: |c - r| 작은 쪽 (정사각형에 가까움)
	 */
	function bestRect(n: number, maxCols: number): { cols: number; rows: number } {
		let bestC = 1;
		let bestR = n;
		let bestArea = n;
		let bestAspect = Math.abs(1 - n);
		const cap = Math.min(n, Math.max(1, maxCols));
		for (let c = 1; c <= cap; c++) {
			const r = Math.ceil(n / c);
			const area = c * r;
			const aspect = Math.abs(c - r);
			if (
				area < bestArea ||
				(area === bestArea && aspect < bestAspect)
			) {
				bestC = c;
				bestR = r;
				bestArea = area;
				bestAspect = aspect;
			}
		}
		return { cols: bestC, rows: bestR };
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
	async function arrangeNodesGrouped(nodesToArrange: NodeSingular[], _cols: number) {
		void _cols;
		if (!cy || arranging) return;
		arranging = true;
		try {
			const cellW = NODE_W + NODE_GAP;
			const cellH = NODE_H + NODE_GAP;
			const baseY = LANE_TOP + 16 + NODE_H / 2;

			const allNodes =
				nodesToArrange.length > 0
					? nodesToArrange
					: (cy!.nodes('[questId]').toArray() as NodeSingular[]);
			if (allNodes.length === 0) return;
			const allIds = new Set(allNodes.map((n) => n.data('questId') as number));

			// 1) 인접 리스트 — allIds 안에 있는 edge 만 사용
			//    (lane 한정 호출 시: lane 안 노드 사이 edge 만 → cross-lane 무시)
			const adj = new Map<number, Set<number>>();
			allIds.forEach((id) => adj.set(id, new Set()));
			cy!.edges().forEach((e) => {
				const s = e.source().data('questId') as number;
				const t = e.target().data('questId') as number;
				if (allIds.has(s) && allIds.has(t)) {
					adj.get(s)!.add(t);
					adj.get(t)!.add(s);
				}
			});

			// 2) BFS 로 components
			const visited = new Set<number>();
			const components: number[][] = [];
			for (const id of allIds) {
				if (visited.has(id)) continue;
				const comp: number[] = [];
				const queue = [id];
				visited.add(id);
				while (queue.length > 0) {
					const cur = queue.shift()!;
					comp.push(cur);
					for (const nb of adj.get(cur) ?? new Set<number>()) {
						if (!visited.has(nb)) {
							visited.add(nb);
							queue.push(nb);
						}
					}
				}
				components.push(comp);
			}

			const isolated: number[] = [];
			const clusters: number[][] = [];
			for (const c of components) {
				if (c.length === 1) isolated.push(c[0]);
				else clusters.push(c);
			}

			const slugOf = (qid: number) =>
				(cy!.getElementById(`q-${qid}`) as NodeSingular).data('questSlug') as string;
			const statusOf = (qid: number) =>
				(cy!.getElementById(`q-${qid}`) as NodeSingular).data('statusId') as number;

			const batchItems: BatchMove['items'] = [];
			const savePromises: Promise<unknown>[] = [];
			const place = (qid: number, col: number, row: number) => {
				const node = cy!.getElementById(`q-${qid}`) as NodeSingular;
				if (node.length === 0) return;
				const sid = node.data('statusId') as number;
				const li = laneOf.get(sid) ?? 0;
				const lcols = laneCols[li] ?? 2;
				const firstX = laneFirstCellX(li, lcols);
				const x = firstX + col * cellW;
				const y = baseY + row * cellH;
				const fromPos = { ...node.position() };
				if (Math.abs(fromPos.x - x) < 0.5 && Math.abs(fromPos.y - y) < 0.5) return;
				batchItems.push({
					questId: qid,
					from: { ...fromPos, statusId: sid },
					to: { x, y, statusId: sid }
				});
				node.animate({ position: { x, y }, duration: 200 });
				savePromises.push(questsApi.updatePosition(qid, { x, y }).catch(() => {}));
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

			// 5) cluster 별 배치 — lane 별 최소 직사각형
			//   cluster 의 노드를 lane 별로 묶고, 각 lane 안에서 lane cols 가로로 wrap
			//   cluster 가 차지하는 row 수 = max over lanes (ceil(N_lane / lane cols))
			//   → cluster AABB 영역 = (참여 lane 들의 X 범위) × (cluster height)
			//   → cluster 사이 1 row gap 으로 Y 분리
			clusters.forEach((cluster, ci) => {
				const byLane = new Map<number, number[]>();
				for (const qid of cluster) {
					const sid = statusOf(qid);
					const li = laneOf.get(sid) ?? 0;
					const arr = byLane.get(li) ?? [];
					arr.push(qid);
					byLane.set(li, arr);
				}
				let clusterHeight = 1;
				for (const [li, ids] of byLane) {
					const lcols = laneCols[li] ?? 2;
					clusterHeight = Math.max(clusterHeight, Math.ceil(ids.length / lcols));
				}
				for (const [li, ids] of byLane) {
					ids.sort((a, b) => slugOf(a).localeCompare(slugOf(b)));
					const lcols = laneCols[li] ?? 2;
					ids.forEach((qid, i) => {
						const col = i % lcols;
						const r = globalRow + Math.floor(i / lcols);
						place(qid, col, r);
					});
				}
				globalRow += clusterHeight;
				if (ci < clusters.length - 1) globalRow += 1;
			});

			if (batchItems.length > 0) {
				undoStack.push({ type: 'batch', items: batchItems });
				if (undoStack.length > MAX_HISTORY) undoStack.shift();
				redoStack.length = 0;
			}
			await Promise.all(savePromises);
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
		const nodes: NodeSingular[] = [];
		for (const id of ids) {
			const n = cy.getElementById(`q-${id}`) as NodeSingular;
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
		const startY = LANE_TOP + 16 + NODE_H / 2;
		const cellW = NODE_W + NODE_GAP;
		const cellH = NODE_H + NODE_GAP;
		const batchItems: BatchMove['items'] = [];
		const savePromises: Promise<unknown>[] = [];

		for (const statusId of ids) {
			const li = laneOf.get(statusId) ?? 0;
			// 이 lane 의 cols 도 인자로 받은 cols 로 동기화 (snap grid 와 일치)
			laneCols[li] = cols;
			const firstX = laneFirstCellX(li, cols);
			const nodes = cy.nodes(`[statusId = ${statusId}]`);
			if (nodes.length === 0) continue;
			const sortedNodes = nodes.toArray().sort((a, b) => {
				const sa = (a as NodeSingular).data('questSlug') as string;
				const sb = (b as NodeSingular).data('questSlug') as string;
				return sa.localeCompare(sb);
			});
			sortedNodes.forEach((n, idx) => {
				const node = n as NodeSingular;
				const fromPos = { ...node.position() };
				const col = idx % cols, row = Math.floor(idx / cols);
				const x = firstX + col * cellW;
				const y = startY + row * cellH;
				const sid = node.data('statusId') as number;
				batchItems.push({ questId: node.data('questId') as number, from: { ...fromPos, statusId: sid }, to: { x, y, statusId: sid } });
				node.animate({ position: { x, y }, duration: 200 });
				savePromises.push(questsApi.updatePosition(node.data('questId') as number, { x, y }).catch(() => {}));
			});
		}
		// laneCols 가 바뀌었으므로 trigger
		laneCols = [...laneCols];
		if (batchItems.length > 0) {
			undoStack.push({ type: 'batch', items: batchItems });
			if (undoStack.length > MAX_HISTORY) undoStack.shift();
			redoStack.length = 0;
		}
		await Promise.all(savePromises);
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

	// ── 키보드 ─────────────────────────────────────────────────

	function handleKeydown(e: KeyboardEvent) {
		if (['ControlLeft','ControlRight','MetaLeft','MetaRight'].includes(e.code)) onCtrlDown();
		const tag = (e.target as HTMLElement).tagName;
		if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
		const ctrl = e.ctrlKey || e.metaKey;
		if (ctrl && e.code === 'KeyZ' && !e.shiftKey) { e.preventDefault(); undo(); }
		else if (ctrl && e.code === 'KeyZ' && e.shiftKey) { e.preventDefault(); redo(); }
		else if (!ctrl && e.code === 'KeyF') { e.preventDefault(); fitView(); }
		else if (!ctrl && e.code === 'KeyG') { e.preventDefault(); toggleGridSnap(); }
		else if (e.code === 'Escape') closeExpanded();
	}
	function handleKeyup(e: KeyboardEvent) {
		if (['ControlLeft','ControlRight','MetaLeft','MetaRight'].includes(e.code)) onCtrlUp();
	}

	// ── 초기화 ──────────────────────────────────────────────────

	onMount(() => {
		try {
			gridSnap = localStorage.getItem('openguild.gridSnap') === 'true';
		} catch {
			/* 무시 */
		}
		window.addEventListener('keydown', handleKeydown);
		window.addEventListener('keyup', handleKeyup);
		window.addEventListener('blur', onCtrlUp);
		window.addEventListener('mousemove', onBoxMouseMove);
		window.addEventListener('mouseup', onBoxMouseUp);
		container.addEventListener('mousedown', onBoxMouseDown, { capture: true });
		return () => {
			window.removeEventListener('keydown', handleKeydown);
			window.removeEventListener('keyup', handleKeyup);
			window.removeEventListener('blur', onCtrlUp);
			window.removeEventListener('mousemove', onBoxMouseMove);
			window.removeEventListener('mouseup', onBoxMouseUp);
			container.removeEventListener('mousedown', onBoxMouseDown, { capture: true });
		};
	});

	onMount(async () => {
		try {
			const [quests, statuses, positions, dependencies] = await Promise.all([
				questsApi.list(),
				metaApi.getQuestStatuses(),
				questsApi.listPositions(),
				questsApi.listDependencies()
			]);
			init(quests, statuses, positions, dependencies);
			// init 직후 store 에 flash id 가 이미 있으면 즉시 처리
			//  (Nav 의 New Quest 모달 → goto → 보드 페이지 도착 흐름)
			const pending = get(flashQuestId);
			if (pending) handleFlash(pending);
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to load';
		} finally {
			loading = false;
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
		if (!cy) return;
		laneCols = laneCols.map(() => cols);
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
				const fresh = await questsApi.list();
				allQuests = fresh;
				quest = fresh.find((q) => q.id === qid);
			}
			if (!quest || !cy) {
				flashQuestId.set(null);
				return;
			}

			let node = cy.getElementById(`q-${qid}`) as NodeSingular;
			if (node.length === 0) {
				// 보드에 없는 노드 — 적당한 위치에 추가하고 위치 저장
				const li = laneOf.get(quest.status_id) ?? 0;
				// 같은 레인의 기존 노드들 아래에 자연스럽게 배치
				const existing = cy.nodes(`[statusId = ${quest.status_id}]`).toArray();
				const maxY = existing.reduce(
					(m, n) => Math.max(m, (n as NodeSingular).position().y),
					LANE_TOP + NODE_H / 2
				);
				const x = li * LANE_STRIDE + LANE_W / 2;
				const y = maxY + NODE_H + NODE_GAP;

				cy.add({
					group: 'nodes',
					data: {
						id: `q-${qid}`,
						label: '',
						questId: qid,
						questSlug: quest.quest_id,
						statusId: quest.status_id,
						urgencyColor: URGENCY_COLOR[quest.urgency],
						urgencyBg: URGENCY_BG[quest.urgency],
						typeColor: quest.type_color,
						nodeBg: makeSvgUrl(quest),
						highlightType: '',
						active: false
					},
					position: { x, y }
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
				questsApi.updatePosition(qid, { x, y }).catch(() => {});
				node = cy.getElementById(`q-${qid}`) as NodeSingular;
			}

			// panTo (해당 노드를 화면 중앙으로)
			cy.animate({ center: { eles: node }, duration: 400 } as Parameters<Core['animate']>[0]);

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

	onDestroy(() => cy?.destroy());

	// ── 레인 HTML ───────────────────────────────────────────────

	function buildLaneDivs(sorted: QuestStatus[]) {
		lanesEl.innerHTML = '';
		sorted.forEach(() => {
			const col = document.createElement('div');
			col.className = 'lane-col';
			lanesEl.appendChild(col);
		});
		headersEl.innerHTML = '';
		// laneCols 초기값: 모두 2열, laneArrangeModes 초기값: 전역 모드
		laneCols = sorted.map(() => 2);
		laneArrangeModes = sorted.map(() => arrangeMode);
		sorted.forEach((s, li) => {
			const hdr = document.createElement('div');
			hdr.className = 'lane-hdr';
			const label = document.createElement('span');
			label.className = 'lane-label';
			label.textContent = s.name_en;
			label.style.color = s.color;
			const sel = document.createElement('select');
			sel.className = 'lane-cols-sel';
			sel.title = '이 레인 정렬 열 수';
			[1, 2, 3].forEach((n) => {
				const opt = document.createElement('option');
				opt.value = String(n);
				opt.textContent = `${n}열`;
				if (n === 2) opt.selected = true;
				sel.appendChild(opt);
			});
			// select 변경 시 laneCols 즉시 업데이트 — snap grid 가 따라옴
			sel.onchange = () => {
				laneCols[li] = parseInt(sel.value);
				laneCols = [...laneCols]; // reactive trigger
				syncLanes(); // grid 시각화 재계산
			};
			const btn = document.createElement('button');
			btn.className = 'lane-arrange-btn';
			btn.title = 'Arrange this lane';
			btn.textContent = '⊟';

			// lane 별 정렬 모드 select (Group/All) — 전역 toolbar 의 mode select 와 같은 역할
			const modeSel = document.createElement('select');
			modeSel.className = 'lane-mode-sel';
			modeSel.title = '이 레인 정렬 모드';
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
					const laneNodes = cy
						.nodes(`[statusId = ${s.id}]`)
						.toArray() as NodeSingular[];
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

			hdr.appendChild(label);
			hdr.appendChild(sel); // cols select 는 별개 (그리드만 갱신)
			hdr.appendChild(arrangeWrap);
			headersEl.appendChild(hdr);
		});
	}

	function syncLanes() {
		if (!cy) return;
		const pan = cy.pan(), zoom = cy.zoom();

		// 그리드 스냅 시각화: lane-col 의 background 으로 dot 패턴.
		// 가로 dot 수가 정확히 lane 의 cols 와 일치하도록, cols 개 dot 가 들어간 SVG 를 한 row 로 두고
		// 세로 방향만 repeat. lane 마다 cols 가 다를 수 있어 lane 별로 SVG 합성.
		const cellHPx = (NODE_H + NODE_GAP) * zoom;
		const dotR = Math.max(1, 1.5 * zoom);

		lanesEl.querySelectorAll<HTMLElement>('.lane-col').forEach((col, i) => {
			col.style.left = `${i * LANE_STRIDE * zoom + pan.x}px`;
			col.style.width = `${LANE_W * zoom}px`;
			if (gridSnap) {
				const cols = laneCols[i] ?? 2;
				// 첫 dot center (lane-col local X) = laneFirstCellX - i*LANE_STRIDE (보드→local 변환 후 zoom)
				const firstCxLocal = (laneFirstCellX(i, cols) - i * LANE_STRIDE) * zoom;
				const cellWPx = (NODE_W + NODE_GAP) * zoom;
				// SVG 너비 = cols * cellW (각 dot 셀 가로폭). 첫 dot 은 셀 0 의 중앙.
				const svgW = cellWPx * cols;
				const svgH = cellHPx;
				const dots = Array.from({ length: cols }, (_, c) => {
					const cx = c * cellWPx + cellWPx / 2;
					const cy = cellHPx / 2;
					return `<circle cx="${cx}" cy="${cy}" r="${dotR}" fill="rgba(245,166,35,0.55)"/>`;
				}).join('');
				const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='${svgW}' height='${svgH}'>${dots}</svg>`;
				const dataUri = `url("data:image/svg+xml;utf8,${encodeURIComponent(svg)}")`;

				// background-position: SVG 의 좌상단 = 첫 dot center - cellW/2 (가로), 첫 dot center Y - cellH/2 (세로)
				const localCyPx = (LANE_TOP + 16 + NODE_H / 2) * zoom + pan.y;
				const bgX = firstCxLocal - cellWPx / 2;
				const bgY = localCyPx - cellHPx / 2;
				col.style.backgroundImage = dataUri;
				col.style.backgroundSize = `${svgW}px ${svgH}px`;
				col.style.backgroundPosition = `${bgX}px ${bgY}px`;
				// 가로는 한 번만, 세로는 반복
				col.style.backgroundRepeat = 'repeat-y';
			} else {
				col.style.backgroundImage = '';
			}
		});
		headersEl.querySelectorAll<HTMLElement>('.lane-hdr').forEach((hdr, i) => {
			hdr.style.left = `${i * LANE_STRIDE * zoom + pan.x}px`;
			hdr.style.width = `${LANE_W * zoom}px`;
		});
		syncExpandedPos();
	}

	// ── Cytoscape 초기화 ────────────────────────────────────────

	function init(
		quests: Quest[],
		statuses: QuestStatus[],
		positions: QuestPosition[],
		dependencies: QuestDependency[]
	) {
		sorted = [...statuses].sort((a, b) => a.sort_order - b.sort_order);
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

		const laneNextY = new Map<number, number>(sorted.map((s) => [s.id, LANE_TOP + 20]));
		posMap.forEach(({ y }, questId) => {
			const quest = quests.find((q) => q.id === questId);
			if (!quest) return;
			const cur = laneNextY.get(quest.status_id) ?? LANE_TOP + 20;
			laneNextY.set(quest.status_id, Math.max(cur, y + NODE_H + NODE_GAP));
		});
		const autoCount = new Map<number, number>();
		const elements: cytoscape.ElementDefinition[] = [];

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
				const startY = laneNextY.get(q.status_id) ?? LANE_TOP + 20;
				const col = n % 3;
				const row = Math.floor(n / 3);
				const laneLeft = li * LANE_STRIDE;
				pos = {
					x: laneLeft + COL_OFFSETS[col],
					y: startY + row * (NODE_H + NODE_GAP)
				};
				autoCount.set(q.status_id, n + 1);
			}
			elements.push({
				data: {
					id: `q-${q.id}`,
					label: '',
					questId: q.id,
					questSlug: q.quest_id,
					statusId: q.status_id,
					urgencyColor: URGENCY_COLOR[q.urgency],
					urgencyBg: URGENCY_BG[q.urgency],
					typeColor: q.type_color,
					nodeBg: makeSvgUrl(q),
					highlightType: '',
					active: false
				},
				position: pos
			});
		});

		dependencies.forEach((d) => {
			elements.push({ data: { id: `pre-${d.prerequisite_id}-${d.quest_id}`, source: `q-${d.prerequisite_id}`, target: `q-${d.quest_id}`, etype: 'pre', dimmed: false } });
		});
		quests.filter((q) => q.parent_quest_id !== null).forEach((q) => {
			elements.push({ data: { id: `sub-${q.parent_quest_id}-${q.id}`, source: `q-${q.parent_quest_id}`, target: `q-${q.id}`, etype: 'sub', dimmed: false } });
		});

		cy = cytoscape({
			container,
			elements,
			style: [
				{
					selector: 'node[questId]',
					style: {
						'background-color': '#0d1117',
						'background-image': 'data(nodeBg)',
						'background-fit': 'cover',
						'background-image-opacity': 1,
						'border-color': 'data(urgencyColor)',
						'border-width': 2,
						width: NODE_W, height: NODE_H,
						shape: 'round-rectangle',
						label: '',
						'z-index': 10
					}
				},
				{ selector: 'node[questId]:active', style: { 'overlay-opacity': 0 } },
				// 확장 카드에 열린 노드 — 긴급도 색 glow
				// (뒤에 오는 :selected가 이기므로 이동용 선택 시 파란 하이라이트로 전환됨)
				{
					selector: 'node[questId][?active]',
					style: {
						'background-color': 'data(urgencyBg)',
						'border-color': 'data(urgencyColor)',
						'border-width': 3,
						'shadow-blur': 18,
						'shadow-color': 'data(urgencyColor)',
						'shadow-opacity': 0.85,
						'shadow-offset-x': 0,
						'shadow-offset-y': 0
					} as cytoscape.Css.Node
				},
				// 선행 퀘스트 (보라)
				{
					selector: 'node[questId][highlightType = "pre"]',
					style: { 'background-color': '#190d33', 'border-color': '#a371f7', 'border-width': 3, 'shadow-blur': 12, 'shadow-color': '#a371f7', 'shadow-opacity': 0.65, 'shadow-offset-x': 0, 'shadow-offset-y': 0 } as cytoscape.Css.Node
				},
				// 서브 퀘스트 (청록)
				{
					selector: 'node[questId][highlightType = "sub"]',
					style: { 'background-color': '#062220', 'border-color': '#3dc9b0', 'border-width': 3, 'shadow-blur': 12, 'shadow-color': '#3dc9b0', 'shadow-opacity': 0.65, 'shadow-offset-x': 0, 'shadow-offset-y': 0 } as cytoscape.Css.Node
				},
				// 후속 퀘스트 (주황)
				{
					selector: 'node[questId][highlightType = "next"]',
					style: { 'background-color': '#2a1200', 'border-color': '#f0883e', 'border-width': 3, 'shadow-blur': 12, 'shadow-color': '#f0883e', 'shadow-opacity': 0.65, 'shadow-offset-x': 0, 'shadow-offset-y': 0 } as cytoscape.Css.Node
				},
				// 부모 퀘스트 (초록)
				{
					selector: 'node[questId][highlightType = "parent"]',
					style: { 'background-color': '#0a2914', 'border-color': '#7ee787', 'border-width': 3, 'shadow-blur': 12, 'shadow-color': '#7ee787', 'shadow-opacity': 0.65, 'shadow-offset-x': 0, 'shadow-offset-y': 0 } as cytoscape.Css.Node
				},
				// 연관 없음: 흐림
				{
					selector: 'node[questId][highlightType = "dim"]',
					style: { opacity: 0.15 } as cytoscape.Css.Node
				},
				// 이동용 선택 — 가장 뒤에 위치해야 [?active] 포함 모든 상태보다 우선함
				{
					selector: 'node[questId]:selected',
					style: {
						'background-color': '#112240',
						'border-color': '#58a6ff',
						'border-width': 3,
						'shadow-blur': 14,
						'shadow-color': '#58a6ff',
						'shadow-opacity': 0.65,
						'shadow-offset-x': 0,
						'shadow-offset-y': 0
					} as cytoscape.Css.Node
				},
				// New Quest 직후 강조 — 모든 상태 위에 덮어쓰기
				{
					selector: 'node[questId][?flash]',
					style: {
						'border-color': '#79c0ff',
						'border-width': 5,
						'shadow-blur': 28,
						'shadow-color': '#79c0ff',
						'shadow-opacity': 1,
						'shadow-offset-x': 0,
						'shadow-offset-y': 0
					} as cytoscape.Css.Node
				},
				// 엣지 흐림
				{ selector: 'edge[?dimmed]', style: { opacity: 0.07 } },
				{
					selector: 'edge[etype = "pre"]',
					style: { 'line-color': '#4a90d9', 'target-arrow-color': '#4a90d9', 'target-arrow-shape': 'triangle', 'line-style': 'solid', 'curve-style': 'bezier', width: 2 }
				},
				{
					selector: 'edge[etype = "sub"]',
					style: { 'line-color': '#484f58', 'target-arrow-color': '#484f58', 'target-arrow-shape': 'vee', 'line-style': 'dashed', 'line-dash-pattern': [6, 3], 'curve-style': 'bezier', width: 1.5 }
				}
			],
			layout: { name: 'preset' },
			minZoom: 0.25,
			maxZoom: 2,
			// wheel zoom 속도 — 기본 1 이 너무 느림 (사용자 피드백).
			// cytoscape 권장 범위 [1, ~3]. 2.5 면 한 번 휠 클릭에 체감 ~2x 빠름.
			wheelSensitivity: 2.5,
			boxSelectionEnabled: false
		});

		cy.on('pan zoom', () => syncLanes());

		// ── 드래그 이벤트 (다중선택 배치 처리) ─────────────────────

		cy.on('grabon', 'node[questId]', (e) => {
			const node = e.target as NodeSingular;
			const questId = node.data('questId') as number;
			// grabbed 노드 + 현재 selected 노드 전부를 시작 상태로 기록.
			// (Cytoscape의 ctrl-click 토글, single-mode 자동 unselect 등으로
			//  selected 상태가 grab 시점에 변할 수 있으므로 여기서 한 번에 캡처.)
			dragStartMap.set(questId, { ...node.position(), statusId: node.data('statusId') as number });
			cy!.nodes('[questId]:selected').forEach((n) => {
				const qid = n.data('questId') as number;
				if (dragStartMap.has(qid)) return;
				dragStartMap.set(qid, { ...n.position(), statusId: n.data('statusId') as number });
			});
		});

		cy.on('dragfree', 'node[questId]', () => {
			// dragStartMap은 단일 source of truth.
			// 첫 dragfree 이벤트 때 모든 항목을 일괄 처리하고 비운다.
			// 이후 co-dragged 노드의 dragfree 이벤트가 추가로 와도 size===0이라 무시.
			if (dragStartMap.size === 0) return;

			for (const [qid, fromState] of dragStartMap) {
				const n = cy!.getElementById(`q-${qid}`) as NodeSingular;
				if (n.length === 0) continue;
				const pos = n.position();
				const li = Math.max(0, Math.min(Math.floor(pos.x / LANE_STRIDE), sorted.length - 1));
				pendingDragBatch.push({
					node: n, questId: qid,
					fromPos: { x: fromState.x, y: fromState.y },
					fromStatus: fromState.statusId,
					toPos: { ...pos },
					toLaneIdx: li
				});
			}
			dragStartMap.clear();

			// 이번 tick에 발생한 모든 dragfree가 모이면 한 번에 처리
			if (pendingDragTimer !== null) clearTimeout(pendingDragTimer);
			pendingDragTimer = setTimeout(() => {
				pendingDragTimer = null;
				processPendingDrags();
			}, 0);
		});

		async function processPendingDrags() {
			const batch = pendingDragBatch.splice(0);
			if (batch.length === 0) return;

			// 레인이 바뀐 항목들을 레인 인덱스별로 그룹화
			const laneChanges = new Map<number, PendingDragItem[]>();
			for (const item of batch) {
				if (sorted[item.toLaneIdx].id !== item.fromStatus) {
					const existing = laneChanges.get(item.toLaneIdx) ?? [];
					existing.push(item);
					laneChanges.set(item.toLaneIdx, existing);
				}
			}

			// 레인 변경 그룹마다 한 번의 확인 다이얼로그 (인앱)
			const confirmedLanes = new Set<number>();
			const rejectedLanes = new Set<number>();
			for (const [laneIdx, items] of laneChanges) {
				const newStatus = sorted[laneIdx];
				const names = items.map((it) => it.node.data('questSlug')).join(', ');
				const msg = items.length === 1
					? `${names} → "${newStatus.name_en}" 상태로 변경할까요?`
					: `${items.length}개 퀘스트를 "${newStatus.name_en}" 상태로 변경할까요?\n(${names})`;
				if (await showConfirm(msg)) {
					confirmedLanes.add(laneIdx);
				} else {
					rejectedLanes.add(laneIdx);
				}
			}

			// 각 항목 처리
			const historyItems: BatchMove['items'] = [];
			const posUpdates: Promise<unknown>[] = [];

			for (const item of batch) {
				const { node, questId, fromPos, fromStatus, toPos, toLaneIdx } = item;
				const newStatus = sorted[toLaneIdx];
				const laneChanged = newStatus.id !== fromStatus;

				if (laneChanged && rejectedLanes.has(toLaneIdx)) {
					// 거부 → 드래그 시작 위치로 정확히 복원
					node.animate({ position: { x: fromPos.x, y: fromPos.y }, duration: 150 });
					continue;
				}

				if (laneChanged && confirmedLanes.has(toLaneIdx)) {
					try {
						await questsApi.changeStatus(questId, { status_id: newStatus.id });
						node.data('statusId', newStatus.id);
						applyStatusChange(questId, newStatus.id);
					} catch {
						// API 실패 → 드래그 시작 위치로 복원
						node.animate({ position: { x: fromPos.x, y: fromPos.y }, duration: 150 });
						continue;
					}
				}

				// 그리드 스냅 (옵션)
				let snappedX = toPos.x;
				let snappedY = toPos.y;
				if (gridSnap) {
					const s = snapToGrid(toPos.x, toPos.y);
					snappedX = s.x;
					snappedY = s.y;
				}
				// X축 클램프 (스냅이 레인 경계를 넘으면 다시 안으로). Y 는 자유.
				const laneLeft = toLaneIdx * LANE_STRIDE;
				const minX = laneLeft + LANE_PAD_X + NODE_W / 2;
				const maxX = laneLeft + LANE_W - LANE_PAD_X - NODE_W / 2;
				const clampedX = Math.max(minX, Math.min(maxX, snappedX));
				const finalY = snappedY;
				if (clampedX !== toPos.x || finalY !== toPos.y) {
					node.position({ x: clampedX, y: finalY });
				}

				const finalStatusId = laneChanged && confirmedLanes.has(toLaneIdx) ? newStatus.id : fromStatus;
				const moved = fromPos.x !== clampedX || fromPos.y !== finalY || fromStatus !== finalStatusId;
				if (moved) {
					historyItems.push({
						questId,
						from: { x: fromPos.x, y: fromPos.y, statusId: fromStatus },
						to: { x: clampedX, y: finalY, statusId: finalStatusId }
					});
					posUpdates.push(questsApi.updatePosition(questId, { x: clampedX, y: finalY }).catch(() => {}));
				}
			}

			if (historyItems.length > 0) {
				const record: HistoryRecord = historyItems.length === 1
					? { type: 'single', questId: historyItems[0].questId, from: historyItems[0].from, to: historyItems[0].to }
					: { type: 'batch', items: historyItems };
				undoStack.push(record);
				if (undoStack.length > MAX_HISTORY) undoStack.shift();
				redoStack.length = 0;
			}

			await Promise.all(posUpdates);
			syncExpandedPos();
		}

		// Ctrl 없는 클릭으로 인한 선택은 즉시 해제 (mousedown → select 순서로 발생)
		// Ctrl+클릭은 이동용 선택이므로 허용.
		// suppressUnselect=true 일 때는 프로그램적 select() 호출이므로 그대로 둔다.
		cy.on('select', 'node[questId]', (e) => {
			if (suppressUnselect) return;
			if (!ctrlHeld) (e.target as NodeSingular).unselect();
		});

		// 일반 클릭 → 확장 카드 열기
		cy.on('tap', 'node[questId]', (e) => {
			const node = e.target as NodeSingular;
			const quest = allQuests.find((q) => q.id === node.data('questId'));
			if (!quest) return;
			cy!.nodes('[questId]').data('active', false).data('highlightType', '');
			cy!.edges().data('dimmed', false);
			node.data('active', true);
			expandedQuest = quest;
			activeHighlights = new Set();
			cardPinned = false;
			cardDrag = null;
			syncExpandedPos();
		});

		cy.on('tap', (e) => {
			if (e.target === cy) {
				cy!.elements().unselect();
				closeExpanded();
			}
		});

		cy.fit(undefined, 60);
		syncLanes();
	}
</script>

<div class="board-wrap">
	<div class="lanes-bg" bind:this={lanesEl}></div>
	<div class="board" bind:this={container}></div>
	<div class="lane-hdrs" bind:this={headersEl}></div>

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
			<span class="drag-hint" title="드래그하여 이동">⠿</span>
			<div class="card-badges">
				<span class="badge" style:--c={expandedQuest.type_color}>{expandedQuest.quest_id}</span>
				<span class="badge" style:--c={URGENCY_COLOR[expandedQuest.urgency]}>{URGENCY_LABEL[expandedQuest.urgency]}</span>
				<span class="badge" style:--c={expandedQuest.status_color}>{expandedQuest.status_name_en}</span>
			</div>
			<button class="card-close" onclick={closeExpanded} title="닫기 (Esc)">×</button>
		</div>

		<p class="card-title">{expandedQuest.title}</p>

		<div class="card-branch">
			<span class="blabel">Branch</span>
			<code class="bname">{expandedQuest.type_prefix}-{String(expandedQuest.number).padStart(3, '0')}</code>
		</div>

		<button class="card-goto" onclick={() => goto(`/quests/${expandedQuest!.quest_id}`)}>
			퀘스트 상세 페이지로 이동 →
		</button>

		<div class="card-divider"></div>
		<p class="card-sec-label">연관 퀘스트 하이라이트 <span class="hl-multi-hint">(다중 선택 가능)</span></p>

		<div class="card-hl-grid">
			<button class="hl-btn all" class:on={activeHighlights.size === 4}
				onclick={toggleAllHighlights}
			>연관 전체</button>
			<button class="hl-btn pre" class:on={activeHighlights.has('pre')}
				onclick={() => toggleHighlight('pre')}
			>● 선행 퀘스트</button>
			<button class="hl-btn sub" class:on={activeHighlights.has('sub')}
				onclick={() => toggleHighlight('sub')}
			>● 서브 퀘스트</button>
			<button class="hl-btn next" class:on={activeHighlights.has('next')}
				onclick={() => toggleHighlight('next')}
			>● 후속 퀘스트</button>
			<button class="hl-btn parent" class:on={activeHighlights.has('parent')}
				onclick={() => toggleHighlight('parent')}
			>● 부모 퀘스트</button>
		</div>

		{#if activeHighlights.size > 0}
			<div class="hl-actions">
				<button class="hl-act sel" onclick={selectHighlighted} title="하이라이트된 노드들을 모두 선택 (드래그·상태변경 대상으로)">
					🔘 선택
				</button>
				<button class="hl-act arr" onclick={arrangeHighlightedGroup} disabled={arranging} title="하이라이트된 노드들을 그룹으로 정렬">
					⊞ 정렬
				</button>
				<button class="hl-act clear" onclick={clearHighlight} title="하이라이트 해제">
					× 해제
				</button>
			</div>
		{/if}

		<p class="card-note">하이라이트는 선택(파란색)과 별개 — '선택' 버튼을 누르면 드래그·상태변경 대상이 됨</p>
	</div>
	{/if}

	<!-- 툴바 -->
	<div class="toolbar">
		<button class="tb-btn" onclick={fitView} title="Fit view (F)"><span class="icon">⊞</span></button>
		<div class="tb-sep"></div>
		<button class="tb-btn" onclick={undo} disabled={undoStack.length === 0} title="Undo (Ctrl+Z)">
			<span class="icon">↩</span>
			{#if undoStack.length > 0}<span class="count">{undoStack.length}</span>{/if}
		</button>
		<button class="tb-btn" onclick={redo} disabled={redoStack.length === 0} title="Redo (Ctrl+Shift+Z)">
			<span class="icon">↪</span>
			{#if redoStack.length > 0}<span class="count">{redoStack.length}</span>{/if}
		</button>
		<div class="tb-sep"></div>
		<button
			class="tb-btn"
			class:tb-on={gridSnap}
			onclick={toggleGridSnap}
			title="그리드 스냅 — 드래그 종료 시 격자에 정렬 (G)"
		>
			<span class="icon">⊞</span><span>Snap</span>
		</button>
		<div class="tb-sep"></div>
		<select
			class="tb-select"
			value={globalCols}
			onchange={(e) => setGlobalCols(parseInt((e.currentTarget as HTMLSelectElement).value))}
			title="레인 그리드 열 수 (그리드만 갱신)"
		>
			<option value={1}>1열</option>
			<option value={2}>2열</option>
			<option value={3}>3열</option>
		</select>
		<div class="tb-sep"></div>
		<!-- arrange 버튼 + mode select 는 하나의 컨트롤처럼 시각적으로 묶음 -->
		<div class="tb-arrange-group">
			<button
				class="tb-btn tb-arrange"
				onclick={() => {
					if (!cy) return;
					if (arrangeMode === 'group') {
						arrangeNodesGrouped(cy.nodes('[questId]').toArray() as NodeSingular[], globalCols);
					} else {
						arrangeNodes(null, globalCols);
					}
				}}
				title={arrangeMode === 'group'
					? '모든 노드 정렬 — 연관 그룹은 직사각형 영역으로 묶고, isolated 는 위쪽에 배치'
					: '모든 노드 정렬 — 슬러그 순으로 lane 안에서 왼쪽 위부터 채움'}
			>
				<span class="icon">⊟</span><span>Arrange</span>
			</button>
			<select class="tb-select tb-mode" bind:value={arrangeMode} title="정렬 모드">
				<option value="group">Group</option>
				<option value="all">All</option>
			</select>
		</div>
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
				<button class="dialog-ok" onclick={() => confirmDialogResolve(true)}>변경</button>
				<button class="dialog-cancel" onclick={() => confirmDialogResolve(false)}>취소</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.board-wrap {
		position: relative;
		width: 100%;
		height: calc(100vh - 52px);
		background: #0d1117;
		overflow: hidden;
	}

	.lanes-bg { position: absolute; inset: 0; z-index: 0; pointer-events: none; }
	.board { position: absolute; inset: 0; z-index: 1; background: transparent; }
	.lane-hdrs { position: absolute; inset: 0; z-index: 2; pointer-events: none; overflow: hidden; }

	:global(.lane-col) {
		position: absolute; top: 0; bottom: 0;
		background: #161b22;
		border-right: 1px solid #21262d;
		box-sizing: border-box;
		pointer-events: none;
	}
	:global(.lane-hdr) {
		position: absolute; top: 0; height: 38px;
		display: flex; align-items: center; gap: 6px;
		padding: 0 8px 0 14px;
		border-right: 1px solid #21262d;
		border-bottom: 1px solid #21262d;
		box-sizing: border-box;
		background: #161b22;
		pointer-events: none;
	}
	:global(.lane-label) {
		flex: 1; font-size: 12px; font-weight: bold;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
		pointer-events: none;
	}
	:global(.lane-cols-sel) {
		flex-shrink: 0; pointer-events: auto;
		background: #0d1117; border: 1px solid #30363d; border-radius: 4px;
		color: #8b949e; font-size: 0.72rem; padding: 1px 3px; cursor: pointer; outline: none;
	}
	:global(.lane-cols-sel:hover) { border-color: #484f58; color: #c9d1d9; }
	:global(.lane-arrange-btn) {
		flex-shrink: 0; pointer-events: auto;
		background: none; border: 1px solid transparent; border-radius: 4px;
		color: #484f58; font-size: 0.85rem; padding: 1px 5px;
		cursor: pointer; line-height: 1.4;
		transition: background 0.1s, color 0.1s, border-color 0.1s;
	}
	:global(.lane-arrange-btn:hover) { background: #21262d; border-color: #30363d; color: #8b949e; }

	/* lane header 의 mode select (Group / All) — lane-cols-sel 과 비슷한 비주얼 */
	:global(.lane-mode-sel) {
		flex-shrink: 0; pointer-events: auto;
		background: #0d1117; border: 1px solid #30363d; border-radius: 4px;
		color: #8b949e; font-size: 0.72rem; padding: 1px 3px; cursor: pointer; outline: none;
	}
	:global(.lane-mode-sel:hover) { border-color: #484f58; color: #c9d1d9; }

	/* lane header 의 ⊟ 버튼 + mode select 를 segmented 컨트롤로 묶음 (toolbar 와 동일 패턴) */
	:global(.lane-arrange-group) {
		flex-shrink: 0;
		display: flex;
		align-items: stretch;
		gap: 0;
		pointer-events: auto;
	}
	:global(.lane-arrange-group .lane-arrange-btn) {
		border: 1px solid #30363d;
		border-right: none;
		border-top-right-radius: 0;
		border-bottom-right-radius: 0;
		background: #0d1117;
	}
	:global(.lane-arrange-group .lane-mode-sel) {
		border-top-left-radius: 0;
		border-bottom-left-radius: 0;
	}

	/* ── 노드 확장 카드 (z:6) ── */
	.node-card {
		position: absolute;
		width: 300px;
		z-index: 6;
		background: #161b22;
		border: 1px solid #30363d;
		border-radius: 10px;
		box-shadow: 0 8px 32px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.04);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		animation: card-expand 0.2s cubic-bezier(0.34, 1.4, 0.64, 1) forwards;
		transform-origin: top center;
		cursor: default;
		user-select: none;
	}
	.node-card:not(.card-dragging) .card-head { cursor: grab; }
	.node-card.card-dragging { cursor: grabbing; box-shadow: 0 16px 48px rgba(0,0,0,0.7); }
	@keyframes card-expand {
		from { opacity: 0; transform: scale(0.72) translateY(-8px); }
		to   { opacity: 1; transform: scale(1)    translateY(0);     }
	}

	.card-head {
		display: flex; align-items: flex-start; gap: 6px;
		padding: 8px 10px 8px 8px;
		border-bottom: 1px solid #21262d;
	}
	.drag-hint {
		flex-shrink: 0;
		color: #30363d;
		font-size: 1rem;
		line-height: 1.4;
		padding: 1px 2px;
		transition: color 0.1s;
	}
	.card-head:hover .drag-hint { color: #484f58; }
	.card-badges { flex: 1; display: flex; flex-wrap: wrap; gap: 4px; }
	.badge {
		padding: 0.15rem 0.5rem; border-radius: 20px;
		font-size: 0.7rem; font-weight: 500;
		background: color-mix(in srgb, var(--c) 18%, transparent);
		color: var(--c);
		border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
		white-space: nowrap;
	}
	.card-close {
		flex-shrink: 0; background: none; border: none;
		color: #484f58; font-size: 1.1rem; line-height: 1;
		padding: 0 2px; cursor: pointer; transition: color 0.1s;
	}
	.card-close:hover { color: #c9d1d9; }

	.card-title {
		margin: 0; padding: 10px 12px 6px;
		font-size: 0.9rem; font-weight: 600; color: #e6edf3;
		line-height: 1.45; word-break: break-word;
	}
	.card-branch {
		display: flex; align-items: center; gap: 8px;
		margin: 0 12px 8px;
		padding: 4px 8px;
		background: #0d1117; border: 1px solid #21262d; border-radius: 5px;
	}
	.blabel { font-size: 0.7rem; color: #484f58; }
	.bname { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 0.78rem; color: #79c0ff; }

	.card-goto {
		margin: 0 12px 10px;
		padding: 6px 10px;
		background: #21262d; border: 1px solid #30363d; border-radius: 6px;
		color: #58a6ff; font-size: 0.78rem; cursor: pointer; text-align: left;
		transition: background 0.1s, border-color 0.1s;
	}
	.card-goto:hover { background: #30363d; border-color: #484f58; }

	.card-divider { height: 1px; background: #21262d; }

	.card-sec-label {
		margin: 10px 12px 4px;
		font-size: 0.67rem; font-weight: 600; color: #484f58;
		text-transform: uppercase; letter-spacing: 0.06em;
	}
	.hl-multi-hint {
		font-size: 0.62rem; color: #30363d;
		text-transform: none; letter-spacing: 0;
		font-weight: 400;
	}

	.card-hl-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 4px;
		padding: 0 12px;
	}
	/* "연관 전체"는 한 줄 전체 */
	.hl-btn.all { grid-column: 1 / -1; }

	.hl-btn {
		padding: 5px 8px;
		border-radius: 5px;
		font-size: 0.75rem; cursor: pointer; text-align: left;
		transition: all 0.12s;
		border: 1px solid transparent;
		background: #0d1117;
	}
	/* 색상 정의 */
	.hl-btn.pre    { color: #a371f7; border-color: rgba(163,113,247,0.25); }
	.hl-btn.sub    { color: #3dc9b0; border-color: rgba(61,201,176,0.25); }
	.hl-btn.next   { color: #f0883e; border-color: rgba(240,136,62,0.25); }
	.hl-btn.parent { color: #7ee787; border-color: rgba(126,231,135,0.25); }
	.hl-btn.all    { color: #c9d1d9; border-color: #30363d; }

	.hl-btn:hover { background: #21262d; }
	.hl-btn.pre.on    { background: rgba(163,113,247,0.15); border-color: #a371f7; }
	.hl-btn.sub.on    { background: rgba(61,201,176,0.15);  border-color: #3dc9b0; }
	.hl-btn.next.on   { background: rgba(240,136,62,0.15);  border-color: #f0883e; }
	.hl-btn.parent.on { background: rgba(126,231,135,0.15); border-color: #7ee787; }
	.hl-btn.all.on    { background: rgba(201,209,217,0.1);  border-color: #8b949e; }

	.hl-actions {
		margin: 6px 12px 0;
		display: flex; gap: 4px;
	}
	.hl-act {
		flex: 1;
		padding: 4px 6px;
		background: #0d1117; border: 1px solid #30363d; border-radius: 5px;
		color: #8b949e; font-size: 0.72rem; cursor: pointer;
		transition: background 0.1s, color 0.1s, border-color 0.1s;
	}
	.hl-act:hover:not(:disabled) { background: #21262d; color: #c9d1d9; }
	.hl-act:disabled { opacity: 0.4; cursor: default; }
	.hl-act.sel { color: #58a6ff; border-color: rgba(88,166,255,0.3); }
	.hl-act.sel:hover:not(:disabled) { background: rgba(88,166,255,0.1); color: #79c0ff; }
	.hl-act.arr { color: #f5a623; border-color: rgba(245,166,35,0.3); }
	.hl-act.arr:hover:not(:disabled) { background: rgba(245,166,35,0.1); color: #ffb84d; }
	.hl-act.clear { color: #484f58; }

	.card-note {
		margin: 6px 12px 10px;
		font-size: 0.67rem; color: #30363d; line-height: 1.4;
	}

	/* ── 툴바 (z:10) ── */
	.toolbar {
		position: absolute; top: 10px; right: 14px;
		z-index: 10; display: flex; align-items: center; gap: 4px;
		pointer-events: auto;
	}
	.tb-btn {
		display: flex; align-items: center; gap: 4px;
		padding: 4px 10px;
		background: #161b22; border: 1px solid #30363d; border-radius: 6px;
		color: #8b949e; font-size: 0.8rem; cursor: pointer;
		transition: background 0.1s, color 0.1s, border-color 0.1s;
	}
	.tb-btn:hover:not(:disabled) { background: #21262d; border-color: #484f58; color: #c9d1d9; }
	.tb-btn:disabled { opacity: 0.35; cursor: default; }
	.tb-btn.tb-on {
		background: rgba(245,166,35,0.12);
		border-color: rgba(245,166,35,0.55);
		color: #f5a623;
	}
	.tb-btn.tb-on:hover:not(:disabled) {
		background: rgba(245,166,35,0.18);
		border-color: #f5a623;
		color: #ffb84d;
	}
	.tb-btn .icon { font-size: 0.95rem; line-height: 1; }
	.tb-btn .count { font-size: 0.7rem; color: #484f58; min-width: 10px; text-align: right; }
	.tb-btn:hover:not(:disabled) .count { color: #8b949e; }
	.tb-sep { width: 1px; background: #30363d; align-self: stretch; margin: 2px 0; }
	.tb-select {
		padding: 3px 6px;
		background: #161b22; border: 1px solid #30363d; border-radius: 6px;
		color: #8b949e; font-size: 0.8rem; cursor: pointer; outline: none;
	}
	.tb-select:hover { border-color: #484f58; color: #c9d1d9; }

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
		padding-left: 4px;
	}

	.overlay {
		position: fixed; inset: 52px 0 0 0;
		display: flex; align-items: center; justify-content: center;
		color: #484f58; font-size: 0.9rem; pointer-events: none; z-index: 2;
	}
	.overlay.error { color: #e94f4f; }

	/* ── 인앱 확인 다이얼로그 ── */
	.dialog-backdrop {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.55);
		z-index: 500;
		display: flex; align-items: center; justify-content: center;
	}
	.dialog {
		background: #161b22;
		border: 1px solid #30363d;
		border-radius: 10px;
		padding: 1.25rem 1.5rem 1rem;
		min-width: 280px; max-width: 420px;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		display: flex; flex-direction: column; gap: 1rem;
	}
	.dialog-msg {
		margin: 0;
		font-size: 0.9rem; color: #c9d1d9; line-height: 1.5;
		white-space: pre-wrap;
	}
	.dialog-btns {
		display: flex; gap: 0.5rem; justify-content: flex-end;
	}
	.dialog-ok {
		padding: 0.4rem 1.1rem;
		background: #238636; border: 1px solid #2ea043; border-radius: 6px;
		color: #fff; font-size: 0.875rem; cursor: pointer;
		transition: background 0.1s;
	}
	.dialog-ok:hover { background: #2ea043; }
	.dialog-cancel {
		padding: 0.4rem 1rem;
		background: transparent; border: 1px solid #30363d; border-radius: 6px;
		color: #8b949e; font-size: 0.875rem; cursor: pointer;
	}
	.dialog-cancel:hover { background: #21262d; }
</style>
