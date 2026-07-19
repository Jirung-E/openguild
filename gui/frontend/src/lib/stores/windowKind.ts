// DEV-255: 자식윈도우(검색 팔레트 "새 창으로 열기") 여부 — 창 label 이
// `open-item.ts` 가 붙이는 `item-*` 접두면 자식윈도우로 판정. TitleBar 가
// 이 값을 보고 메뉴바/뒤로·앞으로/검색 팔레트를 숨긴다(단일 문서 보기 창이라
// 다른 곳으로 이동할 필요가 없음).
import { writable } from 'svelte/store';

// 동기 판정 — `window.__TAURI_INTERNALS__.metadata.currentWindow.label` 은
// (Tauri v2 내부 구현) invoke 없이 바로 읽을 수 있어, 비동기 import 를 기다리는
// 동안 TitleBar 가 잠깐 "메인윈도우처럼"(메뉴/검색 보임) 그려졌다가 사라지는
// 깜빡임을 피할 수 있다. 초기값을 즉시 계산해 store 에 심는다.
function readLabel(): string | null {
	if (typeof window === 'undefined') return null;
	const w = window as unknown as {
		__TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } };
	};
	return w.__TAURI_INTERNALS__?.metadata?.currentWindow?.label ?? null;
}

export const isChildWindow = writable(readLabel()?.startsWith('item-') ?? false);

let detected = false;

/** SSR/초기 렌더 시 window 가 없을 수 있어 mount 후 한 번 더 확정. */
export function detectWindowKind(): void {
	if (detected) return;
	detected = true;
	const label = readLabel();
	if (label) isChildWindow.set(label.startsWith('item-'));
}
