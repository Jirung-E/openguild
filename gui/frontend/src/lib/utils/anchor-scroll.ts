/**
 * DEV-359: 목록 항목이 제자리에서 펼쳐질 때, **커서 밑에 있는 행이 움직이지
 * 않도록** 스크롤을 보정한다.
 *
 * 펼침은 한 번에 한 행만 적용된다. 커서를 아래로 훑으면 위쪽 행이 접히면서
 * 목록 전체가 위로 당겨지고, 그러면 커서 밑의 행이 바뀌어 그게 다시 펼쳐지는
 * 연쇄가 생긴다(DEV-297 이 호버를 툴팁으로 남겨뒀던 이유). 대상 행의 화면상
 * 위치를 펼침 전후로 비교해 차이만큼 `scrollTop` 을 밀어주면 연쇄가 끊긴다.
 *
 * 클래스 변경 **직전에** 호출한다 — 호출 시점의 위치를 기준으로, 다음 프레임
 * (레이아웃 반영 후)에 보정한다.
 */
export function keepRowAnchored(scroller: HTMLElement | null | undefined, row: HTMLElement | null | undefined) {
	if (!scroller || !row) return;
	const before = row.getBoundingClientRect().top;
	requestAnimationFrame(() => {
		if (!row.isConnected) return;
		const after = row.getBoundingClientRect().top;
		const delta = after - before;
		if (delta) scroller.scrollTop += delta;
	});
}
