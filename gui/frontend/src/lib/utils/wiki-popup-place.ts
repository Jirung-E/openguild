/**
 * DEV-172: cross-link 자동완성 팝업의 화면 배치 계산 — DEV-171(댓글 textarea)
 * 에서 만든 로직을 본문 편집기(CodeMirror)와 공유하기 위해 순수 함수로 추출.
 *
 * 입력은 caret 의 화면 좌표(caretTop/caretBottom, viewport 기준)뿐이라 textarea /
 * CodeMirror 둘 다 동일하게 쓸 수 있다 — 각자 caret 좌표를 얻는 방법만 다르다
 * (textarea = mirror-div 측정, CodeMirror = `view.coordsAtPos`).
 */

/** 뷰포트 가장자리 여백 — 좌우/상하 clamp 에 공통 사용. */
export const WIKI_MARGIN = 8;
/** CSS 의 상한(14rem)과 같은 값 — 공간이 남아도 이 이상은 키우지 않는다. */
export const WIKI_MAX_H = 224;

export interface WikiPlace {
	/** top 배치일 때만 값(px) — 아니면 null(=bottom 배치). */
	top: number | null;
	/** bottom 배치일 때만 값(px) — 아니면 null(=top 배치). */
	bottom: number | null;
	maxH: number;
}

/**
 * BUG-209: 배치는 "추정" 이 아니라 "측정". 아래 공간에 실제 높이가 안 들어가고
 * 위가 더 넓으면 위로 띄우고, 어느 쪽이든 남은 공간만큼 max-height 를 물려
 * 팝업이 뷰포트를 벗어나지 못하게 한다. 항목이 펼쳐져 높이가 변해도 스스로
 * 다시 맞춰지므로 같은 문제가 재발하지 않는다.
 *
 * @param caretTop/caretBottom  caret 의 viewport 기준 상/하단 좌표.
 * @param itemCount  후보 개수 — 아직 렌더 전(measuredH=0)일 때 높이 어림잡기용.
 * @param measuredH  팝업의 실제 콘텐츠 높이(ResizeObserver 로 측정, max-height 로
 *   잘리기 전 값). 0 이면 itemCount 로 대략 추정.
 */
export function computeWikiPlace(
	caretTop: number,
	caretBottom: number,
	itemCount: number,
	measuredH: number,
	viewportH: number = window.innerHeight
): WikiPlace {
	const below = viewportH - caretBottom - WIKI_MARGIN;
	const above = caretTop - WIKI_MARGIN;
	// 아직 렌더 전이면 항목 수로 어림잡되, 렌더 직후 측정값으로 교체된다.
	const h = measuredH || Math.min(itemCount * 30 + 8, WIKI_MAX_H);
	const flipUp = h > below && above > below;
	const space = flipUp ? above : below;
	// 너무 납작해지면 오히려 못 쓰므로 하한(2항목분)은 둔다.
	// 45vh 는 기존 모바일 CSS 상한과 같은 뜻 — 화면 절반 이상 덮지 않기.
	const maxH = Math.max(68, Math.min(WIKI_MAX_H, space, viewportH * 0.45));
	return flipUp
		? { top: null, bottom: viewportH - caretTop, maxH }
		: { top: caretBottom, bottom: null, maxH };
}

/**
 * 팝업의 실제 폭(CSS 상한 22rem=352px, 뷰포트에 못 미치면 그만큼)만큼 좌우
 * 여백 안으로 clamp. rawLeft 는 caret 의 viewport 기준 left 좌표.
 */
export function clampWikiLeft(rawLeft: number, viewportW: number = window.innerWidth): number {
	const popW = Math.min(352, viewportW - WIKI_MARGIN * 2);
	return Math.max(WIKI_MARGIN, Math.min(rawLeft, viewportW - popW - WIKI_MARGIN));
}

/**
 * caret(자동완성 대상)이 편집기의 "보이는" 영역(∩ 뷰포트) 밖이면 팝업을 숨겨야
 * 한다 — 편집기 자체가 스크롤돼 caret 이 화면 밖으로 나간 경우.
 * `editorRectTop`/`editorRectBottom` 은 편집기 DOM(`getBoundingClientRect()`)의
 * viewport 기준 상/하단.
 */
export function isWikiCaretVisible(
	caretTop: number,
	caretBottom: number,
	editorRectTop: number,
	editorRectBottom: number,
	viewportH: number = window.innerHeight
): boolean {
	const visTop = Math.max(editorRectTop, 0);
	const visBottom = Math.min(editorRectBottom, viewportH);
	return !(caretBottom < visTop || caretTop > visBottom);
}
