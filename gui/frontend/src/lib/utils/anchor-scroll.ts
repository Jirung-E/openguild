/**
 * DEV-359: 목록 항목이 제자리에서 펼쳐질 때의 표시 방식.
 *
 * ## 왜 "겹쳐 그리기" 인가
 *
 * 처음에는 항목을 흐름(flow) 안에서 키웠다. 그러면 아래 줄이 전부 밀리는데,
 * 그 대가가 컸다:
 *
 * - 밀린 만큼 스크롤을 되돌려 커서 밑 행을 붙들어야 했고(`keepRowAnchored`),
 *   그 보정이 매 프레임 목록 전체 레이아웃을 다시 돌렸다.
 * - 보정은 다음 프레임에야 도는데 브라우저는 그 사이 새 레이아웃으로 hit-test
 *   를 다시 해서, 커서가 가만히 있어도 이웃 항목에 hover 가 들어갔다 —
 *   두 항목이 접혔다 펼쳐졌다 하며 떨었다.
 * - 휠로 굴리는 중에는 사용자의 스크롤과 보정이 서로 밀어 걸리는 느낌이 났다.
 *
 * 전부 "레이아웃이 움직인다"는 한 가지 원인에서 나온다. 그래서 **항목의 흐름상
 * 높이는 그대로 두고**, 펼친 내용만 아래로 겹쳐 그린다. 목록은 미동도 하지
 * 않으므로 보정도, 연쇄도, 휠과의 싸움도 없다.
 */

/** 모션 축소 선호 — 전환을 아예 걸지 않는다. */
function reduceMotion(): boolean {
	return (
		typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
	);
}

/** 항목별 진행 중인 높이 애니메이션 — 뒷정리를 이어받은 쪽만 하도록 구분한다. */
const running = new WeakMap<HTMLElement, Animation>();

function stopRunning(el: HTMLElement) {
	const prev = running.get(el);
	if (prev) {
		running.delete(el);
		prev.cancel();
	}
}

/**
 * 펼침 시작 — 바깥 상자의 높이를 **접힌 높이로 고정**해 흐름에서 빼앗기지 않게
 * 하고, 안쪽 내용이 그 아래로 자라 겹쳐 그려지게 한다.
 *
 * 클래스 변경 **직전에** 호출한다(접힌 높이를 재야 하므로).
 *
 * @param outer 흐름상 자리를 차지하는 상자(행 / `li`)
 * @param inner 펼쳐질 내용(행 본문 / 후보 버튼)
 */
export function beginOverlayExpand(
	outer: HTMLElement | null | undefined,
	inner: HTMLElement | null | undefined,
	durationMs = 90
) {
	if (!outer || !inner) return;
	const collapsed = outer.getBoundingClientRect().height;
	// 이 높이로 흐름상 자리를 붙박아 둔다 — 펼쳐도 아래 줄이 밀리지 않는다.
	outer.style.height = `${collapsed}px`;
	if (reduceMotion()) return;
	stopRunning(inner);
	requestAnimationFrame(() => {
		if (!inner.isConnected) return;
		const full = inner.getBoundingClientRect().height;
		if (full - collapsed < 1) return;
		const anim = inner.animate([{ height: `${collapsed}px` }, { height: `${full}px` }], {
			duration: durationMs,
			easing: 'ease-out'
		});
		running.set(inner, anim);
		anim.finished
			.catch(() => {})
			.finally(() => {
				if (running.get(inner) === anim) running.delete(inner);
			});
	});
}

/**
 * 접힘 — 겹쳐 그려진 내용을 접힌 높이까지 되돌린 뒤 고정을 푼다.
 *
 * 클래스 변경 **직전에** 호출한다.
 */
export function beginOverlayCollapse(
	outer: HTMLElement | null | undefined,
	inner: HTMLElement | null | undefined,
	durationMs = 90
) {
	if (!outer || !inner) return;
	const from = inner.getBoundingClientRect().height;
	const release = () => {
		outer.style.height = '';
		inner.style.overflow = '';
	};
	if (reduceMotion()) {
		release();
		return;
	}
	stopRunning(inner);
	requestAnimationFrame(() => {
		if (!inner.isConnected) return release();
		const to = inner.getBoundingClientRect().height;
		if (from - to < 1) return release();
		// 줄어드는 동안 글자가 삐져나오지 않게 잘라둔다 — 안 그러면 한 줄로 되돌아간
		// 글자 아래 빈 공간만 줄어들어 애니메이션이 안 보인다.
		inner.style.overflow = 'hidden';
		const anim = inner.animate([{ height: `${from}px` }, { height: `${to}px` }], {
			duration: durationMs,
			easing: 'ease-out'
		});
		running.set(inner, anim);
		anim.finished
			.catch(() => {})
			.finally(() => {
				if (running.get(inner) !== anim) return; // 다음 애니메이션이 이어받았다
				running.delete(inner);
				release();
			});
	});
}

/**
 * **커서가 움직이지 않았는데 들어온 hover** 를 걸러낸다.
 *
 * 겹쳐 그리기로 바꾼 뒤에는 목록이 움직이지 않아 이런 hover 자체가 거의 없지만,
 * 휠로 굴리는 동안에는 행이 커서 밑을 지나가며 계속 들어온다. 굴리는 중에 선택이
 * 따라다니면 걸리는 느낌이 나므로, **사람이 커서를 움직였을 때만** 받아들인다.
 */
let lastHoverX = NaN;
let lastHoverY = NaN;

export function isPointerDrivenHover(ev: MouseEvent): boolean {
	if (ev.clientX === lastHoverX && ev.clientY === lastHoverY) return false;
	lastHoverX = ev.clientX;
	lastHoverY = ev.clientY;
	return true;
}
