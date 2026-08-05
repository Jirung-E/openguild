/** 빈 보드 입력면에서 시작한 터치인지 판별한다. UI는 명시적으로 제외하는 대신
 *  이 입력면만 허용해, 새 버튼/팝업이 추가돼도 보드 pan이 click을 가로채지 않는다. */
export function isBoardPanSurfaceTarget(target: EventTarget | null): boolean {
	return target instanceof Element && target.closest('.board') !== null;
}
