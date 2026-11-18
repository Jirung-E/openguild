/**
 * REQ-006: 작업 기록 compact 뷰의 묶음 로직.
 *
 * 원래 `routes/worklog/+page.svelte` 안에 있었다. 컴포넌트 안에 있으면 단위
 * 테스트를 붙일 수 없어 — 첫 등장 순서 유지·시각 범위 표기·뱃지 중복 제거가
 * 전부 육안 확인에만 의존했다 — 여기로 분리했다. 페이지는 이 함수들을 쓴다.
 */
import type { ActivityCounts, ActivityRow } from '$lib/api/worklog';

/** 하루치 활동을 문서 단위로 묶은 결과. */
export interface DocGroup {
	slug: string;
	/** 그룹 링크 — 그 문서의 **첫** 활동을 가리킨다(댓글이면 그 댓글까지). */
	href: string;
	rows: ActivityRow[];
	/** 그룹 안 조작 종류(중복 제거, 등장 순) — 뱃지로 요약 표시. */
	kinds: string[];
	/** 가장 이른 / 늦은 시각 (ISO 전체). 표기는 `groupTimeLabel` 이 만든다. */
	fromTs: string;
	toTs: string;
}

/** 활동 한 건이 가리키는 문서 경로. */
export function activityHref(a: ActivityRow): string {
	// DEV-288: 규칙/도서관 활동은 해당 문서 페이지로(딥링크 쿼리는 각 페이지가 처리).
	if (a.kind === 'rule') return `/rules?slug=${encodeURIComponent(a.slug)}`;
	if (a.kind === 'book') return `/library?id=${encodeURIComponent(a.slug)}`;
	const base = /^C-\d+$/.test(a.slug)
		? `/campaigns/${encodeURIComponent(a.slug)}`
		: `/quests/${encodeURIComponent(a.slug)}`;
	// DEV-296: 댓글/토론 활동은 그 댓글까지 — 문서만 열면 긴 목록에서 다시
	// 찾아야 했다. 댓글 섹션이 `?comment=N` 을 보고 스크롤 + 강조한다.
	return a.ref_id != null ? `${base}?comment=${a.ref_id}` : base;
}

/** 주/월 뷰용 날짜별 그룹핑. 서버가 시간순으로 주므로 연속 구간만 묶는다. */
export function groupByDay(activities: ActivityRow[]): { date: string; rows: ActivityRow[] }[] {
	const out: { date: string; rows: ActivityRow[] }[] = [];
	for (const a of activities) {
		const d = a.ts.slice(0, 10);
		const last = out[out.length - 1];
		if (last && last.date === d) last.rows.push(a);
		else out.push({ date: d, rows: [a] });
	}
	return out;
}

/** 하루치 rows 를 문서(slug) 단위로 묶는다. 문서의 첫 등장 순서를 유지한다. */
export function groupByDoc(rows: ActivityRow[]): DocGroup[] {
	const byslug = new Map<string, DocGroup>();
	for (const a of rows) {
		let g = byslug.get(a.slug);
		if (!g) {
			g = { slug: a.slug, href: activityHref(a), rows: [], kinds: [], fromTs: a.ts, toTs: a.ts };
			byslug.set(a.slug, g);
		}
		g.rows.push(a);
		if (!g.kinds.includes(a.kind)) g.kinds.push(a.kind);
		if (a.ts < g.fromTs) g.fromTs = a.ts;
		if (a.ts > g.toTs) g.toTs = a.ts;
	}
	return [...byslug.values()];
}

/**
 * 그룹 헤더의 시각 표기 — `01:22` 또는 범위면 `01:22–01:26`.
 *
 * 예전엔 `rows.length > 1` 로 범위 여부를 정해서, 같은 분에 두 번 조작한 문서가
 * `00:01–00:01` 로 나왔다(실 데이터에 131건). 건수가 아니라 **표시되는 값이
 * 실제로 다른지**로 판단한다.
 */
export function groupTimeLabel(g: DocGroup): string {
	const from = g.fromTs.slice(11, 16);
	const to = g.toTs.slice(11, 16);
	return from === to ? from : `${from}–${to}`;
}

/** 요약은 첫 줄만 — 여러 줄 본문이 행 높이를 밀어내지 않도록. */
export function firstLine(s: string): string {
	return s.split('\n', 1)[0] ?? '';
}

// ─── DEV-481: 필터 ───

/** 활동이 가리키는 문서 종류. */
export type WorklogDocKind = 'quest' | 'campaign' | 'rule' | 'book';
export const WORKLOG_DOC_KINDS: WorklogDocKind[] = ['quest', 'campaign', 'rule', 'book'];

/** 필터의 활동 종류 — 아래 합계의 칸과 같게 묶는다(상태 · 타입 변경은 서버도 한 칸으로 센다). */
export type WorklogActKind = 'status' | 'comment' | 'created' | 'discussion' | 'doc';
export const WORKLOG_ACT_KINDS: WorklogActKind[] = [
	'status',
	'comment',
	'created',
	'discussion',
	'doc'
];

export function activityDocKind(a: ActivityRow): WorklogDocKind {
	if (a.kind === 'rule') return 'rule';
	if (a.kind === 'book') return 'book';
	return /^C-\d+$/.test(a.slug) ? 'campaign' : 'quest';
}

export function activityActKind(a: ActivityRow): WorklogActKind | null {
	switch (a.kind) {
		case 'status':
		case 'type':
			return 'status';
		case 'comment':
		case 'created':
		case 'discussion':
			return a.kind;
		case 'rule':
		case 'book':
			return 'doc';
		default:
			return null;
	}
}

