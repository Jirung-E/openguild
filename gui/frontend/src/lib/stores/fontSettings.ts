// DEV-272: UI 글꼴 / 코드 글꼴 선택.
//
// 사용자가 고른 이름을 `--font-sans` / `--font-mono` 에 **덧붙여** root 에
// 심는다. 통째로 바꾸지 않는 것이 요점이다 — 뒤쪽 폴백(`-apple-system` …
// `sans-serif`)이 그대로 남아 있어야 설치 안 된 글꼴을 골라도 화면이 깨지지
// 않고, 한글만 없는 글꼴을 골라도 한글은 폴백으로 그려진다.
//
// 영속: localStorage `openguild.font.ui` / `openguild.font.code`. 길드별로
// 나누지 않는다 — "무슨 글꼴로 보고 싶은가" 는 사람의 취향이지 길드 데이터가
// 아니다(theme / uiScale 과 같은 취급).
//
// 후보는 큐레이션 + 직접 입력이다(admin 결정). 시스템 글꼴 열거는 Rust 쪽
// 의존성이 필요하고 **웹/서버 모드에서는 아예 못 쓴다** — 모드마다 다른 UI 가
// 되는 것을 피했다. 직접 입력이 있으므로 목록에 없는 글꼴도 쓸 수 있다.

import { writable } from 'svelte/store';

export type FontKind = 'ui' | 'code';

/** 빈 문자열 = 시스템 기본(토큰의 기본 스택 그대로). */
export type FontChoice = string;

const KEYS: Record<FontKind, string> = {
	ui: 'openguild.font.ui',
	code: 'openguild.font.code'
};

/** 토큰이 들고 있는 기본 스택. 고른 글꼴은 항상 이 **앞에** 붙는다. */
const BASE_STACK: Record<FontKind, string> = {
	ui: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
	code: "'SFMono-Regular', Consolas, ui-monospace, monospace"
};

const CSS_VAR: Record<FontKind, string> = {
	ui: '--font-sans',
	code: '--font-mono'
};

/**
 * 드롭다운 후보. 빈 값이 '시스템 기본'.
 *
 * 설치돼 있는지는 확인하지 않는다 — 없으면 폴백이 받아 주고, 확인하려면
 * 글꼴 열거가 필요한데 그건 웹 모드에서 안 된다. 목록에 있다고 그 글꼴이
 * 보장되는 것은 아니라는 뜻이라, 화면에는 미리보기를 함께 둔다.
 */
export const UI_FONT_PRESETS: FontChoice[] = [
	'',
	'Pretendard',
	'Noto Sans KR',
	'Apple SD Gothic Neo',
	'Malgun Gothic',
	'Segoe UI',
	'Inter'
];

export const CODE_FONT_PRESETS: FontChoice[] = [
	'',
	'D2Coding',
	'JetBrains Mono',
	'Fira Code',
	'Cascadia Code',
	'SF Mono',
	'Consolas'
];

/**
 * 글꼴 이름 자리에 둬도 되는 형태로 다듬는다.
 *
 * 직접 입력이 열려 있으므로 `Foo; } body { display: none` 같은 값이 들어올 수
 * 있다. `style.setProperty` 로 들어가니 선언 밖으로 새지는 않지만, **애초에
 * 이름이 아닌 것을 이름 자리에 두지 않는다.**
 *
 * 위험 문자를 *지우는* 대신 **거기서 자른다.** 지우기만 하면
 * `Foo; } body { display: none` 이 `Foo body display none` 이라는 그럴듯한
 * 이름으로 남아, 사용자는 자기가 뭘 넣었는지도 모른 채 저장하게 된다.
 *
 * 빈 문자열은 '고르지 않음'(시스템 기본).
 */
export function sanitizeFontName(raw: string): string {
	// 1) 스택을 통째로 붙여넣는 경우가 흔하다 — 첫 이름만 본다.
	const firstItem = raw.split(',')[0];
	// 2) 감싼 따옴표는 이름의 일부가 아니다.
	const unquoted = firstItem.replace(/["'`]/g, '');
	// 3) 글꼴 이름에 쓰이는 문자까지만. 한글·CJK·가나도 이름이 될 수 있다.
	const m = unquoted.match(
		/^[\w \-.+\u3040-\u30FF\u3130-\u318F\u4E00-\u9FFF\uAC00-\uD7AF]*/
	);
	return (m ? m[0] : '').replace(/\s+/g, ' ').trim().slice(0, 64);
}

/** 고른 글꼴 + 기본 스택. 고르지 않았으면 기본 스택 그대로. */
export function composeFontStack(kind: FontKind, choice: FontChoice): string {
	const name = sanitizeFontName(choice);
	if (!name) return BASE_STACK[kind];
	return `'${name}', ${BASE_STACK[kind]}`;
}

function loadInitial(kind: FontKind): FontChoice {
	try {
		if (typeof localStorage === 'undefined') return '';
		return sanitizeFontName(localStorage.getItem(KEYS[kind]) ?? '');
	} catch {
		return '';
	}
}

export const uiFont = writable<FontChoice>(loadInitial('ui'));
export const codeFont = writable<FontChoice>(loadInitial('code'));

const STORES = { ui: uiFont, code: codeFont } as const;

/** root 에 실제로 심는다. SSR 에서는 아무것도 안 한다. */
export function applyFontToDocument(kind: FontKind, choice: FontChoice): void {
	if (typeof document === 'undefined') return;
	document.documentElement.style.setProperty(CSS_VAR[kind], composeFontStack(kind, choice));
}

export function setFont(kind: FontKind, choice: FontChoice): void {
	const name = sanitizeFontName(choice);
	STORES[kind].set(name);
	try {
		if (typeof localStorage !== 'undefined') localStorage.setItem(KEYS[kind], name);
	} catch {
		/* quota / disabled — 표시 설정이라 무시해도 된다. */
	}
	applyFontToDocument(kind, name);
}

/** 앱 시작 시 한 번. 저장된 값을 root 에 반영한다. */
export function initFonts(): void {
	applyFontToDocument('ui', loadInitial('ui'));
	applyFontToDocument('code', loadInitial('code'));
}
