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
 *
 * BUG-141: root font-size 변경은 앱 전체 reflow — 슬라이더 드래그가 매
 * pointermove 마다 이걸 호출하면(CustomSlider 는 step 넘을 때마다 onChange)
 * 프레임당 여러 번 reflow 가 쌓여 Linux(WebKitGTK)에서 심하게 버벅였다.
 * requestAnimationFrame 으로 병합해 프레임당 최대 1회만 DOM 에 쓴다
 * (마지막 값만 반영 — 중간 값은 어차피 그 프레임에 안 보임).
 */
let rafId: number | null = null;
let pendingScale = DEFAULT_SCALE;
export function applyUiScaleToDocument(scale: number) {
	if (typeof document === 'undefined') return;
	pendingScale = clamp(scale);
	if (typeof requestAnimationFrame === 'undefined') {
		document.documentElement.style.fontSize = `${(BASE_FONT_PX * pendingScale).toFixed(2)}px`;
		return;
	}
	if (rafId !== null) return; // 이미 이번 프레임에 예약됨 — 값만 갱신.
	rafId = requestAnimationFrame(() => {
		rafId = null;
		document.documentElement.style.fontSize = `${(BASE_FONT_PX * pendingScale).toFixed(2)}px`;
	});
}
