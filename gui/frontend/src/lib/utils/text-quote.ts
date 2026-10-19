// DEV-443: 개인 형광펜의 **자리 잡기** — 글자를 모으고, 고른 범위를 글자 위치로, 저장된 형광펜을 다시 범위로.
//
// 형광펜은 글자 위치가 아니라 **칠한 글 + 앞뒤 몇 글자**로 저장한다(W3C TextQuoteSelector 방식). 본문이
// 바뀌어도 같은 글을 다시 찾아 칠할 수 있고, 같은 글이 여러 번 나오면 앞뒤 글자로 어느 것인지 가린다.
//
// 이 파일은 **레이아웃을 모르고 DOM 을 바꾸지 않는다** — 텍스트 노드를 읽고 `Range` 를 만들 뿐이다. 칠하는
// 것은 CSS Custom Highlight API 가 한다(찾기 [[REQ-018]] 와 같은 방식). 그래야 Svelte 가 소유한 DOM 과
// 어긋나지 않고, 화면 없이 시험할 수 있다.

/** 글자로 보이지 않거나 본문이 아닌 것 — 여기 든 글자는 모으지도, 칠하지도 않는다. */
const SKIP = 'script, style, svg, button, textarea, input, select, [data-hl-skip]';

/** 앞뒤로 적어 둘 글자 수. */
export const CONTEXT = 32;

export interface TextMap {
	/** 모든 텍스트 노드를 이어 붙인 글. */
	text: string;
	pieces: { node: Text; start: number }[];
}

export interface Quote {
	exact: string;
	prefix: string;
	suffix: string;
}

export function collectText(root: Node): TextMap {
	const pieces: TextMap['pieces'] = [];
	let text = '';
	const doc = root.ownerDocument ?? (root as Document);
	const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
		acceptNode(n) {
			const parent = n.parentElement;
			if (parent && parent.closest(SKIP)) {
				// 뿌리 바깥의 조상이 걸린 것이면 상관없다 — 뿌리 안에서 걸렸을 때만 뺀다.
				const hit = parent.closest(SKIP);
				if (hit && root.contains(hit)) return NodeFilter.FILTER_REJECT;
			}
			return NodeFilter.FILTER_ACCEPT;
		}
	});
	for (let n = walker.nextNode(); n; n = walker.nextNode()) {
		const t = n as Text;
		pieces.push({ node: t, start: text.length });
		text += t.data;
	}
	return { text, pieces };
}

/**
 * 경계점(node, offset) → 글자 위치. 텍스트 노드 안이면 그대로, 요소 경계면 그 뒤 첫 텍스트 노드의
 * 시작. 뿌리 밖이거나 모은 글에 없는 노드면 `null`.
 */
export function offsetOf(map: TextMap, node: Node, offset: number): number | null {
	if (node.nodeType === Node.TEXT_NODE) {
		const p = map.pieces.find((x) => x.node === node);
		return p ? p.start + Math.min(offset, (node as Text).data.length) : null;
	}
	const doc = node.ownerDocument;
	if (!doc) return null;
	const at = doc.createRange();
	at.setStart(node, offset);
	for (const p of map.pieces) {
		// 이 조각이 경계점 뒤(또는 그 자리)에서 시작하면 그것이 경계점의 글자 위치다.
		if (at.comparePoint(p.node, 0) >= 0) return p.start;
	}
	return map.text.length;
}

/** 고른 범위 → [시작, 끝) 글자 위치. 비었거나 뿌리 밖이면 `null`. */
export function rangeOffsets(map: TextMap, range: Range): [number, number] | null {
	const s = offsetOf(map, range.startContainer, range.startOffset);
	const e = offsetOf(map, range.endContainer, range.endOffset);
	if (s == null || e == null || e <= s) return null;
	return [s, e];
}

/** [시작, 끝) → 저장할 인용. */
export function quoteAt(text: string, s: number, e: number): Quote {
	return {
		exact: text.slice(s, e),
		prefix: text.slice(Math.max(0, s - CONTEXT), s),
		suffix: text.slice(e, e + CONTEXT)
	};
}

function commonTail(a: string, b: string): number {
	let n = 0;
	while (n < a.length && n < b.length && a[a.length - 1 - n] === b[b.length - 1 - n]) n++;
	return n;
}
function commonHead(a: string, b: string): number {
	let n = 0;
	while (n < a.length && n < b.length && a[n] === b[n]) n++;
	return n;
}

/**
 * 저장된 인용을 지금 글에서 찾는다 → [시작, 끝) 또는 못 찾으면 `null`.
 *
 * 칠한 글이 그대로 있는 자리 중에서 **앞뒤 글자가 가장 많이 맞는 곳**을 고른다. 같으면 앞의 것.
 * 칠한 글 자체가 바뀌었으면 못 찾은 것이다 — 비슷한 곳을 어림으로 칠하면 엉뚱한 글이 칠해진다.
 */
export function findQuote(text: string, q: Quote): [number, number] | null {
	if (!q.exact) return null;
	let best: number | null = null;
	let bestScore = -1;
	for (let i = text.indexOf(q.exact); i >= 0; i = text.indexOf(q.exact, i + 1)) {
		const before = text.slice(Math.max(0, i - q.prefix.length), i);
		const after = text.slice(i + q.exact.length, i + q.exact.length + q.suffix.length);
		const score = commonTail(before, q.prefix) + commonHead(after, q.suffix);
		if (score > bestScore) {
			best = i;
			bestScore = score;
		}
	}
	return best == null ? null : [best, best + q.exact.length];
}

/** [시작, 끝) → `Range`. 칠할 때 쓴다. */
export function rangeAt(map: TextMap, s: number, e: number): Range | null {
	const startPiece = map.pieces.find((p) => p.start <= s && s < p.start + p.node.data.length);
	const endPiece = map.pieces.find((p) => p.start < e && e <= p.start + p.node.data.length);
	if (!startPiece || !endPiece) return null;
	const r = startPiece.node.ownerDocument.createRange();
	r.setStart(startPiece.node, s - startPiece.start);
	r.setEnd(endPiece.node, e - endPiece.start);
	return r;
}
