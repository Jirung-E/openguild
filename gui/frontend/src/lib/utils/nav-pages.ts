// DEV-456: 상단바(Nav)에 놓을 페이지 — 사용자가 고르고 순서를 정한다. 기본은 예전과 같고(홈 · 퀘스트 · 규칙 ·
// 도서관), 홈은 빠질 수 없다. 상단바에 없는 페이지는 ☰ 메뉴(데스크톱은 타이틀바, 웹은 Nav 의 접힌 메뉴)에 간다.
//
// 길드가 아니라 사람의 취향이라 이 기계(localStorage)에 하나만 둔다 — 퀘스트 보기(`quest-view.ts`)와 같다.

import { questViewHref } from './quest-view';

export type NavPageId =
	| 'home'
	| 'quests'
	| 'rules'
	| 'library'
	| 'campaigns'
	| 'worklog'
	| 'tags'
	| 'admin';

/**
 * 고를 수 있는 페이지 — 이 순서로 설정 화면과 ☰ 메뉴에 나온다. 뒤의 넷은 예전 ☰ 메뉴의 순서 그대로(BUG-201:
 * 관리가 맨 위).
 */
export const ALL_NAV_PAGES: readonly NavPageId[] = [
	'home',
	'quests',
	'rules',
	'library',
	'admin',
	'campaigns',
	'worklog',
	'tags'
];

export const DEFAULT_NAV_PAGES: readonly NavPageId[] = ['home', 'quests', 'rules', 'library'];

/** 빠질 수 없는 페이지 — 맨 앞에 고정. */
export const REQUIRED_NAV_PAGE: NavPageId = 'home';

export const NAV_PAGE_LABEL_KEY: Record<NavPageId, string> = {
	home: 'nav.home',
	quests: 'nav.quests',
	rules: 'nav.rules',
	library: 'nav.library',
	campaigns: 'titlebar.menuCampaigns',
	worklog: 'titlebar.menuWorklog',
	tags: 'titlebar.menuTags',
	admin: 'nav.admin'
};

const isId = (v: unknown): v is NavPageId =>
	typeof v === 'string' && (ALL_NAV_PAGES as readonly string[]).includes(v);

/**
 * 저장된 값을 믿을 수 있는 목록으로 — 모르는 이름 · 겹침은 버리고, 홈은 늘 맨 앞에 있다. 읽을 수 없는 값이면
 * 기본값.
 */
export function normalizeNavPages(v: unknown): NavPageId[] {
	if (!Array.isArray(v)) return [...DEFAULT_NAV_PAGES];
	const seen = new Set<NavPageId>();
	for (const x of v) if (isId(x) && x !== REQUIRED_NAV_PAGE) seen.add(x);
	return [REQUIRED_NAV_PAGE, ...seen];
}

/** 상단바에 없는 페이지 — ☰ 메뉴에 놓는다(고를 수 있는 순서대로). */
export function menuPages(nav: readonly NavPageId[]): NavPageId[] {
	return ALL_NAV_PAGES.filter((id) => !nav.includes(id));
}

/** 켜고 끈다. 홈은 끌 수 없다. 새로 켠 페이지는 맨 뒤에 붙는다. */
export function toggleNavPage(nav: readonly NavPageId[], id: NavPageId): NavPageId[] {
	if (id === REQUIRED_NAV_PAGE) return [...nav];
	return nav.includes(id) ? nav.filter((x) => x !== id) : [...nav, id];
}

/** 한 칸 앞(-1) · 뒤(+1)로. 홈 자리(맨 앞)로는 못 간다. */
export function moveNavPage(nav: readonly NavPageId[], id: NavPageId, delta: -1 | 1): NavPageId[] {
	const i = nav.indexOf(id);
	const j = i + delta;
	if (i < 1 || j < 1 || j >= nav.length) return [...nav];
	const out = [...nav];
	[out[i], out[j]] = [out[j], out[i]];
	return out;
}

export function navPageHref(id: NavPageId): string {
	switch (id) {
		case 'home':
			return '/';
		case 'quests':
			// DEV-453: 마지막에 본 쪽(보드 · 목록)으로.
			return questViewHref();
		default:
			return `/${id}`;
	}
}

/** 지금 보고 있는 페이지인지 — 메뉴의 강조. */
export function isNavPageActive(id: NavPageId, url: URL): boolean {
	const path = url.pathname;
	if (id === 'home' || id === 'quests') {
		if (path !== '/') return false;
		const view = url.searchParams.get('view');
		const onQuests = view === 'board' || view === 'list';
		return id === 'quests' ? onQuests : !onQuests;
	}
	return path === `/${id}` || path.startsWith(`/${id}/`);
}
