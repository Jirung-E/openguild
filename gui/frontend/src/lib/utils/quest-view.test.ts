import { describe, it, expect, beforeEach } from 'vitest';
import { loadQuestView, saveQuestView, questViewHref } from './quest-view';

describe('quest-view — DEV-453 보드 · 목록 한 메뉴', () => {
	beforeEach(() => localStorage.clear());

	it('처음에는 보드', () => {
		expect(loadQuestView()).toBe('board');
		expect(questViewHref()).toBe('/?view=board');
	});

	it('마지막에 본 쪽을 기억한다', () => {
		saveQuestView('list');
		expect(loadQuestView()).toBe('list');
		expect(questViewHref()).toBe('/?view=list');
		saveQuestView('board');
		expect(questViewHref()).toBe('/?view=board');
	});

	it('모르는 값은 보드로', () => {
		localStorage.setItem('openguild.questView', 'weird');
		expect(loadQuestView()).toBe('board');
	});
});
