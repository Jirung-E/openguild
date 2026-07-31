/**
 * BUG-179: 새로고침 단축키 판정.
 *
 * 데스크탑(Tauri)에서는 `beforeunload` 를 쓸 수 없다 — BUG-075(창 X 가 안 닫힘,
 * 심각도 1)로 그 가드를 통째로 걷어냈고, 다시 켜면 같은 회귀 위험이 있다.
 * 그래서 새로고침만 keydown 단계에서 가로채 미저장 확인 모달을 띄운다.
 *
 * 판정을 컴포넌트 밖으로 뺀 이유는 단위 테스트 — 조합이 은근히 많다.
 */

/** keydown 이벤트에서 판정에 필요한 것만. (테스트에서 생성하기 쉽게) */
export interface ReloadKeyEvent {
	key: string;
	ctrlKey?: boolean;
	metaKey?: boolean;
	altKey?: boolean;
}

/**
 * F5 / Ctrl+R / Cmd+R / Ctrl+Shift+R(강제 새로고침) 인가.
 *
 * Alt 조합은 제외 — Alt+F5 등은 창 관리 단축키라 새로고침이 아니다.
 * Shift 는 강제 새로고침이라 **포함**한다(내용이 날아가는 건 똑같다).
 */
export function isReloadShortcut(e: ReloadKeyEvent): boolean {
	if (e.altKey) return false;
	if (e.key === 'F5') return true;
	const mod = e.ctrlKey === true || e.metaKey === true;
	return mod && (e.key === 'r' || e.key === 'R');
}
