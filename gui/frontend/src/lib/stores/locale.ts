// DEV-015 (MVP): 영/한 언어 토글. theme.ts 와 동일 패턴.
//
// 범위: 이 store + t() 사전 + 토글 UI(SettingsQuickMenu) 까지가 1차 — 앱 전역
// 문자열(다른 컴포넌트/CLI/core/server 메시지) 스윕은 후속(DEV-205)에서 점진
// 확장. 영속: localStorage `openguild.locale`. 기본은 'ko' (현재 앱 기본과 동일
// — 기존 사용자 경험 변화 없음).

import { writable } from 'svelte/store';

export type Locale = 'ko' | 'en';

const KEY = 'openguild.locale';

function loadInitial(): Locale {
	if (typeof localStorage === 'undefined') return 'ko';
	try {
		const raw = localStorage.getItem(KEY);
		if (raw === 'ko' || raw === 'en') return raw;
		return 'ko';
	} catch {
		return 'ko';
	}
}

export const locale = writable<Locale>(loadInitial());

locale.subscribe((l) => {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(KEY, l);
	} catch {
		/* 무시 */
	}
});

export function setLocale(l: Locale) {
	locale.set(l);
}

/**
 * DEV-015 (MVP): 번역 사전. 키는 의미 단위 — 컴포넌트가 늘어날 때마다 점진
 * 추가(DEV-205). 누락 키는 ko 텍스트를 그대로 반환(항상 안전한 fallback).
 */
const DICT: Record<string, { ko: string; en: string }> = {
	'settings.theme': { ko: '테마', en: 'Theme' },
	'settings.theme.system': { ko: '시스템', en: 'System' },
	'settings.theme.light': { ko: '라이트', en: 'Light' },
	'settings.theme.dark': { ko: '다크', en: 'Dark' },
	'settings.uiScale': { ko: 'UI 크기', en: 'UI Scale' },
	'settings.contentWidth': { ko: '컨텐츠 폭', en: 'Content Width' },
	'settings.language': { ko: '언어', en: 'Language' },
	'settings.all': { ko: '전체 설정 →', en: 'All settings →' }
};

/** 현재 locale 기준 번역. 누락 키는 ko 원문 그대로(안전한 fallback). */
export function t(key: string, l: Locale): string {
	const entry = DICT[key];
	if (!entry) return key;
	return l === 'en' ? entry.en : entry.ko;
}
