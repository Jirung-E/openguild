// DEV-272: UI / 코드 글꼴 선택.
//
// 지켜야 할 것 둘.
//
// 1. **폴백이 남아야 한다.** 고른 글꼴을 통째로 갈아끼우면 설치 안 된 이름을
//    골랐을 때 화면 전체가 기본 serif 로 떨어지고, 한글이 없는 글꼴을 골랐을
//    때 한글이 통째로 깨진다. 항상 기존 스택 **앞에** 붙인다.
// 2. **이름이 아닌 것을 이름 자리에 두지 않는다.** 직접 입력이 열려 있다.
import { describe, it, expect, beforeEach } from 'vitest';
import {
	sanitizeFontName,
	composeFontStack,
	setFont,
	uiFont,
	codeFont,
	initFonts,
	UI_FONT_PRESETS,
	CODE_FONT_PRESETS
} from './fontSettings';
import { get } from 'svelte/store';

describe('sanitizeFontName', () => {
	it('멀쩡한 이름은 그대로 — 공백 있는 이름도', () => {
		expect(sanitizeFontName('Pretendard')).toBe('Pretendard');
		expect(sanitizeFontName('Noto Sans KR')).toBe('Noto Sans KR');
		expect(sanitizeFontName('  D2Coding  ')).toBe('D2Coding');
	});

	it('CSS 를 깨거나 선언 밖으로 새려는 문자는 걷어낸다', () => {
		expect(sanitizeFontName('Foo; } body { display: none')).toBe('Foo');
		expect(sanitizeFontName("Foo'; color: red")).toBe('Foo');
		// 자르기 때문에 통째로 사라진다 — 지우기만 했다면 'script' 가 이름으로
		// 남았을 것이다.
		expect(sanitizeFontName('<script>')).toBe('');
	});

	it('스택을 통째로 붙여넣으면 첫 이름만 — 흔한 실수다', () => {
		expect(sanitizeFontName("'JetBrains Mono', ui-monospace, monospace")).toBe('JetBrains Mono');
	});

	it('빈 값 / 공백뿐이면 빈 문자열 = 시스템 기본', () => {
		expect(sanitizeFontName('')).toBe('');
		expect(sanitizeFontName('   ')).toBe('');
		expect(sanitizeFontName(';;;')).toBe('');
	});

	it('지나치게 긴 입력은 자른다', () => {
		expect(sanitizeFontName('a'.repeat(500)).length).toBe(64);
	});
});

describe('composeFontStack — 폴백이 남아야 한다', () => {
	it('고른 글꼴이 맨 앞, 기존 스택은 뒤에 그대로', () => {
		const s = composeFontStack('ui', 'Pretendard');
		expect(s.startsWith("'Pretendard',")).toBe(true);
		expect(s).toContain('sans-serif');
	});

	it('코드 글꼴도 마찬가지 — 끝은 monospace 여야 한다', () => {
		const s = composeFontStack('code', 'D2Coding');
		expect(s.startsWith("'D2Coding',")).toBe(true);
		expect(s.trimEnd().endsWith('monospace')).toBe(true);
	});

	it('안 골랐으면 기본 스택 그대로 — 앞에 빈 따옴표가 붙으면 안 된다', () => {
		const s = composeFontStack('ui', '');
		expect(s.startsWith("'")).toBe(false);
		expect(s).toContain('sans-serif');
	});

	it('깨뜨리려는 입력도 이름 하나로 감싸여 들어간다', () => {
		const s = composeFontStack('code', 'Foo; } body { display: none');
		expect(s).toBe("'Foo', 'SFMono-Regular', Consolas, ui-monospace, monospace");
	});

	it('sans 와 mono 의 기본 스택은 서로 다르다 — 섞이면 코드가 가변폭이 된다', () => {
		expect(composeFontStack('ui', '')).not.toBe(composeFontStack('code', ''));
	});
});

describe('setFont — 저장과 반영', () => {
	beforeEach(() => {
		localStorage.clear();
		document.documentElement.removeAttribute('style');
	});

	it('store / localStorage / root 셋 다 갱신한다', () => {
		setFont('ui', 'Pretendard');
		expect(get(uiFont)).toBe('Pretendard');
		expect(localStorage.getItem('openguild.font.ui')).toBe('Pretendard');
		expect(document.documentElement.style.getPropertyValue('--font-sans')).toContain('Pretendard');
	});

	it('ui 와 code 가 서로를 덮지 않는다', () => {
		setFont('ui', 'Pretendard');
		setFont('code', 'D2Coding');
		expect(document.documentElement.style.getPropertyValue('--font-sans')).toContain('Pretendard');
		expect(document.documentElement.style.getPropertyValue('--font-mono')).toContain('D2Coding');
		expect(document.documentElement.style.getPropertyValue('--font-sans')).not.toContain('D2Coding');
	});

	it('저장할 때도 다듬은 값이 들어간다 — 원본을 그대로 두면 다음 실행에서 다시 위험해진다', () => {
		setFont('code', "Foo'; color: red");
		expect(localStorage.getItem('openguild.font.code')).toBe('Foo');
	});

	it('initFonts 가 저장된 값을 root 에 되살린다', () => {
		localStorage.setItem('openguild.font.code', 'D2Coding');
		initFonts();
		expect(document.documentElement.style.getPropertyValue('--font-mono')).toContain('D2Coding');
	});

	it('저장소가 오염돼 있어도 initFonts 가 던지지 않는다', () => {
		localStorage.setItem('openguild.font.ui', '} * { display: none } .x {');
		expect(() => initFonts()).not.toThrow();
		const v = document.documentElement.style.getPropertyValue('--font-sans');
		expect(v).not.toContain('display');
	});
});

describe('후보 목록', () => {
	it('둘 다 첫 항목이 시스템 기본(빈 값)이다', () => {
		expect(UI_FONT_PRESETS[0]).toBe('');
		expect(CODE_FONT_PRESETS[0]).toBe('');
	});

	it('후보 이름은 전부 그대로 통과한다 — 목록에 못 쓰는 이름이 있으면 안 된다', () => {
		for (const f of [...UI_FONT_PRESETS, ...CODE_FONT_PRESETS].filter(Boolean)) {
			expect(sanitizeFontName(f)).toBe(f);
		}
	});
});
