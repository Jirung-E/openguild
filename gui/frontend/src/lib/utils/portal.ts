// DEV-233 후속: position:fixed 팝업을 document.body 로 이동시키는 액션.
//
// fixed 는 보통 뷰포트 기준이지만, 조상에 transform / will-change: transform /
// filter / perspective 가 있으면 그 조상이 containing block 이 되어 좌표
// 기준이 틀어지고 조상의 overflow 클리핑도 받는다 (CSS 명세). 홈 캐러셀
// (.track 의 translateX) / 컨베이어(will-change: transform) 안의 CampaignCard
// 툴팁이 잘리던 원인. body 직속으로 옮기면 getBoundingClientRect 로 계산한
// 뷰포트 좌표가 그대로 유효해진다.
//
// 사용: `<div class="tooltip" use:portal style:top=... style:left=...>`.
export function portal(node: HTMLElement) {
	document.body.appendChild(node);
	return {
		destroy() {
			node.remove();
		}
	};
}
