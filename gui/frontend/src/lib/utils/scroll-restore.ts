// DEV-192: 상세 페이지 스크롤 위치 복원.
//
// detail / 마크다운 / 이미지 / 첨부가 비동기로 로드되며 페이지 높이가 복원 직후에도
// 계속 변한다. 단발 scrollTo 로는 빗나가므로(admin: "돌아오는 스크롤 위치가
// 정확하지 않음"), 높이가 안정될 때까지(또는 최대 시간) 매 프레임 목표 위치를
// 재적용한다. 사용자가 실제로 스크롤(wheel/touch/key)하면 즉시 중단해 경합을 막는다.
//
// 애니메이션 없음(instant) — "원래 거기 있던" 느낌.
export function restoreScroll(y: number, opts?: { maxMs?: number; settleMs?: number }): void {
	const maxMs = opts?.maxMs ?? 1200;
	const settleMs = opts?.settleMs ?? 180;

	let cancelled = false;
	const onUser = () => {
		cancelled = true;
		cleanup();
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
			return;
		}
		requestAnimationFrame(tick);
	};
	requestAnimationFrame(tick);
}
