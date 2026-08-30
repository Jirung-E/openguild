#!/usr/bin/env node
// BUG-256: 여백·치수를 리터럴 px 로 적는 것 차단 (rem 만).
//
// UI 크기 조절(DEV-101)은 root font-size 를 바꾼다 — **rem 만 따라간다.**
// `font-size` 는 rem 인데 `padding`/`gap` 만 px 이면 배율을 올렸을 때 글자만
// 커지고 여백은 그대로라 비율이 어긋난다. admin 이 설정의 '업데이트 자동 확인'
// 토글에서 이걸 발견했고([[BUG-254]]), 같은 부류가 20건 넘게 남아 있었다.
//
// 두께도 같다. 진행도 바가 `height: 4px` 라 배율을 올리면 곡률(rem)만 커져
// 과하게 둥근 실선처럼 보였다("캠페인 진행도 표시의 굵기가 안변함").
//
// [[DEV-369]] 의 교훈: **검사 없이 치환만 하면 다음 커밋부터 다시 섞인다.**
// 곡률·테두리는 `check:radius` 가 막고 있고, 여백·치수는 여기가 막는다.
//
// 환산: 기본 배율에서 16px = 1rem. 2px → 0.125rem, 4px → 0.25rem …
//
// **검사하지 않는 것**:
//   - `@media` / `@container` / `@supports` 의 조건절 — 중단점은 px 가 맞다.
//     rem 으로 쓰면 우리가 바꾸는 배율에 중단점까지 따라 움직인다.
//   - `0px` — 0 은 단위가 없는 것과 같다.
//   - `calc(...)` / `var(...)` 안의 px — 실제 값은 토큰이 정하고, 거기 적힌 px
//     는 폴백이거나 토큰과 섞어 쓰는 정상 용법이다
//     (`var(--content-max-width, 1100px)`, `calc(var(--titlebar-h) + 2px)`).
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));

/**
 * 파일 전체를 예외로 두는 곳 — 이유가 파일 자신의 주석에 남아 있다.
 *
 * 타이틀바 창 크롬과 스크롤바는 **OS/네이티브 위젯**이 정한 치수라 앱 배율을
 * 따라가면 오히려 어긋난다(BUG-246).
 *
 * `QuestBoard` 는 **일부러 빠져 있다.** 예전 조사가 보드의 px 를 통째로
 * "노드 px 섬" 으로 묶었는데, 실제로는 노드·레인 기하(JS 상수와 짝)와 순수
 * 화면 UI(툴바·필터칩·HUD)가 섞여 있었다. 후자는 배율을 따라가야 하고,
 * 특히 보드 툴바의 '새 퀘스트' 버튼은 목록 뷰의 같은 버튼과 **치수가 같아야
 * 한다**(DEV-086 — 보드↔목록 전환 시 안 흔들리게). 그래서 파일 전체가 아니라
 * 아래 구간 표시로 섬만 감싼다.
 */
const EXEMPT_FILES = new Set([
	'lib/components/TitleBar.svelte',
	'lib/styles/global.css',
	'lib/components/OverlayScrollbar.svelte'
]);

/**
 * 구간 예외 — px 가 맞는 영역을 소스에서 직접 표시한다.
 *
 *   /* check-spacing:off — 왜 px 인지 *\/
 *   … 이 사이는 검사하지 않는다 …
 *   /* check-spacing:on *\/
 *
 * ALLOW 목록에 수십 줄을 늘어놓는 것보다 **그 자리에** 이유가 남아서 낫다.
 * 표시를 지우면 곧바로 검사 대상이 되므로, 섬이 조용히 넓어지지 않는다.
 */
function blankOptedOutRegions(css) {
	return css.replace(
		/\/\*\s*check-spacing:off[\s\S]*?check-spacing:on\s*\*\//g,
		(m) => ' '.repeat(m.length)
	);
}

/** 파일별로 허용하는 리터럴 — 이유를 코드 주석으로 남긴 곳만 등록한다. */
const ALLOW = new Map([
	// 스크린리더 전용으로 숨긴 요소. 화면에 안 보이는 1px 이라 배율과 무관하다.
	['lib/components/QuestListFilter.svelte', new Set(['width:1px', 'height:1px'])],
	// 같은 목적 — 화면 밖으로 치워 두는 건너뛰기 링크. 배율과 무관하다.
	['lib/components/Nav.svelte', new Set(['left:-9999px'])]
]);

