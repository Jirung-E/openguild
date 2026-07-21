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
/**
 * DEV-275: 상한 1800 → 3200. 1800 은 1080p~1440p 기준이라 울트라와이드/4K
 * (3440·3840px)에서 양옆 여백이 과하게 남는다는 사용자 요청.
 *
 * 그리고 이 **최대값은 "화면 전체"(폭 제한 없음)** 로 취급한다 — 모니터가
 * 얼마나 넓든 슬라이더 끝까지 밀면 창 전체를 쓰게 되므로, 특정 픽셀값이
 * 또 부족해지는 일이 없다(`isFullWidth` / `contentWidthCss` 참조).
 */
export const MAX_CONTENT_WIDTH = 3200;
/** 기본 컨텐츠 폭 (px). 1100 = 기존 Home / Campaigns 페이지의 max-width 와 일치. */
export const DEFAULT_CONTENT_WIDTH = 1100;

/** DEV-275: 최대값 = 폭 제한 해제("화면 전체"). */
export function isFullWidth(w: number): boolean {
	return w >= MAX_CONTENT_WIDTH;
}

/**
 * DEV-275: `--content-max-width` 에 넣을 CSS 값. 최대값이면 `none` 을 줘서
 * 각 페이지의 `max-width: var(--content-max-width, …)` 가 제한 없이 창을
 * 꽉 채우게 한다.
 */
export function contentWidthCss(w: number): string {
	return isFullWidth(w) ? 'none' : `${w}px`;
}

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

// BUG-141 후속(uiScale.ts 와 동일 이유): 동기 localStorage.setItem 이 슬라이더
// 드래그 중 매 pointermove 마다 그대로 실행되던 것을 rAF 병합으로 프레임당
// 1회로 상한.
let persistRafId: number | null = null;
let pendingPersist = DEFAULT_CONTENT_WIDTH;
contentWidth.subscribe((w) => {
	if (typeof localStorage === 'undefined') return;
	pendingPersist = w;
	if (typeof requestAnimationFrame === 'undefined') {
		try {
			localStorage.setItem(KEY, String(pendingPersist));
		} catch {
			/* 무시 */
		}
		return;
	}
	if (persistRafId !== null) return;
	persistRafId = requestAnimationFrame(() => {
		persistRafId = null;
		try {
			localStorage.setItem(KEY, String(pendingPersist));
		} catch {
			/* 무시 */
		}
	});
});

export function setContentWidth(w: number) {
	contentWidth.set(clamp(w));
}

export function resetContentWidth() {
	contentWidth.set(DEFAULT_CONTENT_WIDTH);
}
