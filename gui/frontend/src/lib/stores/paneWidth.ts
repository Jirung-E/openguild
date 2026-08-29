// REQ-015: 좌우 2단 화면(도서관 / 규칙)의 사이드바 폭 — 구분선 드래그로 조절.
//
// **단위가 rem 인 것이 요점이다.** [[BUG-254]] 가 이 사이드바들을 px 고정에서
// rem 으로 바꿨다 — px 면 UI 배율(DEV-101)을 안 따라가 배율을 올렸을 때 칸은
// 그대로인데 안의 버튼·태그·제목이 경계를 넘었다. 사용자가 드래그로 정한 값도
// px 로 저장하면 같은 문제가 그대로 돌아온다. 그래서 드래그의 px 이동량을
// 그때의 root font-size 로 나눠 **rem 으로 저장**한다.
//
// 영속화: localStorage `openguild.paneWidth.{pane}`. 화면마다 적당한 폭이
// 다르므로(도서관은 폴더 트리, 규칙은 slug 목록) pane 별로 따로 기억한다.
//
// 길드별로 나누지 않는다 — 이건 "이 화면을 어떻게 보고 싶은가" 라는 표시
// 설정이지 길드 데이터가 아니다. `uiScale` / `contentWidth` 와 같은 취급.

import { writable, type Writable } from 'svelte/store';

/** 조절 가능한 2단 화면. 값은 localStorage 키에 그대로 들어간다. */
export type PaneId = 'library' | 'rules';

/**
 * 최소/최대 (rem).
 *
 * 최소 10rem — 도서관 폴더 트리의 항목 이름이 두세 글자만 보이면 목록으로서
 * 쓸모가 없다. 최대 30rem — 그 이상은 본문이 눌린다. 둘 다 rem 이므로 배율을
 * 올려도 "글자 기준으로 같은 정도" 를 유지한다.
 */
export const MIN_PANE_REM = 10;
export const MAX_PANE_REM = 30;

/** 기본값 — BUG-254 가 정한 현재 값 그대로. */
const DEFAULTS: Record<PaneId, number> = {
	library: 16.25,
	rules: 15
};

function keyOf(pane: PaneId): string {
	return `openguild.paneWidth.${pane}`;
}

export function clampPaneRem(n: number): number {
	if (!Number.isFinite(n)) return DEFAULTS.library;
	return Math.max(MIN_PANE_REM, Math.min(MAX_PANE_REM, n));
}

function loadInitial(pane: PaneId): number {
	const fallback = DEFAULTS[pane];
	if (typeof localStorage === 'undefined') return fallback;
	try {
		const raw = localStorage.getItem(keyOf(pane));
		if (!raw) return fallback;
		const n = Number.parseFloat(raw);
		return Number.isFinite(n) ? clampPaneRem(n) : fallback;
	} catch {
		return fallback;
	}
}

/**
 * pane 당 store 하나. 같은 pane 을 여러 번 요청해도 같은 store 를 준다 —
 * 페이지가 재마운트돼도 값이 유지되고, 구독이 갈라지지 않는다.
 */
const stores = new Map<PaneId, Writable<number>>();

export function paneWidth(pane: PaneId): Writable<number> {
	const existing = stores.get(pane);
	if (existing) return existing;

	const store = writable<number>(loadInitial(pane));

	// BUG-141 과 같은 이유로 rAF 병합: 드래그 중에는 pointermove 마다 store 가
	// 갱신되는데 `localStorage.setItem` 은 동기 API 라 그대로 두면 메인 스레드가
	// 매 프레임 막힌다. 마지막 값만, 프레임당 최대 1회.
	let rafId: number | null = null;
	let pending = 0;
	store.subscribe((v) => {
		if (typeof localStorage === 'undefined') return;
		pending = v;
		const write = () => {
			try {
				localStorage.setItem(keyOf(pane), String(pending));
			} catch {
				/* storage full / disabled — 표시 설정이라 무시해도 된다. */
			}
		};
		if (typeof requestAnimationFrame === 'undefined') {
			write();
			return;
		}
		if (rafId !== null) return;
		rafId = requestAnimationFrame(() => {
			rafId = null;
			write();
		});
	});

	stores.set(pane, store);
	return store;
}

/** 기본값으로 되돌린다 (더블클릭). */
export function resetPaneWidth(pane: PaneId): void {
	paneWidth(pane).set(DEFAULTS[pane]);
}

/** 그 pane 의 기본 폭 (rem). 테스트/표시용. */
export function defaultPaneRem(pane: PaneId): number {
	return DEFAULTS[pane];
}

/**
 * 드래그 이동량(px)을 rem 으로 바꿔 더한다.
 *
 * `rootPx` 는 그 시점의 root font-size — 배율이 2배면 같은 픽셀을 끌어도
 * rem 증가량은 절반이다. 그래야 화면에서 손이 움직인 거리와 칸이 커지는
 * 거리가 일치한다.
 */
export function applyDragDelta(startRem: number, deltaPx: number, rootPx: number): number {
	const px = rootPx > 0 ? rootPx : 16;
	return clampPaneRem(startRem + deltaPx / px);
}
