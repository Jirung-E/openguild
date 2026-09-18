// DEV-418: `[[` 자동완성이 관리번호로만 찾았다. 번호를 외우지 못하면(대부분 그렇다)
// 쓸모가 없다. 이제 제목으로도, 중간 글자로도 찾되 **번호로 맞은 것이 위에** 온다.

import { describe, it, expect } from 'vitest';
import type { IndexedRef } from '$lib/stores/questIndex';
import { wikiRank, byRankThenId, RANK_ID_PREFIX, RANK_TITLE_PREFIX, RANK_ID_PART, RANK_TITLE_PART } from './wiki-rank';

const quest = (title: string): IndexedRef => ({ kind: 'quest', title }) as IndexedRef;
const book = (title: string, path?: string): IndexedRef =>
	({ kind: 'book', title, path }) as IndexedRef;
const rule = (slug: string): IndexedRef => ({ kind: 'rule', title: slug, slug }) as IndexedRef;

describe('DEV-418 제목으로도 찾는다', () => {
	it('한글 제목의 중간 글자로 찾는다', () => {
		expect(wikiRank('DEV-006', quest('WebSocket 재연결 로직'), '재연결')).toBe(RANK_TITLE_PART);
	});

	it('제목 앞부분이 번호 중간보다 위다', () => {
		expect(wikiRank('DEV-006', quest('재연결 로직'), '재연결')).toBe(RANK_TITLE_PREFIX);
		expect(wikiRank('BUG-045', quest('다른 것'), '045')).toBe(RANK_ID_PART);
		expect(RANK_TITLE_PREFIX).toBeLessThan(RANK_ID_PART);
	});

	it('번호로 맞으면 언제나 맨 위다', () => {
		expect(wikiRank('DEV-006', quest('아무 제목'), 'dev-0')).toBe(RANK_ID_PREFIX);
	});

	it('안 맞으면 null — 목록에서 빠진다', () => {
		expect(wikiRank('DEV-006', quest('훅'), '없는말')).toBeNull();
	});

	it('빈 질의는 전부 보여 준다', () => {
		expect(wikiRank('DEV-006', quest('훅'), '')).toBe(RANK_ID_PREFIX);
	});

	it('도서관은 폴더/제목 경로로도 찾는다', () => {
		expect(wikiRank('BOOK-003', book('라우터 설계', '아키텍처'), '아키텍처/라우터')).toBe(
			RANK_TITLE_PREFIX
		);
		// 제목 중간 글자도.
		expect(wikiRank('BOOK-003', book('라우터 설계', '아키텍처'), '설계')).toBe(RANK_TITLE_PART);
	});

	it('규칙은 slug 가 곧 이름이라 번호 쪽으로 맞는다', () => {
		// 규칙의 id 는 사람이 읽는 slug 그대로다 — 제목 규칙까지 갈 것도 없이 여기서 걸린다.
		expect(wikiRank('릴리즈-절차', rule('릴리즈-절차'), '절차')).toBe(RANK_ID_PART);
	});

	it('대소문자는 무시한다', () => {
		expect(wikiRank('DEV-006', quest('WebSocket 재연결'), 'websocket')).toBe(RANK_TITLE_PREFIX);
	});

	it('정렬 — 정확도 먼저, 같으면 번호순', () => {
		const rows = [
			{ rank: RANK_TITLE_PART, id: 'DEV-002' },
			{ rank: RANK_ID_PREFIX, id: 'DEV-009' },
			{ rank: RANK_TITLE_PART, id: 'DEV-001' }
		];
		expect([...rows].sort(byRankThenId).map((r) => r.id)).toEqual([
			'DEV-009',
			'DEV-001',
			'DEV-002'
		]);
	});
});
