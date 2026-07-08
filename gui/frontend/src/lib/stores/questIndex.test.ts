import { describe, it, expect } from 'vitest';
import { REF_TOKEN, refHref, parseCrossLinkToken } from './questIndex';

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

	// DEV-173: 규칙은 원본 대소문자 slug 로 /rules 딥링크.
	it('refHref routes rules to /rules?slug= with original-case slug', () => {
		expect(refHref('RELEASE-PROCESS', 'rule', 'release-process')).toBe(
			'/rules?slug=release-process'
		);
		// slug 미제공 시 소문자 fallback.
		expect(refHref('FOO-BAR', 'rule')).toBe('/rules?slug=foo-bar');
	});

	// DEV-218: 도서관 문서는 /library?id= 딥링크.
	it('refHref routes books to /library?id=', () => {
		expect(refHref('BOOK-001', 'book')).toBe('/library?id=BOOK-001');
	});

	it('wiki-link extraction regex captures ids inside [[ ]]', () => {
		const re = /\[\[([A-Za-z]{1,}-\d+)\]\]/g;
		const text = '[[DEV-033]] 에서 언급, [[C-001]] 참고, 그리고 [[BUG-9]].';
		const ids = [...text.matchAll(re)].map((m) => m[1]);
		expect(ids).toEqual(['DEV-033', 'C-001', 'BUG-9']);
	});

	// DEV-219: `[[kind:ID]]` 네임스페이스 접두 — 긴 이름 + 짧은 별칭 모두 인식.
	describe('parseCrossLinkToken', () => {
		it('splits a recognized kind prefix from the id', () => {
			expect(parseCrossLinkToken('quest:DEV-033')).toEqual({ kind: 'quest', id: 'DEV-033' });
			expect(parseCrossLinkToken('q:DEV-033')).toEqual({ kind: 'quest', id: 'DEV-033' });
			expect(parseCrossLinkToken('campaign:C-001')).toEqual({ kind: 'campaign', id: 'C-001' });
			expect(parseCrossLinkToken('c:C-001')).toEqual({ kind: 'campaign', id: 'C-001' });
			expect(parseCrossLinkToken('rules:release-process')).toEqual({
				kind: 'rule',
				id: 'release-process'
			});
			expect(parseCrossLinkToken('r:release-process')).toEqual({
				kind: 'rule',
				id: 'release-process'
			});
			expect(parseCrossLinkToken('library:BOOK-001')).toEqual({
				kind: 'book',
				id: 'BOOK-001'
			});
			expect(parseCrossLinkToken('lib:BOOK-001')).toEqual({ kind: 'book', id: 'BOOK-001' });
		});

		it('kind aliases are case-insensitive', () => {
			expect(parseCrossLinkToken('QUEST:DEV-033')).toEqual({ kind: 'quest', id: 'DEV-033' });
		});

		it('falls back to no kind (bare token) when there is no recognized prefix', () => {
			expect(parseCrossLinkToken('DEV-033')).toEqual({ kind: null, id: 'DEV-033' });
			// 콜론이 있어도 별칭이 아니면 통째로 id — 규칙 slug 에 콜론이 올 수도 있음.
			expect(parseCrossLinkToken('foo:bar')).toEqual({ kind: null, id: 'foo:bar' });
		});
	});
});
