/**
 * DEV-359: 목록 항목이 제자리에서 펼쳐질 때의 스크롤 보정 + 높이 전환.
 */

/** 모션 축소 선호 — 전환을 아예 걸지 않는다. */
function reduceMotion(): boolean {
	return typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
}

/**
 * **커서 밑에 있는 행이 움직이지 않도록** 스크롤을 보정한다.
 *
 * 펼침은 한 번에 한 행만 적용된다. 커서를 아래로 훑으면 위쪽 행이 접히면서
 * 목록 전체가 위로 당겨지고, 그러면 커서 밑의 행이 바뀌어 그게 다시 펼쳐지는
 * 연쇄가 생긴다(DEV-297 이 호버를 툴팁으로 남겨뒀던 이유). 대상 행의 화면상
 * 위치를 기준으로 잡고 그만큼 `scrollTop` 을 밀어주면 연쇄가 끊긴다.
 *
 * 높이 전환이 걸려 있으면 위치가 **여러 프레임에 걸쳐** 움직이므로 한 프레임만
 * 보정해서는 애니메이션 내내 행이 흘러간다. 위치가 멎을 때까지(또는
 * `durationMs` 까지) 매 프레임 따라가며 붙든다.
 *
 * 클래스 변경 **직전에** 호출한다 — 호출 시점의 위치가 기준이 된다.
 */
export function keepRowAnchored(
	scroller: HTMLElement | null | undefined,
	row: HTMLElement | null | undefined,
	durationMs = 300
) {
	if (!scroller || !row) return;
	const anchor = row.getBoundingClientRect().top;
	const started = performance.now();
	let settled = 0;
	const step = () => {
		if (!row.isConnected) return;
		const delta = row.getBoundingClientRect().top - anchor;
		// scrollTop 은 소수점에서 미세하게 어긋날 수 있어 반 픽셀 미만은 멎은 것으로 본다.
		if (Math.abs(delta) > 0.5) {
			scroller.scrollTop += delta;
			settled = 0;
		} else {
			settled += 1;
		}
		if (settled < 2 && performance.now() - started < durationMs) requestAnimationFrame(step);
	};
	requestAnimationFrame(step);
}

/**
 * 펼침/접힘의 높이 변화를 애니메이션한다.
 *
 * CSS `transition: height` 로는 안 된다 — 펼친 상태·접힌 상태 모두 `height:
 * auto` 라 계산값이 바뀌지 않아 전환이 아예 시작되지 않는다(실측 확인).
 * `interpolate-size: allow-keywords` 도 한쪽이 길이여야 의미가 있고, 접힌 높이를
 * 상수로 박으면 글꼴·확대 배율에 따라 어긋난다. 그래서 변화 **전후 높이를 재서**
 * Web Animations 로 잇는다 — 상수도 없고, 엔진도 가리지 않는다(WebKit 포함).
 *
 * 클래스 변경 **직전에** 호출한다.
 */
export function animateHeightChange(el: HTMLElement | null | undefined, durationMs = 120) {
	if (!el || reduceMotion()) return;
	const from = el.getBoundingClientRect().height;
	requestAnimationFrame(() => {
		if (!el.isConnected) return;
		const to = el.getBoundingClientRect().height;
		if (Math.abs(to - from) < 1) return;
		// 줄어드는 쪽에서 내용이 잠깐 삐져나오지 않도록 애니메이션 동안만 잘라둔다.
		const overflow = el.style.overflow;
		el.style.overflow = 'hidden';
		const anim = el.animate([{ height: `${from}px` }, { height: `${to}px` }], {
			duration: durationMs,
			easing: 'ease-out'
		});
		anim.finished
			.catch(() => {})
			.finally(() => {
				el.style.overflow = overflow;
			});
	});
}
