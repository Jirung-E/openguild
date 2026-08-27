#!/usr/bin/env node
// DEV-364: pill 모양의 단일 출처 강제.
//
// 같은 `quest_id` 를 그리는 칩이 컴포넌트마다 따로 정의돼 곡률 9/12/20px,
// 배경 16/18%, 테두리 40/55%, 글꼴 sans/monospace 로 갈려 있었다. 정본이
// 없어서 DEV-362 에서 팔레트 칩을 **두 번** 잘못 맞추는 사고까지 났다 —
// 처음엔 테두리를 빠뜨렸고(`.badge` 를 눈으로 베낌), 고친 뒤에도 보드 노드의
// `.node-pill.mono` 와 또 달랐다.
//
// 모양은 `lib/styles/global.css` 의 `.pill` 하나가 정본이다. 이 검사는
// 컴포넌트가 `.pill` 선택자에 **모양 속성**을 다시 적는 것을 막는다. 색
// (`--c`)·크기 파라미터(`--pill-*`)·배치(margin/flex 등)는 그대로 허용한다 —
// 그게 modifier 로 흡수하라고 만든 손잡이이기 때문이다.
//
// `check:no-hex` 와 같은 자리의 검사다. 없으면 다음 커밋부터 다시 섞인다.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));
/** 정본. 여기서만 모양을 정한다. */
const CANON = 'lib/styles/global.css';

/** 컴포넌트가 `.pill` 에 다시 적으면 안 되는 속성 — 이게 '모양' 이다. */
const SHAPE_PROPS = [
	'border-radius',
	'background',
	'background-color',
	'border',
	'border-width',
	'border-color',
	'border-style',
	'font-family',
	'font-size',
	'padding',
	'padding-top',
	'padding-right',
	'padding-bottom',
	'padding-left',
	'height',
	'line-height'
];

const violations = [];

/** CSS 주석 제거 — 주석 안 예시 코드가 위반으로 잡히지 않도록. */
function stripComments(css) {
	return css.replace(/\/\*[\s\S]*?\*\//g, '');
}

/** `<style>` 블록만 뽑는다. */
function styleBlocks(src) {
	const out = [];
	const re = /<style[^>]*>([\s\S]*?)<\/style>/g;
	let m;
	while ((m = re.exec(src))) out.push(m[1]);
	return out;
}

/**
 * 최상위 규칙을 (선택자, 본문) 으로 자른다.
 * 중첩(@media 등)은 본문을 다시 훑어 안쪽 규칙까지 본다.
 */
function rules(css) {
	const out = [];
	let i = 0;
	while (i < css.length) {
		const open = css.indexOf('{', i);
		if (open < 0) break;
		const sel = css.slice(i, open).trim();
		let depth = 1;
		let k = open + 1;
		while (k < css.length && depth > 0) {
			if (css[k] === '{') depth++;
			else if (css[k] === '}') depth--;
			k++;
		}
		const body = css.slice(open + 1, k - 1);
		if (sel.startsWith('@')) out.push(...rules(body));
		else out.push([sel, body]);
		i = k;
	}
	return out;
}

/** 선택자가 pill 을 겨냥하는가. `.pillar` 같은 우연한 일치는 제외. */
function targetsPill(sel) {
	return /\.pill(?![\w-])/.test(sel);
}

function declarations(body) {
	const out = [];
	for (const part of body.split(';')) {
		const t = part.trim();
		if (!t || t.includes('{')) continue;
		const idx = t.indexOf(':');
		if (idx < 0) continue;
		out.push(t.slice(0, idx).trim().toLowerCase());
	}
	return out;
}

function scan(rel, css) {
	for (const [sel, body] of rules(stripComments(css))) {
		if (!targetsPill(sel)) continue;
		for (const prop of declarations(body)) {
			if (SHAPE_PROPS.includes(prop)) {
				violations.push(`${rel}  [${sel.replace(/\s+/g, ' ')}]  ${prop}`);
			}
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
		if (rel === CANON) continue;
		if (p.endsWith('.css')) scan(rel, readFileSync(p, 'utf8'));
		else if (p.endsWith('.svelte')) {
			for (const block of styleBlocks(readFileSync(p, 'utf8'))) scan(rel, block);
		}
	}
}

walk(SRC);

if (violations.length > 0) {
	console.error(`✗ pill 모양을 컴포넌트에서 다시 정의한 곳 ${violations.length}건.`);
	console.error(`  모양은 ${CANON} 의 \`.pill\` 하나가 정본입니다.`);
	console.error('  크기가 달라야 하면 --pill-fs / --pill-py / --pill-px 를 주거나');
	console.error('  .sm / .xs modifier 를 쓰세요. 색은 --c 로 넘깁니다.\n');
	for (const v of violations) console.error('  ' + v);
	process.exit(1);
}
console.log('✓ pill 모양 재정의 없음 — global.css 의 .pill 이 단일 출처.');
