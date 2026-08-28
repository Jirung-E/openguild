// DEV-297: 네이티브 `title` 툴팁 대신 앱 스타일의 커스텀 팝업.
//
// 네이티브 툴팁은 (1) OS 기본 스타일이라 앱 테마와 따로 놀고 (2) 뜨는 데
// ~1초 걸리며 (3) 위치를 제어할 수 없다. 전체 제목을 보여주는 용도
// (cross-link 자동완성 · 검색 팔레트 행 · 도서관 타일 등)엔 부적합해서
// 공용 액션으로 대체한다 — `use:titlePopup={'전체 제목'}`.
//
// 구현 메모:
// - 팝업은 document.body 에 append + position:fixed. 조상의 transform 이
//   fixed 기준을 바꾸는 함정(BUG-157 과 동일)을 피하려면 body 여야 한다.
// - scoped CSS 가 안 먹는 위치라 스타일은 인라인. 색은 테마 토큰(var(--…))을
//   그대로 참조하므로 테마 전환에 자동 반응.
// - hover 뿐 아니라 keyboard focus 에서도 뜬다(사용자 요청: "포커스 또는 호버").

const SHOW_DELAY_MS = 220;
const GAP = 6;

let current: HTMLDivElement | null = null;
/**
 * REQ-004: 지금 떠 있는 팝업이 **어느 노드의 것인지**.
 *
 * `current` 만 보고 `update()` 에서 다시 그리면, 무관한 노드의 label 이 바뀔 때
 * (목록 새로고침 후 타일 재렌더 등) 떠 있던 툴팁이 **그 노드로 끌려가며**
 * 내용까지 바뀐다. 소유자를 함께 들고 자기 것일 때만 갱신한다.
 */
let currentOwner: HTMLElement | null = null;

function destroyPopup() {
	current?.remove();
	current = null;
	currentOwner = null;
}

function createPopup(text: string, anchor: HTMLElement) {
	destroyPopup();
	const el = document.createElement('div');
	el.textContent = text;
	el.setAttribute('role', 'tooltip');
	el.style.cssText = [
		'position:fixed',
		'z-index:12000',
		'max-width:min(520px,80vw)',
		'padding:0.3rem 0.55rem',
		// DEV-369: cssText 문자열이라 곡률 일괄 치환에서 빠져 있었다 —
		// CSS 로 나가므로 토큰을 그대로 쓸 수 있다.
		'border-radius:var(--r-md)',
		'background:var(--bg-elevated)',
		'color:var(--text)',
		'border:var(--bw) solid var(--border)',
		'box-shadow:0 6px 20px rgba(0,0,0,0.35)',
		'font-size:0.78rem',
		'line-height:1.4',
		'white-space:pre-wrap',
		'overflow-wrap:anywhere',
		'pointer-events:none'
	].join(';');
	document.body.appendChild(el);
	current = el;
	currentOwner = anchor;

	// 앵커 기준 배치 — 아래 공간이 부족하면 위로, 좌우는 뷰포트 안으로 clamp.
	const r = anchor.getBoundingClientRect();
	const pr = el.getBoundingClientRect();
	const below = r.bottom + GAP;
	const top = below + pr.height <= window.innerHeight ? below : Math.max(GAP, r.top - GAP - pr.height);
	const left = Math.min(Math.max(GAP, r.left), Math.max(GAP, window.innerWidth - pr.width - GAP));
	el.style.top = `${Math.round(top)}px`;
	el.style.left = `${Math.round(left)}px`;
}

// DEV-297 후속: "가상 포커스"(DOM focus 는 그대로 두고 하이라이트
// 클래스만 옮기는 방식, 예: textarea 를 유지한 채 ↑/↓ 로 자동완성 항목만
// 이동)인 목록에선 focus/blur 이벤트 자체가 안 뜬다 — hover 는 되는데
// 키보드만 안 되는 문제(cross-link 자동완성). 그런 곳에서 호출측이 직접
// 팝업을 띄우기 위한 수동 트리거.
export function showTitlePopupNow(node: HTMLElement, text: string | null | undefined) {
	if (!text?.trim()) return;
	createPopup(text, node);
}
export function hideTitlePopupNow() {
	destroyPopup();
}

export function titlePopup(node: HTMLElement, text: string | null | undefined) {
	let label = text ?? '';
	let timer: ReturnType<typeof setTimeout> | null = null;

	const clearTimer = () => {
		if (timer !== null) {
			clearTimeout(timer);
			timer = null;
		}
	};
	const show = () => {
		if (!label.trim()) return;
		clearTimer();
		timer = setTimeout(() => createPopup(label, node), SHOW_DELAY_MS);
	};
	const hide = () => {
		clearTimer();
		destroyPopup();
	};

	node.addEventListener('mouseenter', show);
	node.addEventListener('mouseleave', hide);
	node.addEventListener('focus', show);
	node.addEventListener('blur', hide);
	// 클릭/스크롤/휠로 맥락이 바뀌면 즉시 숨김(떠 있는 채 남는 것 방지).
	node.addEventListener('click', hide);
	window.addEventListener('scroll', hide, true);
	window.addEventListener('wheel', hide, { passive: true });

	return {
		update(next: string | null | undefined) {
			label = next ?? '';
			// REQ-004: **이 노드의** 팝업이 떠 있을 때만 다시 그린다. 예전엔
			// `if (current)` 만 봐서, 무관한 노드의 label 변경이 남의 툴팁을
			// 자기 쪽으로 끌어왔다.
			if (current && currentOwner === node) createPopup(label, node);
		},
		destroy() {
			hide();
			node.removeEventListener('mouseenter', show);
			node.removeEventListener('mouseleave', hide);
			node.removeEventListener('focus', show);
			node.removeEventListener('blur', hide);
			node.removeEventListener('click', hide);
			window.removeEventListener('scroll', hide, true);
			window.removeEventListener('wheel', hide);
		}
	};
}