// `(?<![-\w])` 가 핵심이다 — 없으면 `border-left: 3px solid ...` 의 `left:` 가
// 위치 지정으로 잡힌다(테두리 두께는 check:radius 의 몫이다).
const SPACING =
	/(?<![-\w])(padding|margin|gap|row-gap|column-gap)(-top|-right|-bottom|-left)?\s*:\s*([^;}]+)/g;
const DIM =
	/(?<![-\w])(width|height|min-width|min-height|max-width|max-height)\s*:\s*([^;}]+)/g;
/** 위치 지정도 여백과 같은 역할을 한다 — 떠 있는 버튼의 top/right 등. */
const POS = /(?<![-\w])(top|right|bottom|left|inset)\s*:\s*([^;}]+)/g;

/** 0 이 아닌 리터럴 px 가 값에 **직접** 적혀 있나. */
function hasRawPx(value) {
	// var()/calc() 는 안쪽부터 반복해서 걷어낸다 — 한 번만 지우면
	// `calc(var(--titlebar-h, 32px) + 2px)` 처럼 중첩된 것이 남는다.
	let v = value;
	for (let i = 0; i < 8; i++) {
		const next = v.replace(/(var|calc|clamp|min|max)\([^()]*\)/g, ' ');
		if (next === v) break;
		v = next;
	}
	for (const m of v.matchAll(/(-?\d*\.?\d+)px\b/g)) {
		if (Number(m[1]) !== 0) return true;
	}
	return false;
}

const violations = [];

function stripComments(css) {
	return css.replace(/\/\*[\s\S]*?\*\//g, '');
}

/** `@media (max-width: 640px)` 의 조건절을 지운다 — 중단점은 px 가 맞다. */
function stripAtRulePreludes(css) {
	return css.replace(/@(media|container|supports)[^{]*/g, '@$1 ');
}

function styleBlocks(src) {
	return [...src.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]);
}

function scan(rel, css) {
	if (EXEMPT_FILES.has(rel)) return;
	const allow = ALLOW.get(rel) ?? new Set();
	// 순서가 중요하다 — 구간 표시가 주석이므로 주석을 지우기 **전에** 걷어낸다.
	const clean = stripAtRulePreludes(stripComments(blankOptedOutRegions(css)));

	for (const re of [SPACING, DIM, POS]) {
		re.lastIndex = 0;
		for (const m of clean.matchAll(re)) {
			const decl = m[0].trim().replace(/\s+/g, ' ');
			const value = m[m.length - 1];
			if (!hasRawPx(value)) continue;
			if (allow.has(decl.replace(/\s+/g, ''))) continue;
			violations.push(`${rel}  ${decl}`);
		}
	}
}

function walk(dir) {
	for (const name of readdirSync(dir)) {
		const p = join(dir, name);
		if (statSync(p).isDirectory()) {
			walk(p);
			continue;
		}
		const rel = relative(SRC, p).replace(/\\/g, '/');
		if (p.endsWith('.css')) scan(rel, readFileSync(p, 'utf8'));
		else if (p.endsWith('.svelte')) {
			for (const b of styleBlocks(readFileSync(p, 'utf8'))) scan(rel, b);
		}
	}
}

walk(SRC);

if (violations.length > 0) {
	console.error(`✗ 여백/치수를 리터럴 px 로 적은 곳 ${violations.length}건.`);
	console.error('  rem 으로 쓰세요 — 기본 배율에서 16px = 1rem.');
	console.error('    1px → 0.0625rem   2px → 0.125rem   4px → 0.25rem   8px → 0.5rem');
	console.error('  테두리 두께는 --bw, 곡률은 --r-* 토큰을 씁니다 (check:radius).');
	console.error('  px 가 맞는 예외라면 이유를 주석으로 남기고 스크립트의 ALLOW 에 등록하세요.\n');
	for (const v of violations) console.error('  ' + v);
	process.exit(1);
}
console.log('✓ 여백·치수 리터럴 px 없음 — rem 만 사용.');
