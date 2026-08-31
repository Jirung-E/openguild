// REQ-018: 페이지 내 검색.
//
// 눈으로는 잘 안 보이는데 틀리기 쉬운 것 셋을 고정한다.
//
// 1. **문단 경계를 넘어 찾으면 안 된다.** 텍스트 노드를 그냥 이어 붙이면
//    앞 문단 끝 "…abc" 와 다음 문단 시작 "def…" 가 "abcdef" 로 잡힌다.
//    화면에서는 붙어 있지 않은 글자다.
// 2. **한 일치가 여러 텍스트 노드에 걸친다.** `**굵게**` 가 중간에 끼면 한
//    문단이 여러 노드로 쪼개진다. Range 가 그걸 넘어가야 한다.
// 3. **겹치는 일치를 만들면 안 된다.** "aaa" 에서 "aa" 를 두 번 세면
//    다음/이전이 같은 자리를 맴돈다.
import { describe, it, expect, beforeEach } from 'vitest';
import {
	buildSegments,
	findMatches,
	matchToRange,
	stepIndex,
	isFindablePath,
	SKIP_ATTR
} from './find-in-page';

function mount(html: string): HTMLElement {
	document.body.innerHTML = `<div id="root">${html}</div>`;
	return document.getElementById('root') as HTMLElement;
}

describe('buildSegments', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
	});

	it('문단마다 따로 모은다', () => {
		const root = mount('<p>hello</p><p>world</p>');
		const segs = buildSegments(root);
		expect(segs.map((s) => s.text)).toEqual(['hello', 'world']);
	});

	it('한 문단 안의 인라인 마크업은 이어 붙인다 — 화면에서 붙어 있으니까', () => {
		const root = mount('<p>퀘스트 <strong>보드</strong> 입니다</p>');
		const segs = buildSegments(root);
		expect(segs).toHaveLength(1);
		expect(segs[0].text).toBe('퀘스트 보드 입니다');
		expect(segs[0].pieces.length).toBe(3);
	});

	it('script / style 은 안 본다', () => {
		const root = mount('<p>보임</p><script>안보임</script><style>.x{}</style>');
		expect(buildSegments(root).map((s) => s.text)).toEqual(['보임']);
	});

	it(`${SKIP_ATTR} 서브트리는 통째로 뺀다 — 찾기 UI 자신이 잡히면 안 된다`, () => {
		const root = mount(`<p>본문</p><div ${SKIP_ATTR}><p>찾기바</p></div>`);
		expect(buildSegments(root).map((s) => s.text)).toEqual(['본문']);
	});

	it('aria-hidden / hidden 도 뺀다', () => {
		const root = mount('<p>본문</p><p aria-hidden="true">숨김</p><p hidden>숨김2</p>');
		expect(buildSegments(root).map((s) => s.text)).toEqual(['본문']);
	});

	it('공백뿐인 조각은 세그먼트가 되지 않는다', () => {
		const root = mount('<p>  </p><p>실제</p>');
		expect(buildSegments(root).map((s) => s.text)).toEqual(['실제']);
	});
});

