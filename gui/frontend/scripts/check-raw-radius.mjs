#!/usr/bin/env node
// DEV-369: 곡률·테두리 두께를 리터럴 px 로 적는 것 차단 (토큰만).
//
// BUG-253 의 전수 조사에서 곡률 331건 / 테두리 331건이 리터럴로 남아 있었다.
// 같은 물건인데 값이 제각각이었다 — 버튼이 6px 과 4px 반반, pill 이 10종.
// admin 지적: "모양이 같은데 재활용을 한 게 아니라 복붙을 한 거냐?"
//
// px 는 UI 크기 조절(DEV-101 — root font-size 배율)을 안 따라간다. 배율을
// 올리면 상자·글자만 커지고 곡률과 테두리는 그대로라 인상이 어긋난다
// (BUG-244 / BUG-254 가 같은 함정을 반복해서 맞았다).
//
// 토큰은 `lib/styles/global.css` 의 --r-xs/sm/md/lg/xl/pill 과 --bw.
//
// **허용되는 예외** (ALLOW 에 등록):
//   - `50%` / `100%` — 정원. 알약(--r-pill)과 성격이 다르므로 토큰으로 묶지 않는다.
//   - `0` — 의도적으로 각지게 두는 곳.
//   - 치수 자체가 px 로 고정된 네이티브 위젯(스크롤바) — 곡률만 rem 으로
//     바꾸면 짝이 안 맞는다.
//   - 강조용 두꺼운 테두리(2px 이상) — 힌트가 아니라 장식이라 별개 판단.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));
const CANON = 'lib/styles/global.css';

/** 파일별로 허용하는 리터럴 — 이유를 코드 주석으로 남긴 곳만 등록한다. */
const ALLOW = new Map([
	// 스크롤바는 폭이 10px 고정인 네이티브 위젯이라 곡률도 px 이어야 짝이 맞는다.
	[CANON, new Set(['border-radius:8px', 'border:2px'])]
]);

const RADIUS = /border-radius\s*:\s*([^;}]+)/g;
const BORDER = /\bborder(?:-top|-right|-bottom|-left)?(?:-width)?\s*:\s*([0-9.]+)px\b/g;
/** 정원·각짐은 토큰 대상이 아니다. */
const OK_RADIUS = /^(0|50%|100%|var\(|inherit|initial|unset)/;

const violations = [];

function stripComments(css) {
	return css.replace(/\/\*[\s\S]*?\*\//g, '');
}

function styleBlocks(src) {
	return [...src.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]);
}

function scan(rel, css) {
	const allow = ALLOW.get(rel) ?? new Set();
	const clean = stripComments(css);

	for (const m of clean.matchAll(RADIUS)) {
		const val = m[1].trim();
		if (OK_RADIUS.test(val)) continue;
		if (allow.has(`border-radius:${val.replace(/\s+/g, '')}`)) continue;
		violations.push(`${rel}  border-radius: ${val}`);
	}
	for (const m of clean.matchAll(BORDER)) {
		const px = Number(m[1]);
		// 2px 이상은 강조용 장식 — hairline 토큰(--bw)의 대상이 아니다.
		if (px >= 2) continue;
		if (allow.has(`border:${m[1]}px`)) continue;
		violations.push(`${rel}  ${m[0]}`);
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
	console.error(`✗ 곡률/테두리를 리터럴 px 로 적은 곳 ${violations.length}건.`);
	console.error(`  토큰을 쓰세요 — ${CANON} 의`);
	console.error('    곡률  --r-xs(2) / --r-sm(4) / --r-md(6) / --r-lg(8) / --r-xl(10) / --r-pill');
	console.error('    테두리 --bw (1px)');
	console.error('  px 가 맞는 예외라면 이유를 주석으로 남기고 스크립트의 ALLOW 에 등록하세요.\n');
	for (const v of violations) console.error('  ' + v);
	process.exit(1);
}
console.log('✓ 곡률·테두리 리터럴 없음 — 토큰만 사용.');
