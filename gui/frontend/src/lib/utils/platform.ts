// BUG-140: 커스텀 타이틀바 사용 플랫폼 판별 — 단일 진리원.
//
// decorations:false 는 tauri.windows.conf.json + tauri.linux.conf.json 이
// 담당(플랫폼별 오버레이). 프론트는 이 함수로 같은 플랫폼 집합을 판별해
// 커스텀 타이틀바 렌더/자식윈도우 decorations 옵션을 맞춘다 — 두 값이
// 어긋나면 타이틀바가 이중으로 뜨거나 아예 없게 되므로 반드시 세트로.
//
// macOS 는 의도적으로 제외 — 네이티브 신호등 버튼/트래픽 라이트 관례가
// 강해 시스템 데코레이션을 그대로 쓴다.
import { detectEnvironment } from '$lib/api/transport';

export function usesCustomTitlebar(): boolean {
	if (detectEnvironment() !== 'tauri') return false;
	if (typeof navigator === 'undefined') return false;
	const ua = navigator.userAgent;
	// 'Linux' 는 Android UA 에도 들어가므로 제외(데스크탑 Tauri 전제지만 방어).
	return ua.includes('Windows') || (ua.includes('Linux') && !ua.includes('Android'));
}
