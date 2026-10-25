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
	| 'select'
	// DEV-454: 예전엔 글자(☰ ⚙ ✎ × ▼ …)로 그리던 UI 아이콘들 — OS · 글꼴마다 모양 · 크기 · 높이가 달랐다.
	| 'close'
	| 'check'
	| 'warn'
	| 'caret-down'
	| 'caret-up'
	| 'caret-left'
	| 'caret-right'
	| 'caret-down-sm'
	| 'caret-right-sm'
	| 'down'
	| 'external'
	| 'download'
	| 'edit'
	| 'menu'
	| 'settings'
	| 'refresh'
	| 'grid'
	| 'arrange'
	| 'vertical'
	| 'horizontal'
	| 'undo'
	| 'redo'
	| 'collapse-right'
	// DEV-454 후속: 남아 있던 글자(‹ › ← → ↺ ⇲ ⠿ ☺ ⊕ ⊖ ⤢ ●).
	| 'chevron-left'
	| 'chevron-right'
	| 'arrow-left'
	| 'arrow-right'
	| 'reset'
	| 'tree'
	| 'grip'
	| 'smile-plus'
	| 'plus-circle'
	| 'minus-circle'
	| 'expand'
	| 'shrink'
	| 'dot';

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
	select: '<circle cx="8" cy="8" r="5.6"/><circle cx="8" cy="8" r="2.1" fill="currentColor"/>',
	// ── DEV-454: 옛 글자 아이콘 ──
	// × ✕ ✗ — 닫기 · 지우기 · 미해결.
	close: '<path d="M4.4 4.4l7.2 7.2M11.6 4.4l-7.2 7.2"/>',
	// ✓
	check: '<path d="M3.4 8.4 6.4 11.3 12.6 4.9"/>',
	// ⚠
	warn: '<path d="M8 2.7 14 13.1H2z"/><path d="M8 6.6v3.1"/><circle cx="8" cy="11.4" r="0.45" fill="currentColor"/>',
	// ▼ ▲ ◀ ▶ — 칠한 삼각형(접기 · 레인 이동). 굵기를 맞추려고 같은 색 선도 두른다.
	'caret-down': '<path d="M3.8 5.6h8.4L8 10.6z" fill="currentColor"/>',
	'caret-up': '<path d="M3.8 10.4h8.4L8 5.4z" fill="currentColor"/>',
	'caret-left': '<path d="M10.4 3.8v8.4L5.4 8z" fill="currentColor"/>',
	'caret-right': '<path d="M5.6 3.8v8.4L10.6 8z" fill="currentColor"/>',
	// ▾ ▸ — 작은 삼각형(필터 · 고급 · 소스 펼치기).
	'caret-down-sm': '<path d="M5.2 6.6h5.6L8 9.8z" fill="currentColor"/>',
	'caret-right-sm': '<path d="M6.6 5.2v5.6L9.8 8z" fill="currentColor"/>',
	// ↓ (↑ 은 위의 up)
	down: '<path d="M8 3.4v8.7M4.4 8.5 8 12.1l3.6-3.6"/>',
	// ↗ — 밖으로(새 창 · 릴리스 페이지).
	external: '<path d="M5 11 11.2 4.8M6.6 4.8h4.6v4.6"/>',
	// ⤓ — 받기.
	download: '<path d="M8 2.8v7.4M4.8 7 8 10.2 11.2 7M3.4 13.2h9.2"/>',
	// ✎ — 고치기.
	edit: '<path d="M3 13l.6-2.7 7.1-7.1a1.3 1.3 0 0 1 1.9 1.9l-7.1 7.1z"/><path d="M9.7 4.2l2.1 2.1"/>',
	// ☰ — 메뉴 · 목록 보기 · 도구 펼치기.
	menu: '<path d="M2.8 4.4h10.4M2.8 8h10.4M2.8 11.6h10.4"/>',
	// ⚙ — 설정.
	// 톱니 8개 — 바깥 6.3 · 안 4.7 반지름(해 모양으로 보이지 않게 이를 붙였다).
	settings:
		'<path d="M6.78 3.46 7.01 1.78h1.98l.23 1.68 1.13.47 1.35-1.03 1.4 1.4-1.03 1.35.47 1.13 1.68.23v1.98l-1.68.23-.47 1.13 1.03 1.35-1.4 1.4-1.35-1.03-1.13.47-.23 1.68H7.01l-.23-1.68-1.13-.47-1.35 1.03-1.4-1.4 1.03-1.35-.47-1.13-1.68-.23V7.01l1.68-.23.47-1.13L2.9 4.3l1.4-1.4 1.35 1.03z"/><circle cx="8" cy="8" r="1.9"/>',
	// ⟳ — 다시 색인. **시계 방향** — 도는 중에는 버튼째 시계 방향(rotate 0→360deg)으로 돌리므로 화살표도
	// 그쪽을 가리켜야 한다(예전 도형은 반시계라 화살표와 반대로 돌았다).
	refresh: '<path d="M12.7 8a4.7 4.7 0 1 1-1.4-3.4"/><path d="M12.9 2.7v2.6h-2.6"/>',
	// ▦ ⊞ — 아이콘 보기 · 격자.
	grid: '<rect x="2.6" y="2.6" width="10.8" height="10.8" rx="1"/><path d="M8 2.6v10.8M2.6 8h10.8"/>',
	// ⊟ — 정렬.
	arrange: '<rect x="2.6" y="2.6" width="10.8" height="10.8" rx="1"/><path d="M5.2 8h5.6"/>',
	// ↕ ↔ — 보드 방향.
	vertical: '<path d="M8 2.6v10.8M5.4 5.2 8 2.6l2.6 2.6M5.4 10.8 8 13.4l2.6-2.6"/>',
	horizontal: '<path d="M2.6 8h10.8M5.2 5.4 2.6 8l2.6 2.6M10.8 5.4 13.4 8l-2.6 2.6"/>',
	// ↩ ↪ — 되돌리기 · 다시 하기(댓글의 '답글 대상' 도 ↩).
	undo: '<path d="M5.6 3.6 2.8 6.4l2.8 2.8"/><path d="M3 6.4h6.4a3.3 3.3 0 0 1 0 6.6H7"/>',
	redo: '<path d="M10.4 3.6 13.2 6.4l-2.8 2.8"/><path d="M13 6.4H6.6a3.3 3.3 0 0 0 0 6.6H9"/>',
	// ⇥ — 도구줄 접기.
	'collapse-right': '<path d="M2.6 8h8.2M7.8 5l3 3-3 3M13.2 3.6v8.8"/>',
	// ── DEV-454 후속 ──
	// ‹ › — 캐러셀 넘기기 · 경로 구분 · 펼침.
	'chevron-left': '<path d="M10 3.4 5.4 8l4.6 4.6"/>',
	'chevron-right': '<path d="M6 3.4 10.6 8 6 12.4"/>',
	// ← → — 목록으로 · 페이지로 이동 · 바뀐 값(옛 값 → 새 값).
	'arrow-left': '<path d="M13 8H3.2M7.2 4 3.2 8l4 4"/>',
	'arrow-right': '<path d="M3 8h9.8M8.8 4l4 4-4 4"/>',
	// ↺ — 기본값으로(반시계).
	reset: '<path d="M3.3 8a4.7 4.7 0 1 0 1.4-3.4"/><path d="M3.1 2.7v2.6h2.6"/>',
	// ⇲ — 트리 보기.
	tree: '<path d="M13.6 4H5.6M13.6 8H8.8M13.6 12H8.8"/><path d="M2.8 4v2.7A1.3 1.3 0 0 0 4.1 8h2.2M2.8 6.7v4A1.3 1.3 0 0 0 4.1 12h2.2"/>',
	// ⠿ — 끌어서 옮기기.
	grip: '<g fill="currentColor" stroke="none"><circle cx="6" cy="4" r="1.1"/><circle cx="10" cy="4" r="1.1"/><circle cx="6" cy="8" r="1.1"/><circle cx="10" cy="8" r="1.1"/><circle cx="6" cy="12" r="1.1"/><circle cx="10" cy="12" r="1.1"/></g>',
	// ☺+ — 반응 달기.
	'smile-plus':
		'<circle cx="7" cy="9" r="4.9"/><path d="M5.1 10.6c1 1 2.8 1 3.8 0"/><circle cx="5.4" cy="7.9" r="0.55" fill="currentColor" stroke="none"/><circle cx="8.6" cy="7.9" r="0.55" fill="currentColor" stroke="none"/><path d="M13.2 1.6v3.6M11.4 3.4H15"/>',
	// ⊕ ⊖ — 모두 펼치기 · 접기.
	'plus-circle': '<circle cx="8" cy="8" r="5.6"/><path d="M8 5.4v5.2M5.4 8h5.2"/>',
	'minus-circle': '<circle cx="8" cy="8" r="5.6"/><path d="M5.4 8h5.2"/>',
	// ⤢ — 늘리기, 그 반대 — 높이 고정.
	expand: '<path d="M9.6 2.8h3.6v3.6M13.2 2.8 9.2 6.8M6.4 13.2H2.8V9.6M2.8 13.2l4-4"/>',
	shrink: '<path d="M9.2 3.2v3.6h3.6M9.2 6.8l4-4M6.8 12.8V9.2H3.2M6.8 9.2l-4 4"/>',
	// ● — 켜져 있음 · 미해결 · 강조 종류(글자색을 따른다).
	dot: '<circle cx="8" cy="8" r="2.6" fill="currentColor" stroke="none"/>'
};

/**
 * DEV-454 후속: 코드로 만드는 단추(`document.createElement('button')`)에 넣을 `<svg>` 문자열.
 * `Icon.svelte` 와 같은 도형 · 굵기 · 세로 정렬(1em 이면 0.375em − 0.5em = −0.125em). 크기는 둘레 글자를 따른다.
 */
export function iconSvgMarkup(name: IconName): string {
	return (
		`<svg width="1em" height="1em" viewBox="0 0 16 16" fill="none" stroke="currentColor" ` +
		`stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" ` +
		`focusable="false" style="vertical-align:-0.125em">${ICON_SHAPES[name]}</svg>`
	);
}

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
