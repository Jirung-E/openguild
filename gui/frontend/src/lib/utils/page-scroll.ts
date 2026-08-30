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

/**
 * 레이아웃이 잦아든 뒤에 대상으로 스크롤한다. 취소 함수를 돌려준다.
 *
 * BUG-258: `?comment=N` 딥링크가 엉뚱한 곳에 서던 문제.
 *
 * 부드러운 스크롤(`behavior: 'smooth'`)은 **시작 시점에 계산한 목표 오프셋**을
 * 향해 애니메이션한다. 그런데 퀘스트 상세는 본문 마크다운 → 첨부 → 서브퀘스트
 * → 댓글 순으로 늦게 레이아웃되고, 딥링크는 앵커가 **생기자마자** 스크롤을
 * 시작한다. 애니메이션이 도는 동안 위쪽이 자라면 목표 숫자는 그대로인데 대상은
 * 아래로 밀려, 엉뚱한 곳에 선다(밀림이 크면 끝까지 간 것처럼 보인다).
 *
 * 그래서 두 단계로 나눈다.
 *
 *  1. **기다린다** — 컨테이너 `scrollHeight` 가 `settleMs` 동안 안 변하면
 *     그때 스크롤한다. 계속 자라기만 하면 `maxWaitMs` 에서 포기하고 간다.
 *  2. **지켜본다** — 이후 `watchMs` 동안 높이가 또 변하면 다시 맞춘다. 이때는
 *     `'auto'` 다. 애니메이션 중에 또 애니메이션을 걸면 서로 싸운다.
 *
 * rAF 가 아니라 타이머를 쓴다 — 숨겨진 문서(배경 자식 창)에서는 rAF 가 아예
 * 발화하지 않아 루프가 첫 회에서 영구히 멈춘다([[BUG-238]], [[BUG-257]]에서
 * 각각 한 번씩 밟았다).
 *
 * @param getEl 매번 다시 찾는다 — 재렌더로 노드가 갈릴 수 있다.
 */
export function scrollIntoViewWhenSettled(
	getEl: () => HTMLElement | null,
	opts?: {
		settleMs?: number;
		maxWaitMs?: number;
		watchMs?: number;
		pollMs?: number;
		smooth?: boolean;
		/** 실제로 스크롤을 건 순간 한 번 불린다(강조 표시 등). */
		onScrolled?: (el: HTMLElement) => void;
	}
): () => void {
	const settleMs = opts?.settleMs ?? 150;
	const maxWaitMs = opts?.maxWaitMs ?? 1500;
	const watchMs = opts?.watchMs ?? 1500;
	const pollMs = opts?.pollMs ?? 32;
	const smooth = opts?.smooth ?? true;

	let cancelled = false;
	let timer: ReturnType<typeof setTimeout> | null = null;
	const startedAt = Date.now();
	let lastH = pageScrollHeight();
	let stableSince = startedAt;
	let scrolledAt: number | null = null;

	const go = (behavior: ScrollBehavior) => {
		const el = getEl();
		if (!el) return false;
		el.scrollIntoView({ behavior, block: 'center' });
		return true;
	};

	const tick = () => {
		if (cancelled) return;
		timer = null;
		const now = Date.now();
		const h = pageScrollHeight();
		if (h !== lastH) {
			lastH = h;
			stableSince = now;
		}

		if (scrolledAt === null) {
			const settled = now - stableSince >= settleMs;
			const gaveUp = now - startedAt >= maxWaitMs;
			if (!settled && !gaveUp) {
				timer = setTimeout(tick, pollMs);
				return;
			}
			const el = getEl();
			if (!el) {
				// 아직 안 그려졌으면 계속 기다린다 — 호출측이 앵커를 기다리는
				// 책임을 지지만, 그 사이 사라졌다 다시 생기는 경우도 있다.
				if (gaveUp) return;
				timer = setTimeout(tick, pollMs);
				return;
			}
			el.scrollIntoView({ behavior: smooth ? 'smooth' : 'auto', block: 'center' });
			opts?.onScrolled?.(el);
			scrolledAt = now;
			timer = setTimeout(tick, pollMs);
			return;
		}

		// 2단계 — 스크롤한 뒤에도 높이가 변하면 다시 맞춘다.
		if (now - scrolledAt >= watchMs) return;
		if (now - stableSince < pollMs) go('auto');
		timer = setTimeout(tick, pollMs);
	};

	timer = setTimeout(tick, 0);

	return () => {
		cancelled = true;
		if (timer !== null) clearTimeout(timer);
		timer = null;
	};
}