describe('findMatches', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
	});

	it('문단 경계를 넘어 찾지 않는다 — 이게 이 파일의 핵심', () => {
		const root = mount('<p>abc</p><p>def</p>');
		const segs = buildSegments(root);
		expect(findMatches(segs, 'abcdef')).toEqual([]);
		// 각각은 찾힌다.
		expect(findMatches(segs, 'abc')).toHaveLength(1);
		expect(findMatches(segs, 'def')).toHaveLength(1);
	});

	it('인라인 마크업을 가로질러서는 찾는다 — 화면에서 이어져 보이니까', () => {
		const root = mount('<p>퀘스트 <strong>보</strong>드</p>');
		const segs = buildSegments(root);
		expect(findMatches(segs, '보드')).toHaveLength(1);
	});

	it('대소문자를 구분하지 않는다', () => {
		const root = mount('<p>Quest Board</p>');
		const segs = buildSegments(root);
		expect(findMatches(segs, 'quest')).toHaveLength(1);
		expect(findMatches(segs, 'BOARD')).toHaveLength(1);
	});

	it('겹치는 일치를 만들지 않는다 — 다음/이전이 제자리를 맴돌면 안 된다', () => {
		const root = mount('<p>aaa</p>');
		const segs = buildSegments(root);
		expect(findMatches(segs, 'aa')).toHaveLength(1);
	});

	it('한 문단에 여러 번 나오면 다 찾는다', () => {
		const root = mount('<p>go go go</p>');
		expect(findMatches(buildSegments(root), 'go')).toHaveLength(3);
	});

	it('빈 질의는 아무것도 안 찾는다 — 안 그러면 전체가 일치가 된다', () => {
		const root = mount('<p>hello</p>');
		expect(findMatches(buildSegments(root), '')).toEqual([]);
	});

	it('여러 문단에 흩어져 있으면 문서 순서대로 나온다', () => {
		const root = mount('<p>x 하나</p><p>y</p><p>x 둘</p>');
		const segs = buildSegments(root);
		const ms = findMatches(segs, 'x');
		expect(ms.map((m) => m.segment)).toEqual([0, 2]);
	});
});

describe('matchToRange', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
	});

	it('한 노드 안의 일치를 정확히 가리킨다', () => {
		const root = mount('<p>hello world</p>');
		const segs = buildSegments(root);
		const [m] = findMatches(segs, 'world');
		const r = matchToRange(segs, m);
		expect(r?.toString()).toBe('world');
	});

	it('여러 텍스트 노드에 걸친 일치도 이어서 가리킨다', () => {
		const root = mount('<p>퀘스트 <strong>보</strong>드 입니다</p>');
		const segs = buildSegments(root);
		const [m] = findMatches(segs, '보드');
		const r = matchToRange(segs, m);
		expect(r?.toString()).toBe('보드');
	});

	it('일치의 시작이 노드 경계와 정확히 겹쳐도 맞다', () => {
		const root = mount('<p><em>abc</em>def</p>');
		const segs = buildSegments(root);
		const [m] = findMatches(segs, 'cd');
		expect(matchToRange(segs, m)?.toString()).toBe('cd');
	});
});

describe('stepIndex — 다음/이전 순환', () => {
	it('일치가 없으면 -1', () => {
		expect(stepIndex(-1, 0, 1)).toBe(-1);
		expect(stepIndex(3, 0, -1)).toBe(-1);
	});

	it('아직 안 고른 상태에서 다음은 첫 번째, 이전은 마지막', () => {
		expect(stepIndex(-1, 5, 1)).toBe(0);
		expect(stepIndex(-1, 5, -1)).toBe(4);
	});

	it('끝에서 다음은 처음으로, 처음에서 이전은 끝으로', () => {
		expect(stepIndex(4, 5, 1)).toBe(0);
		expect(stepIndex(0, 5, -1)).toBe(4);
	});
});

describe('isFindablePath — 어디서 열 것인가', () => {
	it('문서형 화면에서 연다', () => {
		expect(isFindablePath('/quests/DEV-007')).toBe(true);
		expect(isFindablePath('/campaigns/C-001')).toBe(true);
		expect(isFindablePath('/rules')).toBe(true);
		expect(isFindablePath('/library')).toBe(true);
		expect(isFindablePath('/worklog')).toBe(true);
	});

	it('보드/목록/홈에서는 열지 않는다 — 웹 모드의 네이티브 찾기를 남겨 둔다', () => {
		expect(isFindablePath('/')).toBe(false);
		expect(isFindablePath('/settings')).toBe(false);
		expect(isFindablePath('/admin')).toBe(false);
	});

	it('캠페인 **생성 폼**은 문서가 아니다 — 입력칸뿐이라 찾을 글이 없다', () => {
		expect(isFindablePath('/campaigns/new')).toBe(false);
	});
});
