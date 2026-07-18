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
import { isLinux } from '$lib/utils/platform';

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

// BUG-141 후속: `localStorage.setItem` 은 동기 API 라 슬라이더 드래그 중
// 매 pointermove(store.set 호출)마다 그대로 실행되고 있었다 — 아래
// `applyUiScaleToDocument` 의 font-size 쓰기는 이미 rAF 로 프레임당 1회로
// 병합했는데, 이 persist 쓰기는 병합 없이 그대로 남아 있어 실제 버벅임의
// 더 큰 원인이었던 것으로 보임(실기 확인: 슬라이더 드래그 중 이 store 와
// 무관한 rAF 카운터도 거의 멈춤 — 즉 메인 스레드 자체가 막힘). 같은
// rAF 병합 패턴 적용 — 마지막 값만, 프레임당 최대 1회.
let persistRafId: number | null = null;
let pendingPersist = DEFAULT_SCALE;
uiScale.subscribe((s) => {
	if (typeof localStorage === 'undefined') return;
	pendingPersist = s;
	if (typeof requestAnimationFrame === 'undefined') {
		try {
			localStorage.setItem(KEY, String(pendingPersist));
		} catch {
			/* storage full / disabled — 무시. */
		}
		return;
	}
	if (persistRafId !== null) return;
	persistRafId = requestAnimationFrame(() => {
		persistRafId = null;
		try {
			localStorage.setItem(KEY, String(pendingPersist));
		} catch {
			/* storage full / disabled — 무시. */
		}
	});
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
 * BUG-141 후속: root font-size 변경(rem 기반 전체 요소 재측정 + 텍스트
 * reshape)이 리눅스(WebKitGTK)에서만 유독 무겁다 — 실기 WebKit Inspector
 * Timeline 확인 결과 Layout/Rendering 이 대부분을 차지하고, 이건 "호출
 * 빈도" 문제가 아니라 단일 호출 자체가 느린 것(프레임당 1회로 줄여도,
 * 120ms 간격으로 더 줄여도 CPU 스레드 하나가 100% 로 튐 — 창 자체
 * 리사이즈는 매끈했는데 그건 순수 geometry 변경이라 텍스트 재측정이
 * 없어서였음). `transform: scale()` 프리뷰도 시도했으나 실제 레이아웃과
 * 시각적으로 안 맞아 폐기(드래그 종료 시 부자연스럽게 튐).
 *
 * Windows(WebView2)/macOS(WKWebView) 는 이 문제가 없어서(별개 렌더링
 * 엔진 — 크로미움/WKWebView 는 이 정도 reflow 를 문제없이 처리) 기존처럼
 * 프레임당 실시간 반영을 유지해야 하고, **리눅스에서만** 드래그 중 실시간
 * 미리보기를 포기하고 드래그가 끝날 때 딱 1회만 반영하도록 분기한다.
 */
let rafId: number | null = null;
let pendingScale = DEFAULT_SCALE;
let isDragging = false;

function writeFontSize(scale: number) {
	document.documentElement.style.fontSize = `${(BASE_FONT_PX * scale).toFixed(2)}px`;
}

/** 드래그 시작 — 리눅스에서만 이후 `applyUiScaleToDocument` 갱신을
 * 억제하고 드래그 종료까지 미룬다. `CustomSlider` 의 `onDragStart` 에 연결. */
export function beginUiScaleDrag() {
	if (isLinux()) isDragging = true;
}

/** 드래그 종료 — 리눅스에서 억제해뒀던 최종값을 1회 반영. 다른 플랫폼에선
 * isDragging 이 애초에 안 켜져 있어 noop. */
export function endUiScaleDrag() {
	if (!isDragging) return;
	isDragging = false;
	if (typeof document === 'undefined') return;
	if (rafId !== null) {
		cancelAnimationFrame(rafId);
		rafId = null;
	}
	writeFontSize(pendingScale);
}

export function applyUiScaleToDocument(scale: number) {
	if (typeof document === 'undefined') return;
	pendingScale = clamp(scale);

	if (isDragging) {
		// 리눅스 드래그 중 — DOM 은 건드리지 않는다(값만 저장해뒀다가 드래그
		// 종료 시 `endUiScaleDrag` 가 1회 반영). 슬라이더 위치/퍼센트 표시는
		// 이 함수와 별개로 store(`uiScale`) 구독을 통해 정상적으로 실시간
		// 갱신된다 — 화면 전체 재배치만 미룰 뿐.
		return;
	}

	if (typeof requestAnimationFrame === 'undefined') {
		writeFontSize(pendingScale);
		return;
	}
	if (rafId !== null) return; // 이미 이번 프레임에 예약됨 — 값만 갱신.
	rafId = requestAnimationFrame(() => {
		rafId = null;
		writeFontSize(pendingScale);
	});
}
