/**
 * DEV-140 / DEV-171: <textarea> 용 cross-link 자동완성 보조.
 *
 * 댓글 편집기는 CodeMirror 가 아니라 <textarea> 라 editor-links.ts 의 autocomplete
 * 가 안 먹는다. 여기서는 (1) 커서 직전 `XXX-NNN` 토큰의 prefix 로 실재 ID 후보
 * 목록을 만드는 순수 함수, (2) 토큰을 `[[ID]]` 로 치환하는 헬퍼, (3) textarea 안
 * caret 의 픽셀 좌표(mirror-div) 를 제공한다. 제안 팝업 UI 는 호출 측이 렌더.
 */
import type { IndexedRef } from '$lib/stores/questIndex';

/** DEV-173: `[[` 바로 안(아직 안 닫힘)의 부분 slug — 규칙 포함 전체 인덱스 제안.
 *  규칙 slug 는 한글 등 비ASCII 가능 — 공백/대괄호 제외 모든 문자 허용.
 *  DEV-220(사용자 결정): bare 토큰(XXX-NNN 그냥 타이핑) 트리거는 제거 —
 *  자동완성은 `[[` 컨텍스트에서만. */
const BEFORE_CURSOR_WIKI = /\[\[([^[\]\s]*)$/;

export interface WikiItem {
	id: string;
	title: string | null;
	kind: 'quest' | 'campaign' | 'rule' | 'book' | null;
	/** 실재 ID 인지 (false = '새 링크' 후보, 렌더 시 빨강). */
	exists: boolean;
	/** 삽입 텍스트 (규칙 = 원본 대소문자 slug). 미지정 시 id. */
	insert?: string;
}

export interface WikiMatch {
	/** 치환 대상 토큰 범위 [from, to). */
	from: number;
	to: number;
	items: WikiItem[];
	/** DEV-173: `[[` 컨텍스트 매칭 여부 — 치환 범위가 `[[` 를 포함. */
	wikiContext?: boolean;
}

/** caret 직전 ID 토큰의 prefix 로 실재 ID 후보 목록 생성. 없으면 null. */
export function wikiMatch(
	value: string,
	caret: number,
	index: Map<string, IndexedRef>
): WikiMatch | null {
	const before = value.slice(0, caret);

	// DEV-173: `[[` 컨텍스트 — 규칙 포함 전체 인덱스에서 prefix 매칭.
	// 치환 범위는 `[[` 부터 (applyWikiLink 가 `[[id]]` 로 통째 치환).
	const w = BEFORE_CURSOR_WIKI.exec(before);
	if (w) {
		const partial = w[1];
		// DEV-223: 빈 `[[` 에서도 전체 후보 표시 (사용자 결정).
		const upper = partial.toUpperCase();
		const items: WikiItem[] = [];
		for (const [id, ref] of index) {
			if (!id.startsWith(upper)) continue;
			items.push({
				id,
				title: ref.title,
				kind: ref.kind,
				exists: true,
				insert: ref.kind === 'rule' ? (ref.slug ?? id.toLowerCase()) : id
			});
		}
		if (items.length === 0) return null;
		items.sort((a, b) => a.id.localeCompare(b.id));
		return { from: caret - partial.length - 2, to: caret, items, wikiContext: true };
	}

	return null;
}

/** textarea 의 [from,to] 토큰을 `[[ID]]` 로 치환.
 *  DEV-171 후속: execCommand('insertText') 로 삽입해 브라우저 undo 스택 보존
 *  (Ctrl+Z 동작). setRangeText 는 undo 히스토리를 끊어 자동완성 입력이 되돌려지지
 *  않던 문제. execCommand 실패 시에만 setRangeText 로 fallback. */
export function applyWikiLink(ta: HTMLTextAreaElement, from: number, to: number, id: string): void {
	// DEV-223: 치환 범위 바로 뒤에 이미 `]]` 가 있으면(닫힌 위키링크 안에서 재완성
	// 등) 함께 삼켜 [[id]]]] 중복을 방지.
	const to2 = ta.value.slice(to, to + 2) === ']]' ? to + 2 : to;
	const text = `[[${id}]]`;
	ta.focus();
	ta.setSelectionRange(from, to2);
	// execCommand('insertText') 는 선택 영역을 치환하며 input 이벤트도 자동 발화.
	if (document.execCommand && document.execCommand('insertText', false, text)) {
		return;
	}
	// fallback (undo 끊김).
	ta.setRangeText(text, from, to2, 'end');
	ta.dispatchEvent(new Event('input', { bubbles: true }));
}

// mirror-div 로 복제할 스타일 (caret 좌표 계산용).
const MIRROR_PROPS = [
	'boxSizing',
	'width',
	'borderTopWidth',
	'borderRightWidth',
	'borderBottomWidth',
	'borderLeftWidth',
	'borderStyle',
	'paddingTop',
	'paddingRight',
	'paddingBottom',
	'paddingLeft',
	'fontStyle',
	'fontVariant',
	'fontWeight',
	'fontStretch',
	'fontSize',
	'lineHeight',
	'fontFamily',
	'textAlign',
	'textTransform',
	'textIndent',
	'letterSpacing',
	'wordSpacing',
	'tabSize',
	'whiteSpace',
	'wordWrap',
	'wordBreak'
] as const;

/**
 * textarea 안 `pos` 의 caret 좌표(텍스트영역 border-box 기준 left/top) + 줄 높이.
 * mirror div 에 동일 스타일 + caret 까지의 텍스트를 넣어 측정.
 */
export function caretXY(
	ta: HTMLTextAreaElement,
	pos: number
): { left: number; top: number; height: number } {
	const cs = window.getComputedStyle(ta);
	const div = document.createElement('div');
	const s = div.style;
	s.position = 'absolute';
	s.visibility = 'hidden';
	s.whiteSpace = 'pre-wrap';
	s.overflow = 'hidden';
	for (const p of MIRROR_PROPS) {
		s[p as never] = cs[p as never];
	}
	// 높이는 내용에 맞게 — width 만 textarea 와 동일.
	s.height = 'auto';
	div.textContent = ta.value.slice(0, pos);
	const span = document.createElement('span');
	span.textContent = ta.value.slice(pos) || '.';
	div.appendChild(span);
	document.body.appendChild(div);
	const left = span.offsetLeft + parseFloat(cs.borderLeftWidth);
	const top = span.offsetTop + parseFloat(cs.borderTopWidth);
	const height = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.4;
	document.body.removeChild(div);
	return { left, top, height };
}
