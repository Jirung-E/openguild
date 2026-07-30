/**
 * BUG-169 / DEV-302: UI 크롬 아이콘의 **단일 출처**.
 *
 * `Icon.svelte` 는 이 도형들을 컴포넌트로 렌더하고, 보드 노드처럼 SVG 를
 * **문자열로 조립**하는 코드(`quest-node-svg.ts`, `QuestBoard.svelte`)는
 * `iconSvgGroup()` 으로 같은 도형을 얻는다. 예전엔 문자열 조립부만 이모지
 * (⏱ ⛺ 💬) 를 그대로 넣어 OS/폰트에 따라 컬러 이모지로 렌더됐다.
 *
 * 좌표계: 16×16 viewBox, `fill=none` + `stroke=currentColor` 기준.
 * 새 아이콘을 추가할 땐 같은 굵기(stroke-width 1.3)로 그려야 나란히 놓았을 때
 * 무게가 맞는다.
 */

export type IconName =
	| 'folder'
	| 'doc'
	| 'comment'
	| 'memo'
	| 'trash'
	| 'image'
	| 'globe'
	| 'pin'
	| 'up'
	| 'clock'
	| 'link'
	| 'tag'
	| 'campaign'
	| 'select';

/** 아이콘별 SVG 도형(16×16). `<svg>` 래퍼 없이 자식 엘리먼트만. */
export const ICON_SHAPES: Record<IconName, string> = {
	folder:
		'<path d="M2 4.6a1 1 0 0 1 1-1h3l1.3 1.4H13a1 1 0 0 1 1 1v5.4a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>',
	doc: '<path d="M4 2.2h5L12.4 5.6v8.2H4z"/><path d="M9 2.2v3.4h3.4"/>',
	comment:
		'<path d="M2.4 3.6a1 1 0 0 1 1-1h9.2a1 1 0 0 1 1 1v5.6a1 1 0 0 1-1 1H6.4L3.2 13V10.2h-.8a1 1 0 0 1-1-1z"/>',
	memo: '<path d="M3.4 2.4h6.2l3 3v8.2h-9.2z"/><path d="M5.4 7.4h5.2M5.4 9.8h5.2"/>',
	trash: '<path d="M2.8 4.4h10.4M6.2 4.4V2.9h3.6v1.5"/><path d="M4.2 4.4l.7 8.7h6.2l.7-8.7"/>',
	image:
		'<rect x="2.2" y="3.2" width="11.6" height="9.6" rx="1"/><circle cx="5.8" cy="6.4" r="1.1"/><path d="M2.6 11.4 6.4 8l2.4 2.2 2-1.8 2.6 2.6"/>',
	globe:
		'<circle cx="8" cy="8" r="5.7"/><ellipse cx="8" cy="8" rx="2.4" ry="5.7"/><path d="M2.5 8h11"/>',
	pin: '<path d="M6 2.4h4M8 2.4v4.2M5 6.6h6l-.7 2.6H5.7z"/><path d="M8 9.2v4.4"/>',
	up: '<path d="M8 12.6V3.9M4.4 7.5 8 3.9l3.6 3.6"/>',
	clock: '<circle cx="8" cy="8" r="5.6"/><path d="M8 4.6V8l2.4 1.5"/>',
	link: '<path d="M6.6 9.4 9.4 6.6"/><path d="M7.1 4.9 8.5 3.5a2.6 2.6 0 0 1 3.7 3.7l-1.4 1.4"/><path d="M8.9 11.1 7.5 12.5a2.6 2.6 0 0 1-3.7-3.7l1.4-1.4"/>',
	tag: '<path d="M8.4 2.4H13a.6.6 0 0 1 .6.6v4.6l-6 6a.9.9 0 0 1-1.3 0L2.4 9.7a.9.9 0 0 1 0-1.3z"/><circle cx="10.8" cy="5.2" r="0.9"/>',
	// DEV-302: 캠페인(옛 ⛺) — 텐트. 기한의 출처가 캠페인임을 나타낸다.
	campaign: '<path d="M8 2.6 2.2 13.2h11.6z"/><path d="M8 6.4v6.8"/>',
	// DEV-302: 선택 모드(옛 🔘) — 라디오.
	select: '<circle cx="8" cy="8" r="5.6"/><circle cx="8" cy="8" r="2.1" fill="currentColor"/>'
};

/**
 * 문자열로 조립하는 SVG 안에 넣을 아이콘 그룹.
 *
 * @param x 아이콘 좌상단 x (부모 SVG 좌표계)
 * @param y 아이콘 좌상단 y
 * @param size 한 변 길이(px). 16×16 도형을 이 크기로 scale.
 * @param color stroke 색 (data URL SVG 안에서는 CSS 변수가 안 먹으므로 명시).
 */
export function iconSvgGroup(
	name: IconName,
	{ x, y, size = 11, color }: { x: number; y: number; size?: number; color: string }
): string {
	const s = size / 16;
	// data URL SVG 안에서는 `currentColor` 가 색 문맥 없이 검정으로 떨어진다
	// (`<img src="data:...">` 는 문서의 color 를 물려받지 않는다) — 명시 색으로 치환.
	const shapes = ICON_SHAPES[name].split('currentColor').join(color);
	return (
		`<g transform="translate(${round(x)},${round(y)}) scale(${round(s, 4)})" ` +
		`fill="none" stroke="${color}" stroke-width="1.3" stroke-linecap="round" ` +
		`stroke-linejoin="round">${shapes}</g>`
	);
}

function round(n: number, digits = 2): number {
	const f = 10 ** digits;
	return Math.round(n * f) / f;
}
