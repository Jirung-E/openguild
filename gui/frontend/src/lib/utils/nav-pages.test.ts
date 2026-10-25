import { describe, expect, it } from 'vitest';
import {
	DEFAULT_NAV_PAGES,
	isNavPageActive,
	menuPages,
	moveNavPage,
	navPageHref,
	normalizeNavPages,
	toggleNavPage
} from './nav-pages';

describe('DEV-456 상단바 페이지', () => {
	it('기본은 예전과 같다 — 홈 · 퀘스트 · 규칙 · 도서관', () => {
		expect(normalizeNavPages(undefined)).toEqual(['home', 'quests', 'rules', 'library']);
		expect(normalizeNavPages('broken')).toEqual([...DEFAULT_NAV_PAGES]);
	});

	it('홈은 늘 맨 앞에 있고, 모르는 이름 · 겹침은 버린다', () => {
		expect(normalizeNavPages(['library', 'nope', 'library', 'campaigns'])).toEqual([
			'home',
			'library',
			'campaigns'
		]);
		expect(normalizeNavPages(['tags', 'home'])).toEqual(['home', 'tags']);
		expect(normalizeNavPages([])).toEqual(['home']);
	});

	it('홈은 끌 수 없고, 켠 페이지는 뒤에 붙는다', () => {
		const nav = normalizeNavPages(undefined);
		expect(toggleNavPage(nav, 'home')).toEqual(nav);
		expect(toggleNavPage(nav, 'rules')).toEqual(['home', 'quests', 'library']);
		expect(toggleNavPage(nav, 'campaigns')).toEqual([
			'home',
			'quests',
			'rules',
			'library',
			'campaigns'
		]);
	});

	it('순서는 바꿀 수 있지만 홈 자리로는 못 간다', () => {
		const nav = normalizeNavPages(undefined);
		expect(moveNavPage(nav, 'library', -1)).toEqual(['home', 'quests', 'library', 'rules']);
		expect(moveNavPage(nav, 'quests', -1)).toEqual(nav);
		expect(moveNavPage(nav, 'library', 1)).toEqual(nav);
		expect(moveNavPage(nav, 'home', 1)).toEqual(nav);
	});

	it('상단바에 없는 페이지는 메뉴로', () => {
		// 기본은 예전 ☰ 메뉴와 같은 순서.
		expect(menuPages(normalizeNavPages(undefined))).toEqual(['admin', 'campaigns', 'worklog', 'tags']);
		expect(menuPages(['home', 'campaigns'])).toEqual([
			'quests',
			'rules',
			'library',
			'admin',
			'worklog',
			'tags'
		]);
	});

	it('주소와 강조', () => {
		expect(navPageHref('home')).toBe('/');
		expect(navPageHref('tags')).toBe('/tags');
		const u = (s: string) => new URL(s, 'http://x');
		expect(isNavPageActive('home', u('/'))).toBe(true);
		expect(isNavPageActive('home', u('/?view=board'))).toBe(false);
		expect(isNavPageActive('quests', u('/?view=list'))).toBe(true);
		expect(isNavPageActive('campaigns', u('/campaigns/C-001'))).toBe(true);
		expect(isNavPageActive('tags', u('/tagsx'))).toBe(false);
	});
});
