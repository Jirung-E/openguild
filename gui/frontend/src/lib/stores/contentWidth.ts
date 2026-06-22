// DEV-101 fix2: 컨텐츠 표시 영역 폭 — 사용자가 설정 페이지에서 슬라이더로 조정.
//
// UI scale (rem 기반 전체 비례) 과 별개. 페이지의 양옆 안전 영역을 줄여서
// 와이드 모니터에서 컨텐츠 영역을 더 넓게 쓸 수 있게 함.
//
// 적용: `<html style="--content-max-width: Xpx">` → 페이지의
// `max-width: var(--content-max-width, default)` 로 반응. 토큰 미사용 페이지는
// 영향 없음 (점진 마이그레이션).
//
// 영속화: localStorage `openguild.contentWidth`.

import { writable } from 'svelte/store';

const KEY = 'openguild.contentWidth';
export const MIN_CONTENT_WIDTH = 600;
export const MAX_CONTENT_WIDTH = 1800;
/** 기본 컨텐츠 폭 (px). 1100 = 기존 Home / Campaigns 페이지의 max-width 와 일치. */
export const DEFAULT_CONTENT_WIDTH = 1100;

function clamp(n: number): number {
	if (!Number.isFinite(n)) return DEFAULT_CONTENT_WIDTH;
	return Math.max(MIN_CONTENT_WIDTH, Math.min(MAX_CONTENT_WIDTH, Math.round(n)));
}

function loadInitial(): number {
	if (typeof localStorage === 'undefined') return DEFAULT_CONTENT_WIDTH;
	try {
		const raw = localStorage.getItem(KEY);
		if (!raw) return DEFAULT_CONTENT_WIDTH;
		const n = Number.parseFloat(raw);
		return clamp(n);
	} catch {
		return DEFAULT_CONTENT_WIDTH;
	}
}

export const contentWidth = writable<number>(loadInitial());

contentWidth.subscribe((w) => {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(KEY, String(w));
	} catch {
		/* 무시 */
	}
});

export function setContentWidth(w: number) {
	contentWidth.set(clamp(w));
}

export function resetContentWidth() {
	contentWidth.set(DEFAULT_CONTENT_WIDTH);
}
