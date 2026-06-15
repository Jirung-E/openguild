#!/usr/bin/env node
// DEV-131: 컴포넌트 CSS 안 hex 색 직접 사용 차단 (테마 토큰만 허용).
//
// DEV-074 의 테마 토큰 규칙 재발 방지 enforcement (BUG-069 같은 토큰 오용/하드코딩
// 회귀를 자동 차단). .svelte 의 <style> 블록과 .css 파일을 스캔해 `#rrggbb` 형태의
// hex 색을 찾으면 실패한다. hex 는 토큰 정의처(lib/styles/global.css 의 :root /
// [data-theme])와 JS 의 themePalette(stores/theme.ts) 에서만 — 전자는 allowlist,
// 후자는 CSS 가 아니라 애초에 스캔 대상 아님.
//
// 의존성 없음 (stylelint 미설치). CSS 주석 안 hex / var() fallback 안 hex 도 정확히
// 구분. 마크업 inline style="" 은 v1 범위 밖.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));
const ALLOW = new Set(['lib/styles/global.css']); // 토큰 정의 — hex 필수
const HEX = /#[0-9a-fA-F]{3,8}\b/;

const violations = [];

/** 한 줄에서 CSS 블록 주석 부분을 제거. 멀티라인 주석 상태는 state 로 전달. */
function stripComments(line, state) {
	let out = '';
	let s = line;
	while (s.length) {
		if (state.inComment) {
			const end = s.indexOf('*/');
			if (end < 0) return out;
			s = s.slice(end + 2);
			state.inComment = false;
		} else {
			const start = s.indexOf('/*');
			if (start < 0) return out + s;
			out += s.slice(0, start);
			s = s.slice(start + 2);
			state.inComment = true;
		}
	}
	return out;
}

/** lines[from..to] (0-based, inclusive) 를 스캔. 라인 번호는 1-based 파일 기준. */
function scan(lines, from, to, rel) {
	const state = { inComment: false };
	for (let i = from; i <= to; i++) {
		const code = stripComments(lines[i], state);
		const m = code.match(HEX);
		if (m) violations.push(`${rel}:${i + 1}  ${m[0]}`);
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
		if (ALLOW.has(rel)) continue;
		if (p.endsWith('.css')) {
			const lines = readFileSync(p, 'utf8').split('\n');
			scan(lines, 0, lines.length - 1, rel);
		} else if (p.endsWith('.svelte')) {
			const lines = readFileSync(p, 'utf8').split('\n');
			let start = -1;
			for (let i = 0; i < lines.length; i++) {
				if (start < 0 && /<style[\s>]/.test(lines[i])) start = i;
				else if (start >= 0 && /<\/style>/.test(lines[i])) {
					scan(lines, start, i, rel);
					start = -1;
				}
			}
		}
	}
}

walk(SRC);

if (violations.length > 0) {
	console.error(`✗ 컴포넌트 CSS 에 hex 색 ${violations.length}건 — 테마 토큰(var(--…))을 쓰세요.`);
	console.error('  (새 색이면 global.css 의 :root + [data-theme=light] 양쪽에 토큰 신설 후 사용)\n');
	for (const v of violations) console.error('  ' + v);
	process.exit(1);
}
console.log('✓ 컴포넌트 CSS hex 없음 — 토큰만 사용.');
