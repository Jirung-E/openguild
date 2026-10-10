// DEV-419: 목록에서 **여러 개 고르기** — 파일 탐색기와 같은 규칙.
//
// 도서관 아이콘 뷰는 한 번 누르면 바로 열렸다. 그래서 "이 셋을 저 폴더로" 같은 일을 하려면
// 하나씩 열고 닫으며 옮겨야 했다. 탐색기처럼 **누르면 고르고, 두 번 누르면 연다**(admin).
//
// 고르는 규칙만 여기 둔다 — 화면을 그리지 않으니 시험이 쉽고, 트리·아이콘 어느 쪽에서 써도
// 같은 규칙이 된다. 규칙은 macOS Finder / Windows 탐색기가 둘 다 쓰는 것으로 맞췄다.
//
//   그냥 누름      그것만 고른다 (기준점도 그것)
//   ⌘/Ctrl + 누름  그것만 토글 (기준점은 방금 누른 것)
//   Shift + 누름   기준점부터 여기까지 (기준점은 그대로 — 계속 늘렸다 줄일 수 있게)

export interface Picked {
	/** 고른 것들. 순서는 목록 순서를 따로 보면 되므로 여기서는 안 지킨다. */
	ids: Set<string>;
	/** 범위 선택의 기준점. 없으면 Shift 는 그냥 누름처럼 동작한다. */
	anchor: string | null;
	/** DEV-422: 방향키가 출발하는 자리 — 마지막으로 누르거나 옮겨 간 것. 없으면 기준점에서. */
	lead?: string | null;
}

export const EMPTY: Picked = { ids: new Set(), anchor: null };

export interface ClickMods {
	/** macOS 는 ⌘, 그 외는 Ctrl — 호출부가 하나로 정리해서 넘긴다. */
	toggle?: boolean;
	range?: boolean;
}

/**
 * 한 번의 클릭을 새 선택 상태로. `order` 는 **화면에 보이는 순서**여야 한다(범위 선택이
 * 그 순서를 따른다 — 정렬을 바꾸면 범위도 그 정렬을 따라야 자연스럽다).
 */
export function click(state: Picked, id: string, mods: ClickMods, order: string[]): Picked {
	if (mods.range && state.anchor && order.includes(state.anchor) && order.includes(id)) {
		const a = order.indexOf(state.anchor);
		const b = order.indexOf(id);
		const [from, to] = a <= b ? [a, b] : [b, a];
		// 기준점은 **그대로 둔다** — Shift 로 늘렸다 줄였다 할 수 있어야 한다.
		return { ids: new Set(order.slice(from, to + 1)), anchor: state.anchor, lead: id };
	}
	if (mods.toggle) {
		const ids = new Set(state.ids);
		if (ids.has(id)) ids.delete(id);
		else ids.add(id);
		return { ids, anchor: id, lead: id };
	}
	return { ids: new Set([id]), anchor: id, lead: id };
}

/** 빈 곳을 눌렀을 때 — 전부 푼다. */
export function clear(): Picked {
	return { ids: new Set(), anchor: null };
}

/** `⌘/Ctrl+A`. */
export function all(order: string[]): Picked {
	const last = order.at(-1) ?? null;
	return { ids: new Set(order), anchor: last, lead: last };
}

/**
 * 목록이 바뀐 뒤(문서를 옮겼거나 지웠거나) 사라진 것을 떨군다. 안 그러면 없는 것을 고른 채로
 * "3개 옮기기" 같은 말을 하게 된다.
 */
export function prune(state: Picked, order: string[]): Picked {
	const live = new Set(order);
	const ids = new Set([...state.ids].filter((id) => live.has(id)));
	const anchor = state.anchor && live.has(state.anchor) ? state.anchor : null;
	const lead = state.lead && live.has(state.lead) ? state.lead : null;
	return { ids, anchor, lead };
}

export type Arrow = 'left' | 'right' | 'up' | 'down';

/**
 * DEV-422: 방향키. `order` 는 화면 순서, `cols` 는 격자 한 줄에 놓인 칸 수(아이콘 보기는
 * 폴더와 문서가 한 격자에 이어서 놓이므로 순서만 알면 위아래도 계산된다).
 *
 * - 아무것도 안 골랐으면 첫 칸을 고른다.
 * - 끝에서 더 가려고 하면 그 자리에 머문다(위아래도 — 아래 줄이 모자라면 안 움직인다).
 * - `extend`(Shift) 면 기준점은 그대로 두고 범위를 늘리거나 줄인다 — Shift+누름과 같다.
 */
export function arrow(state: Picked, order: string[], cols: number, dir: Arrow, extend = false): Picked {
	if (order.length === 0) return state;
	const from = state.lead ?? state.anchor;
	const cur = from ? order.indexOf(from) : -1;
	if (cur < 0) return { ids: new Set([order[0]]), anchor: order[0], lead: order[0] };
	const step = { left: -1, right: 1, up: -Math.max(1, cols), down: Math.max(1, cols) }[dir];
	const at = cur + step;
	const next = order[at >= 0 && at < order.length ? at : cur];
	if (extend) {
		const anchor = state.anchor && order.includes(state.anchor) ? state.anchor : order[cur];
		const a = order.indexOf(anchor);
		const b = order.indexOf(next);
		const [lo, hi] = a <= b ? [a, b] : [b, a];
		return { ids: new Set(order.slice(lo, hi + 1)), anchor, lead: next };
	}
	return { ids: new Set([next]), anchor: next, lead: next };
}

export interface Box {
	left: number;
	top: number;
	right: number;
	bottom: number;
}

/** DEV-422: 끌어서 그린 사각형에 **조금이라도 걸친** 것들 — 순서는 `items` 순서 그대로. */
export function hitsIn(items: readonly (Box & { id: string })[], box: Box): string[] {
	return items
		.filter((r) => r.left < box.right && r.right > box.left && r.top < box.bottom && r.bottom > box.top)
		.map((r) => r.id);
}

/**
 * DEV-422: 사각형 선택. `base` 는 **끌기 시작할 때의** 선택이고, 수식키도 시작할 때 것으로
 * 정한다 — 끄는 도중에 Shift/⌘ 를 눌렀다 떼도 결과가 출렁이지 않게.
 *
 *   replace  지나간 것만
 *   add      (Shift) 원래 것 + 지나간 것
 *   toggle   (⌘/Ctrl) 원래 것에서 지나간 것을 뒤집는다 — Finder 와 같다
 */
export function marquee(base: Picked, hits: string[], mode: 'replace' | 'add' | 'toggle'): Picked {
	let ids: Set<string>;
	if (mode === 'replace') ids = new Set(hits);
	else if (mode === 'add') ids = new Set([...base.ids, ...hits]);
	else {
		ids = new Set(base.ids);
		for (const h of hits) {
			if (base.ids.has(h)) ids.delete(h);
			else ids.add(h);
		}
	}
	const anchor = mode === 'replace' ? (hits[0] ?? null) : (base.anchor ?? hits[0] ?? null);
	return { ids, anchor, lead: hits.at(-1) ?? base.lead ?? anchor };
}
