/**
 * DEV-359: **커서가 움직이지 않았는데 들어온 hover** 를 걸러낸다.
 *
 * 휠로 목록을 굴리면 행이 커서 밑을 지나가며 `mouseenter` 가 계속 들어온다.
 * 선택이 휠을 따라다니면 굴리는 내내 화면이 바뀌어 걸리는 느낌이 난다.
 * 사람이 커서를 움직여 들어온 hover 는 좌표가 반드시 달라지므로, 좌표가
 * 완전히 같으면 무시한다(레이아웃 변화로 생기는 hover 도 함께 걸러진다).
 */
let lastX = NaN;
let lastY = NaN;

export function isPointerDrivenHover(ev: MouseEvent): boolean {
	if (ev.clientX === lastX && ev.clientY === lastY) return false;
	lastX = ev.clientX;
	lastY = ev.clientY;
	return true;
}
