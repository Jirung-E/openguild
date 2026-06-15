/**
 * DEV-140 후속: <textarea> 용 cross-link 자동완성 보조.
 *
 * 댓글 편집기는 CodeMirror 가 아니라 <textarea> 라 editor-links.ts 의 CodeMirror
 * autocomplete 가 안 먹는다. 여기서는 커서 직전의 `XXX-NNN` 토큰을 감지하는 순수
 * 함수 + 토큰을 `[[ID]]` 로 치환하는 헬퍼만 제공하고, UI(제안 표시)는 호출 측
 * (QuestCommentsSection)이 클릭형 인플로우 제안으로 렌더한다. caret 픽셀 위치
 * 계산이나 Tab/Enter 키 가로채기(=tabInsert 충돌) 없이 견고.
 */
import type { IndexedRef } from '$lib/stores/questIndex';

/** 커서 직전 ID 토큰 — 앞이 `[`/단어/하이픈이 아니어야(이미 위키링크면 제외).
 *  editor-links 의 BEFORE_CURSOR 와 동일 패턴(prefix 2자+). */
const BEFORE_CURSOR = /(^|[^[\w-])([A-Za-z]{2,}-\d+)$/;

export interface WikiSuggestion {
	id: string;
	from: number;
	to: number;
	ref: IndexedRef | null;
}

/** value 의 caret 직전에 ID 토큰이 있으면 제안 정보를 반환, 없으면 null. */
export function wikiSuggestion(
	value: string,
	caret: number,
	index: Map<string, IndexedRef>
): WikiSuggestion | null {
	const before = value.slice(0, caret);
	const m = BEFORE_CURSOR.exec(before);
	if (!m) return null;
	const token = m[2];
	const id = token.toUpperCase();
	return { id, from: caret - token.length, to: caret, ref: index.get(id) ?? null };
}

/** textarea 의 [from,to] 토큰을 `[[ID]]` 로 치환 + input 이벤트(bind:value 동기화). */
export function applyWikiLink(ta: HTMLTextAreaElement, s: WikiSuggestion): void {
	ta.setRangeText(`[[${s.id}]]`, s.from, s.to, 'end');
	ta.dispatchEvent(new Event('input', { bubbles: true }));
	ta.focus();
}
