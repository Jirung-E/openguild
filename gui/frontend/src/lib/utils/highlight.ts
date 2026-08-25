/**
 * REQ-014: 검색 발췌에서 **걸린 부분**을 표시한다.
 *
 * 발췌만 보여주면 "이 문서가 왜 나왔는지" 는 알아도 "어느 글자가 맞았는지" 는
 * 여전히 눈으로 찾아야 한다. 발췌가 짧아도 토큰이 여러 개면 헷갈린다.
 *
 * `{@html}` 로 `<mark>` 를 끼워 넣지 않는다 — 발췌는 문서 본문·댓글에서 온
 * 값이라 마크업이 섞여 있을 수 있고, 그대로 렌더하면 그게 곧 주입 경로다.
 * 대신 **조각 배열**을 돌려주고 컴포넌트가 각 조각을 텍스트로 그린다.
 */

export interface HighlightSegment {
	text: string;
	/** 검색어에 걸린 조각인지. */
	hit: boolean;
}

/**
 * 검색어를 토큰으로 나눈다 — core 의 `search()` 와 같은 규칙(공백 분리).
 * 빈 토큰은 버린다.
 */
export function searchTokens(query: string): string[] {
	return query
		.split(/\s+/)
		.map((t) => t.trim())
		.filter((t) => t.length > 0);
}

/**
 * `text` 를 검색어에 걸린 조각과 아닌 조각으로 나눈다.
 *
 * - 대소문자 무시. 토큰이 여러 개면 **각각 전부** 표시한다(core 의 AND 는
 *   "문서가 나오는 조건" 이고, 표시는 눈에 보이는 모든 일치가 대상이다).
 * - 겹치는 일치는 하나로 합친다 — `가나` 와 `나다` 가 `가나다` 에 겹쳐 걸리면
 *   조각이 쪼개져 어색해진다.
 * - 걸린 게 없으면 원문 한 조각만 돌려준다.
 */
export function highlightSegments(text: string, query: string): HighlightSegment[] {
	if (!text) return [];
	const tokens = searchTokens(query);
	if (tokens.length === 0) return [{ text, hit: false }];

	const lower = text.toLowerCase();
	// 일부 문자는 소문자화하면 길이가 달라져(예: 'İ' → 2글자) 인덱스가 어긋난다.
	// 그런 입력에서는 잘못된 위치를 칠하느니 표시를 포기한다.
	if (lower.length !== text.length) return [{ text, hit: false }];

	// 1) 모든 토큰의 일치 구간 수집.
	const ranges: Array<[number, number]> = [];
	for (const tok of tokens) {
		const t = tok.toLowerCase();
		if (t.length !== tok.length) continue;
		let from = 0;
		for (;;) {
			const at = lower.indexOf(t, from);
			if (at < 0) break;
			ranges.push([at, at + t.length]);
			// 겹치는 일치도 잡는다(`aa` 안의 `a` 두 번).
			from = at + 1;
		}
	}
	if (ranges.length === 0) return [{ text, hit: false }];

	// 2) 시작 위치로 정렬 후 겹치거나 맞닿은 구간 병합.
	ranges.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
	const merged: Array<[number, number]> = [];
	for (const r of ranges) {
		const last = merged[merged.length - 1];
		if (last && r[0] <= last[1]) last[1] = Math.max(last[1], r[1]);
		else merged.push([r[0], r[1]]);
	}

	// 3) 조각으로 펼친다.
	const out: HighlightSegment[] = [];
	let cursor = 0;
	for (const [s, e] of merged) {
		if (s > cursor) out.push({ text: text.slice(cursor, s), hit: false });
		out.push({ text: text.slice(s, e), hit: true });
		cursor = e;
	}
	if (cursor < text.length) out.push({ text: text.slice(cursor), hit: false });
	return out;
}
