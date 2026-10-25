// DEV-456: 상단바에 놓을 페이지(사용자 설정). 규칙은 `utils/nav-pages.ts`.
//
// 이 기계의 취향이라 localStorage 하나에 둔다. 창이 여럿이면(새 창으로 열기) 설정 창에서 바꾼 것이 다른 창에도
// 바로 보이도록 storage 이벤트를 듣는다.

import { writable } from 'svelte/store';
import { normalizeNavPages, type NavPageId } from '$lib/utils/nav-pages';

const KEY = 'openguild.navPages';

function load(): NavPageId[] {
	try {
		const raw = localStorage.getItem(KEY);
		return normalizeNavPages(raw ? JSON.parse(raw) : undefined);
	} catch {
		return normalizeNavPages(undefined);
	}
}

export const navPages = writable<NavPageId[]>(load());

export function setNavPages(next: readonly NavPageId[]): void {
	const v = normalizeNavPages(next);
	navPages.set(v);
	try {
		localStorage.setItem(KEY, JSON.stringify(v));
	} catch {
		/* 못 적어도 이번 판에서는 동작한다. */
	}
}

export function resetNavPages(): void {
	navPages.set(normalizeNavPages(undefined));
	try {
		localStorage.removeItem(KEY);
	} catch {
		/* 무시 */
	}
}

if (typeof window !== 'undefined') {
	window.addEventListener('storage', (e) => {
		if (e.key === KEY || e.key === null) navPages.set(load());
	});
}
