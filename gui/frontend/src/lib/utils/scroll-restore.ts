// DEV-192: 상세 페이지 스크롤 위치 복원.
//
// detail / 마크다운 / 이미지 / 첨부가 비동기로 로드되며 페이지 높이가 복원 직후에도
// 계속 변한다. 단발 scrollTo 로는 빗나가므로(admin: "돌아오는 스크롤 위치가
// 정확하지 않음"), 높이가 안정될 때까지(또는 최대 시간) 매 프레임 목표 위치를
// 재적용한다. 사용자가 실제로 스크롤(wheel/touch/key)하면 즉시 중단해 경합을 막는다.
//
// 애니메이션 없음(instant) — "원래 거기 있던" 느낌.
/**
 * 진행 중인 복원 루프의 취소 핸들.
 *
 * REQ-004: 예전엔 호출마다 rAF 루프와 리스너를 **독립으로** 설치하고 취소
 * 수단이 없었다. 빠른 back/forward 에서 이전 페이지 y 를 겨냥한 루프와 새
 * 루프가 매 프레임 서로 다른 `window.scrollTo` 를 불러 지터가 나고 최종 위치도
 * 엉뚱해졌다. 새 복원이 시작되면 앞선 루프를 끊는다.
 */
let activeCancel: (() => void) | null = null;

/** 진행 중인 복원을 중단한다(페이지 이탈 등). 없으면 no-op. */
export function cancelRestoreScroll(): void {
	activeCancel?.();
	activeCancel = null;
}

export function restoreScroll(y: number, opts?: { maxMs?: number; settleMs?: number }): void {
	const maxMs = opts?.maxMs ?? 1200;
	const settleMs = opts?.settleMs ?? 180;

	// 앞선 복원이 돌고 있으면 먼저 끊는다 — 두 루프가 다른 목표로 경합하면
	// 매 프레임 서로를 되돌린다.
	cancelRestoreScroll();

	let cancelled = false;
	const onUser = () => {
		cancelled = true;
		cleanup();
		activeCancel = null;
	};
	const cleanup = () => {
		// 프로그램 scrollTo 는 'scroll' 만 발생시키고 wheel/touch/key 는 발생시키지
		// 않으므로, 이들로만 '사용자 개입'을 판별한다.
		window.removeEventListener('wheel', onUser);
		window.removeEventListener('touchstart', onUser);
		window.removeEventListener('keydown', onUser);
	};
	window.addEventListener('wheel', onUser, { passive: true });
	window.addEventListener('touchstart', onUser, { passive: true });
	window.addEventListener('keydown', onUser);
	activeCancel = () => {
		cancelled = true;
		cleanup();
	};

	const start = performance.now();
	let lastHeight = -1;
	let stableSince = start;

	const tick = () => {
		if (cancelled) return;
		window.scrollTo(0, y);
		const now = performance.now();
		const h = document.documentElement.scrollHeight;
		if (h !== lastHeight) {
			lastHeight = h;
			stableSince = now;
		}
		// 높이가 settleMs 동안 그대로면(레이아웃 안정) 또는 maxMs 초과면 종료.
		if (now - stableSince >= settleMs || now - start >= maxMs) {
			cleanup();
			activeCancel = null;
			return;
		}
		requestAnimationFrame(tick);
	};
	requestAnimationFrame(tick);
}
