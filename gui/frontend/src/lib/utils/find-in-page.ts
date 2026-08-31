// REQ-018: 페이지 안에서 글자 찾기 (Ctrl/Cmd+F).
//
// **DOM 을 건드리지 않는다.** 일치 항목을 `<mark>` 로 감싸는 흔한 방법은 여기서
// 못 쓴다 — Svelte 가 소유한 DOM 에 남의 노드를 끼워 넣는 것이라, 그 부분이
// 재렌더되는 순간 서로 어긋난다. 대신 `Range` 만 만들어 두고 CSS Custom
// Highlight API 로 칠한다(`CSS.highlights` + `::highlight()`).
//
// 이 파일은 **레이아웃을 모른다** — 텍스트를 모으고, 찾고, Range 로 되돌리는
// 것까지만 한다. 스크롤·강조·단축키는 `FindBar.svelte` 의 몫이다. 그래야
// 실제 화면 없이 테스트할 수 있다.

/** 한 덩어리의 텍스트와, 그것이 어느 텍스트 노드에서 왔는지. */
export interface Segment {
	text: string;
	pieces: { node: Text; start: number; len: number }[];
}

/** 세그먼트 안에서의 일치 구간. */
export interface Match {
	segment: number;
	start: number;
	end: number;
}

/**
 * 내용이 없거나 화면에 글자로 나오지 않는 것들.
 *
 * `<script>`/`<style>` 은 당연하고, `<textarea>`/`<input>` 은 값이 텍스트
 * 노드가 아니라 여기서 안 잡힌다(편집 중인 글은 편집기 자신의 찾기가 맡는다).
 */
const SKIP_TAGS = new Set([
	'SCRIPT',
	'STYLE',
	'NOSCRIPT',
	'TEMPLATE',
	'TEXTAREA',
	'INPUT',
	'SELECT',
	'OPTION',
	'SVG',
	'CANVAS'
]);

/**
 * 여기서 문단이 끊긴다고 보는 태그.
 *
 * 왜 필요한가: 텍스트 노드를 그냥 이어 붙이면 **문단 경계를 넘어 잘못 일치**
 * 한다. 한 문단이 "…abc" 로 끝나고 다음 문단이 "def…" 로 시작하면 "abcdef"
 * 가 찾아진다. 실제로는 화면에서 붙어 있지 않은 글자다.
 */
const BLOCK_TAGS = new Set([
	'ADDRESS','ARTICLE','ASIDE','BLOCKQUOTE','BR','DD','DETAILS','DIALOG','DIV',
	'DL','DT','FIELDSET','FIGCAPTION','FIGURE','FOOTER','FORM','H1','H2','H3',
	'H4','H5','H6','HEADER','HR','LI','MAIN','NAV','OL','P','PRE','SECTION',
	'SUMMARY','TABLE','TBODY','TD','TFOOT','TH','THEAD','TR','UL'
]);

/** 이 속성이 붙은 서브트리는 찾기에서 통째로 뺀다 — 찾기 UI 자신 등. */
export const SKIP_ATTR = 'data-find-skip';

function isSkipped(el: Element): boolean {
	if (SKIP_TAGS.has(el.tagName)) return true;
	if (el.hasAttribute(SKIP_ATTR)) return true;
	if (el.getAttribute('aria-hidden') === 'true') return true;
	if ((el as HTMLElement).hidden) return true;
	// `checkVisibility` 가 있으면 화면에 안 나오는 것(display:none, content-
	// visibility 등)을 걸러 준다. jsdom 에는 없으므로 있을 때만 쓴다.
	const check = (el as HTMLElement & { checkVisibility?: () => boolean }).checkVisibility;
	if (typeof check === 'function' && !check.call(el)) return true;
	return false;
}

/**
 * `root` 아래의 보이는 텍스트를 문단 단위로 모은다.
 *
 * 세그먼트를 나누는 이유는 위 `BLOCK_TAGS` 주석 참고.
 */
