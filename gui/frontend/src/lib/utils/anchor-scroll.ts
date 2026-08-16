import { flushSync } from 'svelte';

/**
 * DEV-359: 목록 항목이 **제자리에서 펼쳐질 때** 커서 밑이 흔들리지 않게 하는 장치.
 *
 * ## 무엇이 흔들리는가
 *
 * 커서가 A행에서 아래 B행으로 넘어가면 A는 접히고(-22px) B는 펼쳐진다(+22px).
 * A가 위에 있으므로 **B의 상단이 22px 위로 올라온다**. 커서는 화면상 고정점이라,
 * 방금 B의 상단부에 들어왔다면 그 지점은 이제 A의 영역이다 → 브라우저가 A에
 * `mouseenter` 를 쏘고 → A가 펼쳐지고 B가 접히고 → 다시 뒤집히는 핑퐁이 된다.
 * 핑퐁이 없어도 행을 넘을 때마다 커서 위쪽이 22px 씩 튄다.
 *
 * ## 왜 이전 시도가 실패했나
 *
 * 보정을 `requestAnimationFrame` 에서 했다. 브라우저는 그 프레임에 **이미 새
 * 레이아웃으로 hit-test 를 끝내고 hover 를 쏜 뒤**라 한 프레임 늦었다.
 *
 * ## 지금 방식
 *
 * 상태 변경을 `flushSync` 로 즉시 반영하고, **같은 이벤트 핸들러 안에서** 스크롤
 * 보정까지 끝낸다. 브라우저는 중간 상태를 그리지도, hit-test 하지도 않는다.
 * 이어지는 높이 애니메이션 동안에도 매 프레임 같은 기준으로 붙든다.
 */

/** 모션 축소 선호 — 전환을 아예 걸지 않는다. */
function reduceMotion(): boolean {
	return (
		typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
	);
}

/** 항목별 진행 중인 높이 애니메이션 — 뒷정리를 이어받은 쪽만 하도록 구분한다. */
const running = new WeakMap<HTMLElement, Animation>();

/**
 * 진행 중인 보정 루프 세대. 새 호출이 들어오면 이전 루프는 다음 프레임에 스스로
 * 물러난다 — 기준점이 다른 루프가 여럿 돌면 서로 `scrollTop` 을 밀며 싸운다.
 */
let anchorGeneration = 0;

/** `from → to` 높이 애니메이션. 접히는 쪽은 펼친 글자 배치를 유지한 채 잘라낸다. */
function animateHeight(el: HTMLElement, from: number, to: number, durationMs: number) {
	if (Math.abs(to - from) < 1) return;
	const prev = running.get(el);
	if (prev) {
		running.delete(el);
		prev.cancel();
	}
	// 접힐 때 높이만 줄이면 애니메이션이 보이지 않는다 — 펼침 클래스가 떨어지는
	// 순간 글자가 이미 한 줄로 돌아가, 남은 건 빈 공간이 줄어드는 것뿐이다.
	// `collapsing` 으로 펼친 배치를 유지하고 그 내용을 잘라내며 줄인다.
	const collapsing = to < from;
	if (collapsing) el.classList.add('collapsing');
	const overflow = el.style.overflow;
	el.style.overflow = 'hidden';
	const anim = el.animate([{ height: `${from}px` }, { height: `${to}px` }], {
		duration: durationMs,
		easing: 'ease-out'
	});
	running.set(el, anim);
	anim.finished
		.catch(() => {}) // cancel() 시 reject — 뒷정리는 이어받은 쪽이 한다.
		.finally(() => {
			if (running.get(el) !== anim) return;
			running.delete(el);
			el.style.overflow = overflow;
			el.classList.remove('collapsing');
		});
}

/** 애니메이션이 진행되는 동안 `row` 의 화면 위치를 `anchor` 에 붙들어 둔다. */
function followAnchor(scroller: HTMLElement, row: HTMLElement, anchor: number, durationMs: number) {
	const mine = ++anchorGeneration;
	const started = performance.now();
	const step = () => {
		if (mine !== anchorGeneration || !row.isConnected) return;
		const delta = row.getBoundingClientRect().top - anchor;
		// scrollTop 은 소수점에서 미세하게 어긋날 수 있어 반 픽셀 미만은 무시한다.
		if (Math.abs(delta) > 0.5) scroller.scrollTop += delta;
		if (performance.now() - started < durationMs) requestAnimationFrame(step);
	};
	requestAnimationFrame(step);
}

/**
 * 호버로 선택을 옮긴다 — 커서 밑 행이 움직이지 않도록 보정까지 한 번에.
 *
 * @param apply 선택 상태를 실제로 바꾸는 함수(`selIndex = i` 등)
 */
