import { describe, it, expect } from 'vitest';
import { REF_TOKEN, refHref } from './questIndex';

describe('DEV-140 cross-link helpers', () => {
	it('REF_TOKEN matches quest / campaign id forms', () => {
		expect(REF_TOKEN.test('DEV-033')).toBe(true);
		expect(REF_TOKEN.test('BUG-12')).toBe(true);
		expect(REF_TOKEN.test('C-001')).toBe(true);
	});

	it('REF_TOKEN rejects non-id text', () => {
		expect(REF_TOKEN.test('DEV')).toBe(false);
		expect(REF_TOKEN.test('033')).toBe(false);
		expect(REF_TOKEN.test('DEV-')).toBe(false);
		expect(REF_TOKEN.test('hello world')).toBe(false);
		// 토큰 양끝 공백/추가문자가 있으면 단독 토큰이 아님.
		expect(REF_TOKEN.test('DEV-033 ')).toBe(false);
	});

	it('refHref routes campaign vs quest to the right detail page', () => {
		expect(refHref('C-001', 'campaign')).toBe('/campaigns/C-001');
		expect(refHref('DEV-033', 'quest')).toBe('/quests/DEV-033');
	});

	it('wiki-link extraction regex captures ids inside [[ ]]', () => {
		const re = /\[\[([A-Za-z]{1,}-\d+)\]\]/g;
		const text = '[[DEV-033]] 에서 언급, [[C-001]] 참고, 그리고 [[BUG-9]].';
		const ids = [...text.matchAll(re)].map((m) => m[1]);
		expect(ids).toEqual(['DEV-033', 'C-001', 'BUG-9']);
	});
});
