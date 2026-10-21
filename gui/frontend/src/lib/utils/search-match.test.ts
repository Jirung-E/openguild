import { describe, it, expect } from 'vitest';
import { needsServerSearch } from './search-match';

describe('BUG-371 needsServerSearch', () => {
	const scope = (titleOnly = false, comments = false, attachments = false) => ({
		titleOnly,
		comments,
		attachments
	});
	it('검색어가 없으면 묻지 않는다', () => {
		expect(needsServerSearch('  ', scope(false, true, true))).toBe(false);
	});
	it('본문까지 찾으면(기본) 서버에 묻는다 — 받은 목록에는 본문이 없다', () => {
		expect(needsServerSearch('render', scope())).toBe(true);
	});
	it('제목만이면 받은 목록으로 충분하다', () => {
		expect(needsServerSearch('render', scope(true))).toBe(false);
	});
	it('제목만이어도 댓글 · 첨부를 넓히면 묻는다', () => {
		expect(needsServerSearch('render', scope(true, true))).toBe(true);
		expect(needsServerSearch('render', scope(true, false, true))).toBe(true);
	});
});
