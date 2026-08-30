#!/usr/bin/env node
// DEV-272: 글꼴 스택을 컴포넌트에 직접 적는 것 차단 (토큰만).
//
// 설정에서 글꼴을 고르는 기능은 `--font-sans` / `--font-mono` 를 root 에 다시
// 심는 방식이다. **컴포넌트가 스택을 직접 적으면 그 곳만 설정을 안 따라간다.**
// 실제로 그랬다 — DEV-364 가 `--font-mono` 토큰을 만들어 뒀는데 쓰이는 곳은
// 한 곳뿐이었고, 나머지 46곳은 여전히 각자 스택을 적고 있었다. 그것도 4종으로
// 갈려서, 같은 `.slug` 인데 캠페인은 JetBrains, 작업기록은 SFMono 였다.
//
// [[DEV-369]] / [[BUG-256]] 과 같은 이유로 검사를 함께 둔다 — 치환만 하고
// 검사를 안 두면 다음 커밋부터 다시 섞인다.
//
// **검사하지 않는 것**:
//   - `inherit` / `var(...)` — 토큰을 쓰는 정상 용법.
//   - `lib/styles/global.css` — 토큰의 정본이 사는 곳.
//   - 아이콘 글꼴 (ALLOW 등록) — 글꼴이 아니라 **아이콘 소스**라 사용자가
//     바꾸면 창 버튼이 깨진다.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));
const CANON = 'lib/styles/global.css';

/** 파일별로 허용하는 스택 — 이유를 코드 주석으로 남긴 곳만 등록한다. */
const ALLOW = new Map([
	[
		// Windows 창 컨트롤(최소화/최대화/닫기)은 이 글꼴의 **글리프**로 그린다.
		// 사용자가 고른 본문 글꼴로 바꾸면 버튼이 네모나 물음표로 나온다.
		'lib/components/TitleBar.svelte',
		new Set(["'SegoeFluentIcons','SegoeMDL2Assets',sans-serif"])
	]
]);

const FF = /font-family\s*:\s*([^;}]+)/g;
/** JS 쪽 — CodeMirror 테마 객체나 `el.style.cssText` 문자열. */
const JS_FF = /fontFamily\s*:\s*['"`]([^'"`]+)['"`]/g;
const JS_FF_KEBAB = /['"`]\s*font-family\s*:\s*([^'"`;]+)/g;

const OK = /^(inherit|initial|unset|revert|var\()/;

const violations = [];

function stripComments(css) {
	return css.replace(/\/\*[\s\S]*?\*\//g, '');
}

function styleBlocks(src) {
	return [...src.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]);
}

/** `.svelte` 의 `<style>` 을 **뺀** 나머지(마크업 + script). */
function nonStyle(src) {
	return src.replace(/<style[^>]*>[\s\S]*?<\/style>/g, '');
}

function key(value) {
	return value.replace(/\s+/g, '');
}

function record(rel, value, where) {
	const val = value.trim();
	if (OK.test(val)) return;
	const allow = ALLOW.get(rel) ?? new Set();
	if (allow.has(key(val))) return;
	violations.push(`${rel}  ${where}font-family: ${val}`);
}

function scan(rel, css) {
	const clean = stripComments(css);
	FF.lastIndex = 0;
	for (const m of clean.matchAll(FF)) record(rel, m[1], '');
}

function scanScript(rel, code) {
	const clean = stripComments(code);
	for (const re of [JS_FF, JS_FF_KEBAB]) {
		re.lastIndex = 0;
		for (const m of clean.matchAll(re)) record(rel, m[1], '(JS) ');
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
			const src = readFileSync(p, 'utf8');
			for (const b of styleBlocks(src)) scan(rel, b);
			scanScript(rel, nonStyle(src));
		} else if (p.endsWith('.ts') && !p.endsWith('.test.ts')) {
			scanScript(rel, readFileSync(p, 'utf8'));
		}
	}
}

walk(SRC);

if (violations.length > 0) {
	console.error(`✗ 글꼴 스택을 직접 적은 곳 ${violations.length}건.`);
	console.error(`  토큰을 쓰세요 — ${CANON} 의`);
	console.error('    본문  var(--font-sans)');
	console.error('    고정폭 var(--font-mono)');
	console.error('  직접 적으면 그 곳만 글꼴 설정(DEV-272)을 안 따라갑니다.');
	console.error('  아이콘 글꼴처럼 예외라면 이유를 주석으로 남기고 ALLOW 에 등록하세요.\n');
	for (const v of violations) console.error('  ' + v);
	process.exit(1);
}
console.log('✓ 글꼴 스택 직접 지정 없음 — 토큰만 사용.');