export function buildSegments(root: Node): Segment[] {
	const segments: Segment[] = [];
	let cur: Segment | null = null;

	const flush = () => {
		if (cur && cur.text.trim().length > 0) segments.push(cur);
		cur = null;
	};

	const visit = (node: Node) => {
		if (node.nodeType === Node.TEXT_NODE) {
			const t = node as Text;
			const data = t.data;
			if (data.length === 0) return;
			if (!cur) cur = { text: '', pieces: [] };
			cur.pieces.push({ node: t, start: cur.text.length, len: data.length });
			cur.text += data;
			return;
		}
		if (node.nodeType !== Node.ELEMENT_NODE) return;
		const el = node as Element;
		if (isSkipped(el)) return;
		const block = BLOCK_TAGS.has(el.tagName);
		if (block) flush();
		for (let c = el.firstChild; c; c = c.nextSibling) visit(c);
		if (block) flush();
	};

	visit(root);
	flush();
	return segments;
}

/**
 * 세그먼트들에서 `query` 를 찾는다. 대소문자는 구분하지 않는다.
 *
 * 겹치는 일치는 만들지 않는다 — "aa" 를 "aaa" 에서 찾으면 1개다(0-2). 두 개로
 * 세면 다음/이전이 같은 자리를 맴돈다.
 */
export function findMatches(segments: Segment[], query: string): Match[] {
	const q = query.toLowerCase();
	if (q.length === 0) return [];
	const out: Match[] = [];
	for (let s = 0; s < segments.length; s++) {
		const hay = segments[s].text.toLowerCase();
		let from = 0;
		for (;;) {
			const i = hay.indexOf(q, from);
			if (i < 0) break;
			out.push({ segment: s, start: i, end: i + q.length });
			from = i + q.length;
		}
	}
	return out;
}

/** 세그먼트 안의 오프셋을 실제 텍스트 노드 위치로. */
function locate(seg: Segment, offset: number): { node: Text; offset: number } | null {
	for (const p of seg.pieces) {
		if (offset <= p.start + p.len) {
			return { node: p.node, offset: Math.max(0, offset - p.start) };
		}
	}
	const last = seg.pieces[seg.pieces.length - 1];
	return last ? { node: last.node, offset: last.len } : null;
}

/**
 * 일치 구간을 `Range` 로. 노드가 이미 사라졌으면 `null`.
 *
 * 한 일치가 **여러 텍스트 노드에 걸칠 수 있다** — `**굵게**` 같은 마크업이
 * 중간에 끼면 한 문단이 여러 노드로 쪼개진다. 그래서 시작/끝을 따로 찾는다.
 */
export function matchToRange(segments: Segment[], m: Match): Range | null {
	const seg = segments[m.segment];
	if (!seg) return null;
	const a = locate(seg, m.start);
	const b = locate(seg, m.end);
	if (!a || !b) return null;
	try {
		const r = document.createRange();
		r.setStart(a.node, a.offset);
		r.setEnd(b.node, b.offset);
		return r;
	} catch {
		// 노드가 문서에서 빠졌거나 오프셋이 어긋난 경우 — 다음 재검색에서 낫는다.
		return null;
	}
}

/** CSS Custom Highlight API 를 쓸 수 있나. 없으면 이동만 하고 칠하지 않는다. */
export function supportsHighlightApi(): boolean {
	return (
		typeof CSS !== 'undefined' &&
		typeof (CSS as unknown as { highlights?: unknown }).highlights !== 'undefined' &&
		typeof (globalThis as unknown as { Highlight?: unknown }).Highlight === 'function'
	);
}

/** 다음/이전 순환. 일치가 없으면 -1. */
export function stepIndex(cur: number, total: number, delta: number): number {
	if (total <= 0) return -1;
	if (cur < 0) return delta >= 0 ? 0 : total - 1;
	return (cur + delta + total) % total;
}

/**
 * 이 경로에서 찾기를 열 것인가.
 *
 * REQ-018 은 **문서형 화면부터**로 정했다(admin) — 긴 글을 읽는 곳이다.
 * 보드는 노드가 transform 된 월드 안에 있어서, 일치 항목으로 가려면 스크롤이
 * 아니라 뷰포트를 옮겨야 한다. 별도 작업이라 여기서 뺀다.
 *
 * 목록/홈을 뺀 것은 필터가 이미 있기 때문이다. 웹 모드에서는 그 화면들에서
 * 네이티브 찾기가 그대로 남는다 — 우리가 가로채지 않으므로.
 */
export function isFindablePath(pathname: string): boolean {
	if (pathname.startsWith('/quests/')) return true;
	if (pathname.startsWith('/campaigns/') && pathname !== '/campaigns/new') return true;
	return pathname === '/rules' || pathname === '/library' || pathname === '/worklog';
}
