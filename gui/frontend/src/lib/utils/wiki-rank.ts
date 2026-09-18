// DEV-418: `[[` 자동완성이 **무엇으로 찾는지**를 한 곳에서 정한다.
//
// 예전에는 관리번호(slug)로만 찾았다. 도서관 문서만 예외로 `폴더/제목` 의 **앞부분**을 봤고,
// 퀘스트·캠페인·규칙은 제목으로 아예 못 찾았다. 번호를 외우지 못하면(대부분 그렇다) 자동완성이
// 쓸모가 없었다.
//
// 이제 **제목으로도, 중간 글자로도** 찾는다. 대신 정확도 순으로 줄을 세운다 — 번호를 아는
// 사람은 그 번호를 정확히 원하기 때문에, 번호로 맞은 것이 제목으로 맞은 것보다 위에 온다.
//
// 편집기 두 곳(CodeMirror 의 `editor-links.ts`, 댓글 textarea 의 `textarea-wikilink.ts`)이
// 같은 규칙을 써야 해서 여기로 뺐다. 예전에는 같은 판정이 두 파일에 복사돼 있었다.

import type { IndexedRef } from '$lib/stores/questIndex';

/** 낮을수록 위. 안 맞으면 `null`. */
export const RANK_ID_PREFIX = 0;
export const RANK_TITLE_PREFIX = 1;
export const RANK_ID_PART = 2;
export const RANK_TITLE_PART = 3;

/** 도서관 문서는 "폴더/제목" 으로도 찾을 수 있어야 한다(DEV-239). */
function titlesOf(ref: IndexedRef): string[] {
	const out: string[] = [];
	if (ref.title) out.push(ref.title);
	if (ref.kind === 'book' && ref.path) out.push(`${ref.path}/${ref.title}`);
	// 규칙은 slug 자체가 사람이 읽는 이름이다.
	if (ref.kind === 'rule' && ref.slug) out.push(ref.slug);
	return out;
}

/**
 * 이 후보가 질의에 얼마나 맞나. 안 맞으면 `null`.
 *
 * 대소문자는 무시한다. 한글에는 대소문자가 없지만 `toLocaleLowerCase` 를 그대로 태워도
 * 안전하고, 영문 제목이 섞여도 같은 규칙으로 걸린다.
 */
export function wikiRank(id: string, ref: IndexedRef, query: string): number | null {
	const q = query.trim();
	// 빈 `[[` 에서는 전부 보여 준다(DEV-223).
	if (!q) return RANK_ID_PREFIX;
	const upper = q.toUpperCase();
	const lower = q.toLocaleLowerCase();
	const idUpper = id.toUpperCase();
	if (idUpper.startsWith(upper)) return RANK_ID_PREFIX;

	const titles = titlesOf(ref).map((t) => t.toLocaleLowerCase());
	if (titles.some((t) => t.startsWith(lower))) return RANK_TITLE_PREFIX;
	if (idUpper.includes(upper)) return RANK_ID_PART;
	if (titles.some((t) => t.includes(lower))) return RANK_TITLE_PART;
	return null;
}

/** 목록 정렬 — 정확도 먼저, 같으면 번호순. */
export function byRankThenId(
	a: { rank: number; id: string },
	b: { rank: number; id: string }
): number {
	return a.rank !== b.rank ? a.rank - b.rank : a.id.localeCompare(b.id);
}
