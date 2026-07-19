// DEV-260: 메뉴바(Nav) overflow 항목 공유 스토어.
//
// 창 폭이 좁아져 Nav 의 페이지 링크가 가로로 다 안 들어가면, Nav 가 넘친
// 항목(우선순위 낮은 오른쪽부터)을 여기로 발행하고 타이틀바(TitleBar)의
// ☰ 메뉴가 구독해 자기 목록 위에 렌더한다 — 브라우저 툴바의 overflow
// menu(priority+ navigation) 패턴. Nav 와 TitleBar 는 +layout 의 형제
// 컴포넌트라 직접 props 를 주고받을 수 없어 스토어로 연결.
//
// label 은 발행 시점에 이미 로컬라이즈된 문자열 — Nav 가 locale 변화에
// 반응해 재발행하므로 소비측(TitleBar)은 그대로 표시만 하면 된다.
import { writable } from 'svelte/store';

export interface NavOverflowItem {
	href: string;
	label: string;
	/** 현재 활성 페이지 여부 — ☰ 안으로 이동해도 하이라이트 유지. */
	active: boolean;
}

export const navOverflowItems = writable<NavOverflowItem[]>([]);
