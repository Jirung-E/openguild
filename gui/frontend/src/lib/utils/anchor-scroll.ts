/**
 * DEV-359: 목록 항목이 제자리에서 펼쳐질 때의 스크롤 보정 + 높이 전환.
 */

/**
 * DEV-359: **커서가 움직이지 않았는데 들어온 hover** 를 걸러낸다.
 *
 * 항목이 펼쳐지면 그 아래 줄들이 밀린다. 스크롤 보정은 다음 프레임에야 도는데,
 * 브라우저는 그 사이 새 레이아웃으로 hit-test 를 다시 해서 **포인터가 가만히
 * 있어도** 이웃 항목에 `mouseenter` 를 쏜다. 그러면 그 항목이 펼쳐지고 다시
 * 레이아웃이 밀리고… 두 항목이 접혔다 펼쳐졌다 하며 떠는 현상이 된다.
 *
 * 사람이 움직여 들어온 hover 는 좌표가 반드시 달라진다. 좌표가 **완전히 같으면**
 * 레이아웃이 만들어낸 hover 이므로 무시한다.
 */
let lastHoverX = NaN;
let lastHoverY = NaN;

export function isPointerDrivenHover(ev: MouseEvent): boolean {
	if (ev.clientX === lastHoverX && ev.clientY === lastHoverY) return false;
	lastHoverX = ev.clientX;
	lastHoverY = ev.clientY;
	return true;
}

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
/**
 * 진행 중인 보정 루프 세대. 새 호출이 들어오면 이전 루프는 다음 프레임에 스스로
 * 물러난다 — 목록을 빠르게 훑으면 호버가 30ms 간격으로 들어와, 세대 구분이
 * 없으면 **서로 다른 기준점을 가진 루프 여러 개가 동시에** `scrollTop` 을 밀며
 * 싸운다(반응이 굼떠지고 스크롤이 떨린다).
 */
let anchorGeneration = 0;

export function keepRowAnchored(
	scroller: HTMLElement | null | undefined,
	row: HTMLElement | null | undefined,
	durationMs = 160
) {
	if (!scroller || !row) return;
	const mine = ++anchorGeneration;
	const anchor = row.getBoundingClientRect().top;
	const started = performance.now();
	let settled = 0;
	const step = () => {
		if (mine !== anchorGeneration || !row.isConnected) return;
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
/** 행별 진행 중인 높이 애니메이션 — 뒷정리를 이어받은 쪽만 하도록 구분한다. */
const running = new WeakMap<HTMLElement, Animation>();

export function animateHeightChange(el: HTMLElement | null | undefined, durationMs = 90) {
	if (!el || reduceMotion()) return;
	// 훑는 속도가 빠르면 같은 행에 이전 애니메이션이 남아 있다 — 겹쳐 두면 높이가
	// 옛 값에 붙들려 다음 측정이 어긋나고, 그만큼 반응이 굼떠 보인다.
	const prev = running.get(el);
	if (prev) {
		running.delete(el);
		prev.cancel();
	}
	el.classList.remove('collapsing');
	const from = el.getBoundingClientRect().height;
	requestAnimationFrame(() => {
		if (!el.isConnected) return;
		const to = el.getBoundingClientRect().height;
		if (Math.abs(to - from) < 1) return;
		// 접히는 쪽은 높이만 줄이면 애니메이션이 **보이지 않는다** — 펼침 클래스가
		// 떨어지는 순간 글자가 이미 한 줄로 되돌아가, 남은 건 빈 공간이 줄어드는
		// 것뿐이라 그냥 툭 끊긴 것처럼 읽힌다. 애니메이션 동안 `collapsing` 을
		// 붙여 **펼친 글자 배치를 유지**하고, 그 내용을 잘라내며 높이를 줄인다.
		// (컴포넌트 쪽에서 `.collapsing` 을 `.expanded` 와 같게 스타일링한다.)
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
				// 이미 다음 애니메이션이 이 행을 이어받았으면 손대지 않는다 — 안 그러면
				// 새 애니메이션의 `collapsing`/overflow 를 옛 정리가 걷어가 버린다.
				if (running.get(el) !== anim) return;
				running.delete(el);
				el.style.overflow = overflow;
				if (collapsing) el.classList.remove('collapsing');
			});
	});
}
