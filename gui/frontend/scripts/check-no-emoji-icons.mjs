#!/usr/bin/env node
// BUG-169: 마크업 안 "컬러 이모지 아이콘" 재발 차단.
//
// 📁 📄 💬 📝 🗑 🖼 🌐 📌 ⬆ 같은 코드포인트는 기본 emoji presentation 이라
// OS/폰트에 따라 컬러로 렌더된다(사용자 보고: "일부 아이콘이 다른 운영체제에서는
// 컬러로 보임"). 환경마다 모양·크기·기준선이 달라 정렬까지 흔들리므로 UI 크롬
// 아이콘은 `lib/components/Icon.svelte`(또는 인라인 SVG, currentColor)를 쓴다.
//
// DEV-131 의 check-no-hex.mjs 와 같은 방식 — 의존성 없이 .svelte 를 스캔한다.
// 텍스트 기본 표현 기호(✓ ✗ ▶ ◀ ● ★ ✎ ☰ ⚙ ⚠ ✕)는 흑백으로 렌더되므로 대상이
// 아니다. 검사 범위는 **마크업**(<script>/<style> 밖) 으로 한정 — 주석이나
// 로직 문자열은 렌더되지 않는다.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const SRC = fileURLToPath(new URL('../src', import.meta.url));

// 기본 emoji presentation 범위(= 폰트가 컬러로 그림).
const COLOR_RANGES = [
	[0x1f300, 0x1faff],
	[0x2705, 0x2705],
	[0x274c, 0x274c],
	[0x2753, 0x2755],
	[0x2b06, 0x2b07],
	[0x2b1b, 0x2b1c],
	[0x2b50, 0x2b50],
	[0x26fa, 0x26fa],
	[0x2764, 0x2764],
	[0x231a, 0x231b],
	[0x23f0, 0x23fa]
];

// 의도적으로 이모지를 쓰는 파일 — 교체 대상이 아니다.
//  · QuestCommentsSection: 댓글 반응(REACTION_SET) 자체가 이모지.
//  · Icon/PlayPauseIcon: 주석에서 교체 대상 문자를 인용한다.
const ALLOW_FILES = new Set(['lib/components/Icon.svelte', 'lib/components/PlayPauseIcon.svelte']);

const isColorEmoji = (cp) => COLOR_RANGES.some(([a, b]) => cp >= a && cp <= b);

// DEV-454: 흑백으로 그려지는 **글자 아이콘**도 — OS · 글꼴마다 모양 · 크기 · 높이가 달라 SVG(Icon.svelte)로
// 옮겼다. 마크업에서만 본다(스크립트의 문장 · 화살표 설명은 대상이 아니다). 일부러 쓰는 글자(곱셈 기호
// 'x3', 마크다운 토글 'M↓', 입력칸 안내 글)는 그 줄이나 윗줄에 `glyph-ok` 주석.
// DEV-454 후속: 첫 목록이 좁아 ‹ › ← → ↺ ⇲ ≡ ⠿ ☺ ⊕ ⊖ ⤢ ● ─ 가 그대로 남아 있었다(캐러셀 ‹ › 가 윈도우에서
// 동그라미 가운데에 안 맞았다).
const GLYPH_ICONS = new Set([...'☰⚙⟲⟳✎▼▾▶▸◀▲✓✗✕×⤓⊞⊟▦⇥⚠↩↪↕↔↑↓↗‹›←→↺⇲≡⠿☺⊕⊖⤢●─']);

/** .svelte 에서 <script>/<style> 블록을 제거하고 마크업만 남긴다. */
function markupOnly(text) {
	return text
		.replace(/<script[\s\S]*?<\/script>/g, (m) => m.replace(/[^\n]/g, ' '))
		.replace(/<style[\s\S]*?<\/style>/g, (m) => m.replace(/[^\n]/g, ' '))
		.replace(/<!--[\s\S]*?-->/g, (m) => m.replace(/[^\n]/g, ' '));
}

/**
 * DEV-302: 마크업만으로는 부족하다 — 보드 노드처럼 SVG 를 **문자열로 조립**하는
 * 코드나 i18n 라벨 문자열에 이모지를 넣으면 그대로 렌더된다(실제로 ⏱ ⛺ 💬 🔘
 * 등이 이 경로로 남아 있었다). 주석은 교체 대상 문자를 인용할 수 있으므로
 * 제외하고 코드만 검사한다.
 *
 * 주석 제거는 naive — 문자열 안의 `//`(URL 등)를 주석으로 오인해 뒤를 지울 수
 * 있다. 지워지면 검사에서 빠질 뿐(오탐이 아니라 미탐) 이라 감수한다.
 */
function codeOnly(text) {
	return text
		.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '))
		.replace(/(^|[^:])\/\/[^\n]*/g, (m, p1) => p1 + ' '.repeat(m.length - p1.length));
}

/** .svelte 의 <script> 블록만 (주석 제거). */
function scriptOnly(text) {
	const out = [];
	const re = /<script[^>]*>([\s\S]*?)<\/script>/g;
	let m;
	while ((m = re.exec(text)) !== null) {
		const before = text.slice(0, m.index + m[0].indexOf(m[1]));
		const startLine = before.split('\n').length;
		out.push({ startLine, body: codeOnly(m[1]) });
	}
	return out;
}

const violations = [];

