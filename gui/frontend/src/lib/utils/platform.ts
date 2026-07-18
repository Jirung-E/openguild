// BUG-140 / DEV-265: 커스텀 타이틀바 사용 플랫폼 판별 — 단일 진리원.
//
// decorations:false(Windows/Linux) 는 tauri.windows.conf.json +
// tauri.linux.conf.json 이 담당. macOS 는 decorations 는 그대로 두고
// `titleBarStyle: "Overlay"`(tauri.macos.conf.json)만 써서 네이티브
// traffic light 는 유지한 채 웹뷰가 그 옆/뒤까지 확장된다 — 이게 Tauri
// 에서 유일하게 "버튼은 네이티브, 나머지는 커스텀"이 진짜로 성립하는
// 플랫폼(VSCode/Spotify 스타일). 프론트는 이 함수로 커스텀 타이틀바
// 렌더/자식윈도우 decorations 옵션을 맞춘다 — 두 값이 어긋나면 타이틀바가
// 이중으로 뜨거나 아예 없게 되므로 반드시 세트로.
//
// isMacOverlay(): true 면 TitleBar.svelte 가 최소화/최대화/닫기 버튼
// 마크업 자체를 렌더링하지 않아야 함(네이티브 traffic light 가 그 자리에
// 이미 있음) — 대신 그만큼 좌측 여백을 확보.
import { detectEnvironment } from '$lib/api/transport';

function ua(): string {
	return typeof navigator === 'undefined' ? '' : navigator.userAgent;
}

export function isMacOverlay(): boolean {
	if (detectEnvironment() !== 'tauri') return false;
	return ua().includes('Mac');
}

export function usesCustomTitlebar(): boolean {
	if (detectEnvironment() !== 'tauri') return false;
	const s = ua();
	if (s === '') return false;
	// 'Linux' 는 Android UA 에도 들어가므로 제외(데스크탑 Tauri 전제지만 방어).
	return s.includes('Windows') || (s.includes('Linux') && !s.includes('Android')) || isMacOverlay();
}
