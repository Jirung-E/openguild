#!/usr/bin/env node
// BUG-264: 높이에 `vh` 를 쓸 때 `dvh` 폴백 쌍을 강제한다.
//
// `100vh` 는 모바일에서 **주소창이 접힌** 상태의 높이(large viewport)다. 실제로
// 보이는 영역은 그보다 작으므로, 그 값으로 상자를 만들면 아래쪽이 화면 밖으로
// 나간다. 실제로 그랬다 — 도서관 상세에서 첨부 섹션 절반부터 잘렸고, 문서가
// 잠겨 있어(BUG-257) 거기에 도달할 방법이 아예 없었다.
//
// 규칙: 높이 계열 속성이 `vh` 를 쓰면, **바로 다음 선언**이 같은 속성의 `dvh`
// 판이어야 한다. 순서가 중요하다 — `dvh` 를 모르는 브라우저는 뒷줄을 무시하고
// 앞줄(`vh`)을 쓴다. 뒤집으면 폴백이 이긴다.
//
// 검사하지 않는 것: `min-height`. 최소값은 넘쳐도 잘리지 않는다.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));
const PROPS = ['height', 'max-height'];

// **`\bvh\b` 로 쓰면 안 된다.** `100vh` 에서 `0` 과 `v` 는 둘 다 단어 문자라
// 그 사이에 단어 경계가 없다 — 아무것도 안 잡히는 검사가 된다(처음에 그렇게
// 썼다가 되돌리기 시험에서 걸렸다). 앞 글자가 알파벳이 **아닐** 때로 본다.
// `lvh` 는 large viewport 라 `vh` 와 같은 문제를 갖는다. `dvh`/`svh` 는 안전.
const RISKY = /(?<![a-z])vh\b|lvh\b/;
const HAS_DVH = /dvh\b/;

const violations = [];

function stripComments(css) {
	return css.replace(/\/\*[\s\S]*?\*\//g, '');
}

function styleBlocks(src) {
	return [...src.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]);
}

/** 선언들을 순서대로 뽑는다 — 순서가 규칙의 일부라 정규식 하나로는 못 본다. */
function declarations(css) {
	const out = [];
	for (const m of css.matchAll(/(^|[;{])\s*([a-z-]+)\s*:\s*([^;{}]+)/g)) {
		out.push({ prop: m[2], value: m[3].trim() });
	}
	return out;
}

function scan(rel, css) {
	const decls = declarations(stripComments(css));
	for (let i = 0; i < decls.length; i++) {
		const d = decls[i];
		if (!PROPS.includes(d.prop)) continue;
		if (!RISKY.test(d.value) || HAS_DVH.test(d.value)) continue;
		const next = decls[i + 1];
		const ok = next && next.prop === d.prop && HAS_DVH.test(next.value);
		if (!ok) violations.push(`${rel}  ${d.prop}: ${d.value}`);
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
	console.error(`✗ dvh 폴백이 없는 vh 높이 ${violations.length}건.`);
	console.error('  모바일에서 vh 는 주소창이 접힌 높이라 실제 화면보다 큽니다 —');
	console.error('  그만큼 아래쪽이 잘립니다(BUG-264). 같은 속성을 두 번 쓰세요:\n');
	console.error('    height: calc(100vh - ...);   /* 미지원 브라우저 폴백 — 먼저 */');
	console.error('    height: calc(100dvh - ...);\n');
	for (const v of violations) console.error('  ' + v);
	process.exit(1);
}
console.log('✓ vh 높이에 dvh 폴백이 모두 있음.');
