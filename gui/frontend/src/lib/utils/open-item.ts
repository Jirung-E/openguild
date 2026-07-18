// DEV-255: 검색 팔레트(및 향후 다른 목록)에서 문서를 여는 3가지 방식 —
// 미리보기(팔레트 내부) / 자식윈도우(Tauri 새 창) / 페이지이동(현재 창 라우팅).
//
// 미리보기는 호출부(SearchPalette) 상태로 직접 처리하므로 여기엔 없음.
// 자식윈도우/페이지이동만 공용 헬퍼로 뽑아 향후 다른 목록(quest board 등)도
// 재사용 가능하게 한다.
//
// 향후 모바일 UI 도입 시 'mobile' 모드 추가는 이 union 에 값 추가 + 아래
// openInWindow 옆에 openInMobile() 류 함수 추가로 확장 가능.
export type OpenMode = 'preview' | 'window' | 'page';

import { goto } from '$app/navigation';
import { detectEnvironment } from '$lib/api/transport';
// BUG-140: 커스텀 타이틀바 플랫폼 판별 — +layout 의 showTitleBar 와 동일 소스.
// DEV-265: macOS 는 usesCustomTitlebar() 도 true(Overlay 적용 대상)이지만
// decorations:false 로 끄면 안 됨 — 네이티브 traffic light 자체가 사라짐.
// 대신 decorations:true(기본) + titleBarStyle:'Overlay' 로 메인 창과 동일.
import { isMacOverlay, usesCustomTitlebar } from '$lib/utils/platform';

let windowSeq = 0;

/**
 * 항목별로 새 Tauri 창을 띄운다(보조 창 재사용 아님 — 여러 개 동시 비교 가능).
 * Tauri 미지원 환경(브라우저/HTTP 모드)에서는 새 탭으로 대체.
 */
export async function openInWindow(href: string, title: string): Promise<void> {
	if (detectEnvironment() !== 'tauri') {
		window.open(href, '_blank');
		return;
	}
	const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
	const label = `item-${Date.now()}-${windowSeq++}`;
	// DEV-255 버그 수정: 목적지 경로를 창 URL 로 직접 주면 Tauri asset
	// protocol 이 그 딥링크 파일을 못 찾아 빈 화면이 뜨는 경우가 있었다.
	// 항상 존재가 보장된 `/` 로 띄운 뒤 +layout 이 `winTarget` 쿼리를 읽어
	// client-side goto 로 진짜 목적지로 이동시킨다.
	const url = `/?winTarget=${encodeURIComponent(href)}`;
	// DEV-255 버그 수정: decorations 기본값(true=네이티브 타이틀바)이라 그
	// 아래에 앱의 커스텀 타이틀바까지 겹쳐 보였다. 메인 창과 동일한 플랫폼
	// 판별(usesCustomTitlebar — BUG-140 부터 Windows/Linux)로 꺼서 커스텀
	// 타이틀바 하나만 보이게 한다.
	new WebviewWindow(label, {
		url,
		title,
		width: 900,
		height: 700,
		minWidth: 480,
		minHeight: 360,
		decorations: isMacOverlay() ? true : !usesCustomTitlebar(),
		...(isMacOverlay() ? { titleBarStyle: 'overlay' as const, hiddenTitle: true } : {}),
		shadow: true
	});
}

/** 현재 창에서 실제 라우트로 이동. */
export function openInPage(href: string): void {
	void goto(href);
}
