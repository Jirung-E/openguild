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
//
// BUG-265: **큰 뷰포트가 정답인 경우가 있다.** 화면을 꽉 채우는 판(보드·목록)
// 은 `dvh` 로 딱 맞추면 스크롤할 여유가 0 이 되고, 모바일에서 주소창을 접는
// 제스처는 곧 문서 스크롤이므로 접을 방법 자체가 사라진다. 그런 자리는 선언
// **바로 앞**에 `/* check-viewport:large */` 를 붙여 넘긴다. 파일 단위 예외
// 목록으로 두지 않는 이유는, 왜 큰 뷰포트여야 하는지가 코드 옆에 남아야
// 다음 사람이 또 기계적으로 `dvh` 로 바꾸지 않기 때문이다.
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

/** 표식은 선언으로 바꿔 살려 두고(순서를 봐야 한다), 나머지 주석만 지운다. */
const MARKER_DECL = '-og-viewport-large: 1;';
function stripComments(css) {
	return css
		.replace(/\/\*\s*check-viewport:large\b[\s\S]*?\*\//g, MARKER_DECL)
		.replace(/\/\*[\s\S]*?\*\//g, '');
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
		// 바로 앞이 표식이면 의도적인 큰 뷰포트다.
		if (decls[i - 1]?.prop === '-og-viewport-large') continue;
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
	console.error('  큰 뷰포트가 의도라면(주소창 접기 여유 — BUG-265) 선언 바로 앞에');
	console.error('  이유를 적은 `/* check-viewport:large */` 주석을 두세요.\n');
	for (const v of violations) console.error('  ' + v);
	process.exit(1);
}
console.log('✓ vh 높이에 dvh 폴백이 모두 있음.');
