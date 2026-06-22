// DEV-074: 다크 / 라이트 / 시스템 테마 토글.
//
// 사용자가 Settings 에서 선택 → store update → +layout 의 effect 가 `<html>`
// 의 `data-theme` 갱신 → CSS variable 자동 swap.
//
// 영속: localStorage `openguild.theme`. 기본은 'system' (OS 기본 따름).
//
// system 모드는 `prefers-color-scheme` media query 감지. OS 테마 변경 시 즉시
// 반영 (matchMedia listener).

import { writable } from 'svelte/store';

export type ThemeChoice = 'dark' | 'light' | 'system';
export type EffectiveTheme = 'dark' | 'light';

const KEY = 'openguild.theme';

function loadInitial(): ThemeChoice {
	if (typeof localStorage === 'undefined') return 'system';
	try {
		const raw = localStorage.getItem(KEY);
		if (raw === 'dark' || raw === 'light' || raw === 'system') return raw;
		return 'system';
	} catch {
		return 'system';
	}
}

export const theme = writable<ThemeChoice>(loadInitial());

theme.subscribe((t) => {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(KEY, t);
	} catch {
		/* 무시 */
	}
});

/** 현재 system preference 의 effective theme. */
export function systemPreference(): EffectiveTheme {
	if (typeof matchMedia === 'undefined') return 'dark';
	return matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

/** ThemeChoice → EffectiveTheme. system 은 OS 감지로 해석. */
export function resolveTheme(t: ThemeChoice): EffectiveTheme {
	if (t === 'system') return systemPreference();
	return t;
}

/** `<html data-theme="...">` 갱신 + CSS variable 자동 적용. */
export function applyThemeToDocument(t: ThemeChoice) {
	if (typeof document === 'undefined') return;
	const eff = resolveTheme(t);
	document.documentElement.setAttribute('data-theme', eff);
}

/** OS 테마 변경 listener 등록 — 호출자가 cleanup 반환. */
export function watchSystemPreference(onChange: (eff: EffectiveTheme) => void): () => void {
	if (typeof matchMedia === 'undefined') return () => {};
	const mq = matchMedia('(prefers-color-scheme: light)');
	const handler = () => onChange(mq.matches ? 'light' : 'dark');
	mq.addEventListener('change', handler);
	return () => mq.removeEventListener('change', handler);
}

export function setTheme(t: ThemeChoice) {
	theme.set(t);
}

/**
 * DEV-074 fix20 (sweep): JS-가-색을-필요로-하는-경우 단일 source.
 *
 * Cytoscape canvas 와 SVG data URL 안에선 CSS `var()` 가 컴퓨팅되지 않아
 * 색을 명시 hex 로 넘겨야 한다. 이전엔 컴포넌트마다 `eff === 'light' ? '#x'
 * : '#y'` 분기를 따로 정의해서 중복 + drift 의 원인이었음 (사용자 보고
 * 2026-06-09).
 *
 * 모든 hex 는 `lib/styles/global.css` 의 토큰과 mirror — 한쪽 변경 시 다른
 * 쪽도 같이 수정해야 함. (CSS 와 JS 둘 다 진리원이 필요한 구조적 한계 —
 * 현재 build 도구에서 CSS var 를 빌드타임 추출하는 방법이 깔끔하지 않아
 * 수동 mirror.)
 *
 * 신규 색 추가 절차:
 *   1. global.css 의 `:root` + `[data-theme='light']` 양쪽에 토큰 추가.
 *   2. `ThemePalette` 인터페이스에 같은 이름의 필드 추가.
 *   3. 아래 두 분기에 hex 추가.
 *   4. 사용처에서 `themePalette(eff).foo` 로 접근.
 */
export interface ThemePalette {
	bg: string;
	bgElevated: string;
	text: string;
	textMuted: string;
	textFaint: string;
	accent: string;
	accentSecondary: string;
	success: string;
	successStrong: string;
	warning: string;
	danger: string;
	orange: string;
	// DEV-074: Quest Board highlight type 색.
	hlPre: string;
	hlPreBg: string;
	hlSub: string;
	hlSubBg: string;
	hlNext: string;
	hlNextBg: string;
	hlParentBg: string;
	selectedBg: string;
	edgePre: string;
}

const DARK_PALETTE: ThemePalette = {
	bg: '#0d1117',
	bgElevated: '#161b22',
	text: '#c9d1d9',
	textMuted: '#8b949e',
	textFaint: '#484f58',
	accent: '#58a6ff',
	accentSecondary: '#79c0ff',
	success: '#56d364',
	successStrong: '#238636',
	warning: '#f5a623',
	danger: '#f85149',
	orange: '#f0883e',
	hlPre: '#a371f7',
	hlPreBg: '#190d33',
	hlSub: '#3dc9b0',
	hlSubBg: '#062220',
	hlNext: '#f0883e',
	hlNextBg: '#2a1200',
	hlParentBg: '#0a2914',
	selectedBg: '#112240',
	edgePre: '#4a90d9'
};

const LIGHT_PALETTE: ThemePalette = {
	bg: '#ffffff',
	bgElevated: '#f6f8fa',
	text: '#1f2328',
	textMuted: '#59636e',
	textFaint: '#8b949e',
	accent: '#0969da',
	accentSecondary: '#0969da',
	success: '#1a7f37',
	successStrong: '#116329',
	warning: '#9a6700',
	danger: '#cf222e',
	orange: '#bc4c00',
	hlPre: '#8250df',
	hlPreBg: '#f3eafe',
	hlSub: '#1a7f64',
	hlSubBg: '#dbf6ee',
	hlNext: '#bc4c00',
	hlNextBg: '#fde6cf',
	hlParentBg: '#dafbe1',
	selectedBg: '#ddf4ff',
	edgePre: '#0969da'
};

export function themePalette(eff: EffectiveTheme): ThemePalette {
	return eff === 'light' ? LIGHT_PALETTE : DARK_PALETTE;
}
