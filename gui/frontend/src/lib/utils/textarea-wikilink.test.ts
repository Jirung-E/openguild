import { describe, it, expect } from 'vitest';
import { wikiMatch } from './textarea-wikilink';
import type { IndexedRef } from '$lib/stores/questIndex';

// DEV-173: `[[` 컨텍스트 자동완성 — 규칙 slug 포함.
describe('wikiMatch', () => {
	const index = new Map<string, IndexedRef>([
		['DEV-033', { title: '퀘스트 목록', kind: 'quest' }],
		['C-001', { title: '베타 0.3.0', kind: 'campaign' }],
		['RELEASE-PROCESS', { title: '릴리즈 패키지 절차', kind: 'rule', slug: 'release-process' }],
		// DEV-218: 도서관 문서 — BOOK-NNN 은 XXX-NNN 형식이라 기존 매칭에 자동 포함.
		['BOOK-001', { title: '설계 결정 기록', kind: 'book' }]
	]);

	// DEV-220: bare 토큰(대괄호 없음)은 더 이상 자동완성 트리거가 아님.
	it('bare 토큰은 제안하지 않음 (DEV-220 — [[ 컨텍스트 전용)', () => {
		const v = '관련: DEV-0';
		expect(wikiMatch(v, v.length, index)).toBeNull();
	});

	it('[[ 컨텍스트는 규칙 slug 도 제안하고 치환 범위가 [[ 를 포함', () => {
		const v = '자세한 건 [[rel';
		const m = wikiMatch(v, v.length, index);
		expect(m).not.toBeNull();
		expect(m!.wikiContext).toBe(true);
		// from 이 `[[` 시작 위치 (applyWikiLink 가 `[[slug]]` 로 통째 치환).
		expect(v.slice(m!.from, m!.to)).toBe('[[rel');
		const rule = m!.items.find((i) => i.kind === 'rule');
		expect(rule).toBeDefined();
		expect(rule!.insert).toBe('release-process');
	});

	it('[[ 컨텍스트에서 quest ID prefix 도 매칭', () => {
		const v = '[[dev-0';
		const m = wikiMatch(v, v.length, index);
		expect(m).not.toBeNull();
		expect(m!.items.some((i) => i.id === 'DEV-033')).toBe(true);
	});

	// DEV-223: 빈 [[ 에서도 전체 후보 표시 (사용자 결정).
	it('빈 [[ 는 전체 후보를 제안', () => {
		const v = '앞 텍스트 [[';
		const m = wikiMatch(v, v.length, index);
		expect(m).not.toBeNull();
		expect(m!.items.length).toBe(index.size);
		expect(v.slice(m!.from, m!.to)).toBe('[[');
	});

	it('매칭 없는 [[ prefix 는 null', () => {
		const v = '[[zzz';
		expect(wikiMatch(v, v.length, index)).toBeNull();
	});

	// DEV-218: 도서관 문서도 [[ 컨텍스트에서 제안 — insert 는 대문자 정규형 ID.
	it('[[ 컨텍스트에서 도서관 BOOK ID 도 매칭', () => {
		const v = '[[book';
		const m = wikiMatch(v, v.length, index);
		expect(m).not.toBeNull();
		const book = m!.items.find((i) => i.kind === 'book');
		expect(book).toBeDefined();
		expect(book!.id).toBe('BOOK-001');
		expect(book!.insert ?? book!.id).toBe('BOOK-001');
	});

	// DEV-173 후속: 한글 등 비ASCII 규칙 slug 도 [[ 컨텍스트에서 매칭.
	it('[[ 컨텍스트에서 한글 규칙 slug 매칭', () => {
		const idx = new Map<string, IndexedRef>([
			['커밋규칙', { title: '커밋 규칙', kind: 'rule', slug: '커밋규칙' }]
		]);
		const v = '[[커밋';
		const m = wikiMatch(v, v.length, idx);
		expect(m).not.toBeNull();
		expect(m!.items[0].insert).toBe('커밋규칙');
	});
});