/** 빈 집합 = 그 축은 거르지 않음(전체). */
export function filterActivities(
	rows: ActivityRow[],
	docKinds: ReadonlySet<WorklogDocKind>,
	actKinds: ReadonlySet<WorklogActKind>
): ActivityRow[] {
	if (docKinds.size === 0 && actKinds.size === 0) return rows;
	return rows.filter((a) => {
		if (docKinds.size > 0 && !docKinds.has(activityDocKind(a))) return false;
		if (actKinds.size > 0) {
			const k = activityActKind(a);
			if (!k || !actKinds.has(k)) return false;
		}
		return true;
	});
}

/** 걸러진 활동으로 합계를 다시 센다 — 서버 `core::ops::worklog::activities` 의 집계와 같은 규칙. */
export function countActivities(rows: ActivityRow[]): ActivityCounts {
	const c: ActivityCounts = {
		status_changes: 0,
		comments: 0,
		created: 0,
		done_transitions: 0,
		discussion_events: 0,
		doc_changes: 0
	};
	for (const a of rows) {
		switch (a.kind) {
			case 'status':
			case 'type':
				c.status_changes += 1;
				if (a.kind === 'status' && a.summary.endsWith('→ done')) c.done_transitions += 1;
				break;
			case 'discussion':
				c.discussion_events += 1;
				break;
			case 'comment':
				c.comments += 1;
				break;
			case 'created':
				c.created += 1;
				break;
			case 'rule':
			case 'book':
				c.doc_changes += 1;
				break;
		}
	}
	return c;
}

// ─── DEV-482: 줄 모양 ───

/** 규칙 · 도서관 줄의 조작 — `op` 가 없는 옛 서버는 summary 에서 짐작한다(rename 만 `a → b`). */
export type DocOp = 'create' | 'update' | 'delete' | 'rename';
export function activityDocOp(a: ActivityRow): DocOp | null {
	if (a.kind !== 'rule' && a.kind !== 'book') return null;
	const op = a.op ?? (a.summary.includes(' → ') ? 'rename' : a.summary);
	return op === 'create' || op === 'update' || op === 'delete' || op === 'rename' ? op : null;
}

/** 상태 칩 하나 — `what` 이 있으면 캠페인의 "다 끝남" 기록(퀘스트 / 체크리스트). */
export interface FlowChip {
	what: 'quests' | 'checklist' | null;
	text: string;
}

/** 묶음 머리 줄의 칩 하나 — 상태는 흐름, 나머지는 활동 종류 칩. */
export type GroupChip =
	| { type: 'flow'; flow: FlowChip }
	| { type: 'kind'; kind: string }
	| { type: 'docOp'; op: DocOp };

function splitFlow(summary: string): [string, string] {
	const i = summary.indexOf(' → ');
	return i < 0 ? ['?', summary] : [summary.slice(0, i), summary.slice(i + 3)];
}

function allDoneWhat(a: ActivityRow): FlowChip['what'] {
	if (a.op === 'quests_all_done') return 'quests';
	if (a.op === 'checklist_all_done') return 'checklist';
	return null;
}

/** 한 줄의 상태 칩 — 상태 줄이 아니면 null. "다 끝남"은 앞의 `?` 없이 값만. */
export function activityFlow(a: ActivityRow): FlowChip | null {
	if (a.kind !== 'status') return null;
	const what = allDoneWhat(a);
	return what ? { what, text: splitFlow(a.summary)[1] } : { what: null, text: a.summary };
}

/**
 * 묶음 머리 줄의 칩들(등장 순, 중복 없이).
 *
 * - 상태 변경은 "상태" 칩 대신 그날 처음 → 마지막 흐름 하나(`open → done`).
 * - 캠페인의 "다 끝남" 기록은 퀘스트 / 체크리스트마다 마지막 값 하나.
 * - 규칙 · 도서관은 한 일(수정 · 생성 · 삭제 · 이름 바꿈)로.
 * - 나머지(댓글 · 생성 · 토론 · 타입)는 지금처럼 활동 종류 칩.
 */
export function groupChips(rows: ActivityRow[]): GroupChip[] {
	const out: GroupChip[] = [];
	const seen = new Map<string, GroupChip>();
	let firstFrom: string | null = null;
	for (const a of rows) {
		const docOp = activityDocOp(a);
		if (docOp) {
			if (!seen.has(`op:${docOp}`)) {
				const c: GroupChip = { type: 'docOp', op: docOp };
				seen.set(`op:${docOp}`, c);
				out.push(c);
			}
			continue;
		}
		if (a.kind === 'status') {
			const what = allDoneWhat(a);
			const key = `flow:${what ?? 'status'}`;
			let text: string;
			if (what) text = splitFlow(a.summary)[1];
			else {
				const [from, to] = splitFlow(a.summary);
				firstFrom ??= from;
				text = `${firstFrom} → ${to}`;
			}
			const prev = seen.get(key);
			if (prev && prev.type === 'flow') prev.flow.text = text;
			else {
				const c: GroupChip = { type: 'flow', flow: { what, text } };
				seen.set(key, c);
				out.push(c);
			}
			continue;
		}
		if (!seen.has(`kind:${a.kind}`)) {
			const c: GroupChip = { type: 'kind', kind: a.kind };
			seen.set(`kind:${a.kind}`, c);
			out.push(c);
		}
	}
	return out;
}
