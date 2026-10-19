// DEV-443: 개인 형광펜을 **칠하는** 곳 — CSS Custom Highlight API(`::highlight(og-mark)`).
//
// 화면에는 형광펜이 든 글이 여럿 있다(본문 · 댓글마다 · 메모). 레지스트리 이름은 하나라서, 각자 자기 몫의
// Range 를 맡기고 여기서 하나로 합쳐 칠한다.
//
// BUG-268 과 같은 함정: WebKit 은 레지스트리의 Highlight 를 **바꿔 끼워도** 옛 자리를 다시 그리지 않는다.
// 그래서 Highlight 하나를 계속 두고, 칠할 때마다 **비운 뒤 다시 채운다**.

import { supportsHighlightApi } from './find-in-page';

const NAME = 'og-mark';

const owners = new Map<symbol, Range[]>();
let hl: Highlight | null = null;

/** 이 환경에서 칠할 수 있나 — 없으면 형광펜 기능 전체를 숨긴다(칠해도 안 보이니). */
export function canPaint(): boolean {
	return supportsHighlightApi();
}

function flush() {
	if (!canPaint()) return;
	try {
		if (!hl) {
			hl = new Highlight();
			// 찾기(Ctrl+F)와 겹치면 찾기가 보여야 한다 — 지금 찾는 글이 어디인지가 더 급하다.
			hl.priority = -1;
			CSS.highlights.set(NAME, hl);
		}
		hl.clear();
		for (const ranges of owners.values()) for (const r of ranges) hl.add(r);
	} catch {
		/* 지원이 반쯤인 환경 — 칠하지 못해도 글은 그대로다. */
	}
}

/** `owner` 의 몫을 바꿔 칠한다. */
export function paintRanges(owner: symbol, ranges: Range[]) {
	owners.set(owner, ranges);
	flush();
}

/** `owner` 의 몫을 지운다(글이 화면에서 사라질 때). */
export function clearRanges(owner: symbol) {
	if (!owners.delete(owner)) return;
	flush();
}
