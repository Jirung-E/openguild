// DEV-101: UI 크기 조절 — root font-size scale.
//
// 사용자가 Settings 의 슬라이더로 변경 → store update → +layout 의 effect 가
// `<html>` 의 font-size 갱신 → rem 기반 layout 전체가 비례 확대/축소.
//
// 영속화: localStorage `openguild.uiScale` (소수, 1.0 = 100%).
// 환경: HTTP 모드도 동작. Tauri webview 의 setZoom 은 별도 옵션 (DEV-101 향후).
//
// 범위: 0.5 ~ 2.0 (50%~200%). 그 밖은 layout 깨짐 가능 → clamp.

import { writable } from 'svelte/store';

const KEY = 'openguild.uiScale';
export const MIN_SCALE = 0.5;
export const MAX_SCALE = 2.0;
export const DEFAULT_SCALE = 1.0;
/** 기본 root font-size — `:root { font-size }` 의 기준값. */
export const BASE_FONT_PX = 16;

function clamp(n: number): number {
	if (!Number.isFinite(n)) return DEFAULT_SCALE;
	return Math.max(MIN_SCALE, Math.min(MAX_SCALE, n));
}

function loadInitial(): number {
	if (typeof localStorage === 'undefined') return DEFAULT_SCALE;
	try {
		const raw = localStorage.getItem(KEY);
		if (!raw) return DEFAULT_SCALE;
		const n = Number.parseFloat(raw);
		return clamp(n);
	} catch {
		return DEFAULT_SCALE;
	}
}

export const uiScale = writable<number>(loadInitial());

uiScale.subscribe((s) => {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(KEY, String(s));
	} catch {
		/* storage full / disabled — 무시. */
	}
});

/** 현재 store 값을 강제 갱신 (slider 값 입력용). 자동 clamp. */
export function setUiScale(scale: number) {
	uiScale.set(clamp(scale));
}

/** 100% 로 reset. */
export function resetUiScale() {
	uiScale.set(DEFAULT_SCALE);
}

/**
 * `<html>` 의 font-size 를 갱신해서 rem 기반 layout 을 확대/축소.
 * `+layout.svelte` 가 onMount + store subscribe 로 호출.
 */
export function applyUiScaleToDocument(scale: number) {
	if (typeof document === 'undefined') return;
	const s = clamp(scale);
	document.documentElement.style.fontSize = `${(BASE_FONT_PX * s).toFixed(2)}px`;
}