export function hoverSelect(opts: {
	scroller: HTMLElement | null | undefined;
	/** 지금 펼쳐져 있는 행(접힐 쪽). 없으면 생략. */
	prev: HTMLElement | null | undefined;
	/** 커서가 올라간 행(펼쳐질 쪽) — 이 행의 화면 위치를 붙든다. */
	next: HTMLElement | null | undefined;
	apply: () => void;
	durationMs?: number;
}) {
	const { scroller, prev, next, apply } = opts;
	const durationMs = opts.durationMs ?? 90;
	if (!scroller || !next) {
		apply();
		return;
	}
	const anchor = next.getBoundingClientRect().top;
	const prevFrom = prev?.getBoundingClientRect().height;
	const nextFrom = next.getBoundingClientRect().height;

	// DOM 을 즉시 반영시켜, 브라우저가 중간 상태를 그리기 전에 여기서 다 끝낸다.
	flushSync(apply);

	// 최종 높이를 재고 **곧바로** 애니메이션을 건다. 순서가 중요하다 — 먼저
	// 스크롤을 보정하고 나중에 애니메이션을 걸면, 애니메이션이 높이를 시작값으로
	// 되돌리면서 방금 맞춰둔 위치가 반대로 22px 어긋난다(실측으로 확인:
	// 보정 직후 커서 밑 행이 바뀌어 있었다).
	if (!reduceMotion()) {
		const prevTo = prev?.getBoundingClientRect().height;
		const nextTo = next.getBoundingClientRect().height;
		if (prev && prevFrom != null && prevTo != null) animateHeight(prev, prevFrom, prevTo, durationMs);
		animateHeight(next, nextFrom, nextTo, durationMs);
	}

	// 애니메이션까지 반영된 **지금 이 프레임의** 위치로 보정한다.
	const delta = next.getBoundingClientRect().top - anchor;
	if (Math.abs(delta) > 0.5) scroller.scrollTop += delta;

	// 높이가 자라는 동안에도 같은 기준으로 계속 붙든다.
	followAnchor(scroller, next, anchor, durationMs + 60);
}

/**
 * 키보드로 선택이 옮겨갈 때의 높이 전환. 커서가 개입하지 않으므로 스크롤 보정은
 * 하지 않는다(선택 행을 보이게 하는 스크롤은 호출측이 따로 처리).
 *
 * 클래스 변경 **직전에** 호출한다 — 바뀌기 전 높이를 재야 한다.
 */
export function animateSelectionChange(
	prev: HTMLElement | null | undefined,
	next: HTMLElement | null | undefined,
	durationMs = 90
) {
	if (reduceMotion()) return;
	const prevFrom = prev?.getBoundingClientRect().height;
	const nextFrom = next?.getBoundingClientRect().height;
	requestAnimationFrame(() => {
		if (prev?.isConnected && prevFrom != null) {
			animateHeight(prev, prevFrom, prev.getBoundingClientRect().height, durationMs);
		}
		if (next?.isConnected && nextFrom != null) {
			animateHeight(next, nextFrom, next.getBoundingClientRect().height, durationMs);
		}
	});
}

/**
 * **커서가 움직이지 않았는데 들어온 hover** 를 걸러낸다.
 *
 * 휠로 굴리는 동안에는 행이 커서 밑을 지나가며 hover 가 계속 들어온다. 선택이
 * 휠을 따라다니면 걸리는 느낌이 나므로, 사람이 커서를 움직였을 때만 받는다.
 * (레이아웃이 만들어내는 hover 도 같은 좌표로 들어오므로 함께 걸러진다.)
 */
let lastHoverX = NaN;
let lastHoverY = NaN;

/**
 * 사용자가 방금 굴렸는지. `scroll` 이벤트는 우리 보정(`scrollTop` 쓰기)에도
 * 발생해 구분이 안 되므로, **입력에서만 나오는** `wheel`/`touchmove` 를 본다.
 */
let lastWheelAt = -Infinity;
export function markUserScroll() {
	lastWheelAt = performance.now();
}

export function isPointerDrivenHover(ev: MouseEvent, scrollQuietMs = 200): boolean {
	// 굴리는 중에는 행이 커서 밑을 지나갈 뿐이므로 선택을 바꾸지 않는다.
	// (커서를 함께 움직이더라도 마찬가지 — 굴리는 동안 화면이 계속 바뀌면
	// 그게 곧 멈칫거림으로 느껴진다.)
	if (performance.now() - lastWheelAt < scrollQuietMs) return false;
	if (ev.clientX === lastHoverX && ev.clientY === lastHoverY) return false;
	lastHoverX = ev.clientX;
	lastHoverY = ev.clientY;
	return true;
}
