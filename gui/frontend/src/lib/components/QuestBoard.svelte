<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { get } from 'svelte/store';
	// DEV-026: cytoscape 는 ~600KB 청크 — board route 진입 시 동적 import.
	// 타입은 erased 라 정적 import 유지, 런타임은 lazy. 반복 호출 시 Vite 가
	// Promise 캐싱하므로 별도 캐시 불필요.
	import type { Core, NodeSingular, Css, StylesheetJson } from 'cytoscape';
	import { goto } from '$app/navigation';
	import { questsApi } from '$lib/api/quests';
	import { metaApi } from '$lib/api/meta';
	import { detectEnvironment } from '$lib/api/transport';
	// BUG-034: 유효 기한 (퀘스트 required_due vs 연결 캠페인 ended_at) 계산 헬퍼.
	import { effectiveQuestDue } from '$lib/utils/quest-node-svg';
	import { flashQuestId } from '$lib/stores';
	import {
		URGENCY_COLOR,
		urgencyColor,
		urgencyLabel,
		URGENCY_BG,
		URGENCY_LABEL,
		urgencyBgFor,
		type Quest,
		type QuestDependency,
		type QuestPosition,
		type QuestStatus
	} from '$lib/types';

	// DEV-084: New Quest 버튼이 toolbar 로 이동 — 클릭 시 부모 (+page) 의 모달 오픈.
	let { onNewQuest }: { onNewQuest?: () => void } = $props();

	const NODE_W = 284;
	const NODE_H = 80;
	/// 정렬 animate 의 duration (ms). 가드 (`arranging`) 가 이 시간만큼 유지되어
	/// 빠른 더블클릭이 진행 중 animate 중간에 새 animate 를 trigger 하지 않도록.
	const ARRANGE_ANIM_MS = 200;

	// 노드 제목 줄바꿈을 실제 픽셀 폭 기준으로 — Canvas measureText API 사용.
	// SVG 의 font 와 동일한 설정으로 ctx.font 잡고 substring 폭 측정.
	const TITLE_FONT = '12px system-ui, -apple-system, sans-serif';
	let _measureCtx: CanvasRenderingContext2D | null = null;

	function getMeasureCtx(): CanvasRenderingContext2D | null {
		if (_measureCtx) return _measureCtx;
		if (typeof document === 'undefined') return null; // SSR
		const c = document.createElement('canvas');
		const ctx = c.getContext('2d');
		if (!ctx) return null;
		ctx.font = TITLE_FONT;
		_measureCtx = ctx;
		return ctx;
	}

	/**
	 * s 를 maxPx 픽셀 폭 안에 맞게 [head, tail] 로 분할.
	 * binary search 로 가장 긴 prefix 찾음 — O(log n) measureText 호출.
	 * SSR / canvas 미지원 시 폴백 (char 기반 휴리스틱).
	 *
	 * **단어 경계 미고려** — mid-word 에서 자름. 사용자 친화 분할은
	 * `splitByPixelWidthAtWord` 사용.
	 */
	function splitByPixelWidth(s: string, maxPx: number): [string, string] {
		const ctx = getMeasureCtx();
		if (!ctx) {
			// 폴백: ASCII=6.5px, CJK=12px 추정
			const maxUnits = Math.floor(maxPx / 6.5);
			let acc = 0;
			for (let i = 0; i < s.length; i++) {
				const code = s.charCodeAt(i);
				const w =
					(code >= 0x1100 && code <= 0x11ff) ||
					(code >= 0x2e80 && code <= 0x9fff) ||
					(code >= 0xa960 && code <= 0xa97f) ||
					(code >= 0xac00 && code <= 0xd7af) ||
					(code >= 0xf900 && code <= 0xfaff) ||
					(code >= 0xff00 && code <= 0xff60)
						? 2
						: 1;
				if (acc + w > maxUnits) return [s.slice(0, i), s.slice(i)];
				acc += w;
			}
			return [s, ''];
		}
		if (ctx.measureText(s).width <= maxPx) return [s, ''];
		// 가장 긴 prefix 가 maxPx 안에 들어가는지 binary search.
		let lo = 0;
		let hi = s.length;
		while (lo < hi) {
			const mid = (lo + hi + 1) >> 1;
			if (ctx.measureText(s.slice(0, mid)).width <= maxPx) {
				lo = mid;
			} else {
				hi = mid - 1;
			}
		}
		return [s.slice(0, lo), s.slice(lo)];
	}

	/**
	 * 단어 경계 우선 분할 — `splitByPixelWidth` 결과를 받아 마지막 whitespace 까지
	 * 되돌려 word-break 유도. 단일 단어가 maxPx 보다 길면 어쩔 수 없이 mid-word.
	 *
	 * 한글 등 공백 없는 텍스트는 fallback 으로 mid-char 분할 (동작 동일).
	 * 영문 / 혼합 텍스트는 단어 단위로 끊김.
	 */
	function splitByPixelWidthAtWord(s: string, maxPx: number): [string, string] {
		const [hardHead, hardTail] = splitByPixelWidth(s, maxPx);
		if (!hardTail) return [hardHead, '']; // 전체 fit

		// 경계가 이미 공백이면 OK — 공백은 head 끝에 두고 tail 의 leading 공백 제거.
		if (/^\s/.test(hardTail)) {
			return [hardHead.replace(/\s+$/, ''), hardTail.replace(/^\s+/, '')];
		}

		// hardHead 가 단어 중간에서 잘림 — 직전 공백까지 백오프.
		const lastWs = hardHead.search(/\s\S*$/);
		if (lastWs > 0) {
			const head = hardHead.slice(0, lastWs);
			const tail = hardHead.slice(lastWs + 1) + hardTail; // +1: 공백 자체는 버림
			return [head.replace(/\s+$/, ''), tail];
		}

		// hardHead 에 공백이 전혀 없음 — 한글 또는 단어 하나가 너무 김. mid-char 유지.
		return [hardHead, hardTail];
	}

	// BUG-057: HiDPI 노드 — SVG 를 dpr 배 사이즈로 발급 (viewBox 로 좌표계 보존).
	// Cytoscape 가 background-image 를 그 사이즈로 raster cache 하므로 표시 시
	// (cover fit, NODE_W × NODE_H) 다운샘플 → 텍스트 또렷.
	function makeSvgUrl(quest: Quest): string {
		const W = NODE_W, H = NODE_H;
		// devicePixelRatio 한도 — 과한 사이즈는 메모리 낭비. 1.0 / 1.5 / 2.0 / 3.0
		// 같은 일반 값을 그대로 사용. 1 미만이면 1 로.
		const dpr = Math.max(1, Math.min(3, window.devicePixelRatio || 1));
		const Wpx = Math.round(W * dpr);
		const Hpx = Math.round(H * dpr);
		// BUG-060: 유효 범위 밖 데이터에도 안전한 헬퍼 사용 — 이전엔 bare access
		// 로 인해 urgency=5 같은 invalid row 가 들어오면 ul/uc 가 undefined →
		// 아래 .length 에서 폭발 (보드 mount 실패).
		const uc = urgencyColor(quest.urgency);
		const tc = quest.type_color;
		const ul = urgencyLabel(quest.urgency);
		const qid = quest.quest_id;
		// DEV-074 fix: SVG data URL 안에선 CSS var() 컴퓨팅 X — 명시 색.
		// DEV-074 fix20: themePalette 단일 source 사용.
		const palette = themePalette(currentEffectiveTheme());
		const textFill = palette.text;
		const dueMutedFill = palette.textMuted;
		const dangerFill = palette.danger;

		const x = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
		const qidW = Math.ceil(qid.length * 6.4) + 16;
		const ulW  = Math.ceil(ul.length  * 5.6) + 14;
		const ulX  = 10 + qidW + 6;

		// DEV-116: 댓글 개수 badge — 상단 우측. 0 이면 표시 X.
		const cc = quest.comment_count ?? 0;
		const ccText = cc > 0 ? `💬 ${cc}` : '';
		const ccW = ccText ? Math.ceil(ccText.length * 6.0) + 14 : 0;
		const ccX = W - 10 - ccW;
		const ccFill = dueMutedFill;

		// 제목 가용 폭: NODE_W - 좌 padding(10) - 우 minimum margin(14) = 260px.
		// 단어 경계 우선 — 공백 있는 텍스트는 단어 단위로, 한글 / 긴 단어는 mid-char.
		const full = quest.title;
		const MAX_PX = 260;
		const [line1, rest1] = splitByPixelWidthAtWord(full, MAX_PX);
		const [rawL2, rest2] = splitByPixelWidthAtWord(rest1, MAX_PX);
		// rest2 가 남았으면 2줄도 넘침 → ellipsis 자리만큼 줄여 자름.
		// 여긴 어차피 잘림 표시이므로 word-break 강제 안 함 (mid-char OK).
		const line2 = rest2.length > 0
			? splitByPixelWidth(rawL2, MAX_PX - 10)[0] + '…'
			: rawL2;

		// BUG-034: 유효 기한 (= min(required_due, earliest_campaign_due)) 표시.
		// source='campaign' 이면 prefix '⛺' — 캠페인이 더 가까워 그게 우세함을 시각 단서로.
		const { date: due, source: dueSrc } = effectiveQuestDue(quest);
		let dueText = '';
		let dueColor = dueMutedFill;
		if (due) {
			dueText = dueSrc === 'campaign' ? `⛺ ${due}` : due;
			const dueMs = new Date(`${due}T23:59:59`).getTime();
			if (!Number.isNaN(dueMs)) {
				const daysLeft = Math.floor((dueMs - Date.now()) / (24 * 60 * 60 * 1000));
				if (daysLeft < 0) dueColor = dangerFill;
				else if (daysLeft <= 7) dueColor = '#f0883e';
			}
		}
		const titleY = dueText ? (line2 ? 40 : 46) : line2 ? 44 : 52;

		// DEV-081: 좌측 urgency 색 strip 제거 — cytoscape 의 node border (urgencyColor)
		// 만으로 강조 충분. 카드 안쪽 strip 은 중복 시각 노이즈.
		// BUG-057: width/height = px (dpr 배), viewBox = logical (W × H) — 좌표 그대로 사용.
		const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${Wpx}" height="${Hpx}" viewBox="0 0 ${W} ${H}">
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
  ${ccText ? `<text x="${ccX + ccW / 2}" y="21.5" text-anchor="middle"
    fill="${ccFill}" font-size="10" font-weight="500"
    font-family="system-ui,sans-serif">${x(ccText)}</text>` : ''}
  <text x="10" y="${titleY}" fill="${textFill}" font-size="12"
    font-family="system-ui,-apple-system,sans-serif">${x(line1)}</text>
  ${line2 ? `<text x="10" y="${titleY + 16}" fill="${textFill}" font-size="12"
    font-family="system-ui,-apple-system,sans-serif">${x(line2)}</text>` : ''}
  ${dueText
		? `<text x="${W - 10}" y="${H - 8}" text-anchor="end"
       fill="${dueColor}" font-size="10" font-weight="500"
       font-family="system-ui,sans-serif">⏱ ${x(dueText)}</text>`
		: ''}
</svg>`;
		return 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
	}
	const NODE_GAP = 28;
	const LANE_PAD_X = 20; // lane 양쪽 가장자리 여백 (한쪽)
	const LANE_W = NODE_W * 3 + NODE_GAP * 2 + LANE_PAD_X * 2; // 948px
	const LANE_GAP = 36;   // lane 사이 시각적 간격
	const LANE_STRIDE = LANE_W + LANE_GAP; // 한 lane 의 X 단위 (다음 lane 시작점까지)
	// DEV-105: collapsed lane 의 좁은 폭 — 세로 라벨 한 줄 들어갈 정도.
	const LANE_COLLAPSED_W = 40;
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
	// BUG: sorted 가 일반 let — svelte 5 reactive 안 됨 (npm check warning). $state 로.
	let sorted: QuestStatus[] = $state([]);
	let laneOf = new Map<number, number>();

	// DEV-048: status_id (number) → status_slug (string). API 는 slug 전용.
	function slugOf(statusId: number): string {
		return sorted.find((s) => s.id === statusId)?.slug ?? '';
	}

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
		return guildKeyPrefix
			? `openguild.${guildKeyPrefix}.${suffix}`
			: `openguild.${suffix}`;
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
			if (parsed && typeof parsed === 'object')
				return parsed as Record<string, HideSetting>;
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
	function loadViewport(): BoardViewport | null {
		try {
			const raw = localStorage.getItem(gk('boardViewport'));
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
	function scheduleViewportSave() {
		if (viewportSaveTimer) clearTimeout(viewportSaveTimer);
		viewportSaveTimer = setTimeout(() => {
			if (!cy) return;
			try {
				const v: BoardViewport = { pan: cy.pan(), zoom: cy.zoom() };
				localStorage.setItem(gk('boardViewport'), JSON.stringify(v));
			} catch {
				/* 무시 */
			}
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
	function computeGroups(
		quests: Quest[],
		deps: QuestDependency[]
	): Map<number, Set<number>> {
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
	 * 결정된 hidden set 을 cytoscape + lane DIV 에 적용.
	 * cytoscape display: 'none' 노드는 자동으로 연결된 edge 도 안 보임.
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
		if (!lanesEl || !headersEl) return;
		sorted.forEach((s, li) => {
			const setting = getHideSetting(s.slug);
			const col = lanesEl.children[li] as HTMLDivElement | undefined;
			const hdr = headersEl.children[li] as HTMLDivElement | undefined;
			if (col) col.style.display = setting.laneHidden ? 'none' : '';
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

	// ── DEV-067: lane 시각 압축 (laneHidden 인 lane 자리 회수) ────
	//
	// 두 좌표계:
	// - **absolute X** = DB positions.x. laneOf 기반 absolute lane left + offset.
	// - **visual X**   = cytoscape position.x. visible lane left + offset.
	//
	// hideSettings 없을 땐 둘이 같음. laneHidden lane 이 생기면 visible 압축
	// 으로 그 lane 뒤의 노드 X 가 STRIDE 만큼 왼쪽으로 시프트.
	//
	// 변환:
	//   visualX = absX - absoluteLaneLeftOfStatus(sid) + visibleLaneLeftOfStatus(sid)
	//   absX    = visualX + absoluteLaneLeftOfStatus(sid) - visibleLaneLeftOfStatus(sid)
	//
	// 모든 사이트 (init / drag dragfree / arrange / DB save / snapToGrid /
	// flashToQuest 의 새 노드 추가) 가 이 변환 사용.

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

	function absToVisualX(absX: number, statusId: number): number {
		return absX - absoluteLaneLeftOfStatus(statusId) + visibleLaneLeftOfStatus(statusId);
	}

	function visualToAbsX(visualX: number, statusId: number): number {
		return visualX + absoluteLaneLeftOfStatus(statusId) - visibleLaneLeftOfStatus(statusId);
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
	function visibleLaneIdxAtVisualX(visualX: number): number {
		let left = 0;
		let visIdx = 0;
		for (const s of sorted) {
			if (getHideSetting(s.slug).laneHidden) continue;
			const stride = laneStride(s.slug);
			if (visualX < left + stride) return visIdx;
			left += stride;
			visIdx++;
		}
		return Math.max(0, visIdx - 1);
	}

	/** 모든 노드의 visual position 을 현재 hide settings 기준으로 재계산.
	 * 노드 data.absX 가 진리원 (DB 와 같은 좌표계). */
	function applyLaneVisualCompression() {
		const c = cy;
		if (!c) return;
		c.batch(() => {
			c.nodes('[questId]').forEach((n) => {
				const sid = n.data('statusId') as number;
				const absX = (n.data('absX') as number | undefined) ?? n.position().x;
				const newX = absToVisualX(absX, sid);
				const p = n.position();
				if (newX !== p.x) n.position({ x: newX, y: p.y });
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
	import { theme, resolveTheme, themePalette } from '$lib/stores/theme';
	function currentEffectiveTheme(): 'dark' | 'light' {
		return resolveTheme(getStore(theme));
	}
	// svelte/store get 임포트 alias.
	import { get as getStore } from 'svelte/store';

	// DEV-105 (partial): lane 접기 — collapsed 시 그 lane 의 노드 hide + label
	// 90도 회전. lane 폭 자체 축소는 미지원 (LANE_W 상수 retrofit 필요 — 별도).
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
				// 모든 노드의 visualX 재계산 — collapsed 변경이 lane left 누적에 영향.
				const absX = (n.data('absX') as number | undefined) ?? n.position().x;
				const visX = absToVisualX(absX, sid);
				n.animate({ position: { x: visX, y: n.position().y }, duration: 150 });
			});
			syncLanes();
		}
	}

	// DEV-059: 사용자 정의 lane 순서 — '보여지는 순서' 만. 파일 / DB / 다른 quest
	// 영향 X. status 추가/삭제는 sort_order 따라 자동 끝에 append (loadFromData
	// 의 ordered + remaining 패턴).
	function loadLaneOrder(): string[] {
		try {
			const raw = localStorage.getItem(gk('laneOrder'));
			if (!raw) return [];
			const arr = JSON.parse(raw);
			return Array.isArray(arr) ? arr.filter((s) => typeof s === 'string') : [];
		} catch {
			return [];
		}
	}
	function saveLaneOrder(slugs: string[]) {
		try {
			localStorage.setItem(gk('laneOrder'), JSON.stringify(slugs));
		} catch {
			/* 무시 */
		}
	}
	// li (lane index) 의 lane 을 한 칸 좌/우 swap. 모든 노드를 새 lane 좌표로
	// 다시 그려야 하므로 cy reload.
	function swapLane(li: number, dir: -1 | 1) {
		const target = li + dir;
		if (target < 0 || target >= sorted.length) return;
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
			// 모든 노드 의 lane 의 새 좌표로 animate (즉시 jump 가 아니라 부드러운 이동 — 사용자가 이동 인지).
			cy.nodes('[questId]').forEach((n) => {
				const sid = n.data('statusId') as number;
				const newLi = laneOf.get(sid) ?? 0;
				const absX = (n.data('absX') as number) ?? n.position().x;
				const oldX = n.position().x;
				// absX 의 lane-local X 를 새 lane 의 left 로.
				const oldLaneLeft = Math.floor(oldX / LANE_STRIDE) * LANE_STRIDE;
				const localX = oldX - oldLaneLeft;
				const newVisX = newLi * LANE_STRIDE + localX;
				n.animate({ position: { x: newVisX, y: n.position().y }, duration: 200 });
				n.data('absX', newLi * LANE_STRIDE + (absX - Math.floor(absX / LANE_STRIDE) * LANE_STRIDE));
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
		// DEV-067: input x 는 visual. visible lane idx → status → absolute lane idx.
		// DEV-105 fix10: 가변 폭 collapsed lane 인식 — 균등 LANE_STRIDE 가정 제거.
		const visCount = Math.max(1, visibleLaneCount());
		const visIdx = Math.max(0, Math.min(visCount - 1, visibleLaneIdxAtVisualX(x)));
		const statusId = statusIdAtVisibleIdx(visIdx);
		const li = statusId !== null ? (laneOf.get(statusId) ?? 0) : 0;
		const cols = laneCols[li] ?? 2;
		// laneFirstCellX(li, cols) 는 absolute X. visible 압축 적용해서 visual X 로 변환.
		const firstX =
			statusId !== null ? absToVisualX(laneFirstCellX(li, cols), statusId) : laneFirstCellX(li, cols);
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
	// 자동 정렬 진행 중 — 정렬 버튼의 disabled 반응성 위해 $state 필요 (BUG-006).
	let arranging = $state(false);
	let ctrlHeld = false;
	let ctrlClickNode: NodeSingular | null = null;
	let boxDragEl: HTMLDivElement | null = null;
	let boxDragStart: { x: number; y: number } | null = null;

	// 드래그 시작 상태 (노드별 Map)
	const dragStartMap = new Map<number, { x: number; y: number; statusId: number }>();
	// DEV-105 fix11: 드래그 중인 노드가 놓일 예정인 lane 의 slug — UI 하이라이트용.
	let dragHighlightSlug = $state<string | null>(null);
	// DEV-115: 최근 움직인 노드를 위로 — 단조 증가 카운터. 기본 z-index 는 10 (cy.style()).
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
		// 확장 카드 동기화 — DEV-046 후속: status_slug 도 함께 갱신 (일관성).
		if (expandedQuest?.id === questId) {
			expandedQuest = { ...expandedQuest, status_id: s.id, status_slug: s.slug, status_name_en: s.name_en, status_name_ko: s.name_ko, status_color: s.color };
		}
		// allQuests 캐시 동기화 (tap으로 확장 시 최신 상태 반영)
		const idx = allQuests.findIndex((q) => q.id === questId);
		if (idx !== -1) {
			allQuests[idx] = { ...allQuests[idx], status_id: s.id, status_slug: s.slug, status_name_en: s.name_en, status_name_ko: s.name_ko, status_color: s.color };
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
			const node = cy.getElementById(`q-${record.questId}`) as NodeSingular;
			if (node.length === 0) { busy = false; return; }
			if (record.from.statusId !== record.to.statusId) {
				try {
					await questsApi.changeStatus(record.questId, { status_slug: slugOf(target.statusId) });
					node.data('statusId', target.statusId);
					applyStatusChange(record.questId, target.statusId);
				} catch { busy = false; return; }
			}
			node.animate({ position: { x: target.x, y: target.y }, duration: 120 });
			// DEV-067: record.to.x 는 visual. DB 는 absolute.
			const absX = visualToAbsX(target.x, target.statusId);
			node.data('absX', absX);
			questsApi.updatePosition(record.questId, { x: absX, y: target.y }).catch(() => {});
			// DEV-115: undo/redo 로 움직인 노드도 위로.
			recentMoveZ += 1;
			node.style('z-index', recentMoveZ);
		} else {
			const promises: Promise<unknown>[] = [];
			for (const item of record.items) {
				const node = cy!.getElementById(`q-${item.questId}`) as NodeSingular;
				if (node.length === 0) continue;
				const target = direction === 'undo' ? item.from : item.to;
				if (item.from.statusId !== item.to.statusId) {
					try {
						await questsApi.changeStatus(item.questId, { status_slug: slugOf(target.statusId) });
						node.data('statusId', target.statusId);
						applyStatusChange(item.questId, target.statusId);
					} catch { continue; }
				}
				node.animate({ position: { x: target.x, y: target.y }, duration: 200 });
				// DEV-067: target.x 는 visual. DB 는 absolute.
				const absX = visualToAbsX(target.x, target.statusId);
				node.data('absX', absX);
				promises.push(questsApi.updatePosition(item.questId, { x: absX, y: target.y }).catch(() => {}));
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
		// DEV-074 fix20: themePalette 사용 — 이전엔 #4a90d9 hardcoded (다크 전용).
		const ePal = themePalette(currentEffectiveTheme());
		boxDragEl = document.createElement('div');
		boxDragEl.style.cssText =
			`position:absolute;left:${sx}px;top:${sy}px;width:0;height:0;` +
			`border:1.5px dashed ${ePal.edgePre};` +
			`background:color-mix(in srgb, ${ePal.edgePre} 10%, transparent);` +
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
	//   parent = 이 퀘스트의 부모 퀘스트              → 초록 var(--success)

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
		// DEV-056 fix1: hidden 노드 제외 — 정렬 시 자리 안 차지하도록.
		nodesToArrange = nodesToArrange.filter((n) => n.style('display') !== 'none');
		// 빈 배열 시 no-op — 빈 lane 의 정렬 버튼이 전체 정렬을 trigger 하지 않도록.
		// 전체 정렬을 원하면 호출자가 명시적으로 모든 노드 전달 (toolbar 의 전체 정렬 버튼처럼).
		if (nodesToArrange.length === 0) return;
		arranging = true;
		try {
			const cellW = NODE_W + NODE_GAP;
			const cellH = NODE_H + NODE_GAP;
			const baseY = LANE_TOP + 16 + NODE_H / 2;

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
				// DEV-067: absolute → visual 변환.
				const absX = firstX + col * cellW;
				const visX = absToVisualX(absX, sid);
				const y = baseY + row * cellH;
				const fromPos = { ...node.position() };
				if (Math.abs(fromPos.x - visX) < 0.5 && Math.abs(fromPos.y - y) < 0.5) return;
				node.data('absX', absX);
				batchItems.push({
					questId: qid,
					from: { ...fromPos, statusId: sid },
					to: { x: visX, y, statusId: sid }
				});
				node.animate({ position: { x: visX, y }, duration: 200 });
				savePromises.push(questsApi.updatePosition(qid, { x: absX, y }).catch(() => {}));
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
			// DEV-056 fix1: hidden 노드 제외 — 정렬 시 자리 안 차지하도록.
			const nodes = cy.nodes(`[statusId = ${statusId}]`).filter(
				(n) => (n as NodeSingular).style('display') !== 'none'
			);
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
				// DEV-067: x = firstX(absolute) → visual 변환. DB save 는 absolute.
				const absX = firstX + col * cellW;
				const sid = node.data('statusId') as number;
				const visX = absToVisualX(absX, sid);
				const y = startY + row * cellH;
				node.data('absX', absX);
				batchItems.push({ questId: node.data('questId') as number, from: { ...fromPos, statusId: sid }, to: { x: visX, y, statusId: sid } });
				node.animate({ position: { x: visX, y }, duration: 200 });
				savePromises.push(questsApi.updatePosition(node.data('questId') as number, { x: absX, y }).catch(() => {}));
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

	// DEV-074 fix3: Cytoscape style 의 색 값 — theme 별 hex 직접 명시.
	// `var(--bg)` 같은 CSS 변수는 Cytoscape style 시스템이 컴퓨팅 못 함 (DEV-074
	// 코멘트 참조) → 모든 색을 명시 hex 로 지정 + theme 변경 시 cy.style() 교체.
	function buildCyStyle(eff: 'dark' | 'light'): StylesheetJson {
		// DEV-074 fix20: themePalette 단일 source. 이전엔 컴포넌트별 inline 분기.
		const p = themePalette(eff);
		const bg = p.bg;
		const accent = p.accent;
		const success = p.success;
		const textFaint = p.textFaint;
		const preBg = p.hlPreBg;
		const preBorder = p.hlPre;
		const subBg = p.hlSubBg;
		const subBorder = p.hlSub;
		const nextBg = p.hlNextBg;
		const nextBorder = p.hlNext;
		const parentBg = p.hlParentBg;
		const selectedBg = p.selectedBg;
		const flashBorder = p.accentSecondary;
		const edgePre = p.edgePre;
		return [
			{
				selector: 'node[questId]',
				style: {
					'background-color': bg,
					// DEV-112: 노드 위치가 자유로워 겹칠 수 있음 — 뒤 노드 윤곽이
					// 비치도록 살짝 투명. border (urgency 색) 는 fully opaque 유지.
					'background-opacity': 0.92,
					'background-image': 'data(nodeBg)',
					'background-fit': 'cover',
					'background-image-opacity': 0.88,
					'border-color': 'data(urgencyColor)',
					'border-width': 2,
					width: NODE_W, height: NODE_H,
					shape: 'round-rectangle',
					label: '',
					'z-index': 10
				}
			},
			{ selector: 'node[questId]:active', style: { 'overlay-opacity': 0 } },
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
				} as Css.Node
			},
			{
				selector: 'node[questId][highlightType = "pre"]',
				style: { 'background-color': preBg, 'border-color': preBorder, 'border-width': 3, 'shadow-blur': 12, 'shadow-color': preBorder, 'shadow-opacity': 0.65, 'shadow-offset-x': 0, 'shadow-offset-y': 0 } as Css.Node
			},
			{
				selector: 'node[questId][highlightType = "sub"]',
				style: { 'background-color': subBg, 'border-color': subBorder, 'border-width': 3, 'shadow-blur': 12, 'shadow-color': subBorder, 'shadow-opacity': 0.65, 'shadow-offset-x': 0, 'shadow-offset-y': 0 } as Css.Node
			},
			{
				selector: 'node[questId][highlightType = "next"]',
				style: { 'background-color': nextBg, 'border-color': nextBorder, 'border-width': 3, 'shadow-blur': 12, 'shadow-color': nextBorder, 'shadow-opacity': 0.65, 'shadow-offset-x': 0, 'shadow-offset-y': 0 } as Css.Node
			},
			{
				selector: 'node[questId][highlightType = "parent"]',
				style: { 'background-color': parentBg, 'border-color': success, 'border-width': 3, 'shadow-blur': 12, 'shadow-color': success, 'shadow-opacity': 0.65, 'shadow-offset-x': 0, 'shadow-offset-y': 0 } as Css.Node
			},
			{
				selector: 'node[questId][highlightType = "dim"]',
				style: { opacity: 0.15 } as Css.Node
			},
			{
				selector: 'node[questId]:selected',
				style: {
					'background-color': selectedBg,
					'border-color': accent,
					'border-width': 3,
					'shadow-blur': 14,
					'shadow-color': accent,
					'shadow-opacity': 0.65,
					'shadow-offset-x': 0,
					'shadow-offset-y': 0
				} as Css.Node
			},
			{
				selector: 'node[questId][?flash]',
				style: {
					'border-color': flashBorder,
					'border-width': 5,
					'shadow-blur': 28,
					'shadow-color': flashBorder,
					'shadow-opacity': 1,
					'shadow-offset-x': 0,
					'shadow-offset-y': 0
				} as Css.Node
			},
			{ selector: 'edge[?dimmed]', style: { opacity: 0.07 } },
			{
				selector: 'edge[etype = "pre"]',
				style: { 'line-color': edgePre, 'target-arrow-color': edgePre, 'target-arrow-shape': 'triangle', 'line-style': 'solid', 'curve-style': 'bezier', width: 2 }
			},
			{
				selector: 'edge[etype = "sub"]',
				style: { 'line-color': textFaint, 'target-arrow-color': textFaint, 'target-arrow-shape': 'vee', 'line-style': 'dashed', 'line-dash-pattern': [6, 3], 'curve-style': 'bezier', width: 1.5 }
			}
		];
	}

	// DEV-074 fix: theme 변경 시 모든 노드의 urgencyBg + nodeBg (SVG) 재생성.
	// DEV-074 fix3: cy.style() 전체 교체 — Cytoscape 자체 색 값도 theme 반영.
	function refreshNodeBgForTheme() {
		if (!cy) return;
		const eff = currentEffectiveTheme();
		cy.nodes('[questId]').forEach((n) => {
			const urgency = n.data('urgency') as number | undefined;
			if (urgency != null) {
				n.data('urgencyBg', urgencyBgFor(urgency, eff));
			}
			// nodeBg SVG 도 theme 색 hardcoded — 다시 생성. quest 데이터 추출.
			const qid = n.data('questId') as number | undefined;
			if (qid != null) {
				const q = allQuests.find((x) => x.id === qid);
				if (q) n.data('nodeBg', makeSvgUrl(q));
			}
		});
		// stylesheet 자체 교체 — base / highlight / selected 의 hardcoded hex 까지 반영.
		cy.style().fromJson(buildCyStyle(eff)).update();
		// DEV-074 fix20: grid snap SVG 의 dot 색도 palette 의존이라 theme 변경 시
		// 캐시 무효화 → 다음 syncLanes 가 새 색으로 재생성.
		gridBgCache.clear();
		syncLanes();
	}

	onMount(() => {
		const unsubTheme = theme.subscribe(() => refreshNodeBgForTheme());

		// gridSnap 은 guildKeyPrefix 가 두 번째 onMount 에서 set 된 직후 다시
		// loadGridSnap 호출. 여기서는 listener 만.
		window.addEventListener('keydown', handleKeydown);
		window.addEventListener('keyup', handleKeyup);
		window.addEventListener('blur', onCtrlUp);
		window.addEventListener('mousemove', onBoxMouseMove);
		window.addEventListener('mouseup', onBoxMouseUp);
		container.addEventListener('mousedown', onBoxMouseDown, { capture: true });
		return () => {
			unsubTheme();
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
			hideSettings = loadHideSettings();
			globalCols = loadGlobalCols();
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
		const [quests, statuses, positions, dependencies] = await Promise.all([
			questsApi.list(),
			metaApi.getQuestStatuses(),
			questsApi.listPositions(),
			questsApi.listDependencies()
		]);
		await init(quests, statuses, positions, dependencies);
		// init 직후 store 에 flash id 가 이미 있으면 즉시 처리
		//  (Nav 의 New Quest 모달 → goto → 보드 페이지 도착 흐름)
		const pending = get(flashQuestId);
		if (pending) handleFlash(pending);
	}

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
				// DEV-067: absX = absolute, visualX = absolute → visual 변환.
				const absX = li * LANE_STRIDE + LANE_W / 2;
				const visX = absToVisualX(absX, quest.status_id);
				const y = maxY + NODE_H + NODE_GAP;

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
						nodeBg: makeSvgUrl(quest),
						highlightType: '',
						active: false,
						absX
					},
					position: { x: visX, y }
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
				questsApi.updatePosition(qid, { x: absX, y }).catch(() => {});
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
			// DEV-105 fix13: grid snap dot 표시를 lane-col 의 background 에서 자식
			// `.lane-dots` 의 transform 으로 분리. background-position 변경은 매
			// frame paint → 빠른 pan 추적 안 됨. transform 은 GPU composite.
			const dots = document.createElement('div');
			dots.className = 'lane-dots';
			col.appendChild(dots);
			lanesEl.appendChild(col);
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
			label.textContent = s.name_en;
			label.style.color = s.color;
			// DEV-105: 클릭으로 collapse 토글. label 이 button — keyboard / 접근성 OK.
			label.type = 'button';
			label.title = '레인 접기/펼치기';
			if (collapsedLanes.has(s.slug)) label.classList.add('collapsed');
			label.onclick = () => {
				toggleLaneCollapsed(s.slug);
				const on = collapsedLanes.has(s.slug);
				label.classList.toggle('collapsed', on);
				hdr.classList.toggle('collapsed', on);
			};
			const sel = document.createElement('select');
			sel.className = 'lane-cols-sel';
			sel.title = '이 레인 정렬 열 수';
			const initialCols = laneCols[li];
			[1, 2, 3].forEach((n) => {
				const opt = document.createElement('option');
				opt.value = String(n);
				opt.textContent = `${n}열`;
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

			// DEV-105 fix5: 레인별 설정 (cols-sel + arrange-group) 토글 ⚙.
			// 자주 안 쓰는데 영역만 차지하므로 기본 접힘 — 사용자가 펼침.
			const settingsBtn = document.createElement('button');
			settingsBtn.className = 'lane-settings-btn';
			settingsBtn.type = 'button';
			settingsBtn.textContent = '⚙';
			const setOpenAttrs = () => {
				const open = lanesSettingsOpen.has(s.slug);
				settingsBtn.title = open ? '레인 설정 접기' : '레인 설정 펼치기';
				settingsBtn.setAttribute('aria-expanded', String(open));
				hdr.classList.toggle('settings-open', open);
			};
			setOpenAttrs();
			settingsBtn.onclick = () => {
				toggleLaneSettings(s.slug);
				setOpenAttrs();
			};

			// DEV-059 fix2: lane 순서 변경은 '보드 설정' 모달로 이전 — 헤더에 ◀ ▶ 안 둠.
			// 헤더 폭이 좁아질 때 라벨이 가려지는 문제 회피.
			hdr.appendChild(label);
			hdr.appendChild(settingsBtn);
			hdr.appendChild(sel); // cols select 는 별개 (그리드만 갱신)
			hdr.appendChild(arrangeWrap);
			headersEl.appendChild(hdr);
		});
	}

	// DEV-105 fix12: grid snap SVG 의 zoom/cols 별 캐시 — pan 도중엔 재생성 안 함.
	// 이전엔 매 pan 이벤트마다 lane 마다 encodeURIComponent + setBackgroundImage 발생 →
	// snap dot 표시가 빠른 pan 을 못 따라옴. 캐시는 (laneIdx → {zoom, cols, dataUri, svgW, svgH, bgX}).
	const gridBgCache = new Map<
		number,
		{ zoom: number; cols: number; dataUri: string; svgW: number; svgH: number; bgX: number }
	>();

	function syncLanes() {
		if (!cy) return;
		const pan = cy.pan(), zoom = cy.zoom();

		// 그리드 스냅 시각화: lane-col 의 background 으로 dot 패턴.
		// 가로 dot 수가 정확히 lane 의 cols 와 일치하도록, cols 개 dot 가 들어간 SVG 를 한 row 로 두고
		// 세로 방향만 repeat. lane 마다 cols 가 다를 수 있어 lane 별로 SVG 합성.
		const cellHPx = (NODE_H + NODE_GAP) * zoom;
		const dotR = Math.max(1, 1.5 * zoom);

		// DEV-067: laneHidden 인 lane 의 col 은 display:none + 위치 skip.
		// DEV-105: collapsed lane 은 좁은 폭으로 표시.
		let curLeft = 0;
		lanesEl.querySelectorAll<HTMLElement>('.lane-col').forEach((col, i) => {
			const s = sorted[i];
			const laneHidden = s ? getHideSetting(s.slug).laneHidden : false;
			if (laneHidden) {
				col.style.display = 'none';
				return;
			}
			const w = s ? laneWidth(s.slug) : LANE_W;
			col.style.display = '';
			col.style.left = `${curLeft * zoom + pan.x}px`;
			col.style.width = `${w * zoom}px`;
			curLeft += w + LANE_GAP;
			const dotsEl = col.firstElementChild as HTMLElement | null;
			if (gridSnap && dotsEl) {
				const cols = laneCols[i] ?? 2;
				const cellWPx = (NODE_W + NODE_GAP) * zoom;
				const cached = gridBgCache.get(i);
				let entry: typeof cached;
				if (cached && cached.zoom === zoom && cached.cols === cols) {
					entry = cached;
				} else {
					// zoom 또는 cols 변경 — SVG 재생성. 첫 dot center (lane-col local X) =
					// laneFirstCellX - i*LANE_STRIDE (보드→local 변환 후 zoom).
					const firstCxLocal = (laneFirstCellX(i, cols) - i * LANE_STRIDE) * zoom;
					const svgW = cellWPx * cols;
					const svgH = cellHPx;
					// DEV-074 fix20: dot 색은 palette.warning. 이전엔 rgba(245,166,35,...)
					// 다크 전용. 캐시는 zoom/cols 외에 theme 변경 시 buildCyStyle 가
					// gridBgCache.clear() 호출 (아래 cy.style 갱신 직후) — 즉시 다시 그림.
					const palette = themePalette(currentEffectiveTheme());
					const dotFill = `color-mix(in srgb, ${palette.warning} 55%, transparent)`;
					const dots = Array.from({ length: cols }, (_, c) => {
						const cx = c * cellWPx + cellWPx / 2;
						const cy = cellHPx / 2;
						return `<circle cx="${cx}" cy="${cy}" r="${dotR}" fill="${dotFill}"/>`;
					}).join('');
					const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='${svgW}' height='${svgH}'>${dots}</svg>`;
					const dataUri = `url("data:image/svg+xml;utf8,${encodeURIComponent(svg)}")`;
					const bgX = firstCxLocal - cellWPx / 2;
					entry = { zoom, cols, dataUri, svgW, svgH, bgX };
					gridBgCache.set(i, entry);
					dotsEl.style.display = '';
					dotsEl.style.backgroundImage = dataUri;
					dotsEl.style.backgroundSize = `${svgW}px ${svgH}px`;
					dotsEl.style.backgroundRepeat = 'repeat-y';
					// 가로 위치 = bgX 는 zoom 만 의존 → 한 번만 설정.
					dotsEl.style.left = `${bgX}px`;
					dotsEl.style.width = `${svgW}px`;
					// DEV-105 fix14: dotsEl 을 cellH 만큼 위로 빼서 wrappedBgY ∈ [0, cellH)
					// 만으로 정확한 정렬 가능 (pattern 의 cellH 주기성 활용). 이전 fix13 의
					// top: -200vh 는 200vh 가 cellH 의 정수배가 아니라 fractional 잔여만큼
					// dot 위치 어긋남 — 노드 snap 위치와 불일치 원인.
					dotsEl.style.top = `${-cellHPx}px`;
					dotsEl.style.bottom = '0';
				}
				// pan 매 프레임 — transform: translateY 만 변경 (composite-only, no paint).
				// bgY 를 cellHPx 로 modulo → wrappedBgY ∈ [0, cellH). pattern 주기성
				// (repeat-y 매 cellH) 으로 정수 cellH 시프트는 시각 동일.
				const localCyPx = (LANE_TOP + 16 + NODE_H / 2) * zoom + pan.y;
				const bgY = localCyPx - cellHPx / 2;
				const wrappedBgY = ((bgY % cellHPx) + cellHPx) % cellHPx;
				dotsEl.style.transform = `translateY(${wrappedBgY}px)`;
			} else if (dotsEl) {
				dotsEl.style.display = 'none';
				gridBgCache.delete(i);
			}
		});
		// DEV-067: header 도 visible 압축. DEV-105: collapsed lane 폭 적용.
		// DEV-105 fix3: 사용자 피드백 — 보드 확대 시 lane 제목 / 제목바도 같이
		// 커지는 게 부자연스러움. 폭 / 좌표만 board 좌표계 반영 (가로 정렬 OK),
		// 높이 / 글자 / 컨트롤 크기는 zoom 무관 (UI overlay).
		let hdrLeft = 0;
		headersEl.querySelectorAll<HTMLElement>('.lane-hdr').forEach((hdr, i) => {
			const s = sorted[i];
			const laneHidden = s ? getHideSetting(s.slug).laneHidden : false;
			if (laneHidden) {
				hdr.style.display = 'none';
				return;
			}
			const w = s ? laneWidth(s.slug) : LANE_W;
			hdr.style.display = '';
			hdr.style.left = `${hdrLeft * zoom + pan.x}px`;
			hdr.style.width = `${w * zoom}px`;
			// 높이 / 글자 크기 inline 설정 제거 — CSS 의 고정값 사용.
			hdrLeft += w + LANE_GAP;
		});
		syncExpandedPos();
	}

	// ── Cytoscape 초기화 ────────────────────────────────────────

	async function init(
		quests: Quest[],
		statuses: QuestStatus[],
		positions: QuestPosition[],
		dependencies: QuestDependency[]
	) {
		// DEV-026: cytoscape 동적 import — board 진입 시점에 fetch.
		const { default: cytoscape } = await import('cytoscape');
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
			// DEV-067: pos.x 는 absolute (DB positions.x). data.absX 진리원으로
			// 저장. cytoscape position 은 visual 좌표 — applyLaneVisualCompression
			// 이 init 끝에서 일관 변환.
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
					nodeBg: makeSvgUrl(q),
					highlightType: '',
					active: false,
					absX: pos.x
				},
				position: pos // 초기엔 absolute 그대로. applyLaneVisualCompression 이 visual 변환.
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
			style: buildCyStyle(currentEffectiveTheme()),
			layout: { name: 'preset' },
			minZoom: 0.25,
			maxZoom: 2,
			// wheel zoom 속도 — 기본 1 이 너무 느림 (사용자 피드백).
			// cytoscape 권장 범위 [1, ~3]. 2.5 면 한 번 휠 클릭에 체감 ~2x 빠름.
			wheelSensitivity: 2.5,
			boxSelectionEnabled: false,
			// BUG-057: HiDPI 캔버스. 기본 'auto' 가 WebView2 에서 1 로 떨어지는
			// 사례 있어 명시. 노드 SVG 도 dpr 배 사이즈로 발급 (makeSvgUrl) →
			// 보더 / 그림자 / 텍스트 모두 또렷.
			pixelRatio: Math.max(1, Math.min(3, window.devicePixelRatio || 1))
		});

		cy.on('pan zoom', () => {
			syncLanes();
			scheduleViewportSave(); // DEV-058
		});

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

		// DEV-105 fix11: 드래그 중인 노드의 현재 visual 위치를 기반으로 놓일 lane
		// 미리보기 — slug 설정 → CSS 가 `.lane-col.drag-target` 으로 강조.
		cy.on('drag', 'node[questId]', (e) => {
			if (dragStartMap.size === 0) return;
			const n = e.target as NodeSingular;
			const pos = n.position();
			const visIdx = visibleLaneIdxAtVisualX(pos.x);
			const sid = statusIdAtVisibleIdx(visIdx);
			if (sid === null) return;
			const s = sorted.find((x) => x.id === sid);
			const slug = s?.slug ?? null;
			if (slug !== dragHighlightSlug) dragHighlightSlug = slug;
		});

		cy.on('dragfree', 'node[questId]', () => {
			// dragStartMap은 단일 source of truth.
			// 첫 dragfree 이벤트 때 모든 항목을 일괄 처리하고 비운다.
			// 이후 co-dragged 노드의 dragfree 이벤트가 추가로 와도 size===0이라 무시.
			if (dragStartMap.size === 0) return;
			// DEV-105 fix11: 드래그 끝 — 하이라이트 해제.
			dragHighlightSlug = null;

			for (const [qid, fromState] of dragStartMap) {
				const n = cy!.getElementById(`q-${qid}`) as NodeSingular;
				if (n.length === 0) continue;
				const pos = n.position();
				// DEV-067: visual idx → status_id → absolute lane idx.
				// pos.x 는 visual 좌표 — visible lane 기준 idx 가 사용자 시각의 lane.
				// DEV-105 fix10: 가변 폭 collapsed lane 인식.
				const visMax = Math.max(0, visibleLaneCount() - 1);
				const visIdx = Math.max(0, Math.min(visibleLaneIdxAtVisualX(pos.x), visMax));
				const targetStatusId = statusIdAtVisibleIdx(visIdx) ?? fromState.statusId;
				const li = laneOf.get(targetStatusId) ?? 0;
				pendingDragBatch.push({
					node: n, questId: qid,
					fromPos: { x: fromState.x, y: fromState.y },
					fromStatus: fromState.statusId,
					toPos: { ...pos },
					toLaneIdx: li
				});
				// DEV-115: 방금 움직인 노드 위로. 단조 증가 z-index 로 최근성 보존.
				recentMoveZ += 1;
				n.style('z-index', recentMoveZ);
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
						await questsApi.changeStatus(questId, { status_slug: newStatus.slug });
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
				// DEV-067: clamp 는 visual 좌표 기준 (cytoscape position 이 visual).
				// laneLeft = visibleLaneLeftOfStatus(targetStatusId).
				const finalStatusId = laneChanged && confirmedLanes.has(toLaneIdx) ? newStatus.id : fromStatus;
				const laneLeftVis = visibleLaneLeftOfStatus(finalStatusId);
				const minX = laneLeftVis + LANE_PAD_X + NODE_W / 2;
				const maxX = laneLeftVis + LANE_W - LANE_PAD_X - NODE_W / 2;
				const clampedX = Math.max(minX, Math.min(maxX, snappedX));
				const finalY = snappedY;
				if (clampedX !== toPos.x || finalY !== toPos.y) {
					node.position({ x: clampedX, y: finalY });
				}

				const moved = fromPos.x !== clampedX || fromPos.y !== finalY || fromStatus !== finalStatusId;
				if (moved) {
					// DB save 는 absolute X. node.data.absX 도 동기화.
					const absX = visualToAbsX(clampedX, finalStatusId);
					node.data('absX', absX);
					historyItems.push({
						questId,
						from: { x: fromPos.x, y: fromPos.y, statusId: fromStatus },
						to: { x: clampedX, y: finalY, statusId: finalStatusId }
					});
					posUpdates.push(questsApi.updatePosition(questId, { x: absX, y: finalY }).catch(() => {}));
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

		// DEV-058: 저장된 viewport 가 있으면 복원, 없으면 fit().
		const savedViewport = loadViewport();
		if (savedViewport) {
			cy.viewport({ pan: savedViewport.pan, zoom: savedViewport.zoom });
		} else {
			cy.fit(undefined, 60);
		}

		// DEV-056: hide settings 적용. computeGroups → applyHideSettings.
		// DEV-105 fix8/9: applyHideSettings 가 이제 collapsedLanes 도 인식 — 별도
		// 코드 불필요.
		groupOf = computeGroups(allQuests, allDependencies);
		applyHideSettings();
		// DEV-067: visible lane 압축 (laneHidden 자리 회수). 노드 visual 좌표
		// 일관 재계산. syncLanes 도 visible 압축 반영.
		applyLaneVisualCompression();
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
				<span class="badge" style:--c={urgencyColor(expandedQuest.urgency)}>{urgencyLabel(expandedQuest.urgency)}</span>
				<span class="badge" style:--c={expandedQuest.status_color}>{expandedQuest.status_name_en}</span>
			</div>
			<button class="card-close" onclick={closeExpanded} title="닫기 (Esc)">×</button>
		</div>

		<p class="card-title">{expandedQuest.title}</p>

		<div class="card-branch">
			<span class="blabel">Branch</span>
			<code class="bname">{expandedQuest.type_prefix}-{String(expandedQuest.number).padStart(3, '0')}</code>
		</div>

		<button class="card-goto" onclick={() => goto(`/quests/${expandedQuest!.quest_id}?from=board`)}>
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

	<!-- DEV-073 fix3: New Quest 는 상단 우측 고정 (항상 노출), 나머지 도구바는
	     그 아래로 내림 (사용자 피드백). 접기 토글로 도구만 숨길 수 있음. -->
	{#if onNewQuest}
		<div class="tb-newquest-wrap">
			<button class="tb-btn tb-new" onclick={onNewQuest} title="새 퀘스트">
				<span class="icon">+</span><span>New Quest</span>
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
			<!-- DEV-056 → DEV-059 fix2: 숨김 + 순서 변경 통합 → '보드 설정'. -->
			<button
				class="tb-btn"
				class:tb-on={Object.values(hideSettings).some(
					(s) => s.laneHidden || s.hideGroup || s.hideSolo
				)}
				onclick={() => (showHideModal = true)}
				title="레인 순서 / 숨김 / 그룹·단독 노드 가리기"
			>
				<span class="icon">⚙</span><span>보드 설정</span>
			</button>
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
			<div class="tb-sep"></div>
		{/if}
		<!-- 토글 버튼 — 항상 우측 끝 (collapsed / expanded 동일 위치, 사용자 피드백). -->
		<button
			class="tb-btn tb-collapse"
			onclick={toggleToolbarCollapsed}
			title={toolbarCollapsed ? '도구바 펼치기' : '도구바 접기 — 레인 라벨이 가려질 때'}
			aria-label={toolbarCollapsed ? '도구바 펼치기' : '도구바 접기'}
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
				<button class="dialog-ok" onclick={() => confirmDialogResolve(true)}>변경</button>
				<button class="dialog-cancel" onclick={() => confirmDialogResolve(false)}>취소</button>
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
				<h3 class="hide-title">보드 설정</h3>
				<button
					class="hide-close"
					onclick={() => (showHideModal = false)}
					aria-label="닫기"
				>×</button>
			</div>
			<p class="hide-help">
				레인 순서 변경 + 숨김 + 그룹·단독 노드 가리기. ◀ / ▶ 로 좌우 이동, 표시 해제 시 그 레인 전체 숨김.
			</p>
			<table class="hide-table">
				<thead>
					<tr>
						<th style="width: 6ch">순서</th>
						<th style="width: 14ch">레인</th>
						<th>표시</th>
						<th>그룹 숨김</th>
						<th>단독 노드 숨김</th>
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
									title="왼쪽으로"
									aria-label="왼쪽으로"
								>◀</button>
								<button
									class="reorder-btn"
									onclick={() => swapLane(li, 1)}
									disabled={li === sorted.length - 1}
									title="오른쪽으로"
									aria-label="오른쪽으로"
								>▶</button>
							</td>
							<td>
								<span class="hide-lane-name" style:color={s.color}>{s.name_en}</span>
							</td>
							<td>
								<input
									type="checkbox"
									checked={laneVisible}
									onchange={() => toggleHideSetting(s.slug, 'laneHidden')}
									title="레인 표시 (체크 해제 시 레인 전체 숨김)"
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
	</div>
{/if}

<style>
	.board-wrap {
		position: relative;
		width: 100%;
		height: calc(100vh - 3.25rem);
		background: var(--bg);
		overflow: hidden;
	}

	.lanes-bg { position: absolute; inset: 0; z-index: 0; pointer-events: none; }
	.board { position: absolute; inset: 0; z-index: 1; background: transparent; }
	.lane-hdrs { position: absolute; inset: 0; z-index: 2; pointer-events: none; overflow: hidden; }

	:global(.lane-col) {
		position: absolute; top: 0; bottom: 0;
		background: var(--bg-elevated);
		border-right: 1px solid var(--bg-subtle);
		box-sizing: border-box;
		pointer-events: none;
		transition: background 0.12s, box-shadow 0.12s;
		overflow: hidden;
	}
	/* DEV-105 fix13/fix14: grid snap dot 표시. background-position 변경은 매
	   frame paint → 느림. 자식 div 의 transform: translateY (modulo cellH) 로
	   전환 → GPU composite + pattern 주기성으로 정확한 정렬.
	   top / left / width / bottom 은 JS 에서 zoom 기반 동적 설정 (cellHPx 만큼
	   위로). */
	:global(.lane-dots) {
		position: absolute;
		pointer-events: none;
		background-repeat: repeat-y;
		will-change: transform;
	}
	/* DEV-105 fix11: 드래그 중 노드가 놓일 lane 강조. */
	:global(.lane-col.drag-target) {
		background: color-mix(in srgb, var(--accent) 14%, var(--bg-elevated));
		box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--accent) 55%, transparent);
	}
	/* DEV-101 fix8: 헤더 height 는 cytoscape `LANE_TOP=52` 와 정렬 위해 px 고정
	   (스케일하면 노드와 겹침). 내부 padding/gap/font 만 rem 으로 — UI 크기에
	   비례해 컨텐츠가 자연스럽게 자람 (max 2x 까지 38px 안에 fit). */
	:global(.lane-hdr) {
		position: absolute; top: 0; height: 38px;
		display: flex; align-items: center; gap: 0.375rem;
		padding: 0 0.5rem 0 0.875rem;
		border-right: 1px solid var(--bg-subtle);
		border-bottom: 1px solid var(--bg-subtle);
		box-sizing: border-box;
		background: var(--bg-elevated);
		pointer-events: none;
	}
	/* DEV-105 fix2: 접혔을 때 label 만 표시 — 다른 컨트롤 (cols-sel, arrange-group)
	   은 좁은 폭에서 시각적으로 깨지고 label 을 가려서 다시 펼치기가 어려워짐. */
	:global(.lane-hdr.collapsed > :not(.lane-label)) {
		display: none !important;
	}
	:global(.lane-hdr.collapsed) {
		padding: 0 0.25rem;
		justify-content: center;
	}
	:global(.lane-label) {
		flex: 1; font-size: 0.75rem; font-weight: bold;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
		/* DEV-105: button 으로 변경 — 기본 button 스타일 reset. */
		background: none; border: none; padding: 0; cursor: pointer;
		text-align: left;
		pointer-events: auto;
		transition: opacity 0.15s;
	}
	:global(.lane-label:hover) { opacity: 0.75; }
	/* DEV-105: collapsed 시 90도 회전 (세로) + 글자 한 줄 압축.
	   DEV-105 fix4: max-height 60px 가 lane-hdr (38px) 보다 커서 긴 이름이 위로
	   삐져나가 잘림. 헤더 안에 들어가도록 28px 로 축소 + ellipsis. */
	:global(.lane-label.collapsed) {
		writing-mode: vertical-rl;
		text-orientation: mixed;
		white-space: nowrap;
		/* DEV-105 fix6: 사용자 피드백 — 위로 짤리는것만 방지. flex 부모의
		   align-items:center 무시하고 위에 붙여, 긴 이름은 아래로 자연스럽게
		   넘어가게 (lane-hdrs 가 board 전체를 덮어 아래 overflow 는 보임). */
		align-self: flex-start;
		padding-top: 4px;
	}
	/* DEV-105 fix5: 레인별 설정 토글 ⚙ — 항상 보임, 작은 라벨 옆 버튼. */
	:global(.lane-settings-btn) {
		flex-shrink: 0; pointer-events: auto;
		background: none; border: 1px solid transparent; border-radius: 4px;
		color: var(--text-faint); font-size: 0.85rem; padding: 0 4px;
		cursor: pointer; line-height: 1.2;
		transition: background 0.1s, color 0.1s, border-color 0.1s;
	}
	:global(.lane-settings-btn:hover) {
		background: var(--bg-subtle); color: var(--text-muted); border-color: var(--border);
	}
	:global(.lane-hdr.settings-open .lane-settings-btn) {
		color: var(--text); background: var(--bg-subtle); border-color: var(--border);
	}
	/* settings 가 닫혀 있으면 cols-sel + arrange-group 숨김. */
	:global(.lane-hdr:not(.settings-open) .lane-cols-sel),
	:global(.lane-hdr:not(.settings-open) .lane-arrange-group) {
		display: none !important;
	}
	:global(.lane-cols-sel) {
		flex-shrink: 0; pointer-events: auto;
		background: var(--bg); border: 1px solid var(--border); border-radius: 4px;
		color: var(--text-muted); font-size: 0.72rem; padding: 1px 3px; cursor: pointer; outline: none;
	}
	:global(.lane-cols-sel:hover) { border-color: var(--text-faint); color: var(--text); }
	:global(.lane-arrange-btn) {
		flex-shrink: 0; pointer-events: auto;
		background: none; border: 1px solid transparent; border-radius: 4px;
		color: var(--text-faint); font-size: 0.85rem; padding: 1px 5px;
		cursor: pointer; line-height: 1.4;
		transition: background 0.1s, color 0.1s, border-color 0.1s;
	}
	:global(.lane-arrange-btn:hover) { background: var(--bg-subtle); border-color: var(--border); color: var(--text-muted); }

	/* DEV-059: lane 순서 변경 — label 양 끝 ◀ ▶. */
	:global(.lane-move-btn) {
		flex-shrink: 0; pointer-events: auto;
		background: none; border: none; border-radius: 4px;
		color: var(--text-faint); font-size: 0.7rem; padding: 0 4px;
		cursor: pointer; line-height: 1;
		transition: background 0.1s, color 0.1s;
	}
	:global(.lane-move-btn:hover:not(:disabled)) { background: var(--bg-subtle); color: var(--text); }
	:global(.lane-move-btn:disabled) { opacity: 0.25; cursor: not-allowed; }

	/* lane header 의 mode select (Group / All) — lane-cols-sel 과 비슷한 비주얼 */
	:global(.lane-mode-sel) {
		flex-shrink: 0; pointer-events: auto;
		background: var(--bg); border: 1px solid var(--border); border-radius: 4px;
		color: var(--text-muted); font-size: 0.72rem; padding: 1px 3px; cursor: pointer; outline: none;
	}
	:global(.lane-mode-sel:hover) { border-color: var(--text-faint); color: var(--text); }

	/* lane header 의 ⊟ 버튼 + mode select 를 segmented 컨트롤로 묶음 (toolbar 와 동일 패턴) */
	:global(.lane-arrange-group) {
		flex-shrink: 0;
		display: flex;
		align-items: stretch;
		gap: 0;
		pointer-events: auto;
	}
	:global(.lane-arrange-group .lane-arrange-btn) {
		border: 1px solid var(--border);
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
		width: 300px;
		z-index: 6;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
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
		border-bottom: 1px solid var(--bg-subtle);
	}
	.drag-hint {
		flex-shrink: 0;
		color: var(--border);
		font-size: 1rem;
		line-height: 1.4;
		padding: 1px 2px;
		transition: color 0.1s;
	}
	.card-head:hover .drag-hint { color: var(--text-faint); }
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
		color: var(--text-faint); font-size: 1.1rem; line-height: 1;
		padding: 0 2px; cursor: pointer; transition: color 0.1s;
	}
	.card-close:hover { color: var(--text); }

	.card-title {
		margin: 0; padding: 10px 12px 6px;
		font-size: 0.9rem; font-weight: 600; color: var(--text-strong);
		line-height: 1.45; word-break: break-word;
	}
	.card-branch {
		display: flex; align-items: center; gap: 8px;
		margin: 0 12px 8px;
		padding: 4px 8px;
		background: var(--bg); border: 1px solid var(--bg-subtle); border-radius: 5px;
	}
	.blabel { font-size: 0.7rem; color: var(--text-faint); }
	.bname { font-family: 'SFMono-Regular', Consolas, monospace; font-size: 0.78rem; color: var(--accent-secondary); }

	.card-goto {
		margin: 0 12px 10px;
		padding: 6px 10px;
		background: var(--bg-subtle); border: 1px solid var(--border); border-radius: 6px;
		color: var(--accent); font-size: 0.78rem; cursor: pointer; text-align: left;
		transition: background 0.1s, border-color 0.1s;
	}
	.card-goto:hover { background: var(--border); border-color: var(--text-faint); }

	.card-divider { height: 1px; background: var(--bg-subtle); }

	.card-sec-label {
		margin: 10px 12px 4px;
		font-size: 0.67rem; font-weight: 600; color: var(--text-faint);
		text-transform: uppercase; letter-spacing: 0.06em;
	}
	.hl-multi-hint {
		font-size: 0.62rem; color: var(--border);
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
		background: var(--bg);
	}
	/* DEV-074 fix20 (sweep): 토큰 + color-mix 로 통일.
	   이전엔 hex 와 rgba() 직접 — 라이트모드에서 색 안 변함. */
	.hl-btn.pre    { color: var(--hl-pre); border-color: color-mix(in srgb, var(--hl-pre) 25%, transparent); }
	.hl-btn.sub    { color: var(--hl-sub); border-color: color-mix(in srgb, var(--hl-sub) 25%, transparent); }
	.hl-btn.next   { color: var(--hl-next); border-color: color-mix(in srgb, var(--hl-next) 25%, transparent); }
	.hl-btn.parent { color: var(--success); border-color: color-mix(in srgb, var(--success) 25%, transparent); }
	.hl-btn.all    { color: var(--text); border-color: var(--border); }

	.hl-btn:hover { background: var(--bg-subtle); }
	.hl-btn.pre.on    { background: color-mix(in srgb, var(--hl-pre) 15%, transparent); border-color: var(--hl-pre); }
	.hl-btn.sub.on    { background: color-mix(in srgb, var(--hl-sub) 15%, transparent); border-color: var(--hl-sub); }
	.hl-btn.next.on   { background: color-mix(in srgb, var(--hl-next) 15%, transparent); border-color: var(--hl-next); }
	.hl-btn.parent.on { background: color-mix(in srgb, var(--success) 15%, transparent); border-color: var(--success); }
	.hl-btn.all.on    { background: color-mix(in srgb, var(--text-muted) 10%, transparent); border-color: var(--text-muted); }

	.hl-actions {
		margin: 6px 12px 0;
		display: flex; gap: 4px;
	}
	.hl-act {
		flex: 1;
		padding: 4px 6px;
		background: var(--bg); border: 1px solid var(--border); border-radius: 5px;
		color: var(--text-muted); font-size: 0.72rem; cursor: pointer;
		transition: background 0.1s, color 0.1s, border-color 0.1s;
	}
	.hl-act:hover:not(:disabled) { background: var(--bg-subtle); color: var(--text); }
	.hl-act:disabled { opacity: 0.4; cursor: default; }
	.hl-act.sel { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
	.hl-act.sel:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--accent-secondary); }
	.hl-act.arr { color: var(--warning); border-color: color-mix(in srgb, var(--warning) 30%, transparent); }
	.hl-act.arr:hover:not(:disabled) { background: color-mix(in srgb, var(--warning) 10%, transparent); color: var(--orange); }
	.hl-act.clear { color: var(--text-faint); }

	.card-note {
		margin: 6px 12px 10px;
		font-size: 0.67rem; color: var(--border); line-height: 1.4;
	}

	/* ── 툴바 (z:10) ── */
	/* DEV-073 fix3: New Quest 는 상단 고정, 나머지 도구바는 그 아래로. */
	.tb-newquest-wrap {
		position: absolute; top: 10px; right: 14px;
		z-index: 10;
		pointer-events: auto;
	}
	.toolbar {
		position: absolute; top: 10px; right: 14px;
		z-index: 10; display: flex; align-items: center; gap: 4px;
		pointer-events: auto;
	}
	/* New Quest 가 있으면 도구바를 그 아래로 내림 — 새 퀘스트 버튼 높이 (~32px) + 여백. */
	.toolbar.has-newquest {
		top: 50px;
	}
	/* DEV-073: collapsed 시 ⊟ 한 버튼만. 배경 / 그림자도 최소화해서 lane 영역 가림 최소. */
	.toolbar.collapsed {
		gap: 0;
	}
	/* DEV-073: 접기 토글 — 항상 표시. */
	.tb-btn.tb-collapse {
		opacity: 0.7;
		padding: 4px 8px;
	}
	.tb-btn.tb-collapse:hover { opacity: 1; }
	.tb-btn {
		display: flex; align-items: center; gap: 4px;
		padding: 4px 10px;
		background: var(--bg-elevated); border: 1px solid var(--border); border-radius: 6px;
		color: var(--text-muted); font-size: 0.8rem; cursor: pointer;
		transition: background 0.1s, color 0.1s, border-color 0.1s;
	}
	.tb-btn:hover:not(:disabled) { background: var(--bg-subtle); border-color: var(--text-faint); color: var(--text); }
	.tb-btn:disabled { opacity: 0.35; cursor: default; }
	.tb-btn.tb-on {
		background: color-mix(in srgb, var(--warning) 12%, transparent);
		border-color: color-mix(in srgb, var(--warning) 55%, transparent);
		color: var(--warning);
	}
	.tb-btn.tb-on:hover:not(:disabled) {
		background: color-mix(in srgb, var(--warning) 18%, transparent);
		border-color: var(--warning);
		color: var(--orange);
	}
	.tb-btn .icon { font-size: 0.95rem; line-height: 1; }
	.tb-btn .count { font-size: 0.7rem; color: var(--text-faint); min-width: 10px; text-align: right; }
	.tb-btn:hover:not(:disabled) .count { color: var(--text-muted); }
	/* DEV-084: New Quest — toolbar 안 primary 강조 (초록).
	   DEV-074 fix6: --btn-primary-* 토큰으로 통일 (dark/light 자동). */
	.tb-btn.tb-new {
		background: var(--btn-primary-bg); border-color: var(--btn-primary-border);
		color: var(--btn-primary-text); font-weight: 600;
	}
	.tb-btn.tb-new:hover:not(:disabled) {
		background: var(--btn-primary-bg-hover); border-color: var(--btn-primary-border-hover);
	}
	.tb-sep { width: 1px; background: var(--border); align-self: stretch; margin: 2px 0; }
	.tb-select {
		padding: 3px 6px;
		background: var(--bg-elevated); border: 1px solid var(--border); border-radius: 6px;
		color: var(--text-muted); font-size: 0.8rem; cursor: pointer; outline: none;
	}
	.tb-select:hover { border-color: var(--text-faint); color: var(--text); }

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
		position: fixed; inset: 3.25rem 0 0 0;
		display: flex; align-items: center; justify-content: center;
		color: var(--text-faint); font-size: 0.9rem; pointer-events: none; z-index: 2;
	}
	.overlay.error { color: var(--danger); }

	/* ── 인앱 확인 다이얼로그 ── */
	.dialog-backdrop {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.55);
		z-index: 500;
		display: flex; align-items: center; justify-content: center;
	}
	.dialog {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 1.25rem 1.5rem 1rem;
		min-width: 280px; max-width: 420px;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		display: flex; flex-direction: column; gap: 1rem;
	}
	.dialog-msg {
		margin: 0;
		font-size: 0.9rem; color: var(--text); line-height: 1.5;
		white-space: pre-wrap;
	}
	.dialog-btns {
		display: flex; gap: 0.5rem; justify-content: flex-end;
	}
	.dialog-ok {
		padding: 0.4rem 1.1rem;
		background: var(--btn-primary-bg); border: 1px solid var(--btn-primary-border); border-radius: 6px;
		color: var(--btn-primary-text); font-size: 0.875rem; cursor: pointer;
		transition: background 0.1s, border-color 0.1s;
	}
	.dialog-ok:hover { background: var(--btn-primary-bg-hover); border-color: var(--btn-primary-border-hover); }
	.dialog-cancel {
		padding: 0.4rem 1rem;
		background: transparent; border: 1px solid var(--border); border-radius: 6px;
		color: var(--text-muted); font-size: 0.875rem; cursor: pointer;
	}
	.dialog-cancel:hover { background: var(--bg-subtle); }

	/* DEV-056: 숨김 설정 모달 */
	.hide-modal {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 1.25rem 1.5rem 1.25rem;
		min-width: 480px; max-width: 640px;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
		display: flex; flex-direction: column; gap: 0.75rem;
		color: var(--text);
	}
	.hide-head {
		display: flex; align-items: center; justify-content: space-between;
	}
	.hide-title {
		margin: 0; font-size: 1rem; font-weight: 600; color: var(--text-strong);
	}
	.hide-close {
		background: transparent; border: none; color: var(--text-muted);
		font-size: 1.4rem; line-height: 1; cursor: pointer; padding: 0 0.3rem;
	}
	.hide-close:hover { color: var(--text); }
	.hide-help {
		margin: 0; font-size: 0.825rem; color: var(--text-muted); line-height: 1.45;
	}
	.hide-table {
		width: 100%; border-collapse: collapse; font-size: 0.875rem;
	}
	.hide-table th, .hide-table td {
		text-align: left; padding: 0.5rem 0.6rem;
		border-bottom: 1px solid var(--bg-subtle);
	}
	.hide-table th { color: var(--text-muted); font-weight: 500; font-size: 0.8rem; }
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
	.hide-table .reorder-cell {
		display: flex;
		gap: 0.2rem;
		align-items: center;
	}
	.hide-table .reorder-btn {
		padding: 0.1rem 0.4rem;
		font-size: 0.75rem;
		background: var(--bg-subtle);
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-muted);
		cursor: pointer;
		transition: background 0.1s, color 0.1s;
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
