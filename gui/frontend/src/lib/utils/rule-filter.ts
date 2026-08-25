/**
 * REQ-013: 규칙 목록 검색.
 *
 * 규칙은 목록 응답(`/api/rules`)이 **본문을 통째로 싣고 온다**(실측 9건 24.6KB).
 * 그래서 도서관의 첨부 이름 검색(REQ-011)처럼 서버에 판정을 맡길 이유가 없다 —
 * 디바운스도 stale 가드도 없이 즉시 필터한다.
 *
 * 규칙엔 댓글도 첨부도 없어서 대상은 slug / 태그 / 본문 셋뿐이다.
 */
import type { RuleEntry } from '$lib/api/rules';

export type RuleMatchField = 'slug' | 'tag' | 'body';

export interface RuleSearchResult {
	entry: RuleEntry;
	/** 어디서 맞았는지 — 본문에서만 맞았을 때 목록에 이유를 보여주려고. */
	matchedIn: RuleMatchField[];
	/** 본문에서 맞았다면 그 지점 주변 발췌. 아니면 빈 문자열. */
	excerpt: string;
}

/** 매치 지점 주변을 잘라낸다. 코드 단위(문자)로 세어 한글이 깨지지 않게. */
export function excerptAround(text: string, at: number, span = 40): string {
	const chars = [...text.replace(/\s+/g, ' ')];
	// `at` 은 원문 기준 인덱스라 공백 정규화 후와 어긋날 수 있다 — 정규화한
	// 문자열에서 다시 찾는 대신, 호출측이 정규화된 인덱스를 넘긴다.
	const from = Math.max(0, at - span);
	const to = Math.min(chars.length, at + span);
	const body = chars.slice(from, to).join('').trim();
	return `${from > 0 ? '…' : ''}${body}${to < chars.length ? '…' : ''}`;
}

/**
 * 규칙 목록을 검색어 + 태그로 거른다.
 *
 * - 검색어: slug / 태그 / 본문 중 **하나라도** 맞으면 통과 (대소문자 무시).
 * - 태그 필터: 선택한 태그를 **전부** 가져야 통과 (AND) — 기존 동작 유지.
 * - 검색어가 비면 태그 필터만 적용한다.
 *
 * 순서는 입력 순서를 유지한다 — 관련도 정렬은 하지 않는다(규칙은 9건 규모라
 * 훑는 비용이 낮고, 순서가 바뀌면 오히려 어디 있었는지 놓친다).
 */
export function filterRules(
	entries: RuleEntry[],
	query: string,
	tags: Set<string> = new Set()
): RuleSearchResult[] {
	const q = query.trim().toLowerCase();
	const out: RuleSearchResult[] = [];
	for (const entry of entries) {
		const eTags = entry.tags ?? [];
		// 태그 AND 필터가 먼저 — 검색어와 무관하게 걸린다.
		let tagOk = true;
		for (const t of tags) {
			if (!eTags.includes(t)) {
				tagOk = false;
				break;
			}
		}
		if (!tagOk) continue;
		if (!q) {
			out.push({ entry, matchedIn: [], excerpt: '' });
			continue;
		}
		const matchedIn: RuleMatchField[] = [];
		if (entry.slug.toLowerCase().includes(q)) matchedIn.push('slug');
		if (eTags.some((t) => t.toLowerCase().includes(q))) matchedIn.push('tag');
		let excerpt = '';
		const body = (entry.content ?? '').replace(/\s+/g, ' ');
		const at = body.toLowerCase().indexOf(q);
		if (at >= 0) {
			matchedIn.push('body');
			// 앞의 정규화와 같은 문자열에서 찾은 인덱스라 그대로 쓴다.
			excerpt = excerptAround(body, [...body.slice(0, at)].length);
		}
		if (matchedIn.length > 0) out.push({ entry, matchedIn, excerpt });
	}
	return out;
}
