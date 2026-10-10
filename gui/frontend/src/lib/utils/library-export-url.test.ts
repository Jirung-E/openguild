// DEV-441: 서버 도서관 내보내기 zip 의 URL — 고른 것이 서버로 그대로 가야 같은 것이 담긴다.

import { describe, it, expect } from 'vitest';
import { libraryExportZipUrl } from './banner';

const parse = (u: string) => new URL(u, 'http://x');

describe('DEV-441 libraryExportZipUrl', () => {
	it('아무것도 안 주면 도서관 전체', () => {
		expect(libraryExportZipUrl({})).toBe('/api/library/export.zip');
	});

	it('폴더 · 문서 하나', () => {
		expect(parse(libraryExportZipUrl({ folder: '설계/결정' })).searchParams.get('folder')).toBe('설계/결정');
		expect(parse(libraryExportZipUrl({ id: 'BOOK-003' })).searchParams.get('id')).toBe('BOOK-003');
	});

	it('고른 것 여럿은 pick 으로 — 화면의 열쇠 그대로, 폴더 · 문서보다 먼저', () => {
		const u = parse(libraryExportZipUrl({ folder: '무시', picks: ['folder:설계 & 기록', 'BOOK-001'] }));
		expect(u.searchParams.getAll('pick')).toEqual(['folder:설계 & 기록', 'BOOK-001']);
		expect(u.searchParams.has('folder')).toBe(false);
	});
});
