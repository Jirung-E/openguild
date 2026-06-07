import '@testing-library/jest-dom/vitest';

// DEV-074 fix14: jsdom 에 ResizeObserver 가 없음 — OverlayScrollbar 가 사용.
// 컴포넌트가 검색 결과 list 안에 mount 될 때 (예: QuestCombobox) test 실패 방지.
if (typeof globalThis.ResizeObserver === 'undefined') {
	globalThis.ResizeObserver = class {
		observe() {}
		unobserve() {}
		disconnect() {}
	} as unknown as typeof ResizeObserver;
}
