import { flushSync } from 'svelte';

/**
 * DEV-359: 목록 항목이 **제자리에서 펼쳐질 때** 생기는 흔들림을 다루는 장치.
 *
 * ## 무엇이 문제인가
 *
 * 커서가 A행에서 아래 B행으로 넘어가면 A는 접히고(-22px) B는 펼쳐진다(+22px).
 * A가 위에 있으므로 **B의 상단이 22px 위로 올라온다**. 커서는 화면상 고정점이라
 * 방금 B의 상단부에 들어왔다면 그 지점이 A의 영역이 되고, 브라우저가 A에
 * `mouseenter` 를 쏴 두 항목이 핑퐁한다.
 *
 * ## 22px 을 어디로 보낼 것인가 (admin 결정)
 *
 * 갈 곳은 셋뿐이다 — (1) 스크롤로 흡수, (2) 흡수하지 않음, (3) 애초에 줄지
 * 않게 함. **(2)** 를 택했다. (1)은 커서 밑을 고정해 주지만 아래로 훑을 때마다
 * 보정이 한 방향으로 쌓여 목록이 통째로 밀려 올라간다(행마다 22px). (3)은
 * 밀도를 깎거나 펼친 행을 여럿 남긴다.
 *
 * 그래서 **스크롤은 건드리지 않는다.** 커서 밑 행은 22px 올라오지만, 그로 인해
 * 들어오는 hover 는 `isPointerDrivenHover` 가 좌표로 걸러내므로(커서가 안
 * 움직였으니 좌표가 같다) 핑퐁으로 번지지 않는다.
 *
 * 상태 변경은 `flushSync` 로 즉시 반영하고 높이 애니메이션도 **같은 태스크**에서
 * 건다 — 다음 프레임으로 미루면 그 사이 한 번 튄다.
 */

/** 모션 축소 선호 — 전환을 아예 걸지 않는다. */
function reduceMotion(): boolean {
	return (
		typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
	);
}

/** 항목별 진행 중인 높이 애니메이션 — 뒷정리를 이어받은 쪽만 하도록 구분한다. */
const running = new WeakMap<HTMLElement, Animation>();

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
	// `fill: 'forwards'` 가 없으면 **끝나는 프레임에 한 번 튄다** — 애니메이션이
	// 끝나는 순간 높이가 '자연 높이'로 돌아가는데, 그때 `collapsing` 이 아직
	// 붙어 있어 접힌 행이 펼친 높이(58px)로 잠깐 되돌아간다. 실측에서 그 프레임에
	// 아래 항목들이 22px 밀렸다가 제자리로 오는 것으로 나타났다(admin 보고:
	// "바운스"). 최종값을 유지시켜 두고, 클래스 제거와 함께 걷어낸다.
	const anim = el.animate([{ height: `${from}px` }, { height: `${to}px` }], {
		duration: durationMs,
		easing: 'ease-out',
		fill: 'forwards'
	});
	running.set(el, anim);
	anim.finished
		.catch(() => {}) // cancel() 시 reject — 뒷정리는 이어받은 쪽이 한다.
		.finally(() => {
			if (running.get(el) !== anim) return;
			running.delete(el);
			// 순서 주의: 클래스를 먼저 떼어 자연 높이를 목표값과 같게 만든 **뒤**
			// 유지 중인 애니메이션을 걷는다. 반대로 하면 그 사이 한 프레임이 튄다.
			el.classList.remove('collapsing');
			el.style.overflow = overflow;
			anim.cancel();
		});
}

/**
 * 호버로 선택을 옮긴다 — 상태 변경과 높이 전환을 같은 태스크에서 끝낸다.
 * 스크롤은 건드리지 않는다(파일 상단 설명 참고).
 *
 * @param apply 선택 상태를 실제로 바꾸는 함수(`selIndex = i` 등)
 */
export function hoverSelect(opts: {
	/** 목록 컨테이너(현재는 쓰지 않지만, 호출측 의미를 남겨 둔다). */
	scroller?: HTMLElement | null;
	/** 지금 펼쳐져 있는 행(접힐 쪽). 없으면 생략. */
	prev: HTMLElement | null | undefined;
	/** 커서가 올라간 행(펼쳐질 쪽). */
	next: HTMLElement | null | undefined;
	apply: () => void;
	durationMs?: number;
}) {
	const { prev, next, apply } = opts;
	const durationMs = opts.durationMs ?? 90;
	if (!next) {
		apply();
		return;
	}
	const prevFrom = prev?.getBoundingClientRect().height;
	const nextFrom = next.getBoundingClientRect().height;

	// DOM 을 즉시 반영시켜, 브라우저가 중간 상태를 그리기 전에 여기서 다 끝낸다.
	flushSync(apply);

	if (reduceMotion()) return;
	// 최종 높이를 재고 **같은 태스크에서** 애니메이션을 건다.
	const prevTo = prev?.getBoundingClientRect().height;
	const nextTo = next.getBoundingClientRect().height;
	if (prev && prevFrom != null && prevTo != null) animateHeight(prev, prevFrom, prevTo, durationMs);
	animateHeight(next, nextFrom, nextTo, durationMs);
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
 * 사용자가 방금 굴렸는지. `scroll` 이벤트는 프로그램이 스크롤을 옮길 때도
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
