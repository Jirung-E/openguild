/**
 * BUG-199: 모달이 떠 있는 동안 **뒤 페이지**가 스크롤되던 문제.
 *
 * 모바일에서 팝업 안을 스크롤하려고 손가락을 움직이면 팝업이 아니라 배경
 * 페이지가 움직였다(admin 보고). 오버레이는 `position:fixed` 라 그 자체가
 * 스크롤을 막아주지 않는다 — 터치는 그대로 아래 문서로 전달된다.
 *
 * 겹쳐 뜨는 모달(퀘스트 추가 위에 확인 대화상자 등)이 있으므로 **참조 계수**로
 * 관리한다. 마지막 하나가 풀릴 때만 원래 스타일로 되돌린다.
 *
 * 스크롤 위치 보존: `overflow:hidden` 만 걸면 브라우저가 스크롤 위치를 유지해
 * 대체로 문제없다. `position:fixed` 로 body 를 고정하는 기법은 위치가 튀는
 * 부작용이 있어 쓰지 않는다.
 */

let depth = 0;
let saved: { overflow: string; overscroll: string } | null = null;

/** 잠그고, 해제 함수를 돌려준다. 같은 모달에서 두 번 호출해도 안전(호출 수만큼 해제 필요). */
export function lockBodyScroll(): () => void {
	if (typeof document === 'undefined') return () => {};
	if (depth === 0) {
		const s = document.body.style;
		saved = { overflow: s.overflow, overscroll: s.overscrollBehavior };
		s.overflow = 'hidden';
		// 모달 내부 끝까지 스크롤한 뒤에도 배경으로 넘어가지 않게(스크롤 체이닝 차단).
		s.overscrollBehavior = 'contain';
	}
	depth += 1;
	let released = false;
	return () => {
		if (released) return;
		released = true;
		depth = Math.max(0, depth - 1);
		if (depth === 0 && saved) {
			document.body.style.overflow = saved.overflow;
			document.body.style.overscrollBehavior = saved.overscroll;
			saved = null;
		}
	};
}
