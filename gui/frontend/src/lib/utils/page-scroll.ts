// BUG-257: 페이지 스크롤 컨테이너는 문서가 아니라 `<main>` 이다.
//
// macOS(WKWebView)는 맨 위에서 위로 더 스크롤하면 문서를 고무줄처럼 당긴다
// (rubber-band). 그때 **문서 안의 모든 것**이 함께 내려온다 — `position: sticky`
// 는 물론 `fixed` 도 딸려간다(실기 확인). 그래서 타이틀바가 창 위쪽에서 떨어져
// 보였다.
//
// 위치 지정으로는 못 막는다. 문서가 아예 스크롤하지 않으면 문서 바운스도
// 없으므로, `html/body` 를 `overflow: hidden` 으로 고정하고 `<main>` 만
// 스크롤하게 한다. 크롬(타이틀바·Nav)은 문서 흐름에 그대로 있지만 문서가
// 움직이지 않으니 절대 흔들리지 않고, 바운스는 `main` 안에서만 일어난다.
//
// 그 대가로 `window.scrollY` / `window.scrollTo` 가 전부 무의미해진다. 흩어져
// 있던 그 호출들이 각자 다른 대상을 보면 스크롤 복원이 조용히 깨지므로,
// **여기 한 곳**을 거치게 한다.

/**
 * 스크롤 컨테이너. `<main>` 이 아직 없으면(마운트 전, 자식 창 등) `null`.
 *
 * 캐시하지 않는다 — `main` 은 layout 에 하나뿐이라 조회가 싸고, 캐시하면
 * 자식 창이나 재마운트에서 낡은 노드를 붙들 수 있다.
 */
export function pageScrollEl(): HTMLElement | null {
	if (typeof document === 'undefined') return null;
	return document.querySelector('main');
}

/** 현재 스크롤 위치. 컨테이너가 없으면 문서 기준으로 물러선다. */
export function pageScrollTop(): number {
	const el = pageScrollEl();
	if (el) return el.scrollTop;
	return typeof window === 'undefined' ? 0 : window.scrollY;
}

/** 스크롤 이동. `smooth` 는 '맨 위로' 버튼처럼 의도된 애니메이션에만. */
export function scrollPageTo(y: number, smooth = false): void {
	const el = pageScrollEl();
	if (el) {
		el.scrollTo({ top: y, left: 0, behavior: smooth ? 'smooth' : 'auto' });
		return;
	}
	if (typeof window !== 'undefined') {
		window.scrollTo({ top: y, left: 0, behavior: smooth ? 'smooth' : 'auto' });
	}
}

/** 컨텐츠 전체 높이 — 복원 목표에 도달 가능한지 판단할 때 쓴다. */
export function pageScrollHeight(): number {
	const el = pageScrollEl();
	if (el) return el.scrollHeight;
	return typeof document === 'undefined' ? 0 : document.documentElement.scrollHeight;
}

/** 보이는 높이. */
export function pageViewportHeight(): number {
	const el = pageScrollEl();
	if (el) return el.clientHeight;
	return typeof window === 'undefined' ? 0 : window.innerHeight;
}

/**
 * 스크롤 이벤트 구독. 해제 함수를 돌려준다.
 *
 * 컨테이너 스크롤은 **window 로 버블하지 않는다** — `window.addEventListener
 * ('scroll')` 로 두면 조용히 아무 일도 안 일어난다.
 */
export function onPageScroll(handler: () => void): () => void {
	const el = pageScrollEl();
	const target: EventTarget | null = el ?? (typeof window === 'undefined' ? null : window);
	if (!target) return () => {};
	target.addEventListener('scroll', handler, { passive: true });
	return () => target.removeEventListener('scroll', handler);
}
