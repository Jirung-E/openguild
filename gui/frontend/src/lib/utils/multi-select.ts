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
		return { ids: new Set(order.slice(from, to + 1)), anchor: state.anchor };
	}
	if (mods.toggle) {
		const ids = new Set(state.ids);
		if (ids.has(id)) ids.delete(id);
		else ids.add(id);
		return { ids, anchor: id };
	}
	return { ids: new Set([id]), anchor: id };
}

/** 빈 곳을 눌렀을 때 — 전부 푼다. */
export function clear(): Picked {
	return { ids: new Set(), anchor: null };
}

/** `⌘/Ctrl+A`. */
export function all(order: string[]): Picked {
	return { ids: new Set(order), anchor: order.at(-1) ?? null };
}

/**
 * 목록이 바뀐 뒤(문서를 옮겼거나 지웠거나) 사라진 것을 떨군다. 안 그러면 없는 것을 고른 채로
 * "3개 옮기기" 같은 말을 하게 된다.
 */
export function prune(state: Picked, order: string[]): Picked {
	const live = new Set(order);
	const ids = new Set([...state.ids].filter((id) => live.has(id)));
	const anchor = state.anchor && live.has(state.anchor) ? state.anchor : null;
	return { ids, anchor };
}
