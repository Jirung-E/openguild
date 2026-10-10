#!/usr/bin/env node
// DEV-427: 글자 크기 · 간격 · 그림자 · 움직임 · 굵기 · 줄 간격을 리터럴로 적는 것 차단 (토큰만).
//
// 화면마다 값을 따로 정하다 보니 글자 크기 42가지, 간격 56가지, 그림자 38가지가 됐고
// (`0.8 / 0.82 / 0.825rem` 처럼 눈으로 구별 안 되는 값까지), "버튼이 띄엄띄엄 놓인다"
// 같은 보고가 거기서 나왔다. 단계는 `lib/styles/global.css` 에 있고 설명은 도서관
// `디자인/` 문서에 있다.
//
// [[DEV-369]] / [[BUG-256]] 과 같은 이유로 검사를 함께 둔다 — 치환만 하고 검사를 안 두면
// 다음 커밋부터 다시 섞인다.
//
// **검사하는 것** (`<style>` 안과 .css):
//   - font-size 의 rem 리터럴
//   - padding · margin · gap 의 0.1 ~ 2rem 리터럴 (그보다 작으면 테두리급, 크면 레이아웃 치수)
//   - 위로 떠 있는 그림자 `0 Ypx Bpx …` (Y > 0)
//   - transition · animation 의 0.45s 이하 시간 (무한 반복 애니메이션은 제외)
//   - font-weight 의 숫자 · bold, line-height 의 단위 없는 1 ~ 1.7
//   - [[DEV-428]] · [[DEV-440]] 컴포넌트에서 공통 부품(`.btn*` · `.input` · `.popup` · `.modal-box`)의
//     **모양**(배경 · 테두리 · 글자색 · 여백 · 글자 크기 · 모서리 · 그림자)을 다시 정하는 것 — 모양은
//     global.css 에 하나씩이다. 배치(margin · width · 위치 등)는 된다.
//
// **검사하지 않는 것**: px · em(부모를 따라 줄어드는 자리) · var() · calc() 안의 값,
// `lib/styles/global.css`(토큰의 정본).
//
// **예외 표시** — 토큰이 맞지 않는 자리(아이콘 · × 글리프 크기처럼 글자가 아니라 그림 크기)는
// 그 선언 줄에 이유를 적는다. 목록을 따로 두는 것보다 그 자리에 이유가 남아서 낫다.
//
//   font-size: 1.2rem; /* token-ok: × 글리프 크기 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));
const CANON = 'lib/styles/global.css';
const OK = '\u0000OK\u0000';

const violations = [];

/** 상단 바의 아이콘 버튼(⋯ · ⚙ · 다시 색인)은 창 크롬이라 `.btn` 이 아니다. */
const BTN_ALLOW = new Set(['lib/components/Nav.svelte']);
const BTN_SEL = /\.(btn(-[\w-]+)?|input|popup|modal-box)(?![\w-])/;
const BTN_LOOK = /^(background(-color)?|border(-[a-z]+)*|color|padding(-[a-z]+)?|font-size|border-radius|box-shadow)$/;

/** 주석을 지우되 `token-ok` 표시는 자리표시자로 남긴다. */
function prepare(css) {
	return css.replace(/\/\*[\s\S]*?\*\//g, (m) => (/token-ok/.test(m) ? OK : ' '));
}

/** var()/calc() 같은 함수 안쪽을 걷어낸다 — 그 안의 값은 토큰이 정한다. */
function stripFns(value) {
	let v = value;
	for (let i = 0; i < 8; i++) {
		const next = v.replace(/(var|calc|clamp|min|max|color-mix)\([^()]*\)/g, ' ');
		if (next === v) break;
		v = next;
	}
	return v;
}

const RULES = [
	{
		prop: /^font-size$/,
		bad: (v) => /(?<![\w.])\d*\.?\d+rem\b/.test(stripFns(v)),
		hint: 'var(--fs-*)'
	},
	{
		prop: /^(padding|margin)(-[a-z-]+)?$|^(row-|column-)?gap$/,
		bad: (v) =>
			[...stripFns(v).matchAll(/(?<![\w.-])(\d*\.?\d+)rem\b/g)].some(
				(m) => Number(m[1]) >= 0.1 && Number(m[1]) <= 2
			),
		hint: 'var(--sp-*)'
	},
	{
		prop: /^box-shadow$/,
		bad: (v) => /(?:^|,)\s*0 [1-9]\d*px \d+px/.test(stripFns(v)),
		hint: 'var(--shadow-sm|md|lg)'
	},
	{
		prop: /^(transition|transition-duration|animation)$/,
		bad: (v) =>
			!/infinite/.test(v) &&
			[...stripFns(v).matchAll(/(?<![\w.-])(\d*\.?\d+)s\b/g)].some((m) => Number(m[1]) <= 0.45 && Number(m[1]) > 0),
		hint: 'var(--dur-*)'
	},
	{
		prop: /^font-weight$/,
		bad: (v) => /^\s*(\d+|bold|bolder)\s*$/.test(v),
		hint: 'var(--fw-*)'
	},
	{
		prop: /^line-height$/,
		bad: (v) => /^\s*\d*\.?\d+\s*$/.test(v) && Number(v) >= 1 && Number(v) <= 1.7,
		hint: 'var(--lh-*)'
	}
];

function scan(rel, css) {
	if (rel === CANON) return;
	const clean = prepare(css);
	if (!BTN_ALLOW.has(rel)) {
		// 선택자 = 앞 중괄호 뒤부터 이 `{` 까지. (`}` 를 소비하는 패턴이면 규칙이 하나 걸러 빠진다.)
		for (const m of clean.matchAll(/([^{}]*)\{([^{}]*)\}/g)) {
			const sel = m[1].trim();
			if (!BTN_SEL.test(sel) || m[2].includes(OK)) continue;
			for (const d of m[2].split(';')) {
				const prop = d.split(':')[0].trim();
				if (BTN_LOOK.test(prop)) {
					violations.push(`${rel}  ${sel.replace(/\s+/g, ' ')} { ${prop} }  → 공통 부품의 모양은 global.css 에서`);
					break;
				}
			}
		}
	}
	// 선언 하나 = `이름: 값;` 그리고 같은 줄의 나머지(뒤에 붙인 token-ok 표시를 보려고).
	for (const m of clean.matchAll(/(?<![-\w])([a-z-]+)\s*:\s*([^;{}]*);([^\n]*)/g)) {
		const [, prop, value, tail] = m;
		if (value.includes(OK) || tail.includes(OK)) continue;
		const rule = RULES.find((r) => r.prop.test(prop));
		if (!rule || !rule.bad(value)) continue;
		violations.push(`${rel}  ${prop}: ${value.trim().replace(/\s+/g, ' ')}  → ${rule.hint}`);
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
			for (const m of readFileSync(p, 'utf8').matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) scan(rel, m[1]);
		}
	}
}

walk(SRC);

if (violations.length > 0) {
	console.error(`✗ 디자인 토큰 대신 리터럴 ${violations.length}곳 — 토큰을 쓰거나, 토큰이 맞지 않는 자리면 줄에 /* token-ok: 이유 */:`);
	for (const v of violations) console.error('  ' + v);
	process.exit(1);
}
console.log('✓ 디자인 토큰 리터럴 없음.');
