import { describe, it, expect } from 'vitest';
import { buildLibraryTree, flattenFolderPaths, searchBooks } from './library-tree';
import type { Book, LibraryFolder } from '$lib/api/library';

function book(id: string, title: string, path: string, body = ''): Book {
	return {
		book_id: id,
		number: Number(id.split('-')[1]),
		title,
		body,
		path,
		created_at: '',
		updated_at: '',
		deleted_at: null,
		attachments: []
	};
}
function folder(path: string): LibraryFolder {
	return { path, created_at: '', updated_at: '' };
}

describe('buildLibraryTree', () => {
	it('root 문서와 폴더를 분리하고 빈 폴더도 트리에 포함', () => {
		const tree = buildLibraryTree(
			[folder('아키텍처'), folder('운영')],
			[book('BOOK-001', '온보딩', ''), book('BOOK-002', '라우터 설계', '아키텍처')]
		);
		expect(tree.rootDocs.map((b) => b.book_id)).toEqual(['BOOK-001']);
		expect(tree.roots.map((n) => n.path).sort()).toEqual(['아키텍처', '운영']);
		const arch = tree.roots.find((n) => n.path === '아키텍처')!;
		expect(arch.docs.map((b) => b.book_id)).toEqual(['BOOK-002']);
		const ops = tree.roots.find((n) => n.path === '운영')!;
		expect(ops.docs).toEqual([]);
	});

	it('명시적 폴더 등록 없이 문서 path 만으로도 폴더 노드가 암묵적으로 생김', () => {
		const tree = buildLibraryTree([], [book('BOOK-001', '가이드', '아키텍처/서브')]);
		const arch = tree.roots.find((n) => n.path === '아키텍처')!;
		expect(arch).toBeDefined();
		expect(arch.children.map((n) => n.path)).toEqual(['아키텍처/서브']);
		expect(arch.children[0].docs.map((b) => b.book_id)).toEqual(['BOOK-001']);
	});

	it('flattenFolderPaths 는 정렬된 전체 경로 목록', () => {
		const tree = buildLibraryTree([folder('운영'), folder('아키텍처')], []);
		expect(flattenFolderPaths(tree)).toEqual(['아키텍처', '운영']);
	});
});

describe('searchBooks', () => {
	const books = [
		book('BOOK-001', '라우터 설계', '', '별 상관없는 본문'),
		book('BOOK-002', '온보딩 가이드', '', '라우터 개념도 잠깐 언급'),
		book('BOOK-003', '무관한 문서', '', '검색과 무관한 내용')
	];

	it('빈 쿼리는 null (검색 비활성)', () => {
		expect(searchBooks(books, '')).toBeNull();
		expect(searchBooks(books, '   ')).toBeNull();
	});

	it('제목 매칭이 본문만 매칭보다 먼저 나옴', () => {
		const results = searchBooks(books, '라우터');
		expect(results?.map((b) => b.book_id)).toEqual(['BOOK-001', 'BOOK-002']);
	});

	it('대소문자 무시', () => {
		const upper = [book('BOOK-010', 'API Guide', '', '')];
		expect(searchBooks(upper, 'api')?.map((b) => b.book_id)).toEqual(['BOOK-010']);
	});

	it('매칭 없으면 빈 배열(null 아님 — 검색은 활성 상태)', () => {
		expect(searchBooks(books, '전혀없는단어')).toEqual([]);
	});
});
