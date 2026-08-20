// DEV-074: 다크 / 라이트 / 시스템 테마 토글.
//
// 사용자가 Settings 에서 선택 → store update → +layout 의 effect 가 `<html>`
// 의 `data-theme` 갱신 → CSS variable 자동 swap.
//
// 영속: localStorage `openguild.theme`. 기본은 'system' (OS 기본 따름).
//
// system 모드는 `prefers-color-scheme` media query 감지. OS 테마 변경 시 즉시
// 반영 (matchMedia listener).

import { writable, get } from 'svelte/store';

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
/**
 * BUG-239: 테마 전환 페이드 클래스를 떼기 위한 타이머 핸들.
 * 연속 전환 시 앞선 해제 예약을 취소하고 다시 잡는다.
 */
let themeFadeTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * BUG-239: 테마 전환용 균일 transition 클래스가 붙어있는 시간(ms).
 *
 * global.css 의 `--theme-fade`(200ms)보다 **길어야** 한다. 페이드 도중 클래스가
 * 빠지면 각 요소가 원래의 제각각인 transition 으로 되돌아가 색이 튄다.
 * 여유분 60ms.
 */
const THEME_FADE_WINDOW_MS = 260;

export function applyThemeToDocument(t: ThemeChoice) {
	if (typeof document === 'undefined') return;
	const eff = resolveTheme(t);
	const root = document.documentElement;

	// BUG-239: 테마가 바뀌면 CSS 변수(--bg/--text/--border …) 값이 바뀌는데,
	// 컴포넌트들이 hover 피드백용으로 걸어둔 `transition: background/color/all`
	// 이 그 변화까지 애니메이션한다. 지속시간이 0.1s~0.4s 로 제각각이라
	// transition 이 없는 요소는 즉시, 긴 요소는 늦게 바뀌어 **시간차**로 보인다.
	//
	// 전환하는 동안만 색 계열에 **균일한** transition 을 덮어씌워, 모든 요소가
	// 같은 곡선·같은 시간에 함께 넘어가게 한다(global.css 의 `.theme-switching`).
	// 클래스를 먼저 붙이고 그 다음 속성을 바꿔야, 값이 바뀔 때 이미 균일한
	// transition 이 걸려 있다.
	root.classList.add('theme-switching');
	root.setAttribute('data-theme', eff);

	// 해제는 **타이머**로. `requestAnimationFrame` 은 숨겨진 문서
	// (`visibilityState: hidden`, 예: 배경 자식 창)에서 발화하지 않아 클래스가
	// 영구히 남는다 — 그러면 그 창은 hover 전환을 영원히 잃는다(BUG-238 에서
	// 같은 함정을 확인했다).
	if (themeFadeTimer !== null) clearTimeout(themeFadeTimer);
	themeFadeTimer = setTimeout(() => {
		root.classList.remove('theme-switching');
		themeFadeTimer = null;
	}, THEME_FADE_WINDOW_MS);
}

/** OS 테마 변경 listener 등록 — 호출자가 cleanup 반환. */
export function watchSystemPreference(onChange: (eff: EffectiveTheme) => void): () => void {
	if (typeof matchMedia === 'undefined') return () => {};
	const mq = matchMedia('(prefers-color-scheme: light)');
	const handler = () => onChange(mq.matches ? 'light' : 'dark');
	mq.addEventListener('change', handler);
	return () => mq.removeEventListener('change', handler);
}

/**
 * BUG-121: 실제 렌더에 쓰이는 effective theme('dark'/'light') 전용 스토어.
 *
 * `theme`(ThemeChoice: 'dark'/'light'/'system')만 구독하면 'system' 모드에서
 * OS 가 테마를 바꿔도 `theme` 스토어 값 자체는 그대로 'system' 이라 svelte
 * writable 이 (같은 값 재-set 은 무시) 구독자에게 전혀 알리지 않는다 — CSS
 * variable(`<html data-theme>`, +layout 에서 별도 처리)은 문제없지만, JS 가
 * 색을 직접 계산해야 하는 곳(Cytoscape/SVG data URL 등, `var()` 미지원)은
 * 이 신호를 못 받아 OS 테마 전환 시 갱신되지 않았다(사용자 보고 — Quest
 * Board 노드가 테마 변경에 반응 안 함).
 *
 * `theme` 변경 + OS preference 변경(단, 'system' 모드일 때만) 둘 다 반영해
 * 실제로 값이 바뀔 때만 set — safe_not_equal 체크를 자연히 통과.
 */
export const effectiveTheme = writable<EffectiveTheme>(resolveTheme(get(theme)));

theme.subscribe((t) => {
	effectiveTheme.set(resolveTheme(t));
});

if (typeof matchMedia !== 'undefined') {
	matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
		if (get(theme) === 'system') {
			effectiveTheme.set(systemPreference());
		}
	});
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

// DEV-114: 커스텀 테마의 palette 연동 — CSS var() 를 못 쓰는 소비처
// (Cytoscape/SVG data URL)에도 사용자 override 가 반영되도록 themePalette()
// 결과에 병합. customThemes 스토어(activate/deactivate)가 설정/해제.
let paletteOverrides: Partial<ThemePalette> = {};

export function setPaletteOverrides(overrides: Partial<ThemePalette>) {
	paletteOverrides = overrides;
}

export function themePalette(eff: EffectiveTheme): ThemePalette {
	const base = eff === 'light' ? LIGHT_PALETTE : DARK_PALETTE;
	return Object.keys(paletteOverrides).length === 0 ? base : { ...base, ...paletteOverrides };
}
