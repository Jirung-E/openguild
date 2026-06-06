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
