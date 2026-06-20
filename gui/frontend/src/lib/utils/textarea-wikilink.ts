/**
 * DEV-140 / DEV-171: <textarea> 용 cross-link 자동완성 보조.
 *
 * 댓글 편집기는 CodeMirror 가 아니라 <textarea> 라 editor-links.ts 의 autocomplete
 * 가 안 먹는다. 여기서는 (1) 커서 직전 `XXX-NNN` 토큰의 prefix 로 실재 ID 후보
 * 목록을 만드는 순수 함수, (2) 토큰을 `[[ID]]` 로 치환하는 헬퍼, (3) textarea 안
 * caret 의 픽셀 좌표(mirror-div) 를 제공한다. 제안 팝업 UI 는 호출 측이 렌더.
 */
import type { IndexedRef } from '$lib/stores/questIndex';

/** 커서 직전 ID 토큰 — 앞이 `[`/단어/하이픈이 아니어야(이미 위키링크면 제외). */
const BEFORE_CURSOR = /(^|[^[\w-])([A-Za-z]{2,}-\d+)$/;
const MAX_ITEMS = 20;

export interface WikiItem {
	id: string;
	title: string | null;
	kind: 'quest' | 'campaign' | null;
	/** 실재 ID 인지 (false = '새 링크' 후보, 렌더 시 빨강). */
	exists: boolean;
}

export interface WikiMatch {
	/** 치환 대상 토큰 범위 [from, to). */
	from: number;
	to: number;
	items: WikiItem[];
}

/** caret 직전 ID 토큰의 prefix 로 실재 ID 후보 목록 생성. 없으면 null. */
export function wikiMatch(
	value: string,
	caret: number,
	index: Map<string, IndexedRef>
): WikiMatch | null {
	const before = value.slice(0, caret);
	const m = BEFORE_CURSOR.exec(before);
	if (!m) return null;
	const token = m[2];
	const upper = token.toUpperCase();

	const items: WikiItem[] = [];
	for (const [id, ref] of index) {
		if (id.startsWith(upper)) {
			items.push({ id, title: ref.title, kind: ref.kind, exists: true });
		}
	}
	items.sort((a, b) => a.id.localeCompare(b.id));
	if (items.length > MAX_ITEMS) items.length = MAX_ITEMS;
	// 정확히 그 토큰이 실재하지 않으면 '새(미존재) 링크' 후보도 제공.
	if (!index.has(upper)) {
		items.push({ id: upper, title: null, kind: null, exists: false });
	}
	if (items.length === 0) return null;
	return { from: caret - token.length, to: caret, items };
}

/** textarea 의 [from,to] 토큰을 `[[ID]]` 로 치환 + input 이벤트(bind:value 동기화). */
export function applyWikiLink(
	ta: HTMLTextAreaElement,
	from: number,
	to: number,
	id: string
): void {
	ta.setRangeText(`[[${id}]]`, from, to, 'end');
	ta.dispatchEvent(new Event('input', { bubbles: true }));
	ta.focus();
}

// mirror-div 로 복제할 스타일 (caret 좌표 계산용).
const MIRROR_PROPS = [
	'boxSizing', 'width', 'borderTopWidth', 'borderRightWidth', 'borderBottomWidth',
	'borderLeftWidth', 'borderStyle', 'paddingTop', 'paddingRight', 'paddingBottom',
	'paddingLeft', 'fontStyle', 'fontVariant', 'fontWeight', 'fontStretch', 'fontSize',
	'lineHeight', 'fontFamily', 'textAlign', 'textTransform', 'textIndent',
	'letterSpacing', 'wordSpacing', 'tabSize', 'whiteSpace', 'wordWrap', 'wordBreak'
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
