// BUG-137: 이 앱은 Tauri 데스크톱 SPA — svelte.config.js 의 adapter-static
// (fallback: index.html)도 "SSR 없이 SPA 로 동작" 의도. 그러나 SSR 을 실제로
// 끄는 설정이 없어 `npm run dev`(웹 dev)가 매 요청을 SSR 하다 브라우저 전용
// API(window/document)를 만지는 컴포넌트(OverlayScrollbar 등)에서 크래시했다.
// SPA 전용으로 못박아 SSR/프리렌더를 끈다 — client 렌더만 수행.
export const ssr = false;
export const prerender = false;
