import { describe, it, expect } from 'vitest';
import { readTab, tabUrl, needsNavigation } from './url-tab';

const TABS = ['structure', 'plugins', 'backup'] as const;
const FIRST = 'structure';
const u = (s: string) => new URL(s, 'http://x');

describe('readTab', () => {
	it('주소에 적힌 탭을 읽는다', () => {
		expect(readTab(u('/admin?tab=plugins'), TABS, FIRST)).toBe('plugins');
	});

	it('안 적혔으면 첫 탭', () => {
		expect(readTab(u('/admin'), TABS, FIRST)).toBe(FIRST);
	});

	// 주소는 사용자가 손으로 고칠 수 있다 — 모르는 값에 빈 화면을 보여주면 안 된다.
	it('모르는 탭이면 첫 탭', () => {
		expect(readTab(u('/admin?tab=없는것'), TABS, FIRST)).toBe(FIRST);
	});
});

describe('tabUrl', () => {
	it('탭을 적는다', () => {
		expect(tabUrl(u('/admin'), 'plugins', FIRST)).toBe('/admin?tab=plugins');
	});

	// 첫 탭은 안 적는다 — `/admin` 과 `/admin?tab=structure` 가 기록에 둘로 쌓이면
	// 뒤로 가기가 한 번 헛돈다.
	it('첫 탭이면 지운다', () => {
		expect(tabUrl(u('/admin?tab=plugins'), FIRST, FIRST)).toBe('/admin');
	});

	it('다른 조건은 그대로 둔다', () => {
		expect(tabUrl(u('/admin?q=x&tab=backup'), 'plugins', FIRST)).toBe('/admin?q=x&tab=plugins');
		expect(tabUrl(u('/admin?q=x&tab=backup'), FIRST, FIRST)).toBe('/admin?q=x');
	});
});

describe('needsNavigation', () => {
	it('이미 그 탭이면 안 움직인다', () => {
		expect(needsNavigation(u('/admin?tab=plugins'), 'plugins', TABS, FIRST)).toBe(false);
		expect(needsNavigation(u('/admin'), FIRST, TABS, FIRST)).toBe(false);
		// `?tab=structure` 라고 적혀 있어도 같은 자리다.
		expect(needsNavigation(u('/admin?tab=structure'), FIRST, TABS, FIRST)).toBe(false);
	});

	it('다른 탭이면 움직인다', () => {
		expect(needsNavigation(u('/admin'), 'backup', TABS, FIRST)).toBe(true);
	});
});
