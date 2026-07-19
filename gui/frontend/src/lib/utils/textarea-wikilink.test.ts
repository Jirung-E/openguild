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
		['BOOK-001', { title: '설계 결정 기록', kind: 'book' }],
		// DEV-239: 폴더 있는 도서관 문서 — 경로 기반 매칭 테스트용.
		['BOOK-014', { title: '라우터 설계', kind: 'book', path: '아키텍처' }]
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
		// DEV-219: 자동완성은 항상 kind 네임스페이스 접두를 붙여 삽입.
		expect(rule!.insert).toBe('rules:release-process');
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
		// DEV-219 후속: 실재 ID 외에 네임스페이스 자체 후보(quest:/campaign:/rules:/library:)
		// 4개가 앞에 추가된다.
		expect(m!.items.filter((i) => i.nsPrefix).length).toBe(4);
		expect(m!.items.filter((i) => !i.nsPrefix).length).toBe(index.size);
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
		const book = m!.items.find((i) => i.kind === 'book' && !i.nsPrefix);
		expect(book).toBeDefined();
		expect(book!.id).toBe('BOOK-001');
		// DEV-219: 삽입은 항상 `library:` 접두 포함.
		expect(book!.insert ?? book!.id).toBe('library:BOOK-001');
	});

	// DEV-219 후속: `[[q` 처럼 콜론 없이 타이핑 중이면 네임스페이스 자체
	// (`quest:` 등)도 후보로 뜬다 — 선택하면 접두만 삽입되고 이어서 ID 타이핑.
	it('[[ 컨텍스트에서 콜론 없는 부분 입력도 네임스페이스 후보를 제안', () => {
		const v = '[[q';
		const m = wikiMatch(v, v.length, index);
		expect(m).not.toBeNull();
		const ns = m!.items.find((i) => i.nsPrefix && i.kind === 'quest');
		expect(ns).toBeDefined();
		expect(ns!.insert).toBe('quest:');
	});

	// DEV-239: 폴더/제목 경로로 타이핑해도 찾되, 삽입은 항상 BOOK-NNN.
	it('[[ 컨텍스트에서 도서관 폴더 경로로도 매칭 — 삽입은 BOOK-NNN', () => {
		const v = '[[아키텍처/라';
		const m = wikiMatch(v, v.length, index);
		expect(m).not.toBeNull();
		const book = m!.items.find((i) => i.id === 'BOOK-014');
		expect(book).toBeDefined();
		expect(book!.insert ?? book!.id).toBe('library:BOOK-014');
	});

	// DEV-173 후속: 한글 등 비ASCII 규칙 slug 도 [[ 컨텍스트에서 매칭.
	it('[[ 컨텍스트에서 한글 규칙 slug 매칭', () => {
		const idx = new Map<string, IndexedRef>([
			['커밋규칙', { title: '커밋 규칙', kind: 'rule', slug: '커밋규칙' }]
		]);
		const v = '[[커밋';
		const m = wikiMatch(v, v.length, idx);
		expect(m).not.toBeNull();
		expect(m!.items[0].insert).toBe('rules:커밋규칙');
	});

	// DEV-219: `[[kind:` 접두를 이미 타이핑하면 그 종류로만 필터.
	it('[[ 컨텍스트에서 kind 접두 타이핑 시 해당 종류로만 필터', () => {
		const v = '[[rules:rel';
		const m = wikiMatch(v, v.length, index);
		expect(m).not.toBeNull();
		expect(m!.items.every((i) => i.kind === 'rule')).toBe(true);
		expect(m!.items.some((i) => i.id === 'RELEASE-PROCESS')).toBe(true);
	});

	it('[[ 컨텍스트에서 짧은 kind 별칭(q:)도 인식', () => {
		const v = '[[q:dev';
		const m = wikiMatch(v, v.length, index);
		expect(m).not.toBeNull();
		expect(m!.items.every((i) => i.kind === 'quest')).toBe(true);
	});
});