function walk(dir) {
	for (const name of readdirSync(dir)) {
		const p = join(dir, name);
		const st = statSync(p);
		if (st.isDirectory()) {
			if (name === 'node_modules') continue;
			walk(p);
			continue;
		}
		const isSvelte = name.endsWith('.svelte');
		const isTs = name.endsWith('.ts') && !name.endsWith('.d.ts') && !name.includes('.test.');
		if (!isSvelte && !isTs) continue;
		const rel = relative(SRC, p).replace(/\\/g, '/');
		if (ALLOW_FILES.has(rel)) continue;
		const text = readFileSync(p, 'utf8');
		const orig = text.split('\n');
		if (isSvelte) {
			scan(rel, markupOnly(text), 1, orig);
			scanGlyphs(rel, markupOnly(text), orig);
			// DEV-302: <script> 안의 코드도 — 문자열로 조립하는 SVG / 라벨.
			for (const { startLine, body } of scriptOnly(text)) scan(rel, body, startLine, orig);
		} else {
			scan(rel, codeOnly(text), 1, orig);
		}
	}
}

function scan(rel, text, startLine, origLines) {
	text.split('\n').forEach((line, i) => {
		// DEV-302: 그 줄이 의도적인 이모지면 `emoji-ok` 주석으로 표시 — 파일 전체를
		// ALLOW_FILES 로 빼면 같은 파일의 나머지 마크업까지 검사에서 빠진다.
		// (주석은 위에서 지워지므로 원본 줄에서 확인.)
		//
		// DEV-326: **바로 윗줄**도 본다 — prettier 가 긴 줄을 접으면서 같은 줄에
		// 둔 주석을 위로 밀어내기 때문에, 같은 줄만 보면 포맷 한 번에 표식이
		// 떨어져 나간다.
		const idx = startLine + i - 1;
		if (origLines[idx]?.includes('emoji-ok') || origLines[idx - 1]?.includes('emoji-ok')) return;
		for (const ch of line) {
			const cp = ch.codePointAt(0);
			if (isColorEmoji(cp)) {
				violations.push(
					`${rel}:${startLine + i}  U+${cp.toString(16).toUpperCase()}  ${line.trim().slice(0, 80)}`
				);
				break;
			}
		}
	});
}

const glyphViolations = [];

// 화살표는 문장에도 흔히 쓴다('a' → 'b' 확인 문구 · 핸들러 안의 주석) — `{…}` 식 밖의 **글자**로 놓였을 때만
// 아이콘으로 본다(예: 이력의 `<span>→</span>`).
const PROSE_ARROWS = new Set([...'←→']);

/** `{…}` 식을 공백으로 — 줄 수는 그대로. 중첩 괄호를 센다(템플릿 문자열의 `${}` 도 짝이 맞는다). */
function textOnly(text) {
	let depth = 0;
	let out = '';
	for (const ch of text) {
		if (ch === '{') depth++;
		const inExpr = depth > 0;
		if (ch === '}' && depth > 0) depth--;
		out += inExpr && ch !== '\n' ? ' ' : ch;
	}
	return out;
}

function scanGlyphs(rel, text, origLines) {
	const plain = textOnly(text).split('\n');
	text.split('\n').forEach((line, i) => {
		if (origLines[i]?.includes('glyph-ok') || origLines[i - 1]?.includes('glyph-ok')) return;
		const hit =
			[...line].find((ch) => GLYPH_ICONS.has(ch) && !PROSE_ARROWS.has(ch)) ??
			[...(plain[i] ?? '')].find((ch) => PROSE_ARROWS.has(ch));
		if (hit) {
			glyphViolations.push(
				`${rel}:${i + 1}  '${hit}'  ${line.trim().slice(0, 80)}`
			);
		}
	});
}

/**
 * DEV-454 후속: 번역 문자열 **앞 · 뒤에 붙은** 글자 아이콘('× 해제', '← 목록', '전체 설정 →') — 마크업만 보면
 * 놓친다. 아이콘은 마크업이 `<Icon>` 으로 그리고 문자열은 말만 둔다. 문장 가운데의 화살표(전체 → 있음)나
 * 이어 붙이는 조각의 끝(' quests → ')은 글이라 대상이 아니다. 일부러 쓰는 줄은 `glyph-ok` 주석.
 */
function scanLocale() {
	const rel = 'lib/stores/locale.ts';
	const lines = readFileSync(join(SRC, rel), 'utf8').split('\n');
	lines.forEach((line, i) => {
		if (line.includes('glyph-ok') || lines[i - 1]?.includes('glyph-ok')) return;
		for (const m of line.matchAll(/(?:ko|en): '((?:[^'\\]|\\.)*)'/g)) {
			const v = m[1];
			const head = [...v][0];
			const tail = [...v].at(-1);
			const prefix = GLYPH_ICONS.has(head) && /^.\s/u.test(v);
			const suffix = GLYPH_ICONS.has(tail) && /\s.$/u.test(v);
			if (prefix || suffix) {
				glyphViolations.push(`${rel}:${i + 1}  '${prefix ? head : tail}'  ${line.trim().slice(0, 80)}`);
				return;
			}
		}
	});
}

walk(SRC);
scanLocale();

if (glyphViolations.length > 0) {
	console.error('글자 아이콘이 마크업에 있습니다 — <Icon name="…" size="1em" /> 로(도형은 utils/icon-paths.ts):\n');
	for (const v of glyphViolations) console.error('  ' + v);
	console.error('\n아이콘이 아니라 글자라면(곱셈 기호 · 안내 글) 그 줄이나 윗줄에 glyph-ok 주석과 이유를 다세요.');
	process.exit(1);
}

if (violations.length > 0) {
	console.error('컬러 이모지 아이콘이 마크업에 있습니다 (Icon.svelte / 인라인 SVG 로 교체):\n');
	for (const v of violations) console.error('  ' + v);
	console.error(
		'\n의도적인 이모지라면 scripts/check-no-emoji-icons.mjs 의 ALLOW_FILES 에 근거와 함께 추가하세요.'
	);
	process.exit(1);
}
console.log('OK — 마크업에 컬러 이모지 · 글자 아이콘 없음');
